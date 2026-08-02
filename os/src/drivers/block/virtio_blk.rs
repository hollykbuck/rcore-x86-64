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
use virtio_drivers::device::blk::{BlkReq, BlkResp, RespStatus, VirtIOBlk};
use virtio_drivers::transport::pci::PciTransport;
use virtio_drivers::{BufferDirection, Hal};

/// How often the block request poll yields to the host. QEMU's single-threaded
/// TCG only services the VirtIO device between translation blocks: a tight
/// `spin_loop()` poll that never leaves its block can starve the device on SMP
/// (the completion is never processed), so every so often we execute a
/// privileged instruction that TCG emulates through a helper call, which
/// forces the translator to exit the current block. On real hardware this is
/// a harmless no-op for the poll.
const POLL_YIELD_ITERS: u64 = 256;

/// Yield the current translation block so QEMU's TCG can service the VirtIO
/// device. `rdmsr` is always emulated by a helper call (it exits the block);
/// `pause` would not. Reading the benign MSR 0x1B (APIC base) has no side
/// effects.
#[inline(never)]
fn poll_yield() {
    unsafe {
        core::arch::asm!(
            "rdmsr",
            in("ecx") 0x1Bu32,
            out("eax") _,
            out("edx") _,
            options(nostack)
        );
    }
}

/// Submit a VirtIO block request (read or write) and wait for completion,
/// polling the used ring with an occasional `cpuid` yield (see `POLL_YIELD_ITERS`).
fn blk_request(
    blk: &mut VirtIOBlk<VirtioHal, PciTransport>,
    block_id: usize,
    is_write: bool,
    buf: &mut [u8],
) {
    let mut req = BlkReq::default();
    let mut resp = BlkResp::default();
    let token = if is_write {
        unsafe { blk.write_block_nb(block_id, &mut req, buf, &mut resp) }
    } else {
        unsafe { blk.read_block_nb(block_id, &mut req, buf, &mut resp) }
    }
    .expect("Error submitting VirtIOBlk request");
    let mut spins: u64 = 0;
    loop {
        if blk.peek_used().is_some() {
            let ret = if is_write {
                unsafe { blk.complete_write_block(token, &req, buf, &mut resp) }
            } else {
                unsafe { blk.complete_read_block(token, &req, buf, &mut resp) }
            };
            ret.expect("Error completing VirtIOBlk request");
            assert_eq!(resp.status(), RespStatus::OK, "VirtIOBlk request failed");
            break;
        }
        spins += 1;
        if spins % POLL_YIELD_ITERS == 0 {
            // exit the translation block so the device completion is serviced
            poll_yield();
        }
    }
}

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
        let mut blk = self.0.exclusive_access();
        blk_request(&mut blk, block_id, false, buf);
    }
    fn write_block(&self, block_id: usize, buf: &[u8]) {
        let mut blk = self.0.exclusive_access();
        let mut wbuf = [0u8; 512];
        wbuf.copy_from_slice(buf);
        blk_request(&mut blk, block_id, true, &mut wbuf);
    }
}
