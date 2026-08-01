//! Implementation of [`Processor`] and the intersection of control flow.
//!
//! The scheduler is an idle control flow (`run_tasks` loop) that fetches
//! ready tasks and `__switch`es to them; `schedule` switches back to the idle
//! control flow when the current task suspends or exits. On x86-64, every
//! switch to a task must (1) activate its page table (`mov cr3`) and (2) point
//! the trap kernel stack (`TSS.rsp0` + `current_stack_top`) at its kernel
//! stack, because the CPU does not switch address spaces on trap.

use super::__switch;
use super::{TaskContext, TaskControlBlock};
use super::{TaskStatus, fetch_task};
use crate::sync::UPSafeCell;
use crate::trap::{TrapContext, set_current_stack_top};
use alloc::sync::Arc;
use lazy_static::*;

/// Processor management structure
pub struct Processor {
    /// The task currently executing on the current processor
    current: Option<Arc<TaskControlBlock>>,
    /// The basic control flow of each core, helping to select and switch process
    idle_task_cx: TaskContext,
}

impl Processor {
    /// Create an empty Processor
    pub fn new() -> Self {
        Self {
            current: None,
            idle_task_cx: TaskContext::zero_init(),
        }
    }
    /// Get mutable reference to `idle_task_cx`
    fn get_idle_task_cx_ptr(&mut self) -> *mut TaskContext {
        &mut self.idle_task_cx as *mut _
    }
    /// Get current task in moving semantics
    pub fn take_current(&mut self) -> Option<Arc<TaskControlBlock>> {
        self.current.take()
    }
    /// Get current task in cloning semantics
    pub fn current(&self) -> Option<Arc<TaskControlBlock>> {
        self.current.as_ref().map(Arc::clone)
    }
}

lazy_static! {
    pub static ref PROCESSOR: UPSafeCell<Processor> =
        unsafe { UPSafeCell::new(Processor::new()) };
}

/// The main part of process execution and scheduling.
///
/// Loop `fetch_task` to get the process that needs to run, and switch to it
/// through `__switch`. The idle control flow runs on the boot stack.
pub fn run_tasks() -> ! {
    loop {
        let mut processor = PROCESSOR.exclusive_access();
        if let Some(task) = fetch_task() {
            let idle_task_cx_ptr = processor.get_idle_task_cx_ptr();
            // access the coming task TCB exclusively
            let mut task_inner = task.inner_exclusive_access();
            let next_task_cx_ptr = &task_inner.task_cx as *const TaskContext;
            task_inner.task_status = TaskStatus::Running;
            // x86-64: activate the task's page table before switching to it
            // (this is the only place CR3 changes; traps never switch it).
            task_inner.memory_set.activate();
            drop(task_inner);
            // x86-64: point the trap kernel stack at the task's own stack so
            // its first user-mode trap lands on the right stack.
            set_current_stack_top(task.kernel_stack.get_top() as u64);
            // release the coming task TCB manually
            processor.current = Some(task);
            // release the processor manually
            drop(processor);
            unsafe {
                __switch(idle_task_cx_ptr, next_task_cx_ptr);
            }
        }
    }
}

/// Take the current task, leaving a `None` in its place
pub fn take_current_task() -> Option<Arc<TaskControlBlock>> {
    PROCESSOR.exclusive_access().take_current()
}

/// Get the running task
pub fn current_task() -> Option<Arc<TaskControlBlock>> {
    PROCESSOR.exclusive_access().current()
}

/// Get the token of the address space of the current task
pub fn current_user_token() -> usize {
    let task = current_task().unwrap();
    let token = task.inner_exclusive_access().get_user_token();
    token
}

/// Get the mutable reference to the trap context of the current task
pub fn current_trap_cx() -> &'static mut TrapContext {
    current_task()
        .unwrap()
        .inner_exclusive_access()
        .get_trap_cx()
}

/// Return to the idle control flow for new scheduling
pub fn schedule(switched_task_cx_ptr: *mut TaskContext) {
    let mut processor = PROCESSOR.exclusive_access();
    let idle_task_cx_ptr = processor.get_idle_task_cx_ptr();
    drop(processor);
    unsafe {
        __switch(switched_task_cx_ptr, idle_task_cx_ptr);
    }
}
