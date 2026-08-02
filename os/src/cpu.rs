//! Per-processor state (SMP, ch8 stage 3).
//!
//! Each processor gets a [`PerCpu`] slot in the static [`PER_CPU`] array. The
//! slot address is installed as this core's `GS.base` (MSR_GS_BASE) at
//! initialization, and `trap.S` uses `swapgs` on trap entry/exit so that the
//! kernel always runs with `GS` pointing at the current core's `PerCpu`. The
//! first field (`cpu_id`) is therefore readable with `gs:[0]`.

use crate::config::NCPU;
use crate::limine_reqs::mp_response;
use crate::task::Processor;
use crate::trap::{Gdt, TaskStateSegment};
use core::arch::asm;
use core::mem::MaybeUninit;
use log::*;
use core::sync::atomic::{AtomicUsize, Ordering};

/// Per-processor data.
#[repr(C)]
pub struct PerCpu {
    /// the id of this processor (gs:[0], used by `current_cpu_id`)
    pub cpu_id: usize,
    /// the kernel stack top of the current task, read by `syscall_entry`
    pub current_stack_top: usize,
    /// the user stack pointer saved by `syscall_entry` while it switches to
    /// the kernel stack (gs:[16]). Per-core (not a global!) because two cores
    /// doing a `syscall` at the same time would otherwise clobber each other's
    /// saved user RSP and resume with the wrong stack.
    pub user_rsp: usize,
    /// this core's TSS (its `rsp0` is used for user-mode exceptions)
    pub tss: TaskStateSegment,
    /// this core's GDT (with the descriptor of `tss`)
    pub gdt: Gdt,
    /// this core's scheduler (current thread + idle task context)
    pub processor: Processor,
    /// the id of the APIC used to program this core's timer
    pub apic_id: u32,
    /// the number of timer periods delivered on this core (informational)
    pub wraps: AtomicUsize,
}

/// The per-processor array. Each slot is initialized by its own processor.
static mut PER_CPU: [MaybeUninit<PerCpu>; NCPU] =
    [const { MaybeUninit::uninit() }; NCPU];

/// The id of the current processor, read from `gs:[0]` (valid in kernel mode,
/// where `GS` points at the current `PerCpu`).
pub fn current_cpu_id() -> usize {
    let id: usize;
    unsafe {
        asm!("mov {0}, qword ptr gs:[0]", out(reg) id, options(nostack, readonly));
    }
    id
}

/// Get a reference to the current processor's `PerCpu`.
pub fn current_per_cpu() -> &'static PerCpu {
    let id = current_cpu_id();
    unsafe { PER_CPU[id].assume_init_ref() }
}

/// Get a mutable reference to the current processor's `PerCpu`.
pub fn current_per_cpu_mut() -> &'static mut PerCpu {
    let id = current_cpu_id();
    unsafe { PER_CPU[id].assume_init_mut() }
}

/// Get the `PerCpu` of processor `id`.
#[allow(unused)]
pub fn per_cpu(id: usize) -> &'static PerCpu {
    unsafe { PER_CPU[id].assume_init_ref() }
}

/// Initialize every `PerCpu` slot (so `per_cpu(id)` is valid for all ids, not
/// just the cores that are actually booted). The real per-core MSRs/GDT/IDT
/// are installed later by `init_cpu`; the slots for cores that never boot only
/// serve as a valid home for shared structures such as the pending queues.
pub fn init_all_per_cpu_slots() {
    for id in 0..NCPU {
        let pc = unsafe { &mut *PER_CPU[id].as_mut_ptr() };
        pc.cpu_id = id;
        pc.current_stack_top = 0;
        pc.user_rsp = 0;
        pc.apic_id = 0;
        pc.wraps = AtomicUsize::new(0);
        pc.processor = Processor::new();
        pc.tss = TaskStateSegment::new();
        pc.gdt = Gdt::new(0);
    }
}

