//! Implementation of [`TaskControlBlock`]

use super::{KernelStack, PidHandle, SignalActions, SignalFlags, TaskContext, pid_alloc};
use crate::fs::{File, Stdin, Stdout};
use crate::mm::MemorySet;
use crate::sync::UPSafeCell;
use crate::trap::TrapContext;
use alloc::sync::{Arc, Weak};
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::cell::RefMut;

pub struct TaskControlBlock {
    // immutable
    pub pid: PidHandle,
    pub kernel_stack: KernelStack,
    // mutable
    inner: UPSafeCell<TaskControlBlockInner>,
}

pub struct TaskControlBlockInner {
    /// virtual address of the `TrapContext` on this process's kernel stack.
    /// The kernel stack lives in the shared kernel high-half mapping, so this
    /// address is valid under any active page table.
    pub trap_cx_ptr: usize,
    #[allow(unused)]
    pub base_size: usize,
    pub task_cx: TaskContext,
    pub task_status: TaskStatus,
    pub memory_set: MemorySet,
    pub parent: Option<Weak<TaskControlBlock>>,
    pub children: Vec<Arc<TaskControlBlock>>,
    pub exit_code: i32,
    pub fd_table: Vec<Option<Arc<dyn File + Send + Sync>>>,
    /// signals received by the current process (pending)
    pub signals: SignalFlags,
    /// signals that should not be responded to
    pub signal_mask: SignalFlags,
    /// the signal currently being handled by the user handler, -1 if none
    pub handling_sig: isize,
    /// the actions of the current process for all signals
    pub signal_actions: SignalActions,
    /// true if the task is killed by a signal (or SIGKILL)
    pub killed: bool,
    /// true if the task is frozen by SIGSTOP, waiting for SIGCONT
    pub frozen: bool,
    /// the trap context saved when a user signal handler is invoked, restored
    /// by `sigreturn`
    pub trap_ctx_backup: Option<TrapContext>,
}

impl TaskControlBlockInner {
    pub fn get_trap_cx(&self) -> &'static mut TrapContext {
        unsafe { &mut *(self.trap_cx_ptr as *mut TrapContext) }
    }
    pub fn get_user_token(&self) -> usize {
        self.memory_set.token()
    }
    fn get_status(&self) -> TaskStatus {
        self.task_status
    }
    pub fn is_zombie(&self) -> bool {
        self.get_status() == TaskStatus::Zombie
    }
    pub fn alloc_fd(&mut self) -> usize {
        if let Some(fd) = (0..self.fd_table.len()).find(|fd| self.fd_table[*fd].is_none()) {
            fd
        } else {
            self.fd_table.push(None);
            self.fd_table.len() - 1
        }
    }
}

