use core::arch::global_asm;

global_asm!(
    ".pushsection .limine_requests, \"a\"",
    ".balign 8",

    // LIMINE_BASE_REVISION(6)
    ".quad 0xf9562b2d5c95a6c8",
    ".quad 0x6a7b384944536bdc",
    ".quad 6",

    // LIMINE_REQUESTS_START_MARKER
    ".quad 0xf6b8f4b39de7d1ae",
    ".quad 0xfab91a6940fcb9cf",
    ".quad 0x785c6ed015d3e316",
    ".quad 0x181e920a7852b9d9",

    // LIMINE_REQUESTS_END_MARKER
    ".quad 0xadc0e0531bb10d03",
    ".quad 0x9572709f31764c62",

    ".popsection",

    // Stack symbols for logging (Limine creates a real stack for us)
    ".section .bss.stack, \"aw\", @nobits",
    ".balign 16",
    ".globl boot_stack_lower_bound",
    "boot_stack_lower_bound:",
    ".space 32768",
    ".globl boot_stack_top",
    "boot_stack_top:",

    ".section .text",
    ".global _start",
    "_start:",
    "    lea rsp, [rip + boot_stack_top]",
    "    call rust_main",
);
