//! The Interrupt Descriptor Table
//!
//! Every one of the 256 vectors points to the corresponding stub in `trap.S`.
//! Only the vectors that the kernel uses in practice are expected; any other
//! exception is reported by [`crate::trap::trap_handler`] as unsupported.

use super::gdt::KERNEL_CS;
use core::arch::asm;

const INTERRUPT_GATE: u8 = 0x8E; // present, DPL 0, interrupt gate

/// A single IDT entry. An x86-64 IDT entry is exactly 16 bytes.
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct IdtEntry {
    offset_low: u16,
    selector: u16,
    ist: u8,
    type_attr: u8,
    offset_mid: u16,
    offset_high: u32,
    reserved: u32,
}

impl IdtEntry {
    const fn new(handler: u64, selector: u16, type_attr: u8) -> Self {
        Self {
            offset_low: handler as u16,
            selector,
            ist: 0,
            type_attr,
            offset_mid: (handler >> 16) as u16,
            offset_high: (handler >> 32) as u32,
            reserved: 0,
        }
    }
}

const fn empty() -> IdtEntry {
    IdtEntry::new(0, 0, 0)
}

/// The IDT, 256 entries of 16 bytes
#[repr(C, align(16))]
pub struct InterruptDescriptorTable {
    entries: [IdtEntry; 256],
}

impl InterruptDescriptorTable {
    /// Create an IDT with all entries empty (interrupt gates, present, DPL 0).
    pub fn new() -> Self {
        Self {
            entries: [empty(); 256],
        }
    }

    /// Set the handler address of the `vector`-th entry.
    pub fn set_handler(&mut self, vector: usize, handler: u64) {
        self.entries[vector] = IdtEntry::new(handler, KERNEL_CS, INTERRUPT_GATE);
    }

    /// Load the IDT with `lidt`.
    pub fn load(&'static self) {
        #[repr(C, packed)]
        struct IdtPointer {
            limit: u16,
            base: u64,
        }
        let ptr = IdtPointer {
            limit: (core::mem::size_of::<Self>() - 1) as u16,
            base: self as *const _ as u64,
        };
        unsafe {
            asm!("lidt [{ptr}]", ptr = in(reg) &ptr, options(readonly, nostack));
        }
    }
}
