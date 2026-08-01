#![no_std]
#![no_main]

#[macro_use]
extern crate user_lib;

use core::arch::asm;

#[unsafe(no_mangle)]
fn main() -> i32 {
    println!("Try to access privileged MSR in U Mode");
    println!("Kernel should kill this application!");
    unsafe {
        asm!("rdmsr", in("ecx") 0x1B, options(nomem, nostack, preserves_flags));
    }
    0
}
