//! Implementation of [`PageTableEntry`] and [`PageTable`].
//!
//! x86-64 4-level paging: PML4 -> PDPT -> PD -> PT. The top-level page
//! table is pointed to by CR3, whose value is a physical address. Only 4KiB
//! pages are used in this chapter.

use super::{FrameTracker, PhysAddr, PhysPageNum, StepByOne, VirtAddr, VirtPageNum, frame_alloc};
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
    /// A read-only `PageTable` rooted at the given root page table address,
    /// used to walk a user address space for argument translation.
    pub fn from_token(token: usize) -> Self {
        Self {
            root_ppn: PhysPageNum::from(token >> 12),
            frames: Vec::new(),
        }
    }
    /// Translate `VirtAddr` to `PhysAddr`.
    pub fn translate_va(&self, va: VirtAddr) -> Option<PhysAddr> {
        self.find_pte(va.floor()).map(|pte| {
            let aligned_pa: PhysAddr = pte.ppn().into();
            (aligned_pa.0 + va.page_offset()).into()
        })
    }
    /// The value to write into CR3: the **physical address** of the PML4
    /// (`PhysPageNum` is a frame index, CR3 wants an address).
    pub fn token(&self) -> usize {
        self.root_ppn.0 << 12
    }
}

/// Translate a pointer to a mutable u8 Vec through page table
#[allow(dead_code)]
pub fn translated_byte_buffer(token: usize, ptr: *const u8, len: usize) -> Vec<&'static mut [u8]> {
    let page_table = PageTable::from_token(token);
    let mut start = ptr as usize;
    let end = start + len;
    let mut v = Vec::new();
    while start < end {
        let start_va = VirtAddr::from(start);
        let mut vpn = start_va.floor();
        let ppn = page_table.translate(vpn).unwrap().ppn();
        vpn.step();
        let mut end_va: VirtAddr = vpn.into();
        end_va = end_va.min(VirtAddr::from(end));
        if end_va.page_offset() == 0 {
            v.push(&mut ppn.get_bytes_array()[start_va.page_offset()..]);
        } else {
            v.push(&mut ppn.get_bytes_array()[start_va.page_offset()..end_va.page_offset()]);
        }
        start = end_va.into();
    }
    v
}

/// Translate a pointer to a mutable u8 Vec end with `\0` through page table to a `String`
#[allow(dead_code)]
pub fn translated_str(token: usize, ptr: *const u8) -> alloc::string::String {
    let page_table = PageTable::from_token(token);
    let mut string = alloc::string::String::new();
    let mut va = ptr as usize;
    loop {
        let ch: u8 = *(page_table
            .translate_va(VirtAddr::from(va))
            .unwrap()
            .get_mut());
        if ch == 0 {
            break;
        }
        string.push(ch as char);
        va += 1;
    }
    string
}

#[allow(unused)]
/// Translate a generic through page table and return a reference
pub fn translated_ref<T>(token: usize, ptr: *const T) -> &'static T {
    let page_table = PageTable::from_token(token);
    page_table
        .translate_va(VirtAddr::from(ptr as usize))
        .unwrap()
        .get_ref()
}
/// Translate a generic through page table and return a mutable reference
#[allow(dead_code)]
pub fn translated_refmut<T>(token: usize, ptr: *mut T) -> &'static mut T {
    let page_table = PageTable::from_token(token);
    let va = ptr as usize;
    page_table
        .translate_va(VirtAddr::from(va))
        .unwrap()
        .get_mut()
}
/// Array of u8 slice that user communicate with os
pub struct UserBuffer {
    /// U8 vec
    pub buffers: Vec<&'static mut [u8]>,
}

impl UserBuffer {
    /// Create a `UserBuffer` by parameter
    pub fn new(buffers: Vec<&'static mut [u8]>) -> Self {
        Self { buffers }
    }
    /// Length of `UserBuffer`
    pub fn len(&self) -> usize {
        let mut total: usize = 0;
        for b in self.buffers.iter() {
            total += b.len();
        }
        total
    }
}

impl IntoIterator for UserBuffer {
    type Item = *mut u8;
    type IntoIter = UserBufferIterator;
    fn into_iter(self) -> Self::IntoIter {
        UserBufferIterator {
            buffers: self.buffers,
            current_buffer: 0,
            current_idx: 0,
        }
    }
}
/// Iterator of `UserBuffer`
pub struct UserBufferIterator {
    buffers: Vec<&'static mut [u8]>,
    current_buffer: usize,
    current_idx: usize,
}

impl Iterator for UserBufferIterator {
    type Item = *mut u8;
    fn next(&mut self) -> Option<Self::Item> {
        if self.current_buffer >= self.buffers.len() {
            None
        } else {
            let r = &mut self.buffers[self.current_buffer][self.current_idx] as *mut _;
            if self.current_idx + 1 == self.buffers[self.current_buffer].len() {
                self.current_idx = 0;
                self.current_buffer += 1;
            } else {
                self.current_idx += 1;
            }
            Some(r)
        }
    }
}