impl TaskControlBlock {
    pub fn inner_exclusive_access(&self) -> RefMut<'_, TaskControlBlockInner> {
        self.inner.exclusive_access()
    }
    pub fn new(elf_data: &[u8]) -> Self {
        // memory_set with elf program headers/user stack
        let (memory_set, user_sp, entry_point) = MemorySet::from_elf(elf_data);
        // alloc a pid and a kernel stack in the shared kernel high-half
        let pid_handle = pid_alloc();
        let kernel_stack = KernelStack::new(&pid_handle);
        let trap_cx_ptr = kernel_stack.push_context(TrapContext::app_init_context(
            entry_point,
            user_sp,
        ));
        let task_control_block = Self {
            pid: pid_handle,
            kernel_stack,
            inner: unsafe {
                UPSafeCell::new(TaskControlBlockInner {
                    trap_cx_ptr,
                    base_size: user_sp,
                    task_cx: TaskContext::goto_restore(trap_cx_ptr),
                    task_status: TaskStatus::Ready,
                    memory_set,
                    parent: None,
                    children: Vec::new(),
                    exit_code: 0,
                    // 0 -> stdin, 1 -> stdout, 2 -> stderr
                    fd_table: vec![
                        Some(Arc::new(Stdin)),
                        Some(Arc::new(Stdout)),
                        Some(Arc::new(Stdout)),
                    ],
                    signals: SignalFlags::empty(),
                    signal_mask: SignalFlags::empty(),
                    handling_sig: -1,
                    signal_actions: SignalActions::default(),
                    killed: false,
                    frozen: false,
                    trap_ctx_backup: None,
                })
            },
        };
        task_control_block
    }
    /// Load a new program image into this process (the `exec` syscall).
    ///
    /// x86-64 note: the trap entry does not switch CR3, so the new address
    /// space must be activated here; otherwise the `iretq` back to user mode
    /// would execute the new entry point under the old page table.
    pub fn exec(&self, elf_data: &[u8], args: Vec<String>) {
        // memory_set with elf program headers/user stack
        let (memory_set, mut user_sp, entry_point) = MemorySet::from_elf(elf_data);
        // push the command line arguments on the user stack, in the same
        // layout as the RISC-V tutorial: a NUL-terminated argv pointer array
        // followed by each argument string. The entry gets `argc`/`argv` in
        // RDI/RSI (x86-64 SysV: first/second argument registers).
        user_sp -= (args.len() + 1) * core::mem::size_of::<usize>();
        let argv_base = user_sp;
        let mut argv: Vec<_> = (0..=args.len())
            .map(|arg| {
                crate::mm::translated_refmut(
                    memory_set.token(),
                    (argv_base + arg * core::mem::size_of::<usize>()) as *mut usize,
                )
            })
            .collect();
        *argv[args.len()] = 0;
        for i in 0..args.len() {
            user_sp -= args[i].len() + 1;
            *argv[i] = user_sp;
            let mut p = user_sp;
            for c in args[i].as_bytes() {
                *crate::mm::translated_refmut(memory_set.token(), p as *mut u8) = *c;
                p += 1;
            }
            *crate::mm::translated_refmut(memory_set.token(), p as *mut u8) = 0;
        }
        // keep the user stack 16-byte aligned (x86-64 SysV ABI)
        user_sp -= user_sp % 16;
        let mut inner = self.inner_exclusive_access();
        // substitute the memory set (the old one is dropped, freeing frames)
        inner.memory_set = memory_set;
        // initialize base_size
        inner.base_size = user_sp;
        // rebuild the TrapContext on the same kernel stack
        let mut trap_cx = TrapContext::app_init_context(entry_point, user_sp);
        trap_cx.rdi = args.len();
        trap_cx.rsi = argv_base;
        let trap_cx_ptr = self.kernel_stack.push_context(trap_cx);
        inner.trap_cx_ptr = trap_cx_ptr;
        // switch to the new address space now
        inner.memory_set.activate();
    }
    /// Create a new process by copying the current one (the `fork` syscall).
    ///
    /// x86-64 note: the TrapContext lives on the kernel stack (not in user
    /// memory like the RISC-V `TRAP_CONTEXT` page), so the child gets an
    /// explicit copy of the parent's current trap context on its own kernel
    /// stack, with `rax` zeroed so `fork` returns 0 in the child.
    pub fn fork(self: &Arc<Self>) -> Arc<Self> {
        // ---- access parent PCB exclusively
        let mut parent_inner = self.inner_exclusive_access();
        // copy user space
        let memory_set = MemorySet::from_existed_user(&parent_inner.memory_set);
        // alloc a pid and a kernel stack in kernel space
        let pid_handle = pid_alloc();
        let kernel_stack = KernelStack::new(&pid_handle);
        // copy the parent's current TrapContext, with rax = 0 for the child
        let mut child_trap_cx = *parent_inner.get_trap_cx();
        child_trap_cx.rax = 0;
        let trap_cx_ptr = kernel_stack.push_context(child_trap_cx);
        // copy the parent's fd table (sharing the underlying files)
        let mut new_fd_table: Vec<Option<Arc<dyn File + Send + Sync>>> = Vec::new();
        for fd in parent_inner.fd_table.iter() {
            new_fd_table.push(fd.clone());
        }
        let task_control_block = Arc::new(TaskControlBlock {
            pid: pid_handle,
            kernel_stack,
            inner: unsafe {
                UPSafeCell::new(TaskControlBlockInner {
                    trap_cx_ptr,
                    base_size: parent_inner.base_size,
                    task_cx: TaskContext::goto_restore(trap_cx_ptr),
                    task_status: TaskStatus::Ready,
                    memory_set,
                    parent: Some(Arc::downgrade(self)),
                    children: Vec::new(),
                    exit_code: 0,
                    fd_table: new_fd_table,
                    signals: SignalFlags::empty(),
                    // inherit the signal_mask and signal_action
                    signal_mask: parent_inner.signal_mask,
                    handling_sig: -1,
                    signal_actions: parent_inner.signal_actions.clone(),
                    killed: false,
                    frozen: false,
                    trap_ctx_backup: None,
                })
            },
        });
        // add child
        parent_inner.children.push(task_control_block.clone());
        // return
        task_control_block
        // ---- release parent PCB automatically
    }
    pub fn getpid(&self) -> usize {
        self.pid.0
    }
}

#[derive(Copy, Clone, PartialEq)]
pub enum TaskStatus {
    Ready,
    Running,
    Zombie,
}
