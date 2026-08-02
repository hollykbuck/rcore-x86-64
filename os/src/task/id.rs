//! Per-thread resources: the kernel-stack allocator, the per-thread user
//! stack, and a recycle allocator for tids.
//!
//! x86-64 note: unlike RISC-V, the `TrapContext` of a thread lives **on the
//! thread's kernel stack** (see `KernelStack::push_context`), so there is no
//! `trap_cx` page in user space. Only the per-thread **user stack** is mapped
//! into the process's address space here.

use super::ProcessControlBlock;
use crate::config::{PAGE_SIZE, USER_STACK_SIZE, kernel_stack_position};
use crate::mm::{KERNEL_SPACE, MapPermission, VirtAddr};
use crate::sync::UPIntrFreeCell;
use crate::trap::TrapContext;
use alloc::sync::{Arc, Weak};
use alloc::vec::Vec;
use lazy_static::*;

/// A simple allocator that recycles freed ids.
pub struct RecycleAllocator {
    current: usize,
    recycled: Vec<usize>,
}

impl RecycleAllocator {
    /// Create an empty `RecycleAllocator` starting from 0.
    pub fn new() -> Self {
        Self::new_start(0)
    }
    /// Create an empty `RecycleAllocator` starting from `current`.
    pub fn new_start(current: usize) -> Self {
        RecycleAllocator {
            current,
            recycled: Vec::new(),
        }
    }
    /// Allocate an id, reusing a recycled one if available.
    pub fn alloc(&mut self) -> usize {
        if let Some(id) = self.recycled.pop() {
            id
        } else {
            self.current += 1;
            self.current - 1
        }
    }
    /// Recycle an id.
    pub fn dealloc(&mut self, id: usize) {
        assert!(id < self.current);
        assert!(
            !self.recycled.iter().any(|i| *i == id),
            "id {} has been deallocated!",
            id
        );
        self.recycled.push(id);
    }
}

lazy_static! {
    static ref KSTACK_ALLOCATOR: UPIntrFreeCell<RecycleAllocator> =
        unsafe { UPIntrFreeCell::new(RecycleAllocator::new()) };
}

/// A per-thread kernel stack, frame-allocated and mapped into `KERNEL_SPACE`
/// (the shared kernel high-half subtree) at `kernel_stack_position(id)`.
pub struct KernelStack {
    id: usize,
}

/// Allocate a kernel stack for a new thread.
pub fn kstack_alloc() -> KernelStack {
    let kstack_id = KSTACK_ALLOCATOR.exclusive_access().alloc();
    let (kernel_stack_bottom, kernel_stack_top) = kernel_stack_position(kstack_id);
    KERNEL_SPACE.exclusive_access().insert_framed_area(
        kernel_stack_bottom.into(),
        kernel_stack_top.into(),
        MapPermission::R | MapPermission::W,
    );
    KernelStack { id: kstack_id }
}

impl KernelStack {
    /// The virtual address just past the top of the stack. The kernel stack is
    /// in the shared kernel high-half mapping, so this address is valid under
    /// any active page table.
    pub fn get_top(&self) -> usize {
        let (_, kernel_stack_top) = kernel_stack_position(self.id);
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
        let (kernel_stack_bottom, _) = kernel_stack_position(self.id);
        let kernel_stack_bottom_va: VirtAddr = kernel_stack_bottom.into();
        KERNEL_SPACE
            .exclusive_access()
            .remove_area_with_start_vpn(kernel_stack_bottom_va.floor());
        KSTACK_ALLOCATOR.exclusive_access().dealloc(self.id);
    }
}

/// The per-thread user resources: the tid and the user stack mapped into the
/// process's address space.
pub struct TaskUserRes {
    /// the thread id
    pub tid: usize,
    /// the bottom of the main-thread user stack; the per-thread stacks of
    /// other threads are mapped above it
    pub ustack_base: usize,
    /// the process this thread belongs to
    pub process: Weak<ProcessControlBlock>,
}

/// The user stack of thread `tid` starts `tid` stack slots above
/// `ustack_base` (the bottom of the main thread's stack).
fn ustack_bottom_from_tid(ustack_base: usize, tid: usize) -> usize {
    ustack_base + tid * (PAGE_SIZE + USER_STACK_SIZE)
}

impl TaskUserRes {
    /// Create the user resources of a thread, allocating the tid (and, if
    /// `alloc_user_res`, the user stack) of the process.
    pub fn new(
        process: Arc<ProcessControlBlock>,
        ustack_base: usize,
        alloc_user_res: bool,
    ) -> Self {
        let tid = process.inner_exclusive_access().alloc_tid();
        let task_user_res = Self {
            tid,
            ustack_base,
            process: Arc::downgrade(&process),
        };
        if alloc_user_res {
            task_user_res.alloc_user_res();
        }
        task_user_res
    }

    /// Map the user stack of this thread into the process's address space.
    pub fn alloc_user_res(&self) {
        let process = self.process.upgrade().unwrap();
        let mut process_inner = process.inner_exclusive_access();
        // alloc user stack
        let ustack_bottom = ustack_bottom_from_tid(self.ustack_base, self.tid);
        let ustack_top = ustack_bottom + USER_STACK_SIZE;
        process_inner.memory_set.insert_framed_area(
            ustack_bottom.into(),
            ustack_top.into(),
            MapPermission::R | MapPermission::W | MapPermission::U,
        );
    }

    fn dealloc_user_res(&self) {
        // dealloc tid
        let process = self.process.upgrade().unwrap();
        let mut process_inner = process.inner_exclusive_access();
        // dealloc ustack manually
        let ustack_bottom_va: VirtAddr = ustack_bottom_from_tid(self.ustack_base, self.tid).into();
        process_inner
            .memory_set
            .remove_area_with_start_vpn(ustack_bottom_va.floor());
    }

    /// Deallocate the tid.
    pub fn dealloc_tid(&self) {
        let process = self.process.upgrade().unwrap();
        let mut process_inner = process.inner_exclusive_access();
        process_inner.dealloc_tid(self.tid);
    }

    /// The bottom of the main thread's user stack.
    pub fn ustack_base(&self) -> usize {
        self.ustack_base
    }
    /// The top of this thread's user stack.
    pub fn ustack_top(&self) -> usize {
        ustack_bottom_from_tid(self.ustack_base, self.tid) + USER_STACK_SIZE
    }
}

impl Drop for TaskUserRes {
    fn drop(&mut self) {
        self.dealloc_tid();
        self.dealloc_user_res();
    }
}
