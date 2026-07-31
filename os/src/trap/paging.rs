//! The page tables
//!
//! Limine hands the kernel over with its own page tables, which map the kernel
//! (and all physical memory through the higher-half direct map) as
//! supervisor-only pages. That is not enough for the user apps of this
//! chapter, which must run in ring 3 without a dedicated address space.
//!
//! Therefore we install our own tiny page tables that:
//!
//! - identity map the low 4GiB with 2MiB pages, marked **user** accessible, so
//!   that both the kernel and the ring-3 apps can reach the app memory;
//! - map the kernel image at `0xffffffff80000000` with 4KiB pages, marked
//!   supervisor-only, so that apps cannot touch kernel memory.
//!
//! The kernel is loaded by Limine at an arbitrary physical address, so we
//! recover the exact physical location by walking Limine's own page tables
//! (accessible through the higher-half direct map) rather than trusting the
//! `limine_executable_address_request` response.

// The page tables are installed once at boot, so `static mut` (via raw
// pointers where needed) is fine here.
#![allow(static_mut_refs)]

use core::arch::asm;

const PAGE_PRESENT: u64 = 1 << 0;
const PAGE_WRITABLE: u64 = 1 << 1;
const PAGE_USER: u64 = 1 << 2;
const PAGE_HUGE: u64 = 1 << 7; // PS bit for 2MiB/1GiB pages
const PAGE_ADDR_MASK: u64 = 0x000F_FFFF_FFFF_F000; // physical address bits

const SZ_4K: u64 = 0x1000;
const SZ_2M: u64 = 0x200000;
const SZ_1G: u64 = 0x40000000;

const KERNEL_BASE: u64 = 0xffffffff80000000;

/// A 4KiB-aligned page table (512 entries)
#[repr(C, align(4096))]
struct PageTable([u64; 512]);

static mut PML4: PageTable = PageTable([0; 512]);
static mut PDPT_LOW: PageTable = PageTable([0; 512]);
static mut PDPT_KERNEL: PageTable = PageTable([0; 512]);
static mut PD_LOW0: PageTable = PageTable([0; 512]);
static mut PD_LOW1: PageTable = PageTable([0; 512]);
static mut PD_LOW2: PageTable = PageTable([0; 512]);
static mut PD_LOW3: PageTable = PageTable([0; 512]);
static mut PD_KERNEL: PageTable = PageTable([0; 512]);
static mut PT_KERNEL0: PageTable = PageTable([0; 512]);
static mut PT_KERNEL1: PageTable = PageTable([0; 512]);
static mut PT_KERNEL2: PageTable = PageTable([0; 512]);
static mut PT_KERNEL3: PageTable = PageTable([0; 512]);

unsafe fn read_cr3() -> u64 {
    let cr3: u64;
    unsafe { asm!("mov rax, cr3", out("rax") cr3, options(nostack)) };
    cr3
}

/// Translate a virtual address to a physical one, by walking the currently
/// active (i.e. Limine's) page tables. All physical accesses go through the
/// higher-half direct map.
fn translate(virt: u64) -> Option<u64> {
    let hhdm = crate::limine_reqs::hhdm_offset();
    // read the (physical) page table entry at `phys + 8 * index`
    let read = |phys: u64, index: usize| -> u64 {
        unsafe {
            *((hhdm + phys + index as u64 * 8) as *const u64)
        }
    };

    let pml4 = unsafe { read_cr3() };
    let e = read(pml4, ((virt >> 39) & 0x1FF) as usize);
    if e & PAGE_PRESENT == 0 {
        return None;
    }
    let pdpt = e & PAGE_ADDR_MASK;
    if e & PAGE_HUGE != 0 {
        return Some((pdpt & !(SZ_1G - 1)) + (virt & (SZ_1G - 1)));
    }

    let e = read(pdpt, ((virt >> 30) & 0x1FF) as usize);
    if e & PAGE_PRESENT == 0 {
        return None;
    }
    let pd = e & PAGE_ADDR_MASK;
    if e & PAGE_HUGE != 0 {
        return Some((pd & !(SZ_2M - 1)) + (virt & (SZ_2M - 1)));
    }

    let e = read(pd, ((virt >> 21) & 0x1FF) as usize);
    if e & PAGE_PRESENT == 0 {
        return None;
    }
    let pt = e & PAGE_ADDR_MASK;
    if e & PAGE_HUGE != 0 {
        return Some((pt & !(SZ_2M - 1)) + (virt & (SZ_2M - 1)));
    }

    let e = read(pt, ((virt >> 12) & 0x1FF) as usize);
    if e & PAGE_PRESENT == 0 {
        return None;
    }
    Some((e & PAGE_ADDR_MASK) + (virt & (SZ_4K - 1)))
}

