//! Semaphore implementation (kernel-level, taken from the RISC-V tutorial
//! ch8).

use crate::sync::UPSafeCell;
use crate::task::{TaskControlBlock, block_current_and_run_next, current_task, wakeup_task};
use alloc::{collections::VecDeque, sync::Arc};

/// A counting semaphore.
pub struct Semaphore {
    /// the inner state
    pub inner: UPSafeCell<SemaphoreInner>,
}

/// The inner state of a semaphore.
pub struct SemaphoreInner {
    /// the current resource count
    pub count: isize,
    /// threads waiting for a resource
    pub wait_queue: VecDeque<Arc<TaskControlBlock>>,
}

impl Semaphore {
    /// Create a new semaphore with `res_count` resources.
    pub fn new(res_count: usize) -> Self {
        Self {
            inner: unsafe {
                UPSafeCell::new(SemaphoreInner {
                    count: res_count as isize,
                    wait_queue: VecDeque::new(),
                })
            },
        }
    }

    /// Release a resource, waking up a waiter if any.
    pub fn up(&self) {
        let mut inner = self.inner.exclusive_access();
        inner.count += 1;
        if inner.count <= 0 {
            if let Some(task) = inner.wait_queue.pop_front() {
                wakeup_task(task);
            }
        }
    }

    /// Acquire a resource, blocking the current thread if none is free.
    pub fn down(&self) {
        let mut inner = self.inner.exclusive_access();
        inner.count -= 1;
        if inner.count < 0 {
            inner.wait_queue.push_back(current_task().unwrap());
            drop(inner);
            block_current_and_run_next();
        }
    }
}
