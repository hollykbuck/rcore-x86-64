//! Timer setup (local APIC timer for both preemption and timekeeping)
//!
//! Limine masks both the 8259 PICs and the IO-APIC before handing over, and
//! the legacy PIT path is unreliable under QEMU's TCG emulation (interrupts
//! are easily lost when the handler is slow). The local APIC timer, on the
//! other hand, fires as reliably as the RISC-V CLINT timer, so it is used for
//! both the time-slice rotation (round-robin preemption) and as the time
//! source.
//!
//! The preemption rate is set to 1000 Hz. QEMU's TCG only advances its
//! virtual clock in real time or under `-icount`, so `-icount shift=auto` is
//! recommended to make the time slices evenly instruction-budgeted rather
//! than host-load dependent (see HANDOVER §5.5).
//!
//! Timekeeping reconstructs a free-running monotonic counter from the APIC
//! timer, the x86-64 analogue of the RISC-V `mtime`:
//!
//! - In periodic mode the current-count register (FEE0 0390H / x2APIC MSR
//!   0x839) is automatically reloaded from the initial-count register each
//!   time the count reaches zero (Intel SDM Vol. 3A §13.5.4).
//! - So `elapsed_ticks = wraps * INITIAL_COUNT + (INITIAL_COUNT - current)`
//!   is a free-running monotonic counter at `APIC_TIMER_FREQ` ticks/second.
//! - The kernel runs with IF=0, so the timer interrupt cannot preempt the two
//!   reads inside `elapsed_ticks()`: `wraps` and `current` are always
//!   mutually consistent, and the counter is inherently monotonic.
//!
//! The APIC timer input frequency is exactly 1 GHz on QEMU (measured: a count
//! of 10,000,000 gives 100 Hz). On real hardware it is the bus/crystal clock
//! (SDM §13.5.4), so the hardcoded 1 GHz only holds for QEMU -- fine for this
//! tutorial. No RTC is involved anymore: the counter is guest-side and
//! inherently monotonic, so the previous RTC + minute-wrap bookkeeping is
//! gone.

use core::arch::asm;
use core::cmp::Ordering;
use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
use log::*;

use crate::sync::UPIntrFreeCell;
use crate::task::{TaskControlBlock, wakeup_task};
use alloc::collections::BinaryHeap;
use alloc::sync::Arc;
use lazy_static::*;

/// number of timer interrupts per second (the preemption rate)
const TICKS_PER_SEC: usize = 1000;
/// the APIC timer input frequency, measured on QEMU (1 GHz)
const APIC_TIMER_FREQ: usize = 1_000_000_000;
/// the count programmed into the APIC timer (period = INITIAL_COUNT ns)
const INITIAL_COUNT: usize = APIC_TIMER_FREQ / TICKS_PER_SEC;

/// the MSR holding the local APIC base/enable state
const APIC_BASE_MSR: u32 = 0x1B;
/// x2APIC enable bit in the APIC base MSR
const APIC_BASE_X2: u64 = 1 << 10;

/// LAPIC register offsets
const LAPIC_EOI: u32 = 0x0B0;
const LAPIC_LVT_TIMER: u32 = 0x320;
const LAPIC_TIMER_INIT_COUNT: u32 = 0x380;
#[allow(unused)]
const LAPIC_TIMER_CURRENT_COUNT: u32 = 0x390;
const LAPIC_TIMER_DIVIDE: u32 = 0x3E0;

/// number of completed timer periods (interrupts delivered) on the bootstrap
/// processor since boot. This is the global time base (1 tick = 1 ms at
/// `TICKS_PER_SEC` Hz). Only core 0's timer interrupt increments it, so it is
/// independent of the core on which `get_time_ms` happens to run.
static GLOBAL_TICKS: AtomicUsize = AtomicUsize::new(0);

unsafe fn rdmsr(msr: u32) -> u64 {
    let mut hi: u32;
    let mut lo: u32;
    unsafe {
        asm!(
            "rdmsr",
            in("ecx") msr,
            out("eax") lo,
            out("edx") hi,
            options(nostack)
        );
    }
    ((hi as u64) << 32) | lo as u64
}

unsafe fn wrmsr(msr: u32, value: u64) {
    unsafe {
        asm!(
            "wrmsr",
            in("ecx") msr,
            in("eax") value as u32,
            in("edx") (value >> 32) as u32,
            options(nostack)
        );
    }
}

