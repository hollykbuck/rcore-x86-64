#![no_std]
#![no_main]

macro_rules! linker_symbol_addr {
    ($symbol:path) => {
        ($symbol as *const ()).addr()
    };
}

use log::*;

#[macro_use]
mod console;
mod lang_items;
mod limine_reqs;
mod logging;
mod uart;

pub fn clear_bss() {
    unsafe extern "C" {
        safe fn sbss();
        safe fn ebss();
    }
    (linker_symbol_addr!(sbss)..linker_symbol_addr!(ebss))
        .for_each(|a| unsafe { (a as *mut u8).write_volatile(0) });
}

#[unsafe(no_mangle)]
pub fn rust_main() -> ! {
    uart::init();
    clear_bss();
    logging::init();

    println!("[kernel] Hello, world!");

    unsafe extern "C" {
        safe fn stext();
        safe fn etext();
        safe fn srodata();
        safe fn erodata();
        safe fn sdata();
        safe fn edata();
        safe fn sbss();
        safe fn ebss();
        safe fn boot_stack_lower_bound();
        safe fn boot_stack_top();
    }
    trace!(
        "[kernel] .text [{:#x}, {:#x})",
        linker_symbol_addr!(stext),
        linker_symbol_addr!(etext)
    );
    debug!(
        "[kernel] .rodata [{:#x}, {:#x})",
        linker_symbol_addr!(srodata),
        linker_symbol_addr!(erodata)
    );
    info!(
        "[kernel] .data [{:#x}, {:#x})",
        linker_symbol_addr!(sdata),
        linker_symbol_addr!(edata)
    );
    warn!(
        "[kernel] boot_stack top={:#x}, bottom={:#x}",
        linker_symbol_addr!(boot_stack_top),
        linker_symbol_addr!(boot_stack_lower_bound)
    );
    error!(
        "[kernel] .bss [{:#x}, {:#x})",
        linker_symbol_addr!(sbss),
        linker_symbol_addr!(ebss)
    );

    uart::shutdown(false)
}
