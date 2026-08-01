//! Memory management implementation
//!
//! x86-64 4-level paging, plus everything about memory management: frame
//! allocator, page table, map area and memory set.

mod address;
mod frame_allocator;
mod heap_allocator;
mod memory_set;
mod page_table;

pub use address::{PhysAddr, PhysPageNum, VirtAddr, VirtPageNum, phys_to_virt};
use address::{StepByOne, VPNRange};
pub use frame_allocator::{FrameTracker, frame_alloc};
pub use memory_set::{KERNEL_SPACE, MemorySet};
use page_table::{PTEFlags, PageTable};
pub use page_table::PageTableEntry;

/// initiate heap allocator, frame allocator and kernel space
pub fn init() {
    // cache Limine's HHDM offset before any physical access
    address::init_phys_virt_offset();
    heap_allocator::init_heap();
    frame_allocator::init_frame_allocator();
    // NX bits are ignored until EFER.NXE is set
    crate::trap::msr::enable_nxe();
    KERNEL_SPACE.exclusive_access().activate();
}
