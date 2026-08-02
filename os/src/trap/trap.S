//! Trap handling assembly for x86-64
//!
//! Two kinds of entries exist:
//!
//! - the `syscall` entry (`syscall_entry`), pointed to by the `LSTAR` MSR.
//!   It synthesizes an interrupt-like frame and forwards to `trap_handler`.
//! - the 256 exception stubs, pointed to by the IDT. The CPU pushes
//!   `SS, RSP, RFLAGS, CS, RIP` (and an error code for some vectors), the
//!   stub then pushes a dummy error code (if needed) and the vector number.
//!
//! After the pushes, both paths share the same frame layout:
//!
//! ```text
//! +0x00  RAX
//! +0x08  RBX
//! +0x10  RCX
//! +0x18  RDX
//! +0x20  RSI
//! +0x28  RDI
//! +0x30  RBP
//! +0x38  R8
//! +0x40  R9
//! +0x48  R10
//! +0x50  R11
//! +0x58  R12
//! +0x60  R13
//! +0x68  R14
//! +0x70  R15
//! +0x78  trap_num
//! +0x80  error_code
//! +0x88  RIP
//! +0x90  CS
//! +0x98  RFLAGS
//! +0xA0  RSP      (only pushed for user mode exceptions)
//! +0xA8  SS       (only pushed for user mode exceptions)
//! ```
//!
//! which matches `TrapContext` in `context.rs`.
//!
//! Every task owns a private kernel stack (see `crate::loader`). The CPU uses
//! `TSS.rsp0` for user-mode exceptions and the `current_stack_top` global
//! (which is kept in sync with `TSS.rsp0` by the task manager) for the
//! `syscall` entry.

.att_syntax prefix
.altmacro

.macro EXC_NOERR vec
    .globl __exc_\vec
__exc_\vec:
    pushq $0
    pushq $\vec
    jmp exception_common
.endm

.macro EXC_ERR vec
    .globl __exc_\vec
__exc_\vec:
    pushq $\vec
    jmp exception_common
.endm

    .section .text

