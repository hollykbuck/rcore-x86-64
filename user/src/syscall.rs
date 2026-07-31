use core::arch::asm;

const SYSCALL_WRITE: usize = 1;
const SYSCALL_EXIT: usize = 60;
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

pub fn sys_write(fd: usize, buffer: &[u8]) -> isize {
    syscall(SYSCALL_WRITE, [fd, buffer.as_ptr() as usize, buffer.len()])
}

pub fn sys_exit(exit_code: i32) -> isize {
    syscall(SYSCALL_EXIT, [exit_code as usize, 0, 0])
}

pub fn sys_yield() -> isize {
    syscall(SYSCALL_YIELD, [0, 0, 0])
}

pub fn sys_get_time() -> isize {
    syscall(SYSCALL_GET_TIME, [0, 0, 0])
}
