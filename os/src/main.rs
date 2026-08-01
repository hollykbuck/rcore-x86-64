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

use core::arch::global_asm;

use log::*;
#[macro_use]
mod console;
mod config;
mod lang_items;
mod limine_reqs;
mod loader;
mod logging;
mod mm;
mod sync;
pub mod syscall;
pub mod task;
mod timer;
pub mod trap;
mod uart;

global_asm!(include_str!("link_app.S"));

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
    trap::init();
    timer::init();
    trap::enable_timer_interrupt();
    task::add_initproc();
    task::run_tasks()
}
