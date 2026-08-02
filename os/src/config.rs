//! Constants used in rCore

pub const USER_STACK_SIZE: usize = 4096 * 2;
pub const KERNEL_STACK_SIZE: usize = 4096 * 2;
pub const KERNEL_HEAP_SIZE: usize = 0x30_0000;
pub const PAGE_SIZE: usize = 0x1000;
pub const PAGE_SIZE_BITS: usize = 0xc;

/// The virtual address where the kernel image is linked
pub const KERNEL_BASE: usize = 0xffffffff80000000;
/// Fallback physical memory size (QEMU `-m 256M`), used if the Limine memory
/// map reports no usable region.
pub const MEMORY_END: usize = 0x1000_0000;
/// The physical base of the local APIC MMIO window
pub const LAPIC_BASE: usize = 0xFEE0_0000;

/// Start of the per-process kernel stack region. This is the *start* of
/// `PML4[511]`/`PDPT[511]`, one PDPT entry above the kernel image (`PDPT[510]`).
pub const KERNEL_STACK_BASE: usize = 0xffffffffc0000000;

/// Maximum number of processors (QEMU `-smp N`, N <= this).
pub const NCPU: usize = 8;

/// Return (bottom, top) of the kernel stack of the process/thread with `id`.
///
/// The layout **ascends** from `KERNEL_STACK_BASE` (one guard page between
/// stacks) so it stays entirely within `PDPT[511]`. Descending from the base
/// would fall into `PDPT[510]` (where the kernel image lives) -- verified with
/// `id = 1` landing at `0xffffffffbfffd000`. `id` is the kernel-stack id
/// allocated by `KSTACK_ALLOCATOR` (one stack per thread since ch8).
pub fn kernel_stack_position(id: usize) -> (usize, usize) {
    let bottom = KERNEL_STACK_BASE + id * (KERNEL_STACK_SIZE + PAGE_SIZE);
    (bottom, bottom + KERNEL_STACK_SIZE)
}
