//! Implementation of [`Processor`] and the intersection of control flow.
//!
//! The scheduler is an idle control flow (`run_tasks` loop) that fetches
//! ready threads and `__switch`es to them; `schedule` switches back to the
//! idle control flow when the current thread suspends or exits. Since ch8
//! stage 3 there is **one `Processor` per core** (in the `PerCpu` slot); the
//! ready queue is global and shared. On x86-64, every switch to a thread must
//! (1) activate its process's page table (`mov cr3`) and (2) point the trap
//! kernel stack (`TSS.rsp0` + `current_stack_top`) at its kernel stack,
//! because the CPU does not switch address spaces on trap.

use super::__switch;
use super::{ProcessControlBlock, TaskContext, TaskControlBlock};
use super::{TaskStatus, add_task, fetch_task};
use crate::cpu::{current_per_cpu, current_per_cpu_mut};
use crate::sync::UPSafeCell;
use crate::trap::{TrapContext, set_current_stack_top};
use alloc::collections::VecDeque;
use alloc::sync::Arc;

/// Processor management structure (one per core).
pub struct Processor {
    /// The thread currently executing on the current processor
    current: Option<Arc<TaskControlBlock>>,
    /// The basic control flow of each core, helping to select and switch threads
    idle_task_cx: TaskContext,
    /// Threads that must be put back into the global ready queue only *after*
    /// their context has been saved by `__switch`. Threads that yielded on
    /// this core are parked here by themselves; threads that were blocked on
    /// some synchronization primitive are parked here by their waker on their
    /// `last_cpu` (the core that owns their context save). The idle loop
    /// drains this before fetching, which prevents another core from resuming
    /// a thread whose context has not been saved yet (SMP race). It is a
    /// spinlock because a waker on another core may push to it.
    pub pending_requeue: UPSafeCell<VecDeque<Arc<TaskControlBlock>>>,
    /// A process whose main thread just exited on this core. The exiting
    /// thread's kernel stack is freed when the last process reference drops,
    /// but the exit path itself still runs on that stack until `__switch` has
    /// switched to the idle loop. The exit path therefore parks the process
    /// here and the idle loop drops it *after* the switch, so another core
    /// (a `waitpid` reaping the zombie) can never free the stack under the
    /// exiting thread (SMP race, see ch8 stage 3).
    pub exiting_process: Option<Arc<ProcessControlBlock>>,
}

impl Processor {
    /// Create an empty `Processor`.
    pub fn new() -> Self {
        Self {
            current: None,
            idle_task_cx: TaskContext::zero_init(),
            pending_requeue: unsafe { UPSafeCell::new(VecDeque::new()) },
            exiting_process: None,
        }
    }
    /// Get mutable reference to `idle_task_cx`.
    fn get_idle_task_cx_ptr(&mut self) -> *mut TaskContext {
        &mut self.idle_task_cx as *mut _
    }
    /// Get current thread in moving semantics.
    pub fn take_current(&mut self) -> Option<Arc<TaskControlBlock>> {
        self.current.take()
    }
    /// Get current thread in cloning semantics.
    pub fn current(&self) -> Option<Arc<TaskControlBlock>> {
        self.current.as_ref().map(Arc::clone)
    }
}

/// Park `task` on the pending queue of processor `cpu_id`, the owner of the
/// thread's context save. Only that processor's idle loop drains it, once the
/// context has been saved, so the thread never becomes globally runnable
/// before it is safe to resume.
pub fn park_pending(cpu_id: usize, task: Arc<TaskControlBlock>) {
    crate::cpu::per_cpu(cpu_id)
        .processor
        .pending_requeue
        .exclusive_access()
        .push_back(task);
}

/// Remove `task` from every processor's pending queue (used when its process
/// terminates while the thread is parked there).
pub fn remove_from_all_pending(task: Arc<TaskControlBlock>) {
    for cpu_id in 0..crate::config::NCPU {
        crate::cpu::per_cpu(cpu_id)
            .processor
            .pending_requeue
            .exclusive_access()
            .retain(|t| Arc::as_ptr(t) != Arc::as_ptr(&task));
    }
}

