use core::arch::asm;

// Linux x86-64 syscall numbers.
const SYSCALL_READ: usize = 0;
const SYSCALL_WRITE: usize = 1;
const SYSCALL_GETPID: usize = 39;
const SYSCALL_FORK: usize = 57;
const SYSCALL_EXEC: usize = 59;
const SYSCALL_EXIT: usize = 60;
const SYSCALL_WAITPID: usize = 61;
const SYSCALL_YIELD: usize = 24;
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

pub fn sys_exec(path: &str) -> isize {
    syscall(SYSCALL_EXEC, [path.as_ptr() as usize, 0, 0])
}

pub fn sys_waitpid(pid: isize, exit_code: *mut i32) -> isize {
    syscall(SYSCALL_WAITPID, [pid as usize, exit_code as usize, 0])
}
