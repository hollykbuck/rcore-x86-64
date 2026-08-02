//! Implementation of [`ProcessControlBlock`]: the process-wide state shared
//! by all threads (the address space, the file descriptor table, the signals,
//! the children, ...).

use super::TaskControlBlock;
use super::id::RecycleAllocator;
use super::manager::insert_into_pid2process;
use super::{PidHandle, SignalActions, SignalFlags, add_task, pid_alloc};
use crate::config::USER_STACK_SIZE;
use crate::fs::{File, Stdin, Stdout};
use crate::mm::{MemorySet, translated_refmut};
use crate::sync::{Condvar, Mutex, Semaphore, UPIntrFreeCell};
use crate::trap::TrapContext;
use alloc::string::String;
use alloc::sync::{Arc, Weak};
use alloc::vec;
use alloc::vec::Vec;

/// The process-wide state shared by all threads of the process.
pub struct ProcessControlBlock {
    /// the pid, immutable
    pub pid: PidHandle,
    // mutable
    inner: UPIntrFreeCell<ProcessControlBlockInner>,
}

pub struct ProcessControlBlockInner {
    pub is_zombie: bool,
    pub memory_set: MemorySet,
    pub parent: Option<Weak<ProcessControlBlock>>,
    pub children: Vec<Arc<ProcessControlBlock>>,
    pub exit_code: i32,
    pub fd_table: Vec<Option<Arc<dyn File + Send + Sync>>>,
    pub signals: SignalFlags,
    pub signal_mask: SignalFlags,
    pub handling_sig: isize,
    pub signal_actions: SignalActions,
    pub killed: bool,
    pub frozen: bool,
    pub trap_ctx_backup: Option<TrapContext>,
    /// the threads of this process, indexed by tid
    pub tasks: Vec<Option<Arc<TaskControlBlock>>>,
    pub task_res_allocator: RecycleAllocator,
    /// kernel-level mutexes of this process, indexed by id
    pub mutex_list: Vec<Option<Arc<dyn Mutex>>>,
    /// kernel-level semaphores of this process, indexed by id
    pub semaphore_list: Vec<Option<Arc<Semaphore>>>,
    /// kernel-level condition variables of this process, indexed by id
    pub condvar_list: Vec<Option<Arc<Condvar>>>,
}

impl ProcessControlBlockInner {
    #[allow(unused)]
    pub fn get_user_token(&self) -> usize {
        self.memory_set.token()
    }

    pub fn alloc_fd(&mut self) -> usize {
        if let Some(fd) = (0..self.fd_table.len()).find(|fd| self.fd_table[*fd].is_none()) {
            fd
        } else {
            self.fd_table.push(None);
            self.fd_table.len() - 1
        }
    }

    pub fn alloc_tid(&mut self) -> usize {
        self.task_res_allocator.alloc()
    }

    pub fn dealloc_tid(&mut self, tid: usize) {
        self.task_res_allocator.dealloc(tid)
    }

    pub fn thread_count(&self) -> usize {
        self.tasks.len()
    }

    pub fn get_task(&self, tid: usize) -> Arc<TaskControlBlock> {
        self.tasks[tid].as_ref().unwrap().clone()
    }
}