/// The main part of thread execution and scheduling (run on every core).
///
/// Loop `fetch_task` to get the thread that needs to run, and switch to it
/// through `__switch`. The idle control flow runs on this core's boot stack;
/// when there is nothing to run, the core halts (`sti; hlt; cli`) until a
/// timer interrupt wakes it.
pub fn run_tasks() -> ! {
    loop {
        // SMP: activate the kernel page table before doing anything else in
        // the idle loop. A core reaches this loop with CR3 still pointing at
        // the page table of whatever task it last ran; if that task's process
        // exited (its `exiting_process` drop below frees the table), CR3 would
        // dangle into freed/reused frames and any kernel access (e.g. the idle
        // timer's LAPIC EOI) would fault or corrupt memory.
        crate::mm::KERNEL_SPACE.exclusive_access().activate();
        // point the trap kernel stack at a stable kernel stack (we are no
        // longer running a task whose stack may be freed)
        set_current_stack_top(crate::trap::bsp_trap_stack_top());
        // drop a process whose main thread exited on this core: only now (we
        // reached the idle loop through `__switch`, and CR3 is on the kernel
        // page table) is the exiting thread's kernel stack and page table no
        // longer in use, so the final reference may release them
        if let Some(exiting) = current_per_cpu_mut().processor.exiting_process.take() {
            drop(exiting);
        }
        // put back the threads parked on this core, whose contexts have now
        // been saved (we reached the idle loop through `__switch`)
        while let Some(task) = current_per_cpu_mut()
            .processor
            .pending_requeue
            .exclusive_access()
            .pop_front()
        {
            add_task(task);
        }
        if let Some(task) = fetch_task() {
            let idle_task_cx_ptr = current_per_cpu_mut().processor.get_idle_task_cx_ptr();
            // access the coming task TCB exclusively
            let mut task_inner = task.inner_exclusive_access();
            let next_task_cx_ptr = &task_inner.task_cx as *const TaskContext;
            task_inner.task_status = TaskStatus::Running;
            drop(task_inner);
            // x86-64: activate the process's page table before switching to it
            // (this is the only place CR3 changes; traps never switch it).
            let process = task.process.upgrade().unwrap();
            process.inner_exclusive_access().memory_set.activate();
            // x86-64: point the trap kernel stack at the thread's own stack so
            // its first user-mode trap lands on the right stack.
            set_current_stack_top(task.kstack.get_top() as u64);
            // this core now owns the thread's context save
            task.last_cpu.store(crate::cpu::current_cpu_id(), core::sync::atomic::Ordering::Relaxed);
            // release the coming task TCB manually
            current_per_cpu_mut().processor.current = Some(task);
            unsafe {
                __switch(idle_task_cx_ptr, next_task_cx_ptr);
            }
        } else {
            // idle: halt until an interrupt (the APIC timer) wakes us up
            unsafe {
                core::arch::asm!("sti", "hlt", "cli", options(nostack));
            }
        }
    }
}

/// Take the current thread, leaving a `None` in its place.
pub fn take_current_task() -> Option<Arc<TaskControlBlock>> {
    current_per_cpu_mut().processor.take_current()
}

/// Get the running thread.
pub fn current_task() -> Option<Arc<TaskControlBlock>> {
    current_per_cpu().processor.current()
}

/// Get the running process (the process of the current thread).
pub fn current_process() -> Arc<ProcessControlBlock> {
    current_task().unwrap().process.upgrade().unwrap()
}

/// Get the token of the address space of the current process.
pub fn current_user_token() -> usize {
    current_task().unwrap().get_user_token()
}

/// Get the mutable reference to the trap context of the current thread.
pub fn current_trap_cx() -> &'static mut TrapContext {
    current_task()
        .unwrap()
        .inner_exclusive_access()
        .get_trap_cx()
}

/// Return to the idle control flow for new scheduling.
pub fn schedule(switched_task_cx_ptr: *mut TaskContext) {
    let idle_task_cx_ptr = current_per_cpu_mut().processor.get_idle_task_cx_ptr();
    unsafe {
        __switch(switched_task_cx_ptr, idle_task_cx_ptr);
    }
}
