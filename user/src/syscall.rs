use core::arch::asm;

use crate::SignalAction;

// Linux x86-64 syscall numbers.
const SYSCALL_READ: usize = 0;
const SYSCALL_WRITE: usize = 1;
const SYSCALL_OPEN: usize = 2;
const SYSCALL_CLOSE: usize = 3;
const SYSCALL_RT_SIGACTION: usize = 13;
const SYSCALL_RT_SIGPROCMASK: usize = 14;
const SYSCALL_RT_SIGRETURN: usize = 15;
const SYSCALL_PIPE: usize = 22;
const SYSCALL_YIELD: usize = 24;
const SYSCALL_DUP: usize = 32;
const SYSCALL_GETPID: usize = 39;
const SYSCALL_FORK: usize = 57;
const SYSCALL_EXEC: usize = 59;
const SYSCALL_EXIT: usize = 60;
const SYSCALL_WAITPID: usize = 61;
const SYSCALL_KILL: usize = 62;
const SYSCALL_GET_TIME: usize = 201;

fn syscall(id: usize, args: [usize; 3]) -> isize {
    let mut ret: isize;
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") id => ret,
            in("rdi") args[0],
            in("rsi") args[1],
            in("rdx") args[2],
            // the `syscall` instruction clobbers RCX (user RIP) and R11
            // (user RFLAGS); the compiler must know about it
            out("rcx") _,
            out("r11") _,
        );
    }
    ret
}

pub fn sys_read(fd: usize, buffer: &mut [u8]) -> isize {
    syscall(
        SYSCALL_READ,
        [fd, buffer.as_mut_ptr() as usize, buffer.len()],
    )
}

pub fn sys_write(fd: usize, buffer: &[u8]) -> isize {
    syscall(SYSCALL_WRITE, [fd, buffer.as_ptr() as usize, buffer.len()])
}

pub fn sys_open(path: &str, flags: u32) -> isize {
    syscall(SYSCALL_OPEN, [path.as_ptr() as usize, flags as usize, 0])
}

pub fn sys_close(fd: usize) -> isize {
    syscall(SYSCALL_CLOSE, [fd, 0, 0])
}

pub fn sys_pipe(pipe: &mut [usize]) -> isize {
    syscall(SYSCALL_PIPE, [pipe.as_mut_ptr() as usize, 0, 0])
}

pub fn sys_dup(fd: usize) -> isize {
    syscall(SYSCALL_DUP, [fd, 0, 0])
}

pub fn sys_exit(exit_code: i32) -> ! {
    syscall(SYSCALL_EXIT, [exit_code as usize, 0, 0]);
    panic!("sys_exit never returns!");
}

pub fn sys_yield() -> isize {
    syscall(SYSCALL_YIELD, [0, 0, 0])
}

pub fn sys_get_time() -> isize {
    syscall(SYSCALL_GET_TIME, [0, 0, 0])
}

pub fn sys_getpid() -> isize {
    syscall(SYSCALL_GETPID, [0, 0, 0])
}

pub fn sys_fork() -> isize {
    syscall(SYSCALL_FORK, [0, 0, 0])
}

pub fn sys_exec(path: &str, args: &[*const u8]) -> isize {
    syscall(SYSCALL_EXEC, [path.as_ptr() as usize, args.as_ptr() as usize, 0])
}

pub fn sys_waitpid(pid: isize, exit_code: *mut i32) -> isize {
    syscall(SYSCALL_WAITPID, [pid as usize, exit_code as usize, 0])
}

pub fn sys_kill(pid: usize, signal: i32) -> isize {
    syscall(SYSCALL_KILL, [pid, signal as usize, 0])
}

pub fn sys_sigaction(
    signum: i32,
    action: *const SignalAction,
    old_action: *mut SignalAction,
) -> isize {
    syscall(
        SYSCALL_RT_SIGACTION,
        [signum as usize, action as usize, old_action as usize],
    )
}

pub fn sys_sigprocmask(mask: u32) -> isize {
    syscall(SYSCALL_RT_SIGPROCMASK, [mask as usize, 0, 0])
}

pub fn sys_sigreturn() -> isize {
    syscall(SYSCALL_RT_SIGRETURN, [0, 0, 0])
}
