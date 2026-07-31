//! The limine boot protocol requests and the kernel entry point
//!
//! Limine boot a kernel as a raw binary (see `limine.conf`). It scans the
//! kernel image for the request tags emitted in the `.limine_requests`
//! section, fills in the corresponding responses, and finally jumps to
//! `_start`. All of the requests and the two markers must be placed inside
//! the `.limine_requests` section (which is `KEEP`ed by `linker.ld`).

use core::arch::global_asm;

global_asm!(
    ".pushsection .limine_requests, \"a\"",
    ".balign 8",

    // LIMINE_REQUESTS_START_MARKER
    ".quad 0xf6b8f4b39de7d1ae",
    ".quad 0xfab91a6940fcb9cf",
    ".quad 0x785c6ed015d3e316",
    ".quad 0x181e920a7852b9d9",

    // LIMINE_BASE_REVISION(6)
    ".quad 0xf9562b2d5c95a6c8",
    ".quad 0x6a7b384944536bdc",
    ".quad 6",

    // LIMINE_HHDM_REQUEST
    ".global limine_hhdm_request",
    "limine_hhdm_request:",
    ".quad 0xc7b1dd30df4c8b88",
    ".quad 0x0a82e883a194f07b",
    ".quad 0x48dcf1cb8ad2b852",
    ".quad 0x63984e959a98244b",
    ".quad 0",
    ".quad 0",

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

/// The response of `limine_hhdm_request`
#[repr(C)]
pub struct LimineHhdmResponse {
    pub revision: u64,
    /// offset of the higher-half direct map, `phys + offset == virt`
    pub offset: u64,
}

/// The request for the higher-half direct map
#[repr(C)]
pub struct LimineHhdmRequest {
    pub id: [u64; 4],
    pub revision: u64,
    pub response: Option<&'static mut LimineHhdmResponse>,
}

unsafe extern "C" {
    static mut limine_hhdm_request: LimineHhdmRequest;
}

/// Get the higher-half direct map offset. Limine maps all physical memory at
/// `phys + offset`, which is how we can touch arbitrary physical addresses
/// before taking over paging.
pub fn hhdm_offset() -> u64 {
    let req = core::ptr::addr_of_mut!(limine_hhdm_request);
    unsafe { (*req).response.as_ref() }
        .expect("hhdm request not responded")
        .offset
}
