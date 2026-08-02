//! Condition variable implementation (kernel-level, taken from the RISC-V
//! tutorial ch8).

use crate::sync::{Mutex, UPSafeCell};
use crate::task::{TaskControlBlock, block_current_and_run_next, current_task, wakeup_task};
use alloc::{collections::VecDeque, sync::Arc};

/// A condition variable.
pub struct Condvar {
    /// the inner state
    pub inner: UPSafeCell<CondvarInner>,
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
                UPSafeCell::new(CondvarInner {
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
}
