//! Implementation of syscalls
//!
//! The single entry point to all system calls, [`syscall()`], is called
//! whenever userspace wishes to perform a system call using the `syscall`
//! instruction. In this case, the processor jumps to the entry pointed to by
//! the `LSTAR` MSR, which is handled as one of the cases in
//! [`crate::trap::trap_handler`].
//!
//! For clarity, each single syscall is implemented as its own function, named
//! `sys_` then the name of the syscall. You can find functions like this in
//! submodules, and you should also implement syscalls this way.

// Syscall numbers follow the Linux x86-64 convention where one exists.
const SYSCALL_READ: usize = 0;
const SYSCALL_WRITE: usize = 1;
const SYSCALL_GETPID: usize = 39;
const SYSCALL_FORK: usize = 57;
const SYSCALL_EXEC: usize = 59;
const SYSCALL_EXIT: usize = 60;
const SYSCALL_WAITPID: usize = 61;
const SYSCALL_YIELD: usize = 24;
/// `get_time` has no direct Linux x86-64 counterpart, so a teaching number is
/// used here (201 happens to be Linux's `time`; we return milliseconds).
const SYSCALL_GET_TIME: usize = 201;

mod fs;
mod process;

use fs::*;
use process::*;

/// handle syscall exception with `syscall_id` and other arguments
pub fn syscall(syscall_id: usize, args: [usize; 3]) -> isize {
    match syscall_id {
        SYSCALL_READ => sys_read(args[0], args[1] as *const u8, args[2]),
        SYSCALL_WRITE => sys_write(args[0], args[1] as *const u8, args[2]),
        SYSCALL_GETPID => sys_getpid(),
        SYSCALL_FORK => sys_fork(),
        SYSCALL_EXEC => sys_exec(args[0] as *const u8),
        SYSCALL_EXIT => sys_exit(args[0] as i32),
        SYSCALL_WAITPID => sys_waitpid(args[0] as isize, args[1] as *mut i32),
        SYSCALL_YIELD => sys_yield(),
        SYSCALL_GET_TIME => sys_get_time(),
        _ => panic!("Unsupported syscall_id: {}", syscall_id),
    }
}
