.att_syntax prefix
.text
.globl switch
# switch(old: *mut TaskContext, new: *const TaskContext)  (SysV: rdi=old, rsi=new)
#
# x86-64 version of the RISC-V coroutine switch. The `TaskContext` keeps the
# RISC-V 15-field `#[repr(C)]` layout; we use:
#   x1 (0x00) = rip      (return address / first entry point)
#   x2 (0x08) = rsp
#   x8 (0x10) = rbp
#   x9 (0x18) = rbx
#   x18..x21 (0x20..0x38) = r12..r15
# All other fields are unused on x86-64.
#
# Unlike RISC-V, the return address lives on the stack (call/ret), so the
# saved rsp points one word above the return address (the top of the caller's
# frame). On resume we `jmp` to the saved rip with that rsp, so the resumed
# frame's own `ret` pops the correct caller return address.
switch:
    # save to old (rdi)
    movq (%rsp), %rax
    movq %rax, 0x00(%rdi)       # x1 = return address
    leaq 8(%rsp), %rax
    movq %rax, 0x08(%rdi)       # x2 = rsp above the return address
    movq %rbp, 0x10(%rdi)
    movq %rbx, 0x18(%rdi)
    movq %r12, 0x20(%rdi)
    movq %r13, 0x28(%rdi)
    movq %r14, 0x30(%rdi)
    movq %r15, 0x38(%rdi)
    # load from new (rsi)
    movq 0x00(%rsi), %rax
    movq 0x08(%rsi), %rsp
    movq 0x10(%rsi), %rbp
    movq 0x18(%rsi), %rbx
    movq 0x20(%rsi), %r12
    movq 0x28(%rsi), %r13
    movq 0x30(%rsi), %r14
    movq 0x38(%rsi), %r15
    jmp *%rax
