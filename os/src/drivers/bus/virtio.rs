//! The `Hal` implementation used by every VirtIO device (ch9 `drivers/bus/virtio.rs`).
//!
//! On x86-64 the kernel maps all physical memory through a physmap at
//! `HHDM` (Limine), so `phys_to_virt` is the physmap offset, DMA allocation
//! uses the kernel frame allocator, and DMA addresses are the physical
//! addresses translated through the kernel page table (`share`). This differs
//! from the RISC-V ch9 `Hal`, which is an identity mapping and translates
//! through `virt_to_phys`; the `virtio-drivers` 0.2.0 trait shape
//! (`share`/`unshare`) is the one we keep.

use crate::mm::{
    FrameTracker, PhysAddr, PhysPageNum, StepByOne, VirtAddr, frame_alloc, frame_dealloc,
    kernel_token, phys_to_virt,
};
use crate::sync::UPIntrFreeCell;
use alloc::vec::Vec;
use core::ptr::NonNull;
use lazy_static::*;
use virtio_drivers::{BufferDirection, Hal};

/// The VirtIO HAL.
pub struct VirtioHal;

lazy_static! {
    /// Frames backing the virtio DMA regions (kept alive until dealloc).
    static ref QUEUE_FRAMES: UPIntrFreeCell<Vec<FrameTracker>> =
        unsafe { UPIntrFreeCell::new(Vec::new()) };
}

impl Hal for VirtioHal {
    fn dma_alloc(pages: usize) -> usize {
        let mut ppn_base = PhysPageNum(0);
        for i in 0..pages {
            let frame = frame_alloc().unwrap();
            if i == 0 {
                ppn_base = frame.ppn;
            }
            // DMA needs contiguous physical pages; the stack allocator hands
            // them out in order, so this holds in practice (same as the
            // RISC-V tutorial).
            assert_eq!(frame.ppn.0, ppn_base.0 + i);
            QUEUE_FRAMES.exclusive_access().push(frame);
        }
        let pa: PhysAddr = ppn_base.into();
        pa.0
    }
    fn dma_dealloc(pa: usize, pages: usize) -> i32 {
        let pa = PhysAddr::from(pa);
        let mut ppn_base: PhysPageNum = pa.into();
        for _ in 0..pages {
            frame_dealloc(ppn_base);
            ppn_base.step();
        }
        0
    }
    fn phys_to_virt(paddr: usize) -> usize {
        phys_to_virt(paddr)
    }
    fn share(buffer: NonNull<[u8]>, _direction: BufferDirection) -> usize {
        let vaddr = buffer.as_ptr() as *mut u8 as usize;
        crate::mm::PageTable::from_token(kernel_token())
            .translate_va(VirtAddr::from(vaddr))
            .unwrap()
            .0
    }
    fn unshare(_paddr: usize, _buffer: NonNull<[u8]>, _direction: BufferDirection) {}
}
