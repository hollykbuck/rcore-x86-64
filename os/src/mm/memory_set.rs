//! Implementation of [`MapArea`] and [`MemorySet`].
//!
//! x86-64 layout (see the ch4 plan for the rationale):
//!
//! - the **kernel high-half** is mapped by a single shared subtree
//!   (`PML4[511]`): every page table we install, kernel or per-app, points at
//!   the same physical kernel page-table frames. Kernel code/data/bss (and
//!   therefore every task's kernel stack) stay reachable under any active
//!   CR3, so traps never need to switch CR3.
//! - the **physmap** (`phys + hhdm_offset == virt`) is mapped under a second
//!   top-level entry (`PML4[256]` on Limine's default x86-64 HHDM base), also
//!   shared by every page table, so physical memory (page-table frames, app
//!   data, the local APIC) can be touched no matter which page table is
//!   active.
//! - the low 4GiB identity mapping from ch3 is gone: it would collide with
//!   the user virtual addresses (apps link at `0x10000`).

use super::address::phys_virt_offset;
use super::{FrameTracker, frame_alloc};
use super::{PTEFlags, PageTable, PageTableEntry};
use super::{PhysAddr, PhysPageNum, VirtAddr, VirtPageNum};
use super::{StepByOne, VPNRange};
use crate::config::{
    KERNEL_BASE, LAPIC_BASE, MEMORY_END, PAGE_SIZE, USER_STACK_SIZE,
};
use crate::sync::UPIntrFreeCell;
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::arch::asm;
use lazy_static::*;

unsafe extern "C" {
    safe fn stext();
    safe fn etext();
    safe fn srodata();
    safe fn erodata();
    safe fn sdata();
    safe fn egot();
    safe fn sbss_with_stack();
    safe fn ebss();
}

lazy_static! {
    /// a memory set instance through lazy_static! managing kernel space
    pub static ref KERNEL_SPACE: Arc<UPIntrFreeCell<MemorySet>> =
        Arc::new(unsafe { UPIntrFreeCell::new(MemorySet::new_kernel()) });
}

/// The root page table entry of the kernel address space (for the `Hal`
/// implementations of DMA address translation).
pub fn kernel_token() -> usize {
    KERNEL_SPACE.exclusive_access().token()
}

/// Map a physical MMIO region (e.g. PCI ECAM / device BAR) into the physmap
/// window (`pa + hhdm_offset`) so it is reachable via [`phys_to_virt`].
///
/// The caller must provide page-aligned `pa` and `len`.
pub fn ioremap(pa: usize, len: usize) {
    debug_assert!(pa % PAGE_SIZE == 0 && len % PAGE_SIZE == 0);
    // Use the 48-bit (masked) physmap base here, exactly like the physmap
    // area in `new_kernel`: `phys_to_virt` adds the raw 64-bit hhdm offset,
    // but `MapArea::new_linear` computes `pa = va - pa_offset`, so the two
    // must agree bit-for-bit or the offset leaks 2^48 into the result.
    let phys_base = VirtAddr::from(phys_virt_offset()).0;
    KERNEL_SPACE.exclusive_access().push(
        MapArea::new_linear(
            (phys_base + pa).into(),
            (phys_base + pa + len).into(),
            MapPermission::R | MapPermission::W,
            phys_base,
        ),
        None,
    );
}

/// memory set structure, controls virtual-memory space
pub struct MemorySet {
    page_table: PageTable,
    areas: Vec<MapArea>,
}

