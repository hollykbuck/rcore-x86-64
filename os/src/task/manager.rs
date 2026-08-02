//! Implementation of [`TaskManager`] and the pid -> process table.
use super::{ProcessControlBlock, TaskControlBlock, TaskStatus};
use crate::sync::UPSafeCell;
use alloc::collections::{BTreeMap, VecDeque};
use alloc::sync::Arc;
use lazy_static::*;

/// A ready queue of `TaskControlBlock`s.
pub struct TaskManager {
    ready_queue: VecDeque<Arc<TaskControlBlock>>,
}

/// A simple FIFO scheduler.
impl TaskManager {
    /// Create an empty `TaskManager`.
    pub fn new() -> Self {
        Self {
            ready_queue: VecDeque::new(),
        }
    }
    /// Add a thread to the ready queue.
    pub fn add(&mut self, task: Arc<TaskControlBlock>) {
        self.ready_queue.push_back(task);
    }
    /// Remove a thread from the ready queue.
    pub fn remove(&mut self, task: Arc<TaskControlBlock>) {
        if let Some((id, _)) = self
            .ready_queue
            .iter()
            .enumerate()
            .find(|(_, t)| Arc::as_ptr(t) == Arc::as_ptr(&task))
        {
            self.ready_queue.remove(id);
        }
    }
    /// Remove the first thread and return it, or `None` if the queue is empty.
    pub fn fetch(&mut self) -> Option<Arc<TaskControlBlock>> {
        self.ready_queue.pop_front()
    }
}

lazy_static! {
    pub static ref TASK_MANAGER: UPSafeCell<TaskManager> =
        unsafe { UPSafeCell::new(TaskManager::new()) };
    /// a pid -> process map, used to send signals and reap a specific process
    pub static ref PID2PROCESS: UPSafeCell<BTreeMap<usize, Arc<ProcessControlBlock>>> =
        unsafe { UPSafeCell::new(BTreeMap::new()) };
}

/// Interface offered to add a thread.
pub fn add_task(task: Arc<TaskControlBlock>) {
    TASK_MANAGER.exclusive_access().add(task);
}

/// Wake up a blocked thread: mark it ready and put it back in the queue.
///
/// SMP: the thread is not added to the global ready queue directly. It is
/// parked on the pending queue of its `last_cpu` (the core that owns its
/// context save); that core's idle loop moves it to the ready queue once the
/// context has been saved by `__switch`. This closes the race where a woken
/// thread is resumed by another core before its block path finished saving
/// its context.
pub fn wakeup_task(task: Arc<TaskControlBlock>) {
    let mut task_inner = task.inner_exclusive_access();
    task_inner.task_status = TaskStatus::Ready;
    drop(task_inner);
    let owner = task.last_cpu.load(core::sync::atomic::Ordering::Relaxed);
    super::processor::park_pending(owner, task);
}

/// Remove a thread from the ready queue (used when a process terminates).
pub fn remove_task(task: Arc<TaskControlBlock>) {
    TASK_MANAGER.exclusive_access().remove(task);
}

/// Interface offered to pop the first thread.
pub fn fetch_task() -> Option<Arc<TaskControlBlock>> {
    TASK_MANAGER.exclusive_access().fetch()
}

/// Look up a process by pid.
pub fn pid2process(pid: usize) -> Option<Arc<ProcessControlBlock>> {
    let map = PID2PROCESS.exclusive_access();
    map.get(&pid).map(Arc::clone)
}

/// Insert a process into the pid -> process map.
pub fn insert_into_pid2process(pid: usize, process: Arc<ProcessControlBlock>) {
    PID2PROCESS.exclusive_access().insert(pid, process);
}

/// Remove a process from the pid -> process map (when it is reaped).
pub fn remove_from_pid2process(pid: usize) {
    let mut map = PID2PROCESS.exclusive_access();
    if map.remove(&pid).is_none() {
        panic!("cannot find pid {} in pid2process!", pid);
    }
}
