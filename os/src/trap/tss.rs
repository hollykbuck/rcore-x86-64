//! The Task State Segment
//!
//! We only use the `rsp0` field: when an interrupt/exception happens in user
//! mode (ring 3), the CPU automatically switches to this kernel stack.
//!
//! Note that `rsp0` lives at offset 4 of the TSS, so it is stored as two 32-bit
//! halves to keep the `#[repr(C)]` layout from inserting padding.

/// The x86-64 TSS layout
#[repr(C)]
#[derive(Clone, Copy)]
pub struct TaskStateSegment {
    reserved1: u32,
    /// kernel stack pointer for ring 0 (low half)
    rsp0_low: u32,
    /// kernel stack pointer for ring 0 (high half)
    rsp0_high: u32,
    rsp1_low: u32,
    rsp1_high: u32,
    rsp2_low: u32,
    rsp2_high: u32,
    reserved2: u64,
    ist: [u64; 7],
    reserved3: u64,
    reserved4: u16,
    /// points past the end of the structure so that the (empty) I/O bitmap
    /// forbids all port accesses from user mode
    iomap_base: u16,
}

impl TaskStateSegment {
    pub const fn new() -> Self {
        Self {
            reserved1: 0,
            rsp0_low: 0,
            rsp0_high: 0,
            rsp1_low: 0,
            rsp1_high: 0,
            rsp2_low: 0,
            rsp2_high: 0,
            reserved2: 0,
            ist: [0; 7],
            reserved3: 0,
            reserved4: 0,
            iomap_base: size_of::<TaskStateSegment>() as u16,
        }
    }

    pub const fn with_rsp0(rsp0: u64) -> Self {
        let mut tss = Self::new();
        tss.rsp0_low = rsp0 as u32;
        tss.rsp0_high = (rsp0 >> 32) as u32;
        tss
    }
}
