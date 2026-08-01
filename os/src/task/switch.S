//! Task switching assembly for x86-64.
//!
//! `__switch` saves the SysV callee-saved registers (RBX, RBP, R12-R15), the
//! kernel stack pointer (RSP) and the return address (RIP), then restores the
//! same registers for the next task and jumps to it. Note that the RIP is
//! taken from the top of the stack (the address pushed by the `call` that
//! invoked `__switch`): a suspended task therefore resumes in the middle of
//! `__switch`'s caller, with the whole `trap_handler` call chain still
//! intact on its private kernel stack.
//!
//! The layout below must match `TaskContext` in `context.rs` exactly:
//! offset 0 = RIP, 8 = RSP, 16/24/32/40/48/56 = RBX/RBP/R12/R13/R14/R15.

    .section .text
    .globl __switch
__switch:
    # __switch(
    #     current_task_cx_ptr: *mut TaskContext,   // RDI
    #     next_task_cx_ptr: *const TaskContext     // RSI
    # )
    # save the return address of the current execution
    movq (%rsp), %rax
    movq %rax, 0(%rdi)          # rip
    # save the kernel stack of the current task. Note that the caller expects
    # the value of RSP that it had *before* the `call` that entered __switch
    # (the return address pushed by `call` sits at %rsp on entry, so the value
    # to save is entry_rsp + 8).
    leaq 8(%rsp), %rax
    movq %rax, 8(%rdi)          # rsp
    # save callee-saved registers of the current execution
    movq %rbx, 16(%rdi)
    movq %rbp, 24(%rdi)
    movq %r12, 32(%rdi)
    movq %r13, 40(%rdi)
    movq %r14, 48(%rdi)
    movq %r15, 56(%rdi)
    # restore callee-saved registers of the next execution
    movq 0(%rsi), %rax          # rip
    movq 8(%rsi), %rsp
    movq 16(%rsi), %rbx
    movq 24(%rsi), %rbp
    movq 32(%rsi), %r12
    movq 40(%rsi), %r13
    movq 48(%rsi), %r14
    movq 56(%rsi), %r15
    # jump to the next execution
    jmp *%rax
