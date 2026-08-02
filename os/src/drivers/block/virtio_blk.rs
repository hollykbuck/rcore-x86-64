//! VirtIO block device driver (VirtIO over PCI, `virtio-drivers` 0.2.0).
//!
//! The `Hal` implementation lives in [`crate::drivers::bus`].

use crate::drivers::VirtioHal;
use crate::sync::UPIntrFreeCell;
use easy_fs::BlockDevice;
use log::*;
use virtio_drivers::device::blk::{BlkReq, BlkResp, RespStatus, VirtIOBlk};
use virtio_drivers::transport::pci::PciTransport;

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

pub struct VirtIOBlock(UPIntrFreeCell<VirtIOBlk<VirtioHal, PciTransport>>);

// All access to the device goes through the `UPIntrFreeCell` in a single-threaded
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
        Self(unsafe { UPIntrFreeCell::new(blk) })
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