// The 256 exception stubs.
    EXC_NOERR 0
    EXC_NOERR 1
    EXC_NOERR 2
    EXC_NOERR 3
    EXC_NOERR 4
    EXC_NOERR 5
    EXC_NOERR 6
    EXC_NOERR 7
    EXC_ERR 8
    EXC_NOERR 9
    EXC_ERR 10
    EXC_ERR 11
    EXC_ERR 12
    EXC_ERR 13
    EXC_ERR 14
    EXC_NOERR 15
    EXC_NOERR 16
    EXC_ERR 17
    EXC_NOERR 18
    EXC_NOERR 19
    EXC_NOERR 20
    EXC_ERR 21
    EXC_NOERR 22
    EXC_NOERR 23
    EXC_NOERR 24
    EXC_NOERR 25
    EXC_NOERR 26
    EXC_NOERR 27
    EXC_NOERR 28
    EXC_ERR 29
    EXC_ERR 30
    EXC_NOERR 31
    EXC_NOERR 32
    EXC_NOERR 33
    EXC_NOERR 34
    EXC_NOERR 35
    EXC_NOERR 36
    EXC_NOERR 37
    EXC_NOERR 38
    EXC_NOERR 39
    EXC_NOERR 40
    EXC_NOERR 41
    EXC_NOERR 42
    EXC_NOERR 43
    EXC_NOERR 44
    EXC_NOERR 45
    EXC_NOERR 46
    EXC_NOERR 47
    EXC_NOERR 48
    EXC_NOERR 49
    EXC_NOERR 50
    EXC_NOERR 51
    EXC_NOERR 52
    EXC_NOERR 53
    EXC_NOERR 54
    EXC_NOERR 55
    EXC_NOERR 56
    EXC_NOERR 57
    EXC_NOERR 58
    EXC_NOERR 59
    EXC_NOERR 60
    EXC_NOERR 61
    EXC_NOERR 62
    EXC_NOERR 63
    EXC_NOERR 64
    EXC_NOERR 65
    EXC_NOERR 66
    EXC_NOERR 67
    EXC_NOERR 68
    EXC_NOERR 69
    EXC_NOERR 70
    EXC_NOERR 71
    EXC_NOERR 72
    EXC_NOERR 73
    EXC_NOERR 74
    EXC_NOERR 75
    EXC_NOERR 76
    EXC_NOERR 77
    EXC_NOERR 78
    EXC_NOERR 79
    EXC_NOERR 80
    EXC_NOERR 81
    EXC_NOERR 82
    EXC_NOERR 83
    EXC_NOERR 84
    EXC_NOERR 85
    EXC_NOERR 86
    EXC_NOERR 87
    EXC_NOERR 88
    EXC_NOERR 89
    EXC_NOERR 90
    EXC_NOERR 91
    EXC_NOERR 92
    EXC_NOERR 93
    EXC_NOERR 94
    EXC_NOERR 95
    EXC_NOERR 96
    EXC_NOERR 97
    EXC_NOERR 98
    EXC_NOERR 99
    EXC_NOERR 100
    EXC_NOERR 101
    EXC_NOERR 102
    EXC_NOERR 103
    EXC_NOERR 104
    EXC_NOERR 105
    EXC_NOERR 106
    EXC_NOERR 107
    EXC_NOERR 108
    EXC_NOERR 109
    EXC_NOERR 110
    EXC_NOERR 111
    EXC_NOERR 112
    EXC_NOERR 113
    EXC_NOERR 114
    EXC_NOERR 115
    EXC_NOERR 116
    EXC_NOERR 117
    EXC_NOERR 118
    EXC_NOERR 119
    EXC_NOERR 120
    EXC_NOERR 121
    EXC_NOERR 122
    EXC_NOERR 123
    EXC_NOERR 124
    EXC_NOERR 125
    EXC_NOERR 126
    EXC_NOERR 127
    EXC_NOERR 128
    EXC_NOERR 129
    EXC_NOERR 130
    EXC_NOERR 131
    EXC_NOERR 132
    EXC_NOERR 133
    EXC_NOERR 134
    EXC_NOERR 135
    EXC_NOERR 136
    EXC_NOERR 137
    EXC_NOERR 138
    EXC_NOERR 139
    EXC_NOERR 140
    EXC_NOERR 141
    EXC_NOERR 142
    EXC_NOERR 143
    EXC_NOERR 144
    EXC_NOERR 145
    EXC_NOERR 146
    EXC_NOERR 147
    EXC_NOERR 148
    EXC_NOERR 149
    EXC_NOERR 150
    EXC_NOERR 151
    EXC_NOERR 152
    EXC_NOERR 153
    EXC_NOERR 154
    EXC_NOERR 155
    EXC_NOERR 156
    EXC_NOERR 157
    EXC_NOERR 158
    EXC_NOERR 159
    EXC_NOERR 160
    EXC_NOERR 161
    EXC_NOERR 162
    EXC_NOERR 163
    EXC_NOERR 164
    EXC_NOERR 165
    EXC_NOERR 166
    EXC_NOERR 167
    EXC_NOERR 168
    EXC_NOERR 169
    EXC_NOERR 170
    EXC_NOERR 171
    EXC_NOERR 172
    EXC_NOERR 173
    EXC_NOERR 174
    EXC_NOERR 175
    EXC_NOERR 176
    EXC_NOERR 177
    EXC_NOERR 178
    EXC_NOERR 179
    EXC_NOERR 180
    EXC_NOERR 181
    EXC_NOERR 182
    EXC_NOERR 183
    EXC_NOERR 184
    EXC_NOERR 185
    EXC_NOERR 186
    EXC_NOERR 187
    EXC_NOERR 188
    EXC_NOERR 189
    EXC_NOERR 190
    EXC_NOERR 191
    EXC_NOERR 192
    EXC_NOERR 193
    EXC_NOERR 194
    EXC_NOERR 195
    EXC_NOERR 196
    EXC_NOERR 197
    EXC_NOERR 198
    EXC_NOERR 199
    EXC_NOERR 200
    EXC_NOERR 201
    EXC_NOERR 202
    EXC_NOERR 203
    EXC_NOERR 204
    EXC_NOERR 205
    EXC_NOERR 206
    EXC_NOERR 207
    EXC_NOERR 208
    EXC_NOERR 209
    EXC_NOERR 210
    EXC_NOERR 211
    EXC_NOERR 212
    EXC_NOERR 213
    EXC_NOERR 214
    EXC_NOERR 215
    EXC_NOERR 216
    EXC_NOERR 217
    EXC_NOERR 218
    EXC_NOERR 219
    EXC_NOERR 220
    EXC_NOERR 221
    EXC_NOERR 222
    EXC_NOERR 223
    EXC_NOERR 224
    EXC_NOERR 225
    EXC_NOERR 226
    EXC_NOERR 227
    EXC_NOERR 228
    EXC_NOERR 229
    EXC_NOERR 230
    EXC_NOERR 231
    EXC_NOERR 232
    EXC_NOERR 233
    EXC_NOERR 234
    EXC_NOERR 235
    EXC_NOERR 236
    EXC_NOERR 237
    EXC_NOERR 238
    EXC_NOERR 239
    EXC_NOERR 240
    EXC_NOERR 241
    EXC_NOERR 242
    EXC_NOERR 243
    EXC_NOERR 244
    EXC_NOERR 245
    EXC_NOERR 246
    EXC_NOERR 247
    EXC_NOERR 248
    EXC_NOERR 249
    EXC_NOERR 250
    EXC_NOERR 251
    EXC_NOERR 252
    EXC_NOERR 253
    EXC_NOERR 254
    EXC_NOERR 255

