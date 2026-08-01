//! File and filesystem-related syscalls

use crate::task::suspend_current_and_run_next;

const FD_STDIN: usize = 0;
const FD_STDOUT: usize = 1;

/// write buf of length `len` to a file with `fd`
pub fn sys_write(fd: usize, buf: *const u8, len: usize) -> isize {
    match fd {
        FD_STDOUT => {
            // the trap handler runs on the current task's own page table, so
            // the user pointer is directly dereferenceable
            let slice = unsafe { core::slice::from_raw_parts(buf, len) };
            let str = core::str::from_utf8(slice).unwrap();
            print!("{}", str);
            len as isize
        }
        _ => {
            panic!("Unsupported fd in sys_write!");
        }
    }
}

/// read `len` bytes from a file with `fd` into `buf`
pub fn sys_read(fd: usize, buf: *const u8, len: usize) -> isize {
    match fd {
        FD_STDIN => {
            assert_eq!(len, 1, "Only support len = 1 in sys_read!");
            let mut c: u8;
            loop {
                c = crate::uart::console_getchar();
                if c == 0 {
                    suspend_current_and_run_next();
                    continue;
                } else {
                    break;
                }
            }
            unsafe {
                (buf as *mut u8).write_volatile(c);
            }
            1
        }
        _ => {
            panic!("Unsupported fd in sys_read!");
        }
    }
}
