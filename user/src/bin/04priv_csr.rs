#![no_std]
#![no_main]

#[macro_use]
extern crate user_lib;

use core::arch::asm;

#[unsafe(no_mangle)]
fn main() -> i32 {
    println!("Try to access privileged register in U Mode");
    println!("Kernel should kill this application!");
    unsafe {
        asm!("mov rax, cr0", out("rax") _, options(nomem, nostack));
    }
    0
}