/// Initialize the current processor's `PerCpu` slot and the per-core MSRs.
///
/// `boot_stack_top` is the initial kernel stack (the BSP's static trap stack,
/// or the Limine-provided stack of an AP).
pub fn init_cpu(cpu_id: usize, apic_id: u32, boot_stack_top: usize) {
    let pc = unsafe { &mut *PER_CPU[cpu_id].as_mut_ptr() };
    pc.cpu_id = cpu_id;
    pc.current_stack_top = boot_stack_top;
    pc.user_rsp = 0;
    pc.apic_id = apic_id;
    pc.wraps = AtomicUsize::new(0);
    pc.processor = Processor::new();
    pc.tss = TaskStateSegment::new();
    pc.tss.set_rsp0(boot_stack_top as u64);
    pc.gdt = Gdt::new(core::ptr::addr_of!(pc.tss) as u64);
    // load this core's GDT/IDT and the syscall MSRs. NOTE: `Gdt::load` does
    // `mov gs, ax`, which zeroes GS.base, so it must run *before* we set
    // MSR_GS_BASE below.
    unsafe {
        (&*(core::ptr::addr_of!(pc.gdt) as *const Gdt)).load();
    }
    crate::trap::load_shared_idt();
    crate::trap::msr::syscall_init(crate::trap::syscall_entry_addr());
    // kernel mode runs with GS = PerCpu; user mode GS is 0. swapgs exchanges
    // GS.base with IA32_KERNEL_GS_BASE on trap entry/exit. Initialize the
    // protocol the Linux way: user GS base = 0, kernel GS base = PerCpu, then
    // swapgs once so the kernel (we are in kernel mode) runs with GS = PerCpu.
    let per_cpu_addr = unsafe { core::ptr::addr_of!(*PER_CPU[cpu_id].assume_init_ref()) } as u64;
    unsafe { wrmsr(0xC0000100, 0); } // MSR_GS_BASE (user)
    unsafe { wrmsr(0xC0000102, per_cpu_addr); } // IA32_KERNEL_GS_BASE
    unsafe {
        asm!("swapgs", options(nostack));
    }
    info!("cpu{}: per_cpu={:#x}", cpu_id, per_cpu_addr);
}

/// The entry point of the application processors, called by Limine when we
/// atomically store it into `goto_address`. Runs with the Limine-provided
/// stack, in long mode with Limine's page tables, `RDI` = `LimineMpInfo*`.
unsafe extern "C" fn ap_entry(info: *mut crate::limine_reqs::LimineMpInfo) -> ! {
    // the current stack pointer is the Limine-provided AP stack
    let boot_stack_top: usize;
    unsafe {
        asm!("mov {0}, rsp", out(reg) boot_stack_top, options(nostack));
    }
    let cpu_id = unsafe { (*info).extra_argument as usize };
    let lapic_id = unsafe { (*info).lapic_id };
    crate::cpu::init_cpu(cpu_id, lapic_id, boot_stack_top);
    // switch to the kernel page tables (built by the BSP): the Limine page
    // tables under which the AP started may not map the LAPIC/PCI MMIO
    crate::mm::KERNEL_SPACE.exclusive_access().activate();
    crate::timer::init();
    ap_ready();
    crate::task::run_tasks()
}

/// Release the application processors parked by Limine (see the Limine MP
/// protocol): atomically store `ap_entry` into each non-BSP `goto_address`.
/// `extra_argument` carries the kernel cpu id assigned to that processor.
pub fn smp_boot_aps() {
    let Some(resp) = mp_response() else {
        info!("mp: no Limine MP response, running single-core");
        return;
    };
    let bsp_id = resp.bsp_lapic_id;
    let count = resp.cpu_count.min(NCPU as u64) as usize;
    info!(
        "mp: {} processors (bsp lapic id {})",
        count, bsp_id
    );
    let mut next_cpu_id = 1usize;
    let mut ap_released = 0usize;
    for i in 0..count {
        let info = unsafe { &mut *resp.cpus.add(i).read() };
        if info.lapic_id == bsp_id {
            continue;
        }
        info.extra_argument = next_cpu_id as u64;
        // release the parked processor
        let addr = &mut info.goto_address as *mut u64;
        unsafe {
            core::ptr::write_volatile(addr, ap_entry as *const () as u64);
        }
        next_cpu_id += 1;
        ap_released += 1;
    }
    // the APs wait for every released AP to be ready before scheduling
    AP_TARGET.store(ap_released, Ordering::Relaxed);
}

/// Called by each AP right after its per-cpu init; blocks until the expected
/// number of processors have reached it (a simple "all APs ready" barrier).
fn ap_ready() {
    static READY: AtomicUsize = AtomicUsize::new(0);
    READY.fetch_add(1, Ordering::SeqCst);
    let expected = ap_target();
    while READY.load(Ordering::SeqCst) < expected {
        unsafe {
            asm!("sti", "hlt", "cli", options(nostack));
        }
    }
}

/// The number of processors that must reach `ap_ready` before any AP runs
/// tasks (cached from the Limine MP response).
static AP_TARGET: AtomicUsize = AtomicUsize::new(1);

fn ap_target() -> usize {
    AP_TARGET.load(Ordering::Relaxed)
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
