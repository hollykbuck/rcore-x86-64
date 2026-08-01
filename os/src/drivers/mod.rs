//! PCI bus enumeration (QEMU `q35` ECAM) and VirtIO block device driver.
pub mod block;
pub mod pci;

pub use block::BLOCK_DEVICE;