// Addresses of the exception stubs, indexed by the vector number.
    .section .rodata
    .balign 8
    .globl exception_stub_addrs
exception_stub_addrs:
    .quad __exc_0
    .quad __exc_1
    .quad __exc_2
    .quad __exc_3
    .quad __exc_4
    .quad __exc_5
    .quad __exc_6
    .quad __exc_7
    .quad __exc_8
    .quad __exc_9
    .quad __exc_10
    .quad __exc_11
    .quad __exc_12
    .quad __exc_13
    .quad __exc_14
    .quad __exc_15
    .quad __exc_16
    .quad __exc_17
    .quad __exc_18
    .quad __exc_19
    .quad __exc_20
    .quad __exc_21
    .quad __exc_22
    .quad __exc_23
    .quad __exc_24
    .quad __exc_25
    .quad __exc_26
    .quad __exc_27
    .quad __exc_28
    .quad __exc_29
    .quad __exc_30
    .quad __exc_31
    .quad __exc_32
    .quad __exc_33
    .quad __exc_34
    .quad __exc_35
    .quad __exc_36
    .quad __exc_37
    .quad __exc_38
    .quad __exc_39
    .quad __exc_40
    .quad __exc_41
    .quad __exc_42
    .quad __exc_43
    .quad __exc_44
    .quad __exc_45
    .quad __exc_46
    .quad __exc_47
    .quad __exc_48
    .quad __exc_49
    .quad __exc_50
    .quad __exc_51
    .quad __exc_52
    .quad __exc_53
    .quad __exc_54
    .quad __exc_55
    .quad __exc_56
    .quad __exc_57
    .quad __exc_58
    .quad __exc_59
    .quad __exc_60
    .quad __exc_61
    .quad __exc_62
    .quad __exc_63
    .quad __exc_64
    .quad __exc_65
    .quad __exc_66
    .quad __exc_67
    .quad __exc_68
    .quad __exc_69
    .quad __exc_70
    .quad __exc_71
    .quad __exc_72
    .quad __exc_73
    .quad __exc_74
    .quad __exc_75
    .quad __exc_76
    .quad __exc_77
    .quad __exc_78
    .quad __exc_79
    .quad __exc_80
    .quad __exc_81
    .quad __exc_82
    .quad __exc_83
    .quad __exc_84
    .quad __exc_85
    .quad __exc_86
    .quad __exc_87
    .quad __exc_88
    .quad __exc_89
    .quad __exc_90
    .quad __exc_91
    .quad __exc_92
    .quad __exc_93
    .quad __exc_94
    .quad __exc_95
    .quad __exc_96
    .quad __exc_97
    .quad __exc_98
    .quad __exc_99
    .quad __exc_100
    .quad __exc_101
    .quad __exc_102
    .quad __exc_103
    .quad __exc_104
    .quad __exc_105
    .quad __exc_106
    .quad __exc_107
    .quad __exc_108
    .quad __exc_109
    .quad __exc_110
    .quad __exc_111
    .quad __exc_112
    .quad __exc_113
    .quad __exc_114
    .quad __exc_115
    .quad __exc_116
    .quad __exc_117
    .quad __exc_118
    .quad __exc_119
    .quad __exc_120
    .quad __exc_121
    .quad __exc_122
    .quad __exc_123
    .quad __exc_124
    .quad __exc_125
    .quad __exc_126
    .quad __exc_127
    .quad __exc_128
    .quad __exc_129
    .quad __exc_130
    .quad __exc_131
    .quad __exc_132
    .quad __exc_133
    .quad __exc_134
    .quad __exc_135
    .quad __exc_136
    .quad __exc_137
    .quad __exc_138
    .quad __exc_139
    .quad __exc_140
    .quad __exc_141
    .quad __exc_142
    .quad __exc_143
    .quad __exc_144
    .quad __exc_145
    .quad __exc_146
    .quad __exc_147
    .quad __exc_148
    .quad __exc_149
    .quad __exc_150
    .quad __exc_151
    .quad __exc_152
    .quad __exc_153
    .quad __exc_154
    .quad __exc_155
    .quad __exc_156
    .quad __exc_157
    .quad __exc_158
    .quad __exc_159
    .quad __exc_160
    .quad __exc_161
    .quad __exc_162
    .quad __exc_163
    .quad __exc_164
    .quad __exc_165
    .quad __exc_166
    .quad __exc_167
    .quad __exc_168
    .quad __exc_169
    .quad __exc_170
    .quad __exc_171
    .quad __exc_172
    .quad __exc_173
    .quad __exc_174
    .quad __exc_175
    .quad __exc_176
    .quad __exc_177
    .quad __exc_178
    .quad __exc_179
    .quad __exc_180
    .quad __exc_181
    .quad __exc_182
    .quad __exc_183
    .quad __exc_184
    .quad __exc_185
    .quad __exc_186
    .quad __exc_187
    .quad __exc_188
    .quad __exc_189
    .quad __exc_190
    .quad __exc_191
    .quad __exc_192
    .quad __exc_193
    .quad __exc_194
    .quad __exc_195
    .quad __exc_196
    .quad __exc_197
    .quad __exc_198
    .quad __exc_199
    .quad __exc_200
    .quad __exc_201
    .quad __exc_202
    .quad __exc_203
    .quad __exc_204
    .quad __exc_205
    .quad __exc_206
    .quad __exc_207
    .quad __exc_208
    .quad __exc_209
    .quad __exc_210
    .quad __exc_211
    .quad __exc_212
    .quad __exc_213
    .quad __exc_214
    .quad __exc_215
    .quad __exc_216
    .quad __exc_217
    .quad __exc_218
    .quad __exc_219
    .quad __exc_220
    .quad __exc_221
    .quad __exc_222
    .quad __exc_223
    .quad __exc_224
    .quad __exc_225
    .quad __exc_226
    .quad __exc_227
    .quad __exc_228
    .quad __exc_229
    .quad __exc_230
    .quad __exc_231
    .quad __exc_232
    .quad __exc_233
    .quad __exc_234
    .quad __exc_235
    .quad __exc_236
    .quad __exc_237
    .quad __exc_238
    .quad __exc_239
    .quad __exc_240
    .quad __exc_241
    .quad __exc_242
    .quad __exc_243
    .quad __exc_244
    .quad __exc_245
    .quad __exc_246
    .quad __exc_247
    .quad __exc_248
    .quad __exc_249
    .quad __exc_250
    .quad __exc_251
    .quad __exc_252
    .quad __exc_253
    .quad __exc_254
    .quad __exc_255

    .section .text

