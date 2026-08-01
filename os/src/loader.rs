//! Loading user applications into memory
//!
//! The apps are linked into the kernel image as full ELF files (no more
//! fixed-address slots or `.bin` blobs). For every task we keep a private
//! kernel stack in the kernel `.bss`; since `.bss` lives in the shared kernel
//! high-half mapping, it is reachable from every page table.

use crate::config::*;
use crate::trap::TrapContext;

#[repr(align(4096))]
#[derive(Copy, Clone)]
struct KernelStack {
    data: [u8; KERNEL_STACK_SIZE],
}

// Force the per-task kernel stacks into the writable `.data` section: they
// are written at runtime (the TrapContext pushed on top), but an all-zero
// `static` that is never touched through `&mut` would otherwise be placed in
// `.rodata` by the compiler (and fault under the read-only mapping).
#[unsafe(link_section = ".data")]
static KERNEL_STACK: [KernelStack; MAX_APP_NUM] = [KernelStack {
    data: [0; KERNEL_STACK_SIZE],
}; MAX_APP_NUM];

impl KernelStack {
    fn get_sp(&self) -> usize {
        self.data.as_ptr() as usize + KERNEL_STACK_SIZE
    }
    pub fn push_context(&self, trap_cx: TrapContext) -> usize {
        let trap_cx_ptr = (self.get_sp() - core::mem::size_of::<TrapContext>()) as *mut TrapContext;
        unsafe {
            *trap_cx_ptr = trap_cx;
        }
        trap_cx_ptr as usize
    }
}

/// Get the top of task `app_id`'s kernel stack. Must be kept in sync with the
/// TSS `rsp0` (see [`crate::trap::set_current_stack_top`]).
pub fn kernel_stack_top(app_id: usize) -> usize {
    KERNEL_STACK[app_id].get_sp()
}

/// Push a `TrapContext` on top of task `app_id`'s kernel stack, returning its
/// virtual address.
pub fn push_context(app_id: usize, trap_cx: TrapContext) -> usize {
    KERNEL_STACK[app_id].push_context(trap_cx)
}

/// Get the total number of applications.
pub fn get_num_app() -> usize {
    unsafe extern "C" {
        safe fn _num_app();
    }
    unsafe { (linker_symbol_addr!(_num_app) as *const usize).read_volatile() }
}

/// Get the ELF image of application `app_id`.
pub fn get_app_data(app_id: usize) -> &'static [u8] {
    unsafe extern "C" {
        safe fn _num_app();
    }
    let num_app_ptr = linker_symbol_addr!(_num_app) as *const usize;
    let num_app = get_num_app();
    let app_start = unsafe { core::slice::from_raw_parts(num_app_ptr.add(1), num_app + 1) };
    assert!(app_id < num_app);
    unsafe {
        core::slice::from_raw_parts(
            app_start[app_id] as *const u8,
            app_start[app_id + 1] - app_start[app_id],
        )
    }
}