/// Set up the page tables and switch to them.
pub fn init() {
    unsafe {
        // the physical address of a kernel symbol, given its virtual address
        let phys_of = |virt: u64| translate(virt).expect("kernel address not mapped");

        let kernel_phys = translate(KERNEL_BASE).expect("kernel not mapped by Limine");

        // identity map the low 4GiB, user accessible. The first 2MiB page is
        // left unmapped so that a null-pointer access from a user app faults
        // (just like on the RISC-V port where address 0 is not backed by RAM).
        for (i, pd) in [&mut PD_LOW0, &mut PD_LOW1, &mut PD_LOW2, &mut PD_LOW3]
            .iter_mut()
            .enumerate()
        {
            for (j, entry) in pd.0.iter_mut().enumerate() {
                let page = i as u64 * 512 + j as u64;
                if page == 0 {
                    continue; // leave the 0..2MiB region unmapped
                }
                *entry = (i as u64 * SZ_1G + j as u64 * SZ_2M)
                    | PAGE_PRESENT
                    | PAGE_WRITABLE
                    | PAGE_USER
                    | PAGE_HUGE;
            }
        }
        let pd_low = [
            &PD_LOW0 as *const _ as u64,
            &PD_LOW1 as *const _ as u64,
            &PD_LOW2 as *const _ as u64,
            &PD_LOW3 as *const _ as u64,
        ];
        for (i, pd) in pd_low.iter().enumerate() {
            PDPT_LOW.0[i] = phys_of(*pd) | PAGE_PRESENT | PAGE_WRITABLE | PAGE_USER;
        }

        // map the kernel image at KERNEL_BASE -> kernel_phys, supervisor-only
        // Note that KERNEL_BASE (0xffffffff80000000) lives at PML4 index 511
        // and PDPT index 510 (the last 1GiB of the last 512GiB region).
        unsafe extern "C" {
            safe fn skernel();
            safe fn ekernel();
        }
        let kernel_size = (ekernel as *const ()).addr() - (skernel as *const ()).addr();
        let num_pages = (kernel_size + SZ_4K as usize - 1) / SZ_4K as usize;
        let mut pts = [
            &mut PT_KERNEL0,
            &mut PT_KERNEL1,
            &mut PT_KERNEL2,
            &mut PT_KERNEL3,
        ];
        let num_pts = (num_pages + 511) / 512;
        assert!(num_pts <= pts.len());
        for (k, pt) in pts.iter_mut().enumerate().take(num_pts) {
            for (j, entry) in pt.0.iter_mut().enumerate() {
                let page = k * 512 + j;
                if page < num_pages {
                    *entry = (kernel_phys + page as u64 * SZ_4K) | PAGE_PRESENT | PAGE_WRITABLE;
                }
            }
            PD_KERNEL.0[k] = phys_of(&**pt as *const _ as u64) | PAGE_PRESENT | PAGE_WRITABLE;
        }
        PDPT_KERNEL.0[510] = phys_of(&PD_KERNEL as *const _ as u64) | PAGE_PRESENT | PAGE_WRITABLE;

        // build the top level
        PML4.0[0] = phys_of(&PDPT_LOW as *const _ as u64)
            | PAGE_PRESENT
            | PAGE_WRITABLE
            | PAGE_USER;
        PML4.0[511] = phys_of(&PDPT_KERNEL as *const _ as u64) | PAGE_PRESENT | PAGE_WRITABLE;

        // switch to the new page tables (also flushes the TLB)
        let cr3 = phys_of(&PML4 as *const _ as u64);
        asm!("mov cr3, {0}", in(reg) cr3, options(nostack));
    }
}