impl MemorySet {
    pub fn new_bare() -> Self {
        Self {
            page_table: PageTable::new(),
            areas: Vec::new(),
        }
    }
    pub fn token(&self) -> usize {
        self.page_table.token()
    }
    /// Assume that no conflicts.
    pub fn insert_framed_area(
        &mut self,
        start_va: VirtAddr,
        end_va: VirtAddr,
        permission: MapPermission,
    ) {
        self.push(
            MapArea::new(start_va, end_va, MapType::Framed, permission),
            None,
        );
    }
    /// Remove the area starting at `start_vpn` (used to recycle a process's
    /// kernel stack on drop).
    pub fn remove_area_with_start_vpn(&mut self, start_vpn: VirtPageNum) {
        if let Some((idx, area)) = self
            .areas
            .iter_mut()
            .enumerate()
            .find(|(_, area)| area.vpn_range.get_start() == start_vpn)
        {
            area.unmap(&mut self.page_table);
            self.areas.remove(idx);
        }
    }
    fn push(&mut self, mut map_area: MapArea, data: Option<&[u8]>) {
        map_area.map(&mut self.page_table);
        if let Some(data) = data {
            map_area.copy_data(&self.page_table, data);
        }
        self.areas.push(map_area);
    }
    /// The physical address of the kernel image, which anchors the kernel's
    /// va -> pa linear offset.
    fn kernel_pa_offset() -> usize {
        let kernel_phys = crate::limine_reqs::kernel_physical_base() as usize;
        VirtAddr::from(KERNEL_BASE).0 - kernel_phys
    }
    /// Build the kernel address space:
    /// - the kernel image mapped linearly at `KERNEL_BASE` (pa = va - offset);
    /// - the physmap (all physical memory, plus the local APIC MMIO);
    pub fn new_kernel() -> Self {
        let mut memory_set = Self::new_bare();
        let kernel_offset = Self::kernel_pa_offset();
        // map kernel sections
        memory_set.push(
            MapArea::new_linear(
                (linker_symbol_addr!(stext)).into(),
                (linker_symbol_addr!(etext)).into(),
                MapPermission::R | MapPermission::X,
                kernel_offset,
            ),
            None,
        );
        memory_set.push(
            MapArea::new_linear(
                (linker_symbol_addr!(srodata)).into(),
                (linker_symbol_addr!(erodata)).into(),
                MapPermission::R,
                kernel_offset,
            ),
            None,
        );
        memory_set.push(
            MapArea::new_linear(
                (linker_symbol_addr!(sdata)).into(),
                (linker_symbol_addr!(egot)).into(),
                MapPermission::R | MapPermission::W,
                kernel_offset,
            ),
            None,
        );
        memory_set.push(
            MapArea::new_linear(
                (linker_symbol_addr!(sbss_with_stack)).into(),
                (linker_symbol_addr!(ebss)).into(),
                MapPermission::R | MapPermission::W,
                kernel_offset,
            ),
            None,
        );
        // the physmap: all physical memory at phys + hhdm_offset. The base is
        // canonicalized to 48 bits so the Linear `pa = va - pa_offset` math
        // works (pa_offset must live in the same 48-bit space as `va`).
        let phys_base = VirtAddr::from(phys_virt_offset()).0;
        memory_set.push(
            MapArea::new_linear(
                (phys_base).into(),
                (phys_base + MEMORY_END).into(),
                MapPermission::R | MapPermission::W,
                phys_base,
            ),
            None,
        );
        // the local APIC MMIO window (used by the timer from trap handlers
        // running on any page table)
        memory_set.push(
            MapArea::new_linear(
                (phys_base + LAPIC_BASE).into(),
                (phys_base + LAPIC_BASE + PAGE_SIZE).into(),
                MapPermission::R | MapPermission::W,
                phys_base,
            ),
            None,
        );
        memory_set
    }
    /// Share the kernel high-half and the physmap with a freshly created app
    /// page table: copy the corresponding top-level entries from
    /// `KERNEL_SPACE`'s page table.
    ///
    /// Every PML4 entry from the physmap onwards (indices 256..512) is shared:
    /// this covers the physmap itself (256), any `ioremap`'d device windows
    /// (e.g. the virtio-blk PCI BAR, which may land on 257), and the kernel
    /// high-half (511). Without the ioremap'd entries a newly created process
    /// faults when it touches a device mapped by the kernel.
    fn graft_kernel_entries(&mut self) {
        let kernel_root = KERNEL_SPACE.exclusive_access().page_table.root_ppn();
        let kernel_root_entries = kernel_root.get_pte_array();
        let my_root = self.page_table.root_ppn();
        let my_entries = my_root.get_pte_array();
        let physmap_idx = (phys_virt_offset() >> 39) & 0x1FF;
        let kernel_idx = (KERNEL_BASE >> 39) & 0x1FF;
        for idx in physmap_idx..=kernel_idx {
            my_entries[idx] = kernel_root_entries[idx];
        }
    }
    /// Include sections in elf and user stack, also returns user_sp and entry
    /// point.
    pub fn from_elf(elf_data: &[u8]) -> (Self, usize, usize) {
        let mut memory_set = Self::new_bare();
        // share the kernel high-half + physmap
        memory_set.graft_kernel_entries();
        // map program headers of elf, with U flag
        let elf = xmas_elf::ElfFile::new(elf_data).unwrap();
        let elf_header = elf.header;
        let magic = elf_header.pt1.magic;
        assert_eq!(magic, [0x7f, 0x45, 0x4c, 0x46], "invalid elf!");
        let ph_count = elf_header.pt2.ph_count();
        let mut max_end_vpn = VirtPageNum(0);
        for i in 0..ph_count {
            let ph = elf.program_header(i).unwrap();
            if ph.get_type().unwrap() == xmas_elf::program::Type::Load {
                let start_va: VirtAddr = (ph.virtual_addr() as usize).into();
                let end_va: VirtAddr = ((ph.virtual_addr() + ph.mem_size()) as usize).into();
                let mut map_perm = MapPermission::U;
                let ph_flags = ph.flags();
                if ph_flags.is_read() {
                    map_perm |= MapPermission::R;
                }
                if ph_flags.is_write() {
                    map_perm |= MapPermission::W;
                }
                if ph_flags.is_execute() {
                    map_perm |= MapPermission::X;
                }
                let map_area = MapArea::new(start_va, end_va, MapType::Framed, map_perm);
                max_end_vpn = map_area.vpn_range.get_end();
                memory_set.push(
                    map_area,
                    Some(&elf.input[ph.offset() as usize..(ph.offset() + ph.file_size()) as usize]),
                );
            }
        }
        // map user stack with U flags
        let max_end_va: VirtAddr = max_end_vpn.into();
        let mut user_stack_bottom: usize = max_end_va.into();
        // guard page
        user_stack_bottom += PAGE_SIZE;
        let user_stack_top = user_stack_bottom + USER_STACK_SIZE;
        memory_set.push(
            MapArea::new(
                user_stack_bottom.into(),
                user_stack_top.into(),
                MapType::Framed,
                MapPermission::R | MapPermission::W | MapPermission::U,
            ),
            None,
        );
        (
            memory_set,
            user_stack_top,
            elf.header.pt2.entry_point() as usize,
        )
    }
    /// Switch to this address space by writing CR3 (which also flushes the
    /// whole TLB; there is no `sfence.vma` on x86).
    pub fn activate(&self) {
        let cr3 = self.page_table.token();
        unsafe {
            asm!("mov cr3, {0}", in(reg) cr3, options(nostack));
        }
    }
    /// Translate a virtual page number through this address space's page table.
    pub fn translate(&self, vpn: VirtPageNum) -> Option<PageTableEntry> {
        self.page_table.translate(vpn)
    }
    #[allow(unused)]
    pub fn shrink_to(&mut self, start: VirtAddr, new_end: VirtAddr) -> bool {
        if let Some(area) = self
            .areas
            .iter_mut()
            .find(|area| area.vpn_range.get_start() == start.floor())
        {
            area.shrink_to(&mut self.page_table, new_end.ceil());
            true
        } else {
            false
        }
    }
    #[allow(unused)]
    pub fn append_to(&mut self, start: VirtAddr, new_end: VirtAddr) -> bool {
        if let Some(area) = self
            .areas
            .iter_mut()
            .find(|area| area.vpn_range.get_start() == start.floor())
        {
            area.append_to(&mut self.page_table, new_end.ceil());
            true
        } else {
            false
        }
    }
    /// Copy an existing user address space (used by `fork`).
    pub fn from_existed_user(user_space: &Self) -> Self {
        let mut memory_set = Self::new_bare();
        // x86-64: share the kernel high-half + physmap with the parent
        memory_set.graft_kernel_entries();
        // copy data sections/user_stack
        for area in user_space.areas.iter() {
            let new_area = MapArea::from_another(area);
            memory_set.push(new_area, None);
            // copy data from the other space
            for vpn in area.vpn_range {
                let src_ppn = user_space.translate(vpn).unwrap().ppn();
                let dst_ppn = memory_set.translate(vpn).unwrap().ppn();
                dst_ppn
                    .get_bytes_array()
                    .copy_from_slice(src_ppn.get_bytes_array());
            }
        }
        memory_set
    }
    /// Remove all `MapArea`s, freeing their frames (used on `exit`).
    pub fn recycle_data_pages(&mut self) {
        self.areas.clear();
    }
}