// Entry of an exception/IDT interrupt. `CS` is always at 24(%rsp) after the
// stub pushes, both for user mode (full frame) and kernel mode (short frame).
exception_common:
    testq $3, 24(%rsp)          // low 2 bits of CS == CPL
    jz 1f                       // from kernel mode: panic
    // from user mode: switch GS to this core's PerCpu (user GS base is 0)
    swapgs
    // save the general purpose registers; RAX must end up at offset 0
    pushq %r15
    pushq %r14
    pushq %r13
    pushq %r12
    pushq %r11
    pushq %r10
    pushq %r9
    pushq %r8
    pushq %rbp
    pushq %rdi
    pushq %rsi
    pushq %rdx
    pushq %rcx
    pushq %rbx
    pushq %rax
    movq %rsp, %rdi
    call trap_handler
    movq %rax, %rsp
    jmp restore_all

1:
    // Exception/interrupt in kernel mode. Two things can legitimately happen:
    //
    // - the APIC timer (vector 32) while a core is idle (`sti; hlt; cli`):
    //   treat it as a wakeup (EOI + check timers) and return.
    // - a device IRQ (vector >= 33) while IF was set in kernel mode. This
    //   currently happens on the same idle `sti; hlt` wakeup (the kernel
    //   otherwise keeps IF cleared): acknowledge it and dispatch to the
    //   registered handler, then return. Handlers must not schedule.
    //
    // Anything else is a bug.
    cmpq $32, (%rsp)
    je 3f
    cmpq $33, (%rsp)
    jae 4f
    jmp 2f
