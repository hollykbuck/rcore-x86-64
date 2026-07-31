//! The x86-64 trap context
//!
//! The layout must match the frame pushed by `trap.S` exactly, both for
//! exceptions (where the CPU pushes `SS, RSP, RFLAGS, CS, RIP` and an error
//! code) and for the `syscall` entry (which synthesizes the same frame).

/// User code segment selector (index 3, RPL 3)
pub const USER_CS: usize = 0x1B;
/// User data segment selector (index 4, RPL 3)
pub const USER_SS: usize = 0x23;
/// The `trap_num` used to mark a `syscall` entry
pub const TRAP_SYSCALL: usize = 0x80;

/// Trap Context
#[repr(C)]
pub struct TrapContext {
    /// general register RAX
    pub rax: usize,
    /// general register RBX
    pub rbx: usize,
    /// general register RCX
    pub rcx: usize,
    /// general register RDX
    pub rdx: usize,
    /// general register RSI
    pub rsi: usize,
    /// general register RDI
    pub rdi: usize,
    /// frame pointer RBP
    pub rbp: usize,
    /// general register R8
    pub r8: usize,
    /// general register R9
    pub r9: usize,
    /// general register R10
    pub r10: usize,
    /// general register R11
    pub r11: usize,
    /// general register R12
    pub r12: usize,
    /// general register R13
    pub r13: usize,
    /// general register R14
    pub r14: usize,
    /// general register R15
    pub r15: usize,
    /// identifies the source of the trap: the IDT vector for exceptions,
    /// `TRAP_SYSCALL` for the `syscall` instruction
    pub trap_num: usize,
    /// the CPU-pushed (or synthesized) error code
    pub error_code: usize,
    /// user RIP
    pub rip: usize,
    /// user CS
    pub cs: usize,
    /// user RFLAGS
    pub rflags: usize,
    /// user RSP
    pub rsp: usize,
    /// user SS
    pub ss: usize,
}

impl TrapContext {
    /// init app context: entry point and stack pointer of an app, running in
    /// ring 3
    pub fn app_init_context(entry: usize, sp: usize) -> Self {
        Self {
            rax: 0,
            rbx: 0,
            rcx: 0,
            rdx: 0,
            rsi: 0,
            rdi: 0,
            rbp: 0,
            r8: 0,
            r9: 0,
            r10: 0,
            r11: 0,
            r12: 0,
            r13: 0,
            r14: 0,
            r15: 0,
            trap_num: TRAP_SYSCALL,
            error_code: 0,
            rip: entry, // entry point of app
            cs: USER_CS,
            rflags: 0x2, // the reserved bit 1 is always set
            rsp: sp,     // app's user stack pointer
            ss: USER_SS,
        }
    }
}
