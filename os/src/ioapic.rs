//! IO-APIC programming (Intel SDM Vol 3A §10.4, "I/O APIC").
//!
//! Limine masks every IO-APIC redirection-table entry before handing over to
//! the kernel, so device interrupts (the 16550 UART at ISA IRQ4, the virtio
//! devices' INTx pins, ...) never reach the local APIC. To deliver a device
//! interrupt we program the corresponding redirection-table entry (RTE): the
//! RTE's vector is delivered as an IDT interrupt on the LAPIC whose APIC ID
//! matches the destination field.
//!
//! The q35 machine exposes the IO-APIC as MMIO at `0xFEC0_0000`; registers
//! are accessed through an indirect pair: write the register index to
//! `IOREGSEL` (offset 0x00), then read/write `IOWIN` (offset 0x10). RTE `i`
//! lives at index `0x10 + 2*i` (low dword) and `0x11 + 2*i` (high dword).

use crate::mm::phys_to_virt;
use log::*;

/// The IO-APIC MMIO base (Intel-standard, q35).
const IOAPIC_BASE: usize = 0xFEC0_0000;
/// indirect register select
const IOREGSEL: u32 = 0x00;
/// indirect data window
const IOWIN: u32 = 0x10;

/// The IO-APIC.
pub struct IoApic {
    base: *mut u32,
}

// The raw pointer is only used to access the (mapped) MMIO window.
unsafe impl Send for IoApic {}
unsafe impl Sync for IoApic {}

impl IoApic {
    /// Map the IO-APIC window and open it.
    pub fn new() -> Self {
        crate::mm::ioremap(IOAPIC_BASE, 0x1000);
        let base = phys_to_virt(IOAPIC_BASE) as *mut u32;
        Self { base }
    }

    /// Read an IO-APIC register (index into the indirect space).
    fn read(&self, reg: u32) -> u32 {
        unsafe {
            core::ptr::write_volatile(self.base.add((IOREGSEL / 4) as usize), reg);
            core::ptr::read_volatile(self.base.add((IOWIN / 4) as usize))
        }
    }

    /// Write an IO-APIC register (index into the indirect space).
    fn write(&self, reg: u32, val: u32) {
        unsafe {
            core::ptr::write_volatile(self.base.add((IOREGSEL / 4) as usize), reg);
            core::ptr::write_volatile(self.base.add((IOWIN / 4) as usize), val);
        }
    }

    /// Program RTE `pin` to deliver `vector` to the LAPIC `apic_id`.
    ///
    /// Delivery mode = Fixed (000b), trigger = edge, unmasked; destination
    /// mode = physical (matches the 8-bit APIC ID).
    pub fn route(&self, pin: u32, vector: u32, apic_id: u32) {
        assert!(pin < 24, "IO-APIC pin {} out of range", pin);
        let low = vector & 0xFF; // Fixed delivery (bits 8:10 = 000), edge (bit 15 = 0), unmasked (bit 16 = 0)
        let high = (apic_id & 0xFF) << 24; // destination APIC ID (bits 63:56), physical mode (bit 11 = 0)
        info!(
            "ioapic: route pin {} -> vector {:#x}, dest apic {}",
            pin, vector, apic_id
        );
        self.write(0x10 + 2 * pin, low);
        self.write(0x10 + 2 * pin + 1, high);
        // read back for verification
        let rlow = self.read(0x10 + 2 * pin);
        let rhigh = self.read(0x10 + 2 * pin + 1);
        info!(
            "ioapic: RTE{} = {:#010x} {:#010x}",
            pin, rhigh, rlow
        );
    }
}