/// Write a local APIC register. Works in both xAPIC (MMIO) and x2APIC (MSR)
/// modes.
fn lapic_write(offset: u32, value: u32) {
    unsafe {
        let apic_base = rdmsr(APIC_BASE_MSR);
        if apic_base & APIC_BASE_X2 != 0 {
            wrmsr(0x800 + offset, value as u64);
        } else {
            // xAPIC MMIO: reach the physical window through the physmap
            let base = crate::mm::phys_to_virt((apic_base & !0xFFF) as usize) as *mut u32;
            core::ptr::write_volatile(base.add((offset / 4) as usize), value);
        }
    }
}

/// Read a local APIC register. Works in both xAPIC (MMIO) and x2APIC (MSR)
/// modes.
#[allow(unused)]
fn lapic_read(offset: u32) -> u32 {
    unsafe {
        let apic_base = rdmsr(APIC_BASE_MSR);
        if apic_base & APIC_BASE_X2 != 0 {
            rdmsr(0x800 + offset) as u32
        } else {
            let base = crate::mm::phys_to_virt((apic_base & !0xFFF) as usize) as *const u32;
            core::ptr::read_volatile(base.add((offset / 4) as usize))
        }
    }
}

/// Program the current core's APIC timer in periodic mode to interrupt at
/// `TICKS_PER_SEC` Hz on vector 32. Called by every processor.
pub fn init() {
    // divide by 1
    lapic_write(LAPIC_TIMER_DIVIDE, 0x0B);
    // periodic mode, vector 32, unmasked
    lapic_write(LAPIC_LVT_TIMER, 0x20 | (1 << 17));
    // count for TICKS_PER_SEC Hz
    lapic_write(LAPIC_TIMER_INIT_COUNT, INITIAL_COUNT as u32);
    info!("timer: APIC timer interrupt at ~{} Hz", TICKS_PER_SEC);
}

/// Count one timer interrupt (one full period elapsed). Called from the timer
/// interrupt handler on the **bootstrap processor only**; this drives the
/// global time base.
pub fn tick() {
    GLOBAL_TICKS.fetch_add(1, AtomicOrdering::Relaxed);
}

/// Send the End-Of-Interrupt to the local APIC. Must be called at the start
/// of the timer handler, otherwise no further interrupt is delivered.
pub fn timer_eoi() {
    lapic_write(LAPIC_EOI, 0);
}

/// get current time in milliseconds
///
/// Derived from the number of periods completed by the bootstrap processor's
/// APIC timer, so the resolution is 1 ms (and monotonic by construction,
/// independent of which core calls it).
pub fn get_time_ms() -> usize {
    GLOBAL_TICKS.load(AtomicOrdering::Relaxed)
}

/// set the next timer interrupt
///
/// The APIC timer is programmed in periodic mode, so it reloads its count
/// automatically and there is nothing to reprogram per tick. Kept for
/// symmetry with the RISC-V port.
pub fn set_next_trigger() {}

/// A timer event: wake `task` once `expire_ms` is reached.
pub struct TimerCondVar {
    pub expire_ms: usize,
    pub task: Arc<TaskControlBlock>,
}

impl PartialEq for TimerCondVar {
    fn eq(&self, other: &Self) -> bool {
        self.expire_ms == other.expire_ms
    }
}
impl Eq for TimerCondVar {}
impl PartialOrd for TimerCondVar {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        let a = -(self.expire_ms as isize);
        let b = -(other.expire_ms as isize);
        Some(a.cmp(&b))
    }
}

impl Ord for TimerCondVar {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap()
    }
}

lazy_static! {
    /// a min-heap of pending timer events
    static ref TIMERS: UPIntrFreeCell<BinaryHeap<TimerCondVar>> =
        unsafe { UPIntrFreeCell::new(BinaryHeap::<TimerCondVar>::new()) };
}

/// Add a timer event that wakes `task` at `expire_ms`.
pub fn add_timer(expire_ms: usize, task: Arc<TaskControlBlock>) {
    let mut timers = TIMERS.exclusive_access();
    timers.push(TimerCondVar { expire_ms, task });
}

/// Remove all timer events of `task` (used when a process terminates).
pub fn remove_timer(task: Arc<TaskControlBlock>) {
    let mut timers = TIMERS.exclusive_access();
    let mut temp = BinaryHeap::<TimerCondVar>::new();
    for condvar in timers.drain() {
        if Arc::as_ptr(&task) != Arc::as_ptr(&condvar.task) {
            temp.push(condvar);
        }
    }
    timers.clear();
    timers.append(&mut temp);
}

/// Wake up every task whose timer has expired. Called from the timer
/// interrupt handler.
pub fn check_timer() {
    let current_ms = get_time_ms();
    let mut timers = TIMERS.exclusive_access();
    while let Some(timer) = timers.peek() {
        if timer.expire_ms <= current_ms {
            wakeup_task(Arc::clone(&timer.task));
            timers.pop();
        } else {
            break;
        }
    }
}
