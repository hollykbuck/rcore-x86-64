#![no_std]

#[macro_use]
pub mod console;
mod lang_items;
mod syscall;

use buddy_system_allocator::LockedHeap;
use core::ptr::addr_of_mut;
use syscall::*;

unsafe extern "Rust" {
    fn main() -> i32;
}

const USER_HEAP_SIZE: usize = 16384;

static mut HEAP_SPACE: [u8; USER_HEAP_SIZE] = [0; USER_HEAP_SIZE];

#[global_allocator]
static HEAP: LockedHeap = LockedHeap::empty();

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.entry")]
pub extern "C" fn _start() -> ! {
    unsafe {
        HEAP.lock()
            .init(addr_of_mut!(HEAP_SPACE) as usize, USER_HEAP_SIZE);
    }
    unsafe {
        exit(main());
    }
}

pub fn read(fd: usize, buf: &mut [u8]) -> isize {
    sys_read(fd, buf)
}
pub fn write(fd: usize, buf: &[u8]) -> isize {
    sys_write(fd, buf)
}
pub fn exit(exit_code: i32) -> ! {
    sys_exit(exit_code);
}
pub fn yield_() -> isize {
    sys_yield()
}
pub fn get_time() -> isize {
    sys_get_time()
}
pub fn getpid() -> isize {
    sys_getpid()
}
pub fn fork() -> isize {
    sys_fork()
}
pub fn exec(path: &str) -> isize {
    sys_exec(path)
}
pub fn wait(exit_code: &mut i32) -> isize {
    loop {
        match sys_waitpid(-1, exit_code as *mut _) {
            -2 => {
                yield_();
            }
            // -1 or a real pid
            exit_pid => return exit_pid,
        }
    }
}

pub fn waitpid(pid: usize, exit_code: &mut i32) -> isize {
    loop {
        match sys_waitpid(pid as isize, exit_code as *mut _) {
            -2 => {
                yield_();
            }
            // -1 or a real pid
            exit_pid => return exit_pid,
        }
    }
}

pub fn sleep(period_ms: usize) {
    let start = get_time();
    while get_time() < start + period_ms as isize {
        yield_();
    }
}
