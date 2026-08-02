//! Implementation of [`PidAllocator`] and [`PidHandle`].
//!
//! The per-thread kernel stacks and the per-thread user resources live in
//! `task/id.rs` since ch8 (threads of one process share the pid but each has
//! its own kernel stack and user stack).

use crate::sync::UPIntrFreeCell;
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
    pub static ref PID_ALLOCATOR: UPIntrFreeCell<PidAllocator> =
        unsafe { UPIntrFreeCell::new(PidAllocator::new()) };
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
