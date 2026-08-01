//! UART (COM1) driver for the console
//!
//! `qemu-system-x86_64` redirects the first serial port to the stdio, so we
//! simply talk to the 16550 UART at the well-known I/O ports `0x3F8..0x3FF`.

use core::arch::asm;

/// initialize the 16550 UART
pub fn init() {
    unsafe {
        asm!("out dx, al", in("dx") 0x3F8 + 1, in("al") 0x00u8); // disable interrupts

        asm!("out dx, al", in("dx") 0x3F8 + 3, in("al") 0x80u8); // enable DLAB

        asm!("out dx, al", in("dx") 0x3F8 + 0, in("al") 0x01u8); // divisor low byte
        asm!("out dx, al", in("dx") 0x3F8 + 1, in("al") 0x00u8); // divisor high byte

        asm!("out dx, al", in("dx") 0x3F8 + 3, in("al") 0x03u8); // 8 bits, no parity, one stop bit

        asm!("out dx, al", in("dx") 0x3F8 + 2, in("al") 0xC7u8); // enable FIFO, clear them, with 14-byte threshold

        asm!("out dx, al", in("dx") 0x3F8 + 4, in("al") 0x0Bu8); // IRQs enabled, RTS/DSR set
    }
}

/// output a byte on the serial console
pub fn console_putchar(c: usize) {
    unsafe {
        loop {
            let status: u8;
            asm!("in al, dx", out("al") status, in("dx") 0x3F8 + 5);
            if status & 0x20 != 0 {
                break;
            }
        }
        asm!("out dx, al", in("dx") 0x3F8 + 0, in("al") c as u8);
    }
}

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
