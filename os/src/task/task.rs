//! Implementation of [`TaskControlBlock`] (a thread).
//!
//! Since ch8 the scheduling unit is a **thread**; the process-wide state
//! (address space, file descriptor table, signals, children, exit code) lives
//! in [`ProcessControlBlock`](crate::task::ProcessControlBlock). Each thread
//! owns its kernel stack (with the `TrapContext` on top, x86-64 style), its
//! task context and its per-thread user stack.

use super::id::TaskUserRes;
use super::{KernelStack, ProcessControlBlock, TaskContext, kstack_alloc};
use crate::trap::TrapContext;
use crate::{sync::UPIntrFreeCell};
use alloc::sync::{Arc, Weak};
use core::sync::atomic::AtomicUsize;

/// A thread of a process (the scheduling unit since ch8).
pub struct TaskControlBlock {
    /// the process this thread belongs to
    pub process: Weak<ProcessControlBlock>,
    /// the kernel stack of this thread
    pub kstack: KernelStack,
    /// the id of the processor that last ran (or will run) this thread. It is
    /// the owner of this thread's context save: a woken thread must be parked
    /// on its owner's pending queue and only re-enter the global ready queue
    /// after the owner has switched away from it (SMP race, see ch8 stage 3).
    pub last_cpu: AtomicUsize,
    // mutable
    inner: UPIntrFreeCell<TaskControlBlockInner>,
}

impl TaskControlBlock {
    /// Get the mutable reference to the inner structure of the thread.
    pub fn inner_exclusive_access(&self) -> crate::sync::UPIntrRefMut<'_, TaskControlBlockInner> {
        self.inner.exclusive_access()
    }

    /// Get the token of the address space of the process this thread belongs to.
    pub fn get_user_token(&self) -> usize {
        let process = self.process.upgrade().unwrap();
        let inner = process.inner_exclusive_access();
        inner.memory_set.token()
    }
}

pub struct TaskControlBlockInner {
    /// per-thread user resources (tid and user stack)
    pub res: Option<TaskUserRes>,
    /// virtual address of the `TrapContext` on this thread's kernel stack
    pub trap_cx_ptr: usize,
    /// the context to resume this thread's kernel flow after scheduling
    pub task_cx: TaskContext,
    /// the scheduling state of this thread
    pub task_status: TaskStatus,
    /// the exit code, set when the thread exits
    pub exit_code: Option<i32>,
}

impl TaskControlBlockInner {
    pub fn get_trap_cx(&self) -> &'static mut TrapContext {
        unsafe { &mut *(self.trap_cx_ptr as *mut TrapContext) }
    }
}

impl TaskControlBlock {
    /// Create a thread of `process`. If `alloc_user_res` is set, a new user
    /// stack is mapped for it (used by `sys_thread_create`); the main thread
    /// of a freshly loaded/forked process has its stack already mapped by
    /// `MemorySet::from_elf`/`from_existed_user`, so it passes `false`.
    pub fn new(
        process: Arc<ProcessControlBlock>,
        ustack_base: usize,
        alloc_user_res: bool,
    ) -> Self {
        let res = TaskUserRes::new(Arc::clone(&process), ustack_base, alloc_user_res);
        let kstack = kstack_alloc();
        // the trap context is written by the caller afterwards; reserve the
        // slot at the top of the kernel stack now so `trap_cx_ptr` is stable
        let trap_cx_ptr = kstack.push_context(TrapContext::app_init_context(0, 0));
        Self {
            process: Arc::downgrade(&process),
            kstack,
            last_cpu: AtomicUsize::new(0),
            inner: unsafe {
                UPIntrFreeCell::new(TaskControlBlockInner {
                    res: Some(res),
                    trap_cx_ptr,
                    task_cx: TaskContext::goto_restore(trap_cx_ptr),
                    task_status: TaskStatus::Ready,
                    exit_code: None,
                })
            },
        }
    }
}

/// The scheduling state of a thread.
#[derive(Copy, Clone, PartialEq)]
pub enum TaskStatus {
    /// waiting in the ready queue
    Ready,
    /// running on a processor
    Running,
    /// blocked (waiting for a timer or a synchronization primitive)
    Blocked,
}
