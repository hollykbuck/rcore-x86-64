//! Implementation of [`PageTableEntry`] and [`PageTable`].
//!
//! x86-64 4-level paging: PML4 -> PDPT -> PD -> PT. The top-level page
//! table is pointed to by CR3, whose value is a physical address. Only 4KiB
//! pages are used in this chapter.

use super::{FrameTracker, PhysPageNum, VirtAddr, VirtPageNum, frame_alloc};
use alloc::vec;
use alloc::vec::Vec;
use bitflags::*;
use core::arch::asm;

bitflags! {
    /// page table entry flags, x86-64 encoding
    pub struct PTEFlags: u64 {
        const V = 1 << 0;   // present
        const W = 1 << 1;   // writable
        const U = 1 << 2;   // user accessible
        const PS = 1 << 7;  // page size (2MiB/1GiB)
        const NX = 1 << 63; // not executable (honored iff EFER.NXE is set)
    }
}

#[derive(Copy, Clone)]
#[repr(C)]
/// page table entry structure
pub struct PageTableEntry {
    pub bits: u64,
}

/// physical address bits of a page table entry
const PTE_ADDR_MASK: u64 = 0x000F_FFFF_FFFF_F000;

impl PageTableEntry {
    pub fn new(ppn: PhysPageNum, flags: PTEFlags) -> Self {
        PageTableEntry {
            bits: (ppn.0 as u64) << 12 | flags.bits,
        }
    }
    pub fn empty() -> Self {
        PageTableEntry { bits: 0 }
    }
    pub fn ppn(&self) -> PhysPageNum {
        PhysPageNum(((self.bits & PTE_ADDR_MASK) >> 12) as usize)
    }
    pub fn flags(&self) -> PTEFlags {
        // the address bits (12..52) are not part of the flag bitflags, so
        // they must be dropped before decoding (unlike SV39 where the flags
        // live in a byte of their own)
        PTEFlags::from_bits_truncate(self.bits)
    }
    pub fn is_valid(&self) -> bool {
        (self.flags() & PTEFlags::V) != PTEFlags::empty()
    }
    pub fn writable(&self) -> bool {
        (self.flags() & PTEFlags::W) != PTEFlags::empty()
    }
    pub fn executable(&self) -> bool {
        (self.flags() & PTEFlags::NX) == PTEFlags::empty()
    }
}

/// page table structure
pub struct PageTable {
    root_ppn: PhysPageNum,
    frames: Vec<FrameTracker>,
}

/// Assume that it won't oom when creating/mapping.
impl PageTable {
    pub fn new() -> Self {
        let frame = frame_alloc().unwrap();
        PageTable {
            root_ppn: frame.ppn,
            frames: vec![frame],
        }
    }
    /// The value to load into CR3: the physical address of the PML4.
    pub fn root_ppn(&self) -> PhysPageNum {
        self.root_ppn
    }
    fn find_pte_create(&mut self, vpn: VirtPageNum) -> Option<&mut PageTableEntry> {
        let idxs = vpn.indexes();
        let mut ppn = self.root_ppn;
        let mut result: Option<&mut PageTableEntry> = None;
        for (i, idx) in idxs.iter().enumerate() {
            let pte = &mut ppn.get_pte_array()[*idx];
            if i == 3 {
                result = Some(pte);
                break;
            }
            if !pte.is_valid() {
                let frame = frame_alloc().unwrap();
                // intermediate entries must allow user access and writes:
                // on x86 the W and U bits are checked at *every* level of the
                // walk (unlike SV39, which only checks the leaf), so V|W|U is
                // required for both kernel writes and ring-3 accesses
                *pte = PageTableEntry::new(frame.ppn, PTEFlags::V | PTEFlags::W | PTEFlags::U);
                self.frames.push(frame);
            }
            ppn = pte.ppn();
        }
        result
    }
    fn find_pte(&self, vpn: VirtPageNum) -> Option<&mut PageTableEntry> {
        let idxs = vpn.indexes();
        let mut ppn = self.root_ppn;
        let mut result: Option<&mut PageTableEntry> = None;
        for (i, idx) in idxs.iter().enumerate() {
            let pte = &mut ppn.get_pte_array()[*idx];
            if i == 3 {
                result = Some(pte);
                break;
            }
            if !pte.is_valid() {
                return None;
            }
            ppn = pte.ppn();
        }
        result
    }
    #[allow(unused)]
    pub fn map(&mut self, vpn: VirtPageNum, ppn: PhysPageNum, flags: PTEFlags) {
        let pte = self.find_pte_create(vpn).unwrap();
        assert!(!pte.is_valid(), "vpn {:?} is mapped before mapping", vpn);
        *pte = PageTableEntry::new(ppn, flags | PTEFlags::V);
    }
    #[allow(unused)]
    pub fn unmap(&mut self, vpn: VirtPageNum) {
        let pte = self.find_pte(vpn).unwrap();
        assert!(pte.is_valid(), "vpn {:?} is invalid before unmapping", vpn);
        *pte = PageTableEntry::empty();
        // The TLB may still cache the old mapping of this page in the current
        // address space; there is no `sfence.vma` on x86, so invalidate the
        // single page explicitly. (Without this, a just-freed sbrk page keeps
        // being accessible.)
        let va: VirtAddr = vpn.into();
        unsafe {
            asm!("invlpg [{0}]", in(reg) usize::from(va), options(nostack));
        }
    }
    pub fn translate(&self, vpn: VirtPageNum) -> Option<PageTableEntry> {
        self.find_pte(vpn).map(|pte| *pte)
    }
    /// The value to write into CR3: the **physical address** of the PML4
    /// (`PhysPageNum` is a frame index, CR3 wants an address).
    pub fn token(&self) -> usize {
        self.root_ppn.0 << 12
    }
}
