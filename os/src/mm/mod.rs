//! Memory management implementation
//!
//! x86-64 4-level paging, plus everything about memory management: frame
//! allocator, page table, map area and memory set.

mod address;
mod frame_allocator;
mod heap_allocator;
mod memory_set;
mod page_table;

pub use address::{PhysAddr, PhysPageNum, StepByOne, VirtAddr, VirtPageNum, phys_to_virt};
use address::VPNRange;
pub use frame_allocator::{FrameTracker, frame_alloc, frame_dealloc};
pub use memory_set::{KERNEL_SPACE, MapPermission, MemorySet, ioremap, kernel_token};
use page_table::PTEFlags;
#[allow(unused_imports)]
pub use page_table::{
    PageTable, PageTableEntry, UserBuffer, UserBufferIterator, translated_byte_buffer,
    translated_ref, translated_refmut, translated_str,
};

/// memory setup that must precede any heap allocation or `PerCpu` use: cache
/// the Limine HHDM offset and initialize the kernel heap.
///
/// Since ch9 the console UART is a heap-allocated `Arc<dyn CharDevice>`
/// (`drivers::UART`), so the heap must be ready before any `println!` (and
/// before `cpu::init_cpu`, whose `info!` goes through the console). It is
/// called once from `rust_main` before `init_cpu`; `init` must then not
/// re-initialize the heap (that would drop existing allocations).
pub fn init_early() {
    // cache Limine's HHDM offset before any physical access
    address::init_phys_virt_offset();
    heap_allocator::init_heap();
}

/// initiate heap allocator, frame allocator and kernel space
pub fn init() {
    // `init_early` was already called (rust_main runs it before `init_cpu`);
    // here we only finish the memory subsystem once GS (PerCpu) is up, which
    // the interrupt-masking locks below require.
    frame_allocator::init_frame_allocator();
    // NX bits are ignored until EFER.NXE is set
    crate::trap::msr::enable_nxe();
    KERNEL_SPACE.exclusive_access().activate();
}
