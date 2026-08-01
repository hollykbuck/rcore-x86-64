//! Implementation of [`TaskContext`]

/// Task Context
#[derive(Copy, Clone)]
#[repr(C)]
pub struct TaskContext {
    /// the address to resume at: `__restore` for a freshly created task, the
    /// return address into `__switch`'s caller for a suspended one
    rip: usize,
    /// kernel stack pointer of the task
    rsp: usize,
    /// callee saved registers (SysV ABI): rbx
    rbx: usize,
    /// callee saved registers (SysV ABI): rbp
    rbp: usize,
    /// callee saved registers (SysV ABI): r12
    r12: usize,
    /// callee saved registers (SysV ABI): r13
    r13: usize,
    /// callee saved registers (SysV ABI): r14
    r14: usize,
    /// callee saved registers (SysV ABI): r15
    r15: usize,
}

impl TaskContext {
    /// init task context
    pub fn zero_init() -> Self {
        Self {
            rip: 0,
            rsp: 0,
            rbx: 0,
            rbp: 0,
            r12: 0,
            r13: 0,
            r14: 0,
            r15: 0,
        }
    }

    /// set task context {__restore, kernel stack, callee saved registers}
    ///
    /// `kstack_ptr` must point at a `TrapContext` already pushed on the task's
    /// kernel stack: after `__switch` restores this context, `rsp` points at
    /// the `TrapContext` and control transfers to `__restore`, which
    /// `iretq`s to user mode.
    pub fn goto_restore(kstack_ptr: usize) -> Self {
        unsafe extern "C" {
            unsafe fn __restore();
        }
        Self {
            rip: linker_symbol_addr!(__restore),
            rsp: kstack_ptr,
            rbx: 0,
            rbp: 0,
            r12: 0,
            r13: 0,
            r14: 0,
            r15: 0,
        }
    }
}
