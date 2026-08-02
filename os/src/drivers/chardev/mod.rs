//! Character devices (ch9 `drivers/chardev`).
//!
//! The `CharDevice` trait abstracts a byte-oriented device. The only character
//! device on x86-64 QEMU is the 16550 UART at COM1 (I/O port `0x3F8`), which
//! is the console: its receiver is interrupt driven (see [`ns16550a`]).

mod ns16550a;

use alloc::sync::Arc;
use lazy_static::*;
pub use ns16550a::NS16550a;

/// A character device.
pub trait CharDevice: Send + Sync {
    /// Initialize the device (enable interrupts, configure the line).
    fn init(&self);
    /// Read one byte, blocking until a byte is available.
    fn read(&self) -> u8;
    /// Write one byte.
    fn write(&self, ch: u8);
    /// Handle an interrupt raised by the device (drain + wake waiters).
    fn handle_irq(&self);
}

lazy_static! {
    /// The console UART (COM1).
    pub static ref UART: Arc<dyn CharDevice> = Arc::new(NS16550a::new());
}
