use core::arch::asm;

pub fn init() {
    unsafe {
        asm!("out dx, al", in("dx") 0x3F8 + 1, in("al") 0x00u8);

        asm!("out dx, al", in("dx") 0x3F8 + 3, in("al") 0x80u8);

        asm!("out dx, al", in("dx") 0x3F8 + 0, in("al") 0x01u8);
        asm!("out dx, al", in("dx") 0x3F8 + 1, in("al") 0x00u8);

        asm!("out dx, al", in("dx") 0x3F8 + 3, in("al") 0x03u8);

        asm!("out dx, al", in("dx") 0x3F8 + 2, in("al") 0xC7u8);

        asm!("out dx, al", in("dx") 0x3F8 + 4, in("al") 0x0Bu8);
    }
}

pub fn console_putchar(c: usize) {
    unsafe {
        loop {
            let status: u8;
            asm!("in al, dx", out("al") status, in("dx") 0x3F8 + 5);
            if status & 0x20 != 0 {
                break;
            }
        }
        asm!("out dx, al", in("dx") 0x3F8 + 0, in("al") c as u8);
    }
}

pub fn shutdown(_failure: bool) -> ! {
    unsafe {
        asm!("out dx, ax", in("dx") 0x604u16, in("ax") 0x2000u16);
        asm!("out dx, ax", in("dx") 0xB004u16, in("ax") 0x2000u16);
        asm!("out dx, eax", in("dx") 0xf4u16, in("eax") 0x31u32);
    }
    loop {
        unsafe { asm!("hlt") }
    }
}
