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

// Syscall numbers follow the Linux x86-64 convention where one exists
// (write = 1, exit = 60, sched_yield = 24).
const SYSCALL_WRITE: usize = 1;
const SYSCALL_EXIT: usize = 60;
const SYSCALL_YIELD: usize = 24;
/// `get_time` has no direct Linux x86-64 counterpart, so a teaching number is
/// used here (201 happens to be Linux's `time`; we return milliseconds).
const SYSCALL_GET_TIME: usize = 201;
/// `sbrk` keeps the tutorial's teaching number (214, Linux x86-64's `brk`)
const SYSCALL_SBRK: usize = 214;

mod fs;
mod process;

use fs::*;
use process::*;

/// handle syscall exception with `syscall_id` and other arguments
pub fn syscall(syscall_id: usize, args: [usize; 3]) -> isize {
    match syscall_id {
        SYSCALL_WRITE => sys_write(args[0], args[1] as *const u8, args[2]),
        SYSCALL_EXIT => sys_exit(args[0] as i32),
        SYSCALL_YIELD => sys_yield(),
        SYSCALL_GET_TIME => sys_get_time(),
        SYSCALL_SBRK => sys_sbrk(args[0] as i32),
        _ => panic!("Unsupported syscall_id: {}", syscall_id),
    }
}
