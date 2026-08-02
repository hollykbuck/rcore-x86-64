//! Task management implementation
//!
//! Everything about task management, like starting and switching tasks is
//! implemented here.
//!
//! A single global instance of [`TaskManager`] called `TASK_MANAGER` controls
//! all the threads in the whole operating system.
//!
//! A single global instance of [`Processor`] called `PROCESSOR` monitors the
//! running thread(s) for each core.
//!
//! A single global instance of [`PidAllocator`] called `PID_ALLOCATOR`
//! allocates pids for user apps.
//!
//! Be careful when you see `__switch` ASM function in `switch.S`. Control flow around this function
//! might not be what you expect.

mod action;
mod context;
mod id;
mod manager;
mod pid;
mod process;
mod processor;
mod signal;
mod switch;
#[allow(clippy::module_inception)]
mod task;

use crate::fs::{OpenFlags, open_file};
use crate::uart::shutdown;
use alloc::sync::Arc;
use alloc::vec::Vec;
use lazy_static::*;
pub use manager::{
    add_task, fetch_task, insert_into_pid2process, pid2process, remove_from_pid2process,
    remove_task, wakeup_task,
};
use switch::__switch;

pub use action::{SignalAction, SignalActions};
pub use context::TaskContext;
pub use id::{KernelStack, RecycleAllocator, TaskUserRes, kstack_alloc};
pub use manager::TaskManager;
pub use pid::{PidAllocator, PidHandle, pid_alloc};
pub use process::ProcessControlBlock;
pub use processor::{
    Processor, current_process, current_task, current_trap_cx, current_user_token, run_tasks,
    schedule, take_current_task,
};
pub use signal::{MAX_SIG, SignalFlags};
pub use task::{TaskControlBlock, TaskStatus};

/// The pid of the init process
pub const IDLE_PID: usize = 1;

/// Suspend the current 'Running' thread and run the next thread in the ready queue.
pub fn suspend_current_and_run_next() {
    // There must be an application running.
    let task = take_current_task().unwrap();

    // ---- access current TCB exclusively
    let mut task_inner = task.inner_exclusive_access();
    let task_cx_ptr = &mut task_inner.task_cx as *mut TaskContext;
    // Change status to Ready
    task_inner.task_status = TaskStatus::Ready;
    drop(task_inner);
    // ---- release current TCB

    // push back to ready queue.
    add_task(task);
    // jump to the scheduling cycle
    schedule(task_cx_ptr);
}

/// Block the current 'Running' thread and run the next thread in the ready queue.
pub fn block_current_and_run_next() {
    let task = take_current_task().unwrap();
    let mut task_inner = task.inner_exclusive_access();
    let task_cx_ptr = &mut task_inner.task_cx as *mut TaskContext;
    task_inner.task_status = TaskStatus::Blocked;
    drop(task_inner);
    schedule(task_cx_ptr);
}

/// Exit the current 'Running' thread and run the next thread.
///
/// The exiting thread becomes a zombie (its `exit_code` is recorded, its user
/// resources are freed); the process terminates at once if this was its main
/// thread.
pub fn exit_current_and_run_next(exit_code: i32) {
    let task = take_current_task().unwrap();
    let mut task_inner = task.inner_exclusive_access();
    let process = task.process.upgrade().unwrap();
    let tid = task_inner.res.as_ref().unwrap().tid;
    // record exit code
    task_inner.exit_code = Some(exit_code);
    task_inner.res = None;
    // here we do not remove the thread since we are still using the kstack
    // it will be deallocated when sys_waittid is called
    drop(task_inner);
    drop(task);
    // however, if this is the main thread of current process
    // the process should terminate at once
    if tid == 0 {
        let pid = process.getpid();
        if pid == IDLE_PID {
            println!(
                "[kernel] Idle process exit with exit_code {} ...",
                exit_code
            );
            if exit_code != 0 {
                shutdown(true)
            } else {
                shutdown(false)
            }
        }
        remove_from_pid2process(pid);
        let mut process_inner = process.inner_exclusive_access();
        // mark this process as a zombie process
        process_inner.is_zombie = true;
        // record exit code of main process
        process_inner.exit_code = exit_code;

        {
            // move all child processes under init process
            let mut initproc_inner = INITPROC.inner_exclusive_access();
            for child in process_inner.children.iter() {
                child.inner_exclusive_access().parent = Some(Arc::downgrade(&INITPROC));
                initproc_inner.children.push(child.clone());
            }
        }

        // deallocate user res (including tid/ustack) of all threads
        // it has to be done before we dealloc the whole memory_set
        // otherwise they will be deallocated twice
        let mut recycle_res = Vec::<TaskUserRes>::new();
        for task in process_inner.tasks.iter().filter(|t| t.is_some()) {
            let task = task.as_ref().unwrap();
            // if other tasks are Ready in TaskManager or waiting for a timer
            // to be expired, we should remove them.
            remove_inactive_task(Arc::clone(&task));
            let mut task_inner = task.inner_exclusive_access();
            if let Some(res) = task_inner.res.take() {
                recycle_res.push(res);
            }
        }
        // dealloc_tid and dealloc_user_res require access to PCB inner, so we
        // need to collect those user res first, then release process_inner
        // for now to avoid deadlock/double borrow problem.
        drop(process_inner);
        recycle_res.clear();

        let mut process_inner = process.inner_exclusive_access();
        process_inner.children.clear();
        // deallocate other data in user space i.e. program code/data section
        process_inner.memory_set.recycle_data_pages();
        // drop file descriptors
        process_inner.fd_table.clear();
        // Remove all tasks except for the main thread itself.
        // This is because we are still using the kstack under the TCB
        // of the main thread. This TCB, including its kstack, will be
        // deallocated when the process is reaped via waitpid.
        while process_inner.tasks.len() > 1 {
            process_inner.tasks.pop();
        }
    }
    drop(process);
    // we do not have to save task context
    let mut _unused = TaskContext::zero_init();
    schedule(&mut _unused as *mut _);
}

