#![no_std]
#![no_main]

#[macro_use]
extern crate user_lib;

const LEN: usize = 100;

// a static variable in `.data` to exercise the ELF data segment mapping
static mut P: u64 = 7;

#[unsafe(no_mangle)]
fn main() -> i32 {
    let p = unsafe { P };
    let m = 998244353u64;
    let iter: usize = 160000;
    let mut s = [0u64; LEN];
    let mut cur = 0usize;
    s[cur] = 1;
    for i in 1..=iter {
        let next = if cur + 1 == LEN { 0 } else { cur + 1 };
        s[next] = s[cur] * p % m;
        cur = next;
        if i % 10000 == 0 {
            println!("power_7 [{}/{}]", i, iter);
        }
    }
    println!("{}^{} = {}(MOD {})", p, iter, s[cur], m);
    println!("Test power_7 OK!");
    0
}
