//! Device drivers (ch9 layout).
//!
//! - [`bus`]: the VirtIO `Hal` shared by all VirtIO devices.
//! - [`block`]: the VirtIO block device.
//! - [`chardev`]: the 16550 UART (console).
//! - [`pci`]: PCI/ECAM enumeration (x86-64 only).

pub mod block;
pub mod bus;
pub mod chardev;
pub mod pci;

pub use block::BLOCK_DEVICE;
pub use bus::*;
pub use chardev::UART;
