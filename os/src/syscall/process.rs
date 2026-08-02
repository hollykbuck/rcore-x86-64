//! Process management syscalls
use crate::fs::{OpenFlags, open_file};
use crate::mm::{translated_ref, translated_refmut, translated_str};
use crate::task::{
    MAX_SIG, SignalAction, SignalFlags, current_process, current_user_token,
    exit_current_and_run_next, pid2process, suspend_current_and_run_next,
};
use crate::timer::get_time_ms;
use log::warn;

use alloc::string::String;
use alloc::vec::Vec;

/// thread exits and submit an exit code
pub fn sys_exit(exit_code: i32) -> isize {
    exit_current_and_run_next(exit_code);
    0
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    suspend_current_and_run_next();
    0
}

/// get time in milliseconds
pub fn sys_get_time() -> isize {
    get_time_ms() as isize
}

/// get the pid of the current process
pub fn sys_getpid() -> isize {
    current_process().getpid() as isize
}

/// create a child process, which is a copy of the current one
pub fn sys_fork() -> isize {
    let current_process = current_process();
    let new_task = current_process.fork();
    let new_pid = new_task.getpid();
    new_pid as isize
}

/// Maximum number of `argv` entries `sys_exec` will read. The user is supposed
/// to NUL-terminate the pointer array; without a bound, a corrupted or
/// out-of-bounds `argv` (e.g. the ch8 `usertests` harness historically did
/// not NUL-terminate its 4-entry array) would make `translated_str` spin on an
/// unterminated string.
const MAX_ARGV: usize = 32;

/// replace the current process with the program named by `path`, which is
/// loaded from the file system; `args` is a NUL-terminated array of argument
/// string pointers in the user address space.
pub fn sys_exec(path: *const u8, mut args: *const usize) -> isize {
    let token = current_user_token();
    let path = translated_str(token, path);
    let mut args_vec: Vec<String> = Vec::new();
    loop {
        let arg_str_ptr = *translated_ref(token, args);
        if arg_str_ptr == 0 {
            break;
        }
        args_vec.push(translated_str(token, arg_str_ptr as *const u8));
        unsafe {
            args = args.add(1);
        }
        if args_vec.len() >= MAX_ARGV {
            warn!("exec: argv longer than {} entries, truncating", MAX_ARGV);
            break;
        }
    }
    if let Some(app_inode) = open_file(path.as_str(), OpenFlags::RDONLY) {
        let all_data = app_inode.read_all();
        let process = current_process();
        let argc = args_vec.len();
        process.exec(all_data.as_slice(), args_vec);
        // return argc because cx.rdi will be covered with it later
        argc as isize
    } else {
        -1
    }
}

/// If there is not a child process whose pid is same as given, return -1.
/// Else if there is a child process but it is still running, return -2.
pub fn sys_waitpid(pid: isize, exit_code_ptr: *mut i32) -> isize {
    let process = current_process();
    // find a child process

    // ---- access current TCB exclusively
    let mut inner = process.inner_exclusive_access();
    if !inner
        .children
        .iter()
        .any(|p| pid == -1 || pid as usize == p.getpid())
    {
        return -1;
        // ---- release current PCB
    }
    let pair = inner.children.iter().enumerate().find(|(_, p)| {
        // ++++ temporarily access child PCB lock exclusively
        p.inner_exclusive_access().is_zombie && (pid == -1 || pid as usize == p.getpid())
        // ++++ release child PCB
    });
    if let Some((idx, _)) = pair {
        let child = inner.children.remove(idx);
        // SMP note: the child process is *not* deallocated here even though
        // this may be the last reference held by the parent: the exiting
        // main thread parks its own reference (with the still-live kernel
        // stack) on the core's `exiting_process` slot and only releases it
        // after it has switched away. So the kernel stack of the exited main
        // thread is freed by that core's idle loop, never under the exiting
        // thread itself. (single-core RISC-V asserted `strong_count == 1`
        // here, which is not valid under SMP.)
        let found_pid = child.getpid();
        // ++++ temporarily access child TCB exclusively
        let exit_code = child.inner_exclusive_access().exit_code;
        // ++++ release child PCB
        // x86-64: the trap handler runs on this task's page table, so the user
        // pointer is directly dereferenceable
        unsafe {
            *exit_code_ptr = exit_code;
        }
        found_pid as isize
    } else {
        -2
    }
    // ---- release current PCB lock automatically
}

/// Send the signal `signum` to the process `pid`, if it is legal.
pub fn sys_kill(pid: usize, signum: i32) -> isize {
    if let Some(process) = pid2process(pid) {
        if let Some(flag) = SignalFlags::from_bits(1 << signum) {
            // insert the signal if legal
            let mut process_ref = process.inner_exclusive_access();
            if process_ref.signals.contains(flag) {
                return -1;
            }
            process_ref.signals.insert(flag);
            0
        } else {
            -1
        }
    } else {
        -1
    }
}

/// Change the signal mask of the current process, returning the old mask.
pub fn sys_sigprocmask(mask: u32) -> isize {
    let process = current_process();
    let mut inner = process.inner_exclusive_access();
    let old_mask = inner.signal_mask;
    if let Some(flag) = SignalFlags::from_bits(mask) {
        inner.signal_mask = flag;
        old_mask.bits() as isize
    } else {
        -1
    }
}

/// Return from a user signal handler: restore the trap context saved when the
/// handler was entered.
///
/// x86-64 note: the syscall return value goes in `rax`, and after the trap
/// context is restored the original user code resumes, so the value returned
/// here is the one saved in `rax`.
pub fn sys_sigreturn() -> isize {
    let process = current_process();
    let mut inner = process.inner_exclusive_access();
    inner.handling_sig = -1;
    // restore the trap context
    let task = crate::task::current_task().unwrap();
    let task_inner = task.inner_exclusive_access();
    let trap_ctx = task_inner.get_trap_cx();
    *trap_ctx = inner.trap_ctx_backup.unwrap();
    // Here we return the value of rax in the trap_ctx,
    // otherwise it will be overwritten after we trap
    // back to the original execution of the application.
    trap_ctx.rax as isize
}

fn check_sigaction_error(signal: SignalFlags, action: usize, old_action: usize) -> bool {
    if action == 0
        || old_action == 0
        || signal == SignalFlags::SIGKILL
        || signal == SignalFlags::SIGSTOP
    {
        true
    } else {
        false
    }
}

/// Set the action of `signum`, returning the previous action through
/// `old_action`.
pub fn sys_sigaction(
    signum: i32,
    action: *const SignalAction,
    old_action: *mut SignalAction,
) -> isize {
    let token = current_user_token();
    let process = current_process();
    let mut inner = process.inner_exclusive_access();
    if signum as usize > MAX_SIG {
        return -1;
    }
    if let Some(flag) = SignalFlags::from_bits(1 << signum) {
        if check_sigaction_error(flag, action as usize, old_action as usize) {
            return -1;
        }
        let prev_action = inner.signal_actions.table[signum as usize];
        *translated_refmut(token, old_action) = prev_action;
        inner.signal_actions.table[signum as usize] = *translated_ref(token, action);
        0
    } else {
        -1
    }
}