/// Remove a thread from the ready queue and the timer queue (used when the
/// process terminates while some of its threads are still inactive).
pub fn remove_inactive_task(task: Arc<TaskControlBlock>) {
    remove_task(Arc::clone(&task));
    crate::timer::remove_timer(Arc::clone(&task));
}

lazy_static! {
    /// The global process that init user shell
    pub static ref INITPROC: Arc<ProcessControlBlock> = {
        let inode = open_file("initproc", OpenFlags::RDONLY).unwrap();
        let v = inode.read_all();
        ProcessControlBlock::new(v.as_slice())
    };
}

/// Add init process to the manager
pub fn add_initproc() {
    let _initproc = INITPROC.clone();
}

/// If the current process has a signal whose default action is to terminate
/// it, return the (exit code, message) pair so the trap handler can kill it.
pub fn check_signals_error_of_current() -> Option<(i32, &'static str)> {
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    process_inner.signals.check_error()
}

/// Add a signal to the current process's pending set.
pub fn current_add_signal(signal: SignalFlags) {
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    process_inner.signals |= signal;
}

/// The kernel's own handling of the "kernel" signals: SIGSTOP freezes the
/// process, SIGCONT unfreezes it, everything else (SIGKILL/SIGDEF) kills it.
fn call_kernel_signal_handler(signal: SignalFlags) {
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    match signal {
        SignalFlags::SIGSTOP => {
            process_inner.frozen = true;
            process_inner.signals ^= SignalFlags::SIGSTOP;
        }
        SignalFlags::SIGCONT => {
            if process_inner.signals.contains(SignalFlags::SIGCONT) {
                process_inner.signals ^= SignalFlags::SIGCONT;
                process_inner.frozen = false;
            }
        }
        _ => {
            process_inner.killed = true;
        }
    }
}

/// Dispatch `signal` to the user's registered handler on the current thread.
///
/// x86-64 note: the trap context's `rip` is overwritten with the handler
/// address (the `iretq` pops `RIP` from the frame) and the signal number is
/// passed as the first SysV argument `rdi`. `sigreturn` restores the backup.
fn call_user_signal_handler(sig: usize, signal: SignalFlags) {
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();

    let handler = process_inner.signal_actions.table[sig].handler;
    if handler != 0 {
        // user handler
        process_inner.handling_sig = sig as isize;
        process_inner.signals ^= signal;

        // backup the current trap context
        let task = current_task().unwrap();
        let task_inner = task.inner_exclusive_access();
        let trap_ctx = task_inner.get_trap_cx();
        process_inner.trap_ctx_backup = Some(*trap_ctx);

        // jump to the handler on return to user mode
        trap_ctx.rip = handler;
        // pass the signal number as the first argument
        trap_ctx.rdi = sig;
    } else {
        println!("[K] task/call_user_signal_handler: default action: ignore it or kill process");
    }
}

fn check_pending_signals() {
    for sig in 0..(MAX_SIG + 1) {
        let process = current_process();
        let process_inner = process.inner_exclusive_access();
        let signal = SignalFlags::from_bits(1 << sig).unwrap();
        if process_inner.signals.contains(signal) && (!process_inner.signal_mask.contains(signal))
        {
            let mut masked = true;
            let handling_sig = process_inner.handling_sig;
            if handling_sig == -1 {
                masked = false;
            } else {
                let handling_sig = handling_sig as usize;
                if !process_inner.signal_actions.table[handling_sig]
                    .mask
                    .contains(signal)
                {
                    masked = false;
                }
            }
            if !masked {
                drop(process_inner);
                drop(process);
                if signal == SignalFlags::SIGKILL
                    || signal == SignalFlags::SIGSTOP
                    || signal == SignalFlags::SIGCONT
                    || signal == SignalFlags::SIGDEF
                {
                    // signal is a kernel signal
                    call_kernel_signal_handler(signal);
                } else {
                    // signal is a user signal: run the handler and return
                    call_user_signal_handler(sig, signal);
                    return;
                }
            }
        }
    }
}

/// Called at the end of the trap handler: run any pending signal handlers. A
/// process frozen by SIGSTOP stays suspended until SIGCONT wakes it up.
pub fn handle_signals() {
    loop {
        check_pending_signals();
        let (frozen, killed) = {
            let process = current_process();
            let process_inner = process.inner_exclusive_access();
            (process_inner.frozen, process_inner.killed)
        };
        if !frozen || killed {
            break;
        }
        suspend_current_and_run_next();
    }
}