/// map area structure, controls a contiguous piece of virtual memory
pub struct MapArea {
    vpn_range: VPNRange,
    data_frames: BTreeMap<VirtPageNum, FrameTracker>,
    map_type: MapType,
    map_perm: MapPermission,
}

impl MapArea {
    pub fn new(
        start_va: VirtAddr,
        end_va: VirtAddr,
        map_type: MapType,
        map_perm: MapPermission,
    ) -> Self {
        let start_vpn: VirtPageNum = start_va.floor();
        let end_vpn: VirtPageNum = end_va.ceil();
        Self {
            vpn_range: VPNRange::new(start_vpn, end_vpn),
            data_frames: BTreeMap::new(),
            map_type,
            map_perm,
        }
    }
    /// Create a linear (va -> pa = va - `pa_offset`) map area.
    pub fn new_linear(
        start_va: VirtAddr,
        end_va: VirtAddr,
        map_perm: MapPermission,
        pa_offset: usize,
    ) -> Self {
        Self::new(start_va, end_va, MapType::Linear { pa_offset }, map_perm)
    }
    /// Copy the mapping configuration of another area (used by `fork`); the
    /// frames are allocated fresh by `map`.
    pub fn from_another(another: &Self) -> Self {
        Self {
            vpn_range: VPNRange::new(another.vpn_range.get_start(), another.vpn_range.get_end()),
            data_frames: BTreeMap::new(),
            map_type: another.map_type,
            map_perm: another.map_perm,
        }
    }
    pub fn map_one(&mut self, page_table: &mut PageTable, vpn: VirtPageNum) {
        let ppn: PhysPageNum;
        match self.map_type {
            MapType::Framed => {
                let frame = frame_alloc().unwrap();
                ppn = frame.ppn;
                self.data_frames.insert(vpn, frame);
            }
            MapType::Linear { pa_offset } => {
                let va: VirtAddr = vpn.into();
                let pa: PhysAddr = (va.0 - pa_offset).into();
                ppn = pa.floor();
            }
        }
        let pte_flags = pte_flags_from_perm(self.map_perm);
        page_table.map(vpn, ppn, pte_flags);
    }
    #[allow(unused)]
    pub fn unmap_one(&mut self, page_table: &mut PageTable, vpn: VirtPageNum) {
        if self.map_type == MapType::Framed {
            self.data_frames.remove(&vpn);
        }
        page_table.unmap(vpn);
    }
    pub fn map(&mut self, page_table: &mut PageTable) {
        for vpn in self.vpn_range {
            self.map_one(page_table, vpn);
        }
    }
    #[allow(unused)]
    pub fn unmap(&mut self, page_table: &mut PageTable) {
        for vpn in self.vpn_range {
            self.unmap_one(page_table, vpn);
        }
    }
    #[allow(unused)]
    pub fn shrink_to(&mut self, page_table: &mut PageTable, new_end: VirtPageNum) {
        for vpn in VPNRange::new(new_end, self.vpn_range.get_end()) {
            self.unmap_one(page_table, vpn)
        }
        self.vpn_range = VPNRange::new(self.vpn_range.get_start(), new_end);
    }
    #[allow(unused)]
    pub fn append_to(&mut self, page_table: &mut PageTable, new_end: VirtPageNum) {
        for vpn in VPNRange::new(self.vpn_range.get_end(), new_end) {
            self.map_one(page_table, vpn)
        }
        self.vpn_range = VPNRange::new(self.vpn_range.get_start(), new_end);
    }
    /// data: start-aligned but maybe with shorter length
    /// assume that all frames were cleared before
    pub fn copy_data(&mut self, page_table: &PageTable, data: &[u8]) {
        assert_eq!(self.map_type, MapType::Framed);
        let mut start: usize = 0;
        let mut current_vpn = self.vpn_range.get_start();
        let len = data.len();
        loop {
            let src = &data[start..len.min(start + PAGE_SIZE)];
            let dst = &mut page_table
                .translate(current_vpn)
                .unwrap()
                .ppn()
                .get_bytes_array()[..src.len()];
            dst.copy_from_slice(src);
            start += PAGE_SIZE;
            if start >= len {
                break;
            }
            current_vpn.step();
        }
    }
}

/// Convert a `MapPermission` to x86 page table flags. `R` is implied (x86 has
/// no separate read bit); `X` becomes the inverse of the `NX` bit.
fn pte_flags_from_perm(perm: MapPermission) -> PTEFlags {
    let mut flags = PTEFlags::V;
    if perm.contains(MapPermission::W) {
        flags |= PTEFlags::W;
    }
    if perm.contains(MapPermission::U) {
        flags |= PTEFlags::U;
    }
    if !perm.contains(MapPermission::X) {
        flags |= PTEFlags::NX;
    }
    flags
}

#[derive(Copy, Clone, PartialEq, Debug)]
/// map type for memory set: framed or linear
pub enum MapType {
    /// allocate a new frame for every virtual page
    Framed,
    /// `pa = va - pa_offset`
    Linear { pa_offset: usize },
}

bitflags! {
    /// map permission corresponding to that in pte: `R W X U`
    pub struct MapPermission: u8 {
        const R = 1 << 1;
        const W = 1 << 2;
        const X = 1 << 3;
        const U = 1 << 4;
    }
}
