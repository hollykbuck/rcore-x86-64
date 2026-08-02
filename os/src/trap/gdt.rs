//! The Global Descriptor Table
//!
//! Long mode ignores segment limits/base addresses, but the descriptors are
//! still used to hold the privilege level (DPL) and the code/data flags, and
//! the TSS is described here too.

use core::arch::asm;

/// kernel code segment selector (index 1, DPL 0)
pub const KERNEL_CS: u16 = 0x08;
/// kernel data segment selector (index 2, DPL 0)
pub const KERNEL_DS: u16 = 0x10;
/// TSS segment selector (index 5, DPL 0)
pub const TSS_SELECTOR: u16 = 0x28;

/// DPL of the user mode segments
const DPL_USER: u64 = 3;
const LIMIT_HIGH: u64 = 0xF << 48;
const GRANULARITY_4K: u64 = 1 << 55;
const LONG_MODE: u64 = 1 << 53;
const DEFAULT_OP_SIZE: u64 = 1 << 54;

/// Build a 64-bit code/data segment descriptor.
///
/// `access`: `0x9A` for a code segment, `0x92` for a data segment.
const fn segment(access: u64, dpl: u64, long: bool, db: bool) -> u64 {
    let mut upper = LIMIT_HIGH | GRANULARITY_4K;
    if long {
        upper |= LONG_MODE;
    }
    if db {
        upper |= DEFAULT_OP_SIZE;
    }
    0xFFFF // limit low
        | (access << 40)
        | (dpl << 45)
        | upper
}

/// Build the 64-bit TSS descriptor (two 8-byte entries).
fn tss_segment(base: u64) -> (u64, u64) {
    let limit = (core::mem::size_of::<super::tss::TaskStateSegment>() - 1) as u64;
    let low = limit // limit low
        | ((base & 0xFFFF) << 16) // base[15:0]
        | (((base >> 16) & 0xFF) << 32) // base[23:16]
        | (0x89 << 40) // present, available 64-bit TSS
        | (((base >> 24) & 0xFF) << 56); // base[31:24]
    let high = (base >> 32) & 0xFFFFFFFF; // base[63:32]
    (low, high)
}

/// The GDT, with space for the TSS descriptor
#[repr(C, align(8))]
pub struct Gdt {
    table: [u64; 8],
}

impl Gdt {
    pub fn new(tss_base: u64) -> Self {
        let (tss_low, tss_high) = tss_segment(tss_base);
        Self {
            table: [
                0,                              // 0: null
                segment(0x9A, 0, true, false),  // 1: kernel code
                segment(0x92, 0, false, true),  // 2: kernel data
                segment(0x9A, DPL_USER, true, false), // 3: user code
                segment(0x92, DPL_USER, false, true), // 4: user data
                tss_low,                        // 5: TSS (low)
                tss_high,                       // 6: TSS (high)
                0,
            ],
        }
    }

    /// Load the GDT and reload the segment registers, including `ltr`.
    pub fn load(&'static self) {
        #[repr(C, packed)]
        struct GdtPointer {
            limit: u16,
            base: u64,
        }
        let ptr = GdtPointer {
            limit: (self.table.len() * 8 - 1) as u16,
            base: self.table.as_ptr() as u64,
        };
        unsafe {
            // load the new GDT
            asm!("lgdt [{ptr}]", ptr = in(reg) &ptr, options(readonly, nostack));
            // reload CS via a far return
            asm!(
                "push {cs}",
                "lea rax, [rip + 2f]",
                "push rax",
                "retfq",
                "2:",
                cs = const KERNEL_CS,
                out("rax") _,
                options(nostack)
            );
            // reload the data segments
            asm!(
                "mov ax, {ds}",
                "mov ds, ax",
                "mov es, ax",
                "mov fs, ax",
                "mov gs, ax",
                "mov ss, ax",
                ds = const KERNEL_DS,
                out("ax") _,
                options(nostack)
            );
            // load the TSS
            asm!(
                "mov ax, {sel}",
                "ltr ax",
                sel = const TSS_SELECTOR,
                out("ax") _,
                options(nostack)
            );
        }
    }
}
