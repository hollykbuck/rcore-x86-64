//! NS16550A UART driver (I/O port variant, ch9 `drivers/chardev/ns16550a.rs`).
//!
//! On x86-64 the 16550 lives at the standard I/O ports `0x3F8..0x3FF` (COM1)
//! instead of a memory-mapped window, so the register access is done through
//! `in`/`out` rather than the MMIO `volatile` structs of the RISC-V driver.
//! The logic otherwise mirrors ch9: the receiver is interrupt driven (IER bit
//! 0), the IRQ handler drains the receive buffer into a ring and wakes the
//! blocked reader through a `Condvar`, and a read blocks (parks on the condvar
//! + schedules to the idle loop) until a character arrives.
//!
//! References: ns16550a datasheet; Intel SDM (I/O port instructions).

use super::CharDevice;
use crate::sync::{Condvar, UPIntrFreeCell};
use crate::task::schedule;
use alloc::collections::VecDeque;
use core::arch::asm;

/// COM1 base I/O port.
const COM1: u16 = 0x3F8;

/// 16550 register offsets (relative to the base port).
const REG_RBR_THR: u16 = 0; // receiver buffer (read) / transmitter holding (write)
const REG_IER: u16 = 1; // interrupt enable
const REG_FCR: u16 = 2; // FIFO control
const REG_LCR: u16 = 3; // line control
const REG_MCR: u16 = 4; // modem control
const REG_LSR: u16 = 5; // line status

/// Line status register bits.
const LSR_DATA_READY: u8 = 0x01; // receiver buffer holds a byte
const LSR_THR_EMPTY: u8 = 0x20; // transmitter holding register empty

/// Interrupt enable register: receiver data available.
const IER_RX_AVAILABLE: u8 = 0x01;

/// Modem control register: OUT2 gates the IRQ line to the interrupt controller.
const MCR_OUT2: u8 = 0x08;

/// Read a byte from an I/O port.
fn inb(port: u16) -> u8 {
    let val: u8;
    unsafe {
        asm!("in al, dx", out("al") val, in("dx") port, options(nostack));
    }
    val
}

/// Write a byte to an I/O port.
fn outb(port: u16, val: u8) {
    unsafe {
        asm!("out dx, al", in("dx") port, in("al") val, options(nostack));
    }
}

/// The receive state of the UART.
struct NS16550aInner {
    read_buffer: VecDeque<u8>,
}

/// The UART device.
pub struct NS16550a {
    inner: UPIntrFreeCell<NS16550aInner>,
    condvar: Condvar,
}

impl NS16550a {
    /// Create the UART device.
    pub fn new() -> Self {
        Self {
            inner: unsafe {
                UPIntrFreeCell::new(NS16550aInner {
                    read_buffer: VecDeque::new(),
                })
            },
            condvar: Condvar::new(),
        }
    }
}

impl CharDevice for NS16550a {
    fn init(&self) {
        outb(COM1 + REG_IER, 0x00); // disable interrupts (for now)

        outb(COM1 + REG_LCR, 0x80); // enable DLAB

        outb(COM1 + 0, 0x01); // divisor low byte (115200 baud)
        outb(COM1 + REG_IER, 0x00); // divisor high byte

        outb(COM1 + REG_LCR, 0x03); // 8 bits, no parity, one stop bit

        outb(COM1 + REG_FCR, 0x01); // FIFO on, 1-byte trigger level (interrupt on every byte)

        outb(COM1 + REG_MCR, 0x03 | MCR_OUT2); // DTR/RTS/OUT2 set (OUT2 gates the IRQ line)

        outb(COM1 + REG_IER, IER_RX_AVAILABLE); // enable receiver-data-available interrupt
    }

    fn read(&self) -> u8 {
        loop {
            let mut inner = self.inner.exclusive_access();
            if let Some(ch) = inner.read_buffer.pop_front() {
                return ch;
            } else {
                // park on the condvar; schedule only after the lock is dropped
                let task_cx_ptr = self.condvar.wait_no_sched();
                drop(inner);
                schedule(task_cx_ptr);
            }
        }
    }

    fn write(&self, ch: u8) {
        // wait for the transmitter holding register to be empty
        loop {
            if inb(COM1 + REG_LSR) & LSR_THR_EMPTY != 0 {
                break;
            }
        }
        outb(COM1 + REG_RBR_THR, ch);
    }

    fn handle_irq(&self) {
        let mut count = 0;
        self.inner.exclusive_session(|inner| {
            while inb(COM1 + REG_LSR) & LSR_DATA_READY != 0 {
                inner.read_buffer.push_back(inb(COM1 + REG_RBR_THR));
                count += 1;
            }
        });
        if count > 0 {
            self.condvar.signal();
        }
    }
}
