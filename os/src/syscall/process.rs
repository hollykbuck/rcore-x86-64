//! Process management syscalls
use crate::loader::get_app_data_by_name;
use crate::task::{
    add_task, current_task, exit_current_and_run_next, suspend_current_and_run_next,
};
use crate::timer::get_time_ms;
use alloc::sync::Arc;

/// task exits and submit an exit code
pub fn sys_exit(exit_code: i32) -> ! {
    exit_current_and_run_next(exit_code)
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

/// get the pid of the current task
pub fn sys_getpid() -> isize {
    current_task().unwrap().pid.0 as isize
}

/// create a child process, which is a copy of the current one
pub fn sys_fork() -> isize {
    let current_task = current_task().unwrap();
    let new_task = current_task.fork();
    let new_pid = new_task.pid.0;
    // add new task to scheduler
    add_task(new_task);
    new_pid as isize
}

/// Read a NUL-terminated UTF-8 string from the user address space. The trap
/// handler runs on the current task's own page table, so the user pointer is
/// directly dereferenceable (the x86 analogue of `translated_str`).
fn get_user_str(ptr: *const u8) -> &'static str {
    let mut end = ptr;
    unsafe {
        while end.read_volatile() != b'\0' {
            end = end.add(1);
        }
        let slice = core::slice::from_raw_parts(ptr, end as usize - ptr as usize);
        core::str::from_utf8(slice).unwrap()
    }
}

/// replace the current process with the program named by `path`
pub fn sys_exec(path: *const u8) -> isize {
    let path = get_user_str(path);
    if let Some(data) = get_app_data_by_name(path) {
        let task = current_task().unwrap();
        task.exec(data);
        0
    } else {
        -1
    }
}

/// If there is not a child process whose pid is same as given, return -1.
/// Else if there is a child process but it is still running, return -2.
pub fn sys_waitpid(pid: isize, exit_code_ptr: *mut i32) -> isize {
    let task = current_task().unwrap();
    // find a child process

    // ---- access current TCB exclusively
    let mut inner = task.inner_exclusive_access();
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
        p.inner_exclusive_access().is_zombie() && (pid == -1 || pid as usize == p.getpid())
        // ++++ release child PCB
    });
    if let Some((idx, _)) = pair {
        let child = inner.children.remove(idx);
        // confirm that child will be deallocated after removing from children list
        assert_eq!(Arc::strong_count(&child), 1);
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