impl ProcessControlBlock {
    /// Get the mutable reference to the inner structure of the process.
    pub fn inner_exclusive_access(&self) -> crate::sync::UPIntrRefMut<'_, ProcessControlBlockInner> {
        self.inner.exclusive_access()
    }

    /// Load a new process from `elf_data`.
    pub fn new(elf_data: &[u8]) -> Arc<Self> {
        // memory_set with elf program headers and the main-thread user stack
        let (memory_set, user_sp, entry_point) = MemorySet::from_elf(elf_data);
        // bottom of the main-thread user stack (the per-thread stacks of
        // additional threads are mapped above it, see `TaskUserRes`)
        let ustack_base = user_sp - USER_STACK_SIZE;
        // allocate a pid
        let pid_handle = pid_alloc();
        let process = Arc::new(Self {
            pid: pid_handle,
            inner: unsafe {
                UPIntrFreeCell::new(ProcessControlBlockInner {
                    is_zombie: false,
                    memory_set,
                    parent: None,
                    children: Vec::new(),
                    exit_code: 0,
                    fd_table: vec![
                        // 0 -> stdin
                        Some(Arc::new(Stdin)),
                        // 1 -> stdout
                        Some(Arc::new(Stdout)),
                        // 2 -> stderr
                        Some(Arc::new(Stdout)),
                    ],
                    signals: SignalFlags::empty(),
                    signal_mask: SignalFlags::empty(),
                    handling_sig: -1,
                    signal_actions: SignalActions::default(),
                    killed: false,
                    frozen: false,
                    trap_ctx_backup: None,
                    tasks: Vec::new(),
                    task_res_allocator: RecycleAllocator::new(),
                    mutex_list: Vec::new(),
                    semaphore_list: Vec::new(),
                    condvar_list: Vec::new(),
                })
            },
        });
        // create the main thread; its user stack is already mapped by
        // `MemorySet::from_elf`, so do not allocate user resources again
        let task = Arc::new(TaskControlBlock::new(Arc::clone(&process), ustack_base, false));
        // prepare the trap context of the main thread
        let task_inner = task.inner_exclusive_access();
        let trap_cx = task_inner.get_trap_cx();
        let ustack_top = task_inner.res.as_ref().unwrap().ustack_top();
        drop(task_inner);
        *trap_cx = TrapContext::app_init_context(entry_point, ustack_top);
        // add the main thread to the process
        let mut process_inner = process.inner_exclusive_access();
        process_inner.tasks.push(Some(Arc::clone(&task)));
        drop(process_inner);
        insert_into_pid2process(process.getpid(), Arc::clone(&process));
        // add the main thread to the scheduler
        add_task(task);
        process
    }

    /// Load a new program image into this process (the `exec` syscall).
    ///
    /// x86-64 note: the trap entry does not switch CR3, so the new address
    /// space must be activated here; otherwise the `iretq` back to user mode
    /// would execute the new entry point under the old page table.
    pub fn exec(self: &Arc<Self>, elf_data: &[u8], args: Vec<String>) {
        assert_eq!(self.inner_exclusive_access().thread_count(), 1);
        // memory_set with elf program headers and the main-thread user stack
        let (memory_set, user_sp, entry_point) = MemorySet::from_elf(elf_data);
        let ustack_base = user_sp - USER_STACK_SIZE;
        let new_token = memory_set.token();
        // substitute the memory set (the old one is dropped, freeing frames)
        self.inner_exclusive_access().memory_set = memory_set;
        // rebase the main-thread user stack on the new address space
        let task = self.inner_exclusive_access().get_task(0);
        let mut task_inner = task.inner_exclusive_access();
        task_inner.res.as_mut().unwrap().ustack_base = ustack_base;
        // push the command line arguments on the user stack (NUL-terminated
        // argv pointer array followed by each argument string), keeping the
        // stack 16-byte aligned (x86-64 SysV ABI)
        let mut user_sp = task_inner.res.as_mut().unwrap().ustack_top();
        user_sp -= (args.len() + 1) * core::mem::size_of::<usize>();
        let argv_base = user_sp;
        let mut argv: Vec<_> = (0..=args.len())
            .map(|arg| {
                translated_refmut(
                    new_token,
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
                *translated_refmut(new_token, p as *mut u8) = *c;
                p += 1;
            }
            *translated_refmut(new_token, p as *mut u8) = 0;
        }
        user_sp -= user_sp % 16;
        // rebuild the trap context (argc/argv in RDI/RSI)
        let mut trap_cx = TrapContext::app_init_context(entry_point, user_sp);
        trap_cx.rdi = args.len();
        trap_cx.rsi = argv_base;
        *task_inner.get_trap_cx() = trap_cx;
        drop(task_inner);
        // switch to the new address space now
        self.inner_exclusive_access().memory_set.activate();
    }

    /// Create a child process by copying the current one (the `fork` syscall).
    ///
    /// Only supports processes with a single thread.
    pub fn fork(self: &Arc<Self>) -> Arc<Self> {
        let mut parent = self.inner_exclusive_access();
        assert_eq!(parent.thread_count(), 1);
        // clone parent's address space completely (including all user stacks)
        let memory_set = MemorySet::from_existed_user(&parent.memory_set);
        // alloc a pid
        let pid = pid_alloc();
        // copy the fd table (sharing the underlying files)
        let mut new_fd_table: Vec<Option<Arc<dyn File + Send + Sync>>> = Vec::new();
        for fd in parent.fd_table.iter() {
            new_fd_table.push(fd.clone());
        }
        // create the child process pcb
        let child = Arc::new(Self {
            pid,
            inner: unsafe {
                UPIntrFreeCell::new(ProcessControlBlockInner {
                    is_zombie: false,
                    memory_set,
                    parent: Some(Arc::downgrade(self)),
                    children: Vec::new(),
                    exit_code: 0,
                    fd_table: new_fd_table,
                    signals: SignalFlags::empty(),
                    // inherit the signal_mask and signal_action
                    signal_mask: parent.signal_mask,
                    handling_sig: -1,
                    signal_actions: parent.signal_actions.clone(),
                    killed: false,
                    frozen: false,
                    trap_ctx_backup: None,
                    tasks: Vec::new(),
                    task_res_allocator: RecycleAllocator::new(),
                    mutex_list: Vec::new(),
                    semaphore_list: Vec::new(),
                    condvar_list: Vec::new(),
                })
            },
        });
        // add child to parent's children
        parent.children.push(Arc::clone(&child));
        // create the main thread of the child process (user stack already
        // copied with the address space, so no user resources are allocated)
        let task = Arc::new(TaskControlBlock::new(
            Arc::clone(&child),
            parent
                .get_task(0)
                .inner_exclusive_access()
                .res
                .as_ref()
                .unwrap()
                .ustack_base(),
            false,
        ));
        // attach the thread to the child process
        let mut child_inner = child.inner_exclusive_access();
        child_inner.tasks.push(Some(Arc::clone(&task)));
        drop(child_inner);
        // copy the parent's current trap context, with rax = 0 so that `fork`
        // returns 0 in the child
        let mut child_trap_cx = *parent.get_task(0).inner_exclusive_access().get_trap_cx();
        child_trap_cx.rax = 0;
        let task_inner = task.inner_exclusive_access();
        *task_inner.get_trap_cx() = child_trap_cx;
        drop(task_inner);
        insert_into_pid2process(child.getpid(), Arc::clone(&child));
        // add the thread to the scheduler
        add_task(task);
        child
    }

    /// Get the pid of this process.
    pub fn getpid(&self) -> usize {
        self.pid.0
    }
}
