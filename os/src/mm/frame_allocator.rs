//! Implementation of [`FrameAllocator`] which
//! controls all the frames in the operating system.
//!
//! The allocatable frames are gathered from Limine's memory map
//! ([`crate::limine_reqs::memmap_entries`]): usable, bootloader-reclaimable
//! and kernel-and-modules regions, minus the kernel image itself. Frames are
//! handed out as a set of contiguous ranges, recycled on deallocation.

use super::{PhysAddr, PhysPageNum};
use crate::config::MEMORY_END;
use crate::sync::UPSafeCell;
use alloc::vec::Vec;
use core::fmt::{self, Debug, Formatter};
use lazy_static::*;
use log::*;

/// manage a frame which has the same lifecycle as the tracker
pub struct FrameTracker {
    pub ppn: PhysPageNum,
}

impl FrameTracker {
    pub fn new(ppn: PhysPageNum) -> Self {
        // page cleaning
        let bytes_array = ppn.get_bytes_array();
        for i in bytes_array {
            *i = 0;
        }
        Self { ppn }
    }
}

impl Debug for FrameTracker {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!("FrameTracker:PPN={:#x}", self.ppn.0))
    }
}

impl Drop for FrameTracker {
    fn drop(&mut self) {
        frame_dealloc(self.ppn);
    }
}

trait FrameAllocator {
    fn new() -> Self;
    fn alloc(&mut self) -> Option<PhysPageNum>;
    fn dealloc(&mut self, ppn: PhysPageNum);
}

/// A frame allocator over an arbitrary set of contiguous ranges
pub struct FrameDequeAllocator {
    /// the original start of each free range
    start: Vec<PhysPageNum>,
    /// the end of each free range
    end: Vec<PhysPageNum>,
    /// the current allocation pointer of each range
    current: Vec<PhysPageNum>,
    /// index of the range we are currently allocating from
    active: usize,
    /// recycled (freed) frame numbers
    recycled: Vec<PhysPageNum>,
}

impl FrameDequeAllocator {
    pub fn init(&mut self, ranges: &[(PhysPageNum, PhysPageNum)]) {
        self.start.clear();
        self.end.clear();
        self.current.clear();
        for (l, r) in ranges {
            if l < r {
                self.start.push(*l);
                self.end.push(*r);
                self.current.push(*l);
            }
        }
        self.active = 0;
        self.recycled.clear();
    }
}
impl FrameAllocator for FrameDequeAllocator {
    fn new() -> Self {
        Self {
            start: Vec::new(),
            end: Vec::new(),
            current: Vec::new(),
            active: 0,
            recycled: Vec::new(),
        }
    }
    fn alloc(&mut self) -> Option<PhysPageNum> {
        if let Some(ppn) = self.recycled.pop() {
            Some(ppn)
        } else if self.active >= self.start.len() {
            None
        } else {
            let ppn = self.current[self.active];
            self.current[self.active].0 += 1;
            if self.current[self.active] == self.end[self.active] {
                self.active += 1;
            }
            Some(ppn)
        }
    }
    fn dealloc(&mut self, ppn: PhysPageNum) {
        // validity check against the original ranges (`current` advances, so
        // it cannot be used to decide whether a frame was ever allocated)
        let in_range = (0..self.start.len())
            .any(|i| self.start[i] <= ppn && ppn < self.end[i]);
        if !in_range || self.recycled.iter().any(|&v| v == ppn) {
            panic!("Frame ppn={:#x} has not been allocated!", ppn.0);
        }
        // recycle
        self.recycled.push(ppn);
    }
}

type FrameAllocatorImpl = FrameDequeAllocator;

lazy_static! {
    /// frame allocator instance through lazy_static!
    pub static ref FRAME_ALLOCATOR: UPSafeCell<FrameAllocatorImpl> =
        unsafe { UPSafeCell::new(FrameAllocatorImpl::new()) };
}

/// physical memory map types from the Limine protocol
const MEMMAP_USABLE: u64 = 0;

/// Build the list of allocatable ranges from Limine's memory map. Only
/// `USABLE` regions are used: bootloader-reclaimable memory still holds
/// Limine's own page tables and data structures while we are running on
/// them, so recycling those frames would corrupt the active address space.
pub fn init_frame_allocator() {
    let mut ranges: Vec<(PhysPageNum, PhysPageNum)> = Vec::new();
    let entries = crate::limine_reqs::memmap_entries();
    for entry in entries {
        if entry.typ != MEMMAP_USABLE {
            continue;
        }
        let base = entry.base as usize;
        let top = (entry.base + entry.length) as usize;
        let mut l = PhysAddr::from(base).ceil();
        let r = PhysAddr::from(top).floor();
        // never hand out physical page 0 (leave address 0 unmapped so that a
        // null pointer access faults)
        if l.0 == 0 {
            l.0 = 1;
        }
        if l < r {
            ranges.push((l, r));
        }
    }
    // remove the kernel image range [kernel_phys, kernel_phys + kernel_size)
    let kernel_phys = crate::limine_reqs::kernel_physical_base() as usize;
    unsafe extern "C" {
        safe fn skernel();
        safe fn ekernel();
    }
    let kernel_size = linker_symbol_addr!(ekernel) - linker_symbol_addr!(skernel);
    let k_l = PhysAddr::from(kernel_phys).ceil();
    let k_r = PhysAddr::from(kernel_phys + kernel_size).floor();
    let mut filtered: Vec<(PhysPageNum, PhysPageNum)> = Vec::new();
    for (l, r) in ranges {
        if r <= k_l || l >= k_r {
            filtered.push((l, r));
        } else if l < k_l {
            filtered.push((l, k_l));
            if k_r < r {
                filtered.push((k_r, r));
            }
        } else if k_r < r {
            filtered.push((k_r, r));
        }
    }
    if filtered.is_empty() {
        // fall back to the low MEMORY_END (QEMU `-m 256M`)
        let l = PhysAddr::from(0x100000).ceil();
        let r = PhysAddr::from(MEMORY_END).floor();
        filtered.push((l, r));
    }
    FRAME_ALLOCATOR.exclusive_access().init(&filtered);
    info!(
        "[kernel] frame allocator: {} ranges initialized",
        filtered.len()
    );
}

/// allocate a frame
pub fn frame_alloc() -> Option<FrameTracker> {
    FRAME_ALLOCATOR
        .exclusive_access()
        .alloc()
        .map(FrameTracker::new)
}

/// deallocate a frame
pub fn frame_dealloc(ppn: PhysPageNum) {
    FRAME_ALLOCATOR.exclusive_access().dealloc(ppn);
}
