//! Model Specific Register helpers
//!
//! Used to enable the `syscall`/`sysret` instructions (`EFER.SCE`) and to
//! point `LSTAR` at the `syscall_entry` stub.

use core::arch::asm;

const EFER_MSR: u32 = 0xC0000080;
const STAR_MSR: u32 = 0xC0000081;
const LSTAR_MSR: u32 = 0xC0000082;
const SFMASK_MSR: u32 = 0xC0000084;

const EFER_SCE: u64 = 1 << 0; // enables syscall/sysret

unsafe fn wrmsr(msr: u32, value: u64) {
    unsafe {
        asm!(
            "wrmsr",
            in("ecx") msr,
            in("eax") value as u32,
            in("edx") (value >> 32) as u32,
            options(nostack)
        );
    }
}

unsafe fn rdmsr(msr: u32) -> u64 {
    let mut hi: u32;
    let mut lo: u32;
    unsafe {
        asm!(
            "rdmsr",
            in("ecx") msr,
            out("eax") lo,
            out("edx") hi,
            options(nostack)
        );
    }
    ((hi as u64) << 32) | lo as u64
}

/// Set up the MSRs required by the `syscall` instruction.
///
/// `syscall_entry` must be reachable from ring 3 and must stay alive.
pub fn syscall_init(syscall_entry: u64) {
    unsafe {
        // STAR: kernel CS in bits 47:32, user CS - 0x10 in bits 63:48
        wrmsr(STAR_MSR, (0x08 << 32) | (0x08 << 48));
        // LSTAR: the address of the syscall entry point
        wrmsr(LSTAR_MSR, syscall_entry);
        // SFMASK: mask IF (and a few other flags) off on entry
        wrmsr(SFMASK_MSR, 0x200);
        // EFER: enable the syscall/sysret instructions
        let efer = rdmsr(EFER_MSR);
        wrmsr(EFER_MSR, efer | EFER_SCE);
    }
}