3:
    pushq %rax
    pushq %rcx
    pushq %rdx
    call kernel_timer_eoi
    popq %rdx
    popq %rcx
    popq %rax
    addq $16, %rsp              // skip vector + error code
    iretq
4:
    // device IRQ in kernel mode: save all GPRs, pass the vector number, and
    // dispatch. The vector sits at 120(%rsp): the stub pushed error code +
    // vector on top of the CPU frame, then this path pushed 15 registers.
    pushq %r15
    pushq %r14
    pushq %r13
    pushq %r12
    pushq %r11
    pushq %r10
    pushq %r9
    pushq %r8
    pushq %rbp
    pushq %rdi
    pushq %rsi
    pushq %rdx
    pushq %rcx
    pushq %rbx
    pushq %rax
    movq 120(%rsp), %rdi        // vector
    call kernel_irq_dispatch
    popq %rax
    popq %rbx
    popq %rcx
    popq %rdx
    popq %rsi
    popq %rdi
    popq %rbp
    popq %r8
    popq %r9
    popq %r10
    popq %r11
    popq %r12
    popq %r13
    popq %r14
    popq %r15
    addq $16, %rsp              // skip vector + error code
    iretq
2:
    // Exception in kernel mode: pass the vector number, the error code
    // (pushed below the vector number by the stub), the faulting RIP and CS.
    movq (%rsp), %rdi           // vector
    movq 8(%rsp), %rsi          // error code
    movq 16(%rsp), %rdx         // faulting RIP
    movq 24(%rsp), %rcx         // CS
    call kernel_exception_panic

// Entry of the `syscall` instruction, pointed to by the LSTAR MSR.
// On `syscall`, RCX = user RIP and R11 = user RFLAGS.
.globl syscall_entry
syscall_entry:
    swapgs                      // GS = this core's PerCpu
    // save the user rsp in this core's PerCpu slot (offset 16). It must not
    // be a global: two cores executing `syscall` concurrently would clobber
    // each other's saved value and resume with the wrong stack pointer.
    movq %rsp, %gs:16
    // switch to this core's current kernel stack. The offset 8 is hardcoded:
    // `PerCpu` is `#[repr(C)]` with a `usize cpu_id` first field, so
    // `current_stack_top` sits at offset 8. No register is clobbered here, so
    // the user `rax` (the syscall number) is preserved for the frame below.
    movq %gs:8, %rsp
    pushq $0x23                 // SS
    pushq %gs:16                // RSP
    pushq %r11                  // RFLAGS
    pushq $0x1B                 // CS
    pushq %rcx                  // RIP
    pushq $0                    // error code
    pushq $0x80                 // trap_num = TRAP_SYSCALL
    pushq %r15
    pushq %r14
    pushq %r13
    pushq %r12
    pushq %r11
    pushq %r10
    pushq %r9
    pushq %r8
    pushq %rbp
    pushq %rdi
    pushq %rsi
    pushq %rdx
    pushq %rcx
    pushq %rbx
    pushq %rax
    movq %rsp, %rdi
    call trap_handler
    movq %rax, %rsp
    jmp restore_all

// Restore a `TrapContext` (whose address is in RAX) and return to user mode.
restore_all:
    popq %rax
    popq %rbx
    popq %rcx
    popq %rdx
    popq %rsi
    popq %rdi
    popq %rbp
    popq %r8
    popq %r9
    popq %r10
    popq %r11
    popq %r12
    popq %r13
    popq %r14
    popq %r15
    addq $16, %rsp              // skip trap_num and error_code
    swapgs                      // restore the user GS base
    iretq

// Jump into user mode with a fresh `TrapContext`. The task manager arranges
// that RSP already points at the `TrapContext` when this label is reached via
// `__switch`. `GS` currently points at this core's PerCpu (set at
// initialization); `restore_all` swaps it back to the user GS base (0).
.globl __restore
__restore:
    jmp restore_all

    .section .bss

// The trap kernel stack, used as the fallback `current_stack_top` value
// before the first task starts.
    .section .bss.stack, "aw", @nobits
    .balign 16
    .globl trap_stack_bottom
trap_stack_bottom:
    .space 4096 * 2
    .globl trap_stack_top
trap_stack_top:
