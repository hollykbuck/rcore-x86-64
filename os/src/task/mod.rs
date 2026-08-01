//! Task management implementation
//!
//! Everything about task management, like starting and switching tasks is
//! implemented here.
//!
//! A single global instance of [`TaskManager`] called `TASK_MANAGER` controls
//! all the tasks in the whole operating system.
//!
//! A single global instance of [`Processor`] called `PROCESSOR` monitors the
//! running task(s) for each core.
//!
//! A single global instance of [`PidAllocator`] called `PID_ALLOCATOR`
//! allocates pids for user apps.
//!
//! Be careful when you see `__switch` ASM function in `switch.S`. Control flow around this function
//! might not be what you expect.

mod action;
mod context;
mod manager;
mod pid;
mod processor;
mod signal;
mod switch;
#[allow(clippy::module_inception)]
mod task;

use crate::fs::{OpenFlags, open_file};
use crate::uart::shutdown;
use alloc::sync::Arc;
use lazy_static::*;
pub use manager::{TaskManager, fetch_task, pid2task, remove_from_pid2task};
use switch::__switch;
use task::{TaskControlBlock, TaskStatus};

pub use action::{SignalAction, SignalActions};
pub use context::TaskContext;
pub use manager::add_task;
pub use pid::{KernelStack, PidAllocator, PidHandle, pid_alloc};
pub use processor::{
    Processor, current_task, current_trap_cx, current_user_token, run_tasks, schedule,
    take_current_task,
};
pub use signal::{MAX_SIG, SignalFlags};

/// Suspend the current 'Running' task and run the next task in the ready queue.
pub fn suspend_current_and_run_next() {
    // There must be an application running.
    let task = take_current_task().unwrap();

    // ---- access current TCB exclusively
    let mut task_inner = task.inner_exclusive_access();
    let task_cx_ptr = &mut task_inner.task_cx as *mut TaskContext;
    // Change status to Ready
    task_inner.task_status = TaskStatus::Ready;
    drop(task_inner);
    // ---- release current PCB

    // push back to the ready queue
    add_task(task);
    // jump to the scheduling cycle
    schedule(task_cx_ptr);
}

/// pid of the usertests app when `make run TEST=1` copies it to initproc
pub const IDLE_PID: usize = 1;

/// Exit the current 'Running' task and run the next task in the ready queue.
///
/// The exiting task becomes a zombie; its user data pages are recycled but its
/// kernel stack is kept alive (the trap handler is still running on it) until
/// the parent reaps it (drops the `TaskControlBlock`).
pub fn exit_current_and_run_next(exit_code: i32) -> ! {
    // take from Processor
    let task = take_current_task().unwrap();

    let pid = task.getpid();
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

    // **** access current TCB exclusively
    let mut inner = task.inner_exclusive_access();
    // Change status to Zombie
    inner.task_status = TaskStatus::Zombie;
    // Record exit code
    inner.exit_code = exit_code;
    // do not move to its parent but under initproc

    // ++++++ access initproc TCB exclusively
    {
        let mut initproc_inner = INITPROC.inner_exclusive_access();
        for child in inner.children.iter() {
            child.inner_exclusive_access().parent = Some(Arc::downgrade(&INITPROC));
            initproc_inner.children.push(child.clone());
        }
    }
    // ++++++ release parent PCB

    inner.children.clear();
    // deallocate user space
    inner.memory_set.recycle_data_pages();
    drop(inner);
    // **** release current PCB
    // remove from the pid -> TCB map so that signals can no longer reach it
    remove_from_pid2task(pid);
    // drop task manually to maintain rc correctly
    drop(task);
    // we do not have to save the task context
    let mut _unused = TaskContext::zero_init();
    schedule(&mut _unused as *mut _);
    panic!("unreachable in exit_current_and_run_next!");
}

lazy_static! {
    /// The global process that init user shell
    pub static ref INITPROC: Arc<TaskControlBlock> = Arc::new({
        let inode = open_file("initproc", OpenFlags::RDONLY).unwrap();
        let v = inode.read_all();
        TaskControlBlock::new(v.as_slice())
    });
}

/// Add init process to the manager
pub fn add_initproc() {
    add_task(INITPROC.clone());
}

/// If the current task has a signal whose default action is to terminate it,
/// return the (exit code, message) pair so the trap handler can kill it.
pub fn check_signals_error_of_current() -> Option<(i32, &'static str)> {
    let task = current_task().unwrap();
    let task_inner = task.inner_exclusive_access();
    task_inner.signals.check_error()
}

/// Add a signal to the current task's pending set.
pub fn current_add_signal(signal: SignalFlags) {
    let task = current_task().unwrap();
    let mut task_inner = task.inner_exclusive_access();
    task_inner.signals |= signal;
}

/// The kernel's own handling of the "kernel" signals: SIGSTOP freezes the
/// task, SIGCONT unfreezes it, everything else (SIGKILL/SIGDEF) kills it.
fn call_kernel_signal_handler(signal: SignalFlags) {
    let task = current_task().unwrap();
    let mut task_inner = task.inner_exclusive_access();
    match signal {
        SignalFlags::SIGSTOP => {
            task_inner.frozen = true;
            task_inner.signals ^= SignalFlags::SIGSTOP;
        }
        SignalFlags::SIGCONT => {
            if task_inner.signals.contains(SignalFlags::SIGCONT) {
                task_inner.signals ^= SignalFlags::SIGCONT;
                task_inner.frozen = false;
            }
        }
        _ => {
            task_inner.killed = true;
        }
    }
}

/// Dispatch `signal` to the user's registered handler.
///
/// x86-64 note: instead of the RISC-V `sepc`/`x[10]`, the trap context's `rip`
/// is overwritten with the handler address (the `iretq` pops `RIP` from the
/// frame) and the signal number is passed as the first SysV argument `rdi`.
/// `sigreturn` will restore the backup context.
fn call_user_signal_handler(sig: usize, signal: SignalFlags) {
    let task = current_task().unwrap();
    let mut task_inner = task.inner_exclusive_access();

    let handler = task_inner.signal_actions.table[sig].handler;
    if handler != 0 {
        // user handler
        task_inner.handling_sig = sig as isize;
        task_inner.signals ^= signal;

        // backup the current trap context
        let trap_ctx = task_inner.get_trap_cx();
        task_inner.trap_ctx_backup = Some(*trap_ctx);

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
        let task = current_task().unwrap();
        let task_inner = task.inner_exclusive_access();
        let signal = SignalFlags::from_bits(1 << sig).unwrap();
        if task_inner.signals.contains(signal) && (!task_inner.signal_mask.contains(signal)) {
            let mut masked = true;
            let handling_sig = task_inner.handling_sig;
            if handling_sig == -1 {
                masked = false;
            } else {
                let handling_sig = handling_sig as usize;
                if !task_inner.signal_actions.table[handling_sig]
                    .mask
                    .contains(signal)
                {
                    masked = false;
                }
            }
            if !masked {
                drop(task_inner);
                drop(task);
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
/// task frozen by SIGSTOP stays suspended until SIGCONT wakes it up.
pub fn handle_signals() {
    loop {
        check_pending_signals();
        let (frozen, killed) = {
            let task = current_task().unwrap();
            let task_inner = task.inner_exclusive_access();
            (task_inner.frozen, task_inner.killed)
        };
        if !frozen || killed {
            break;
        }
        suspend_current_and_run_next();
    }
}
