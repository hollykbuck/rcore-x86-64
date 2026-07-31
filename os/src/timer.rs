//! Timer setup (local APIC timer for preemption + CMOS RTC for timekeeping)
//!
//! Limine masks both the 8259 PICs and the IO-APIC before handing over, and
//! the legacy PIT path is unreliable under QEMU's TCG emulation (interrupts
//! are easily lost when the handler is slow). The local APIC timer, on the
//! other hand, fires as reliably as the RISC-V CLINT timer, so it is used for
//! the time-slice rotation (round-robin preemption).
//!
//! The preemption rate is set to 1000 Hz: QEMU's TCG emulation only advances
//! its virtual clock (and thus fires timers) when the guest does I/O or runs
//! under `-icount`, so a high nominal rate is needed for the round-robin to
//! visibly preempt the compute-bound power apps.
//!
//! Wall-clock time, however, must not depend on how often the slow handler
//! happens to run nor on how fast the emulator's virtual clock advances, so
//! `get_time_ms` is derived from the CMOS RTC, which is wall-clock accurate
//! (1 s resolution, made monotonic across minute wraps by the timer handler).
//! The 03sleep test only needs ~3 s precision.
//!
//! The APIC timer input frequency is exactly 1 GHz on QEMU (measured: a count
//! of 10,000,000 gives 100 Hz), so the count for `TICKS_PER_SEC` Hz is
//! `APIC_TIMER_FREQ / TICKS_PER_SEC`.

use core::arch::asm;
use core::sync::atomic::{AtomicI8, AtomicUsize, Ordering};
use log::*;

/// number of timer interrupts per second (the preemption rate)
const TICKS_PER_SEC: usize = 1000;
/// the APIC timer input frequency, measured on QEMU (1 GHz)
const APIC_TIMER_FREQ: u64 = 1_000_000_000;

/// the MSR holding the local APIC base/enable state
const APIC_BASE_MSR: u32 = 0x1B;
/// x2APIC enable bit in the APIC base MSR
const APIC_BASE_X2: u64 = 1 << 10;

/// LAPIC register offsets
const LAPIC_EOI: u32 = 0x0B0;
const LAPIC_LVT_TIMER: u32 = 0x320;
const LAPIC_TIMER_INIT_COUNT: u32 = 0x380;
const LAPIC_TIMER_DIVIDE: u32 = 0x3E0;

/// the CMOS RTC ports and registers
const RTC_PORT: u16 = 0x70;
const RTC_DATA: u16 = 0x71;
/// RTC register A, bit 7 = update in progress
const RTC_REG_A: u8 = 0x0A;
/// RTC seconds register
const RTC_REG_SECONDS: u8 = 0x00;

/// number of timer ticks since boot (informational only)
static TICKS: AtomicUsize = AtomicUsize::new(0);
/// the last RTC seconds value seen, `-1` before the first read
static LAST_RTC_SEC: AtomicI8 = AtomicI8::new(-1);
/// seconds accumulated across RTC minute wraps, to keep time monotonic
static BASE_SECONDS: AtomicUsize = AtomicUsize::new(0);

unsafe fn outb(port: u16, value: u8) {
    unsafe {
        asm!("out dx, al", in("dx") port, in("al") value, options(nostack));
    }
}

unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    unsafe {
        asm!("in al, dx", out("al") value, in("dx") port, options(nostack));
    }
    value
}

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
            let base = (apic_base & !0xFFF) as *mut u32;
            core::ptr::write_volatile(base.add((offset / 4) as usize), value);
        }
    }
}

/// Wait until the RTC is not updating (register A bit 7, UIP, is clear).
fn rtc_wait_update_done() {
    unsafe {
        outb(RTC_PORT, RTC_REG_A); // select register A
        while inb(RTC_DATA) & 0x80 != 0 {}
    }
}

/// Read a CMOS RTC register, waiting for any update in progress to finish.
fn rtc_read(reg: u8) -> u8 {
    rtc_wait_update_done();
    unsafe {
        outb(RTC_PORT, reg);
        inb(RTC_DATA)
    }
}

/// Decode a BCD value (0x00-0x59) to binary.
fn bcd_to_bin(bcd: u8) -> u8 {
    (bcd & 0x0F) + ((bcd >> 4) & 0x0F) * 10
}

/// The current RTC seconds value (0..59).
fn rtc_seconds() -> u8 {
    bcd_to_bin(rtc_read(RTC_REG_SECONDS))
}

/// A monotonic seconds value based on the RTC, accounting for minute wraps.
fn rtc_monotonic_seconds() -> usize {
    let sec = rtc_seconds() as i8;
    let last = LAST_RTC_SEC.swap(sec, Ordering::Relaxed);
    if last >= 0 && sec < last {
        // the RTC seconds wrapped from 59 back to 0
        BASE_SECONDS.fetch_add(60, Ordering::Relaxed);
    }
    BASE_SECONDS.load(Ordering::Relaxed) + sec as usize
}

/// Initialize the timer: program the APIC timer in periodic mode to interrupt
/// at `TICKS_PER_SEC` Hz on vector 32.
pub fn init() {
    // divide by 1
    lapic_write(LAPIC_TIMER_DIVIDE, 0x0B);
    // periodic mode, vector 32, unmasked
    lapic_write(LAPIC_LVT_TIMER, 0x20 | (1 << 17));
    // count for TICKS_PER_SEC Hz
    lapic_write(
        LAPIC_TIMER_INIT_COUNT,
        (APIC_TIMER_FREQ / TICKS_PER_SEC as u64) as u32,
    );
    info!("timer: APIC timer interrupt at ~{} Hz", TICKS_PER_SEC);
}

/// Count one timer tick. Called from the timer interrupt handler.
///
/// Also keeps the RTC minute-wrap bookkeeping up to date.
pub fn tick() {
    TICKS.fetch_add(1, Ordering::Relaxed);
    let _ = rtc_monotonic_seconds();
}

/// Send the End-Of-Interrupt to the local APIC. Must be called at the start
/// of the timer handler, otherwise no further interrupt is delivered.
pub fn timer_eoi() {
    lapic_write(LAPIC_EOI, 0);
}

/// get current time in milliseconds
///
/// Derived from the CMOS RTC, which is wall-clock accurate regardless of how
/// many timer interrupts the emulator actually delivers. Resolution is 1 s,
/// which is enough for the `03sleep` test (waits ~3000 ms).
pub fn get_time_ms() -> usize {
    rtc_monotonic_seconds() * 1000
}

/// set the next timer interrupt
///
/// The APIC timer is programmed in periodic mode, so it reloads its count
/// automatically and there is nothing to reprogram per tick. Kept for
/// symmetry with the RISC-V port.
pub fn set_next_trigger() {}
