//! The main module and entrypoint
//!
//! Various facilities of the kernels are implemented as submodules. The most
//! important ones are:
//!
//! - [`trap`]: Handles all cases of switching from userspace to the kernel
//! - [`task`]: Task management
//! - [`syscall`]: System call handling and implementation
//!
//! The operating system also starts in this module. Kernel code starts
//! executing from `_start` (defined in `limine_reqs.rs`), after which
//! [`rust_main()`] is called to initialize various pieces of functionality.
//! (See its source code for details.)
//!
//! We then call [`task::add_initproc()`] and let the scheduler
//! ([`task::run_tasks()`]) bring up the init process and everything after it.

#![deny(missing_docs)]
#![deny(warnings)]
#![no_std]
#![no_main]

extern crate alloc;

#[macro_use]
extern crate bitflags;

macro_rules! linker_symbol_addr {
    ($symbol:path) => {
        ($symbol as *const ()).addr()
    };
}

use log::*;
#[macro_use]
mod console;
mod config;
mod cpu;
mod drivers;
pub mod fs;
mod lang_items;
mod limine_reqs;
mod logging;
mod mm;
mod sync;
pub mod syscall;
pub mod task;
mod timer;
pub mod trap;
mod uart;

/// clear BSS segment
fn clear_bss() {
    unsafe extern "C" {
        safe fn sbss();
        safe fn ebss();
    }
    unsafe {
        core::slice::from_raw_parts_mut(
            linker_symbol_addr!(sbss) as *mut u8,
            linker_symbol_addr!(ebss) - linker_symbol_addr!(sbss),
        )
        .fill(0);
    }
}

/// the rust entry-point of os
#[unsafe(no_mangle)]
pub fn rust_main() -> ! {
    uart::init();
    clear_bss();
    logging::init();

    println!("[kernel] Hello, world!");

    info!("[kernel] Initializing memory management ...");
    mm::init();
    // initialize the bootstrap processor's per-cpu state (GDT/TSS/IDT/MSRs).
    // `bsp_apic_id` is 0 when no Limine MP response is available (single CPU).
    let bsp_apic_id = crate::limine_reqs::bsp_lapic_id();
    cpu::init_all_per_cpu_slots();
    cpu::init_cpu(0, bsp_apic_id, trap::bsp_trap_stack_top() as usize);
    timer::init();
    trap::enable_timer_interrupt();
    fs::list_apps();
    task::add_initproc();
    // bring up the application processors (no-op until the Limine MP response
    // is wired up)
    cpu::smp_boot_aps();
    task::run_tasks()
}
