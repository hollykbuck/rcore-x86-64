//! Implementation of [`PidAllocator`], [`PidHandle`] and the per-process
//! [`KernelStack`].
//!
//! On x86-64 the per-process kernel stacks are frame-allocated and mapped into
//! the shared kernel high-half subtree (`KERNEL_SPACE`), so every page table
//! (they all graft `PML4[511]`) can reach the currently running process's
//! kernel stack through `TSS.rsp0`.

use crate::config::kernel_stack_position;
use crate::mm::{KERNEL_SPACE, MapPermission, VirtAddr};
use crate::sync::UPSafeCell;
use crate::trap::TrapContext;
use alloc::vec::Vec;
use lazy_static::*;

/// Pid Allocator struct
pub struct PidAllocator {
    current: usize,
    recycled: Vec<usize>,
}

impl PidAllocator {
    /// Create an empty `PidAllocator`
    pub fn new() -> Self {
        PidAllocator {
            current: 1,
            recycled: Vec::new(),
        }
    }
    /// Allocate a pid
    pub fn alloc(&mut self) -> PidHandle {
        if let Some(pid) = self.recycled.pop() {
            PidHandle(pid)
        } else {
            self.current += 1;
            PidHandle(self.current - 1)
        }
    }
    /// Recycle a pid
    pub fn dealloc(&mut self, pid: usize) {
        assert!(pid < self.current);
        assert!(
            !self.recycled.iter().any(|ppid| *ppid == pid),
            "pid {} has been deallocated!",
            pid
        );
        self.recycled.push(pid);
    }
}

lazy_static! {
    pub static ref PID_ALLOCATOR: UPSafeCell<PidAllocator> =
        unsafe { UPSafeCell::new(PidAllocator::new()) };
}

/// Bind pid lifetime to `PidHandle`
pub struct PidHandle(pub usize);

impl Drop for PidHandle {
    fn drop(&mut self) {
        PID_ALLOCATOR.exclusive_access().dealloc(self.0);
    }
}

/// Allocate a pid from PID_ALLOCATOR
pub fn pid_alloc() -> PidHandle {
    PID_ALLOCATOR.exclusive_access().alloc()
}

/// A per-process kernel stack, frame-allocated and mapped into `KERNEL_SPACE`
/// at `kernel_stack_position(pid)`.
pub struct KernelStack {
    pid: usize,
}

impl KernelStack {
    /// Create a kernel stack for `pid_handle`, mapping its frames into
    /// `KERNEL_SPACE` (the shared kernel high-half subtree).
    pub fn new(pid_handle: &PidHandle) -> Self {
        let pid = pid_handle.0;
        let (kernel_stack_bottom, kernel_stack_top) = kernel_stack_position(pid);
        KERNEL_SPACE.exclusive_access().insert_framed_area(
            kernel_stack_bottom.into(),
            kernel_stack_top.into(),
            MapPermission::R | MapPermission::W,
        );
        KernelStack { pid }
    }
    /// The virtual address just past the top of the stack. The kernel stack is
    /// in the shared kernel high-half mapping, so this address is valid under
    /// any active page table.
    pub fn get_top(&self) -> usize {
        let (_, kernel_stack_top) = kernel_stack_position(self.pid);
        kernel_stack_top
    }
    /// Push a `TrapContext` on top of the stack, returning its virtual address.
    pub fn push_context(&self, trap_cx: TrapContext) -> usize {
        let kernel_stack_top = self.get_top();
        let trap_cx_ptr =
            (kernel_stack_top - core::mem::size_of::<TrapContext>()) as *mut TrapContext;
        unsafe {
            *trap_cx_ptr = trap_cx;
        }
        trap_cx_ptr as usize
    }
}

impl Drop for KernelStack {
    fn drop(&mut self) {
        let (kernel_stack_bottom, _) = kernel_stack_position(self.pid);
        let kernel_stack_bottom_va: VirtAddr = kernel_stack_bottom.into();
        KERNEL_SPACE
            .exclusive_access()
            .remove_area_with_start_vpn(kernel_stack_bottom_va.floor());
    }
}
