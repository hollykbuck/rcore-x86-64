//! VirtIO block device driver (VirtIO over PCI, `virtio-drivers` 0.2.0).
//!
//! The `Hal` implementation uses the kernel's physmap (`pa + hhdm_offset`)
//! for `phys_to_virt` and the kernel page table for DMA address translation.

use crate::mm::{
    FrameTracker, PhysAddr, PhysPageNum, StepByOne, VirtAddr, frame_alloc, frame_dealloc,
    kernel_token, phys_to_virt,
};
use crate::sync::UPSafeCell;
use alloc::vec::Vec;
use core::ptr::NonNull;
use easy_fs::BlockDevice;
use lazy_static::*;
use log::*;
use virtio_drivers::device::blk::VirtIOBlk;
use virtio_drivers::transport::pci::PciTransport;
use virtio_drivers::{BufferDirection, Hal};

pub struct VirtioHal;

lazy_static! {
    static ref QUEUE_FRAMES: UPSafeCell<Vec<FrameTracker>> =
        unsafe { UPSafeCell::new(Vec::new()) };
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

pub struct VirtIOBlock(UPSafeCell<VirtIOBlk<VirtioHal, PciTransport>>);

// All access to the device goes through the `UPSafeCell` in a single-threaded
// kernel, so we can safely claim Send/Sync even though the raw pointers inside
// `virtio-drivers`' PciTransport are not.
unsafe impl Send for VirtIOBlock {}
unsafe impl Sync for VirtIOBlock {}

impl VirtIOBlock {
    pub fn new() -> Self {
        let mut pci = crate::drivers::pci::Pci::new();
        let bdf = pci
            .find_block()
            .expect("no VirtIO block device found on the PCI bus");
        let transport =
            PciTransport::new::<VirtioHal>(pci.root(), bdf).expect("failed to init PCI transport");
        let blk = VirtIOBlk::new(transport).expect("failed to init VirtIO block device");
        info!("virtio-blk capacity: {} sectors", blk.capacity());
        Self(unsafe { UPSafeCell::new(blk) })
    }
}

impl BlockDevice for VirtIOBlock {
    fn read_block(&self, block_id: usize, buf: &mut [u8]) {
        self.0
            .exclusive_access()
            .read_block(block_id, buf)
            .expect("Error when reading VirtIOBlk");
    }
    fn write_block(&self, block_id: usize, buf: &[u8]) {
        self.0
            .exclusive_access()
            .write_block(block_id, buf)
            .expect("Error when writing VirtIOBlk");
    }
}
