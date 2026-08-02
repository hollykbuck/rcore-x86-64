//! QEMU platform helpers that do not belong to a driver.
//!
//! The console UART now lives in [`crate::drivers::chardev`]; this module only
//! keeps the QEMU `isa-debug-exit` shutdown helper.

use core::arch::asm;

/// shut down QEMU using the `isa-debug-exit` device.
///
/// Writing `0x10` to the I/O port `0x501` makes QEMU exit with code `0x21`
/// (success), while writing `0x11` makes it exit with code `0x23` (failure).
pub fn shutdown(failure: bool) -> ! {
    unsafe {
        if failure {
            asm!("out dx, ax", in("dx") 0x501u16, in("ax") 0x11u16);
        } else {
            asm!("out dx, ax", in("dx") 0x501u16, in("ax") 0x10u16);
        }
    }
    loop {
        unsafe { asm!("hlt") }
    }
}
