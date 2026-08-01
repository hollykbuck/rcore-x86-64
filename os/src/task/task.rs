//! Types related to task management
use super::TaskContext;
use crate::mm::{MemorySet, VirtAddr};
use crate::trap::TrapContext;

/// task control block structure
pub struct TaskControlBlock {
    pub task_status: TaskStatus,
    pub task_cx: TaskContext,
    pub memory_set: MemorySet,
    /// virtual address of the `TrapContext` pushed on this task's kernel
    /// stack. The kernel stack lives in the shared kernel high-half mapping,
    /// so this address is valid under any active page table.
    pub trap_cx_ptr: usize,
    #[allow(unused)]
    pub base_size: usize,
    pub heap_bottom: usize,
    pub program_brk: usize,
}

impl TaskControlBlock {
    pub fn get_trap_cx(&self) -> &'static mut TrapContext {
        unsafe { &mut *(self.trap_cx_ptr as *mut TrapContext) }
    }
    pub fn get_user_token(&self) -> usize {
        self.memory_set.token()
    }
    pub fn new(elf_data: &[u8], app_id: usize) -> Self {
        // memory_set with elf program headers/user stack
        let (memory_set, user_sp, entry_point) = MemorySet::from_elf(elf_data);
        let task_status = TaskStatus::Ready;
        // prepare a TrapContext on the task's kernel stack
        let trap_cx_ptr = crate::loader::push_context(
            app_id,
            TrapContext::app_init_context(entry_point, user_sp),
        );
        let task_control_block = Self {
            task_status,
            task_cx: TaskContext::goto_restore(trap_cx_ptr),
            memory_set,
            trap_cx_ptr,
            base_size: user_sp,
            heap_bottom: user_sp,
            program_brk: user_sp,
        };
        task_control_block
    }
    /// change the location of the program break. return None if failed.
    pub fn change_program_brk(&mut self, size: i32) -> Option<usize> {
        let old_break = self.program_brk;
        let new_brk = self.program_brk as isize + size as isize;
        if new_brk < self.heap_bottom as isize {
            return None;
        }
        let result = if size < 0 {
            self.memory_set
                .shrink_to(VirtAddr(self.heap_bottom), VirtAddr(new_brk as usize))
        } else {
            self.memory_set
                .append_to(VirtAddr(self.heap_bottom), VirtAddr(new_brk as usize))
        };
        if result {
            self.program_brk = new_brk as usize;
            Some(old_break)
        } else {
            None
        }
    }
}

#[derive(Copy, Clone, PartialEq)]
/// task status: Ready, Running, Exited
pub enum TaskStatus {
    Ready,
    Running,
    Exited,
}
