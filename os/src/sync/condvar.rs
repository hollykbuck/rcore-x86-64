//! Condition variable implementation (kernel-level, taken from the RISC-V
//! tutorial ch8, with ch9's `wait_no_sched`).

use crate::sync::{Mutex, UPIntrFreeCell};
use crate::task::{
    TaskContext, TaskControlBlock, block_current_and_run_next, block_current_task, current_task,
    wakeup_task,
};
use alloc::{collections::VecDeque, sync::Arc};

/// A condition variable.
pub struct Condvar {
    /// the inner state
    pub inner: UPIntrFreeCell<CondvarInner>,
}

/// The inner state of a condition variable.
pub struct CondvarInner {
    /// threads waiting on this condition variable
    pub wait_queue: VecDeque<Arc<TaskControlBlock>>,
}

impl Condvar {
    /// Create a new `Condvar`.
    pub fn new() -> Self {
        Self {
            inner: unsafe {
                UPIntrFreeCell::new(CondvarInner {
                    wait_queue: VecDeque::new(),
                })
            },
        }
    }

    /// Wake up one waiter.
    pub fn signal(&self) {
        let mut inner = self.inner.exclusive_access();
        if let Some(task) = inner.wait_queue.pop_front() {
            wakeup_task(task);
        }
    }

    /// Release `mutex`, block until signaled, then re-acquire `mutex`.
    pub fn wait(&self, mutex: Arc<dyn Mutex>) {
        mutex.unlock();
        let mut inner = self.inner.exclusive_access();
        inner.wait_queue.push_back(current_task().unwrap());
        drop(inner);
        block_current_and_run_next();
        mutex.lock();
    }

    /// Enqueue the current task on this condition variable and block it,
    /// returning its task context pointer. **The caller must drop the lock it
    /// holds and then call `schedule(task_cx_ptr)`** (ch9): the wait-queue
    /// push must happen while the interrupt-masking lock is still held so an
    /// IRQ-driven `signal` cannot observe the waiter before it is blocked.
    pub fn wait_no_sched(&self) -> *mut TaskContext {
        self.inner.exclusive_session(|inner| {
            inner.wait_queue.push_back(current_task().unwrap());
        });
        block_current_task()
    }
}
