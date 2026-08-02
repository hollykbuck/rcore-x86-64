OUTPUT_FORMAT(elf64-x86-64)
ENTRY(_start)

VIRT_BASE = 0xffffffff80000000;

SECTIONS
{
    . = VIRT_BASE;

    skernel = .;

    .text : ALIGN(4K) {
        stext = .;
        *(.text .text.*)
        etext = .;
    }

    .rodata : ALIGN(4K) {
        srodata = .;
        *(.rodata .rodata.*)
        *(.srodata .srodata.*)
        erodata = .;
    }

    .data : ALIGN(4K) {
        sdata = .;
        *(.data .data.*)
        *(.sdata .sdata.*)
        KEEP(*(.limine_requests))
        edata = .;
    }

    .got : ALIGN(8) {
        *(.got .got.*)
        egot = .;
    }

    .bss : ALIGN(4K) {
        sbss_with_stack = .;
        *(.bss.stack)
        sbss = .;
        *(.bss .bss.*)
        *(.sbss .sbss.*)
        ebss = .;
    }

    ekernel = .;

    /DISCARD/ : {
        *(.dynsym)
        *(.gnu.hash)
        *(.hash)
        *(.dynstr)
        *(.dynamic)
        *(.got.plt)
        *(.eh_frame .eh_frame_hdr)
        *(.comment)
        *(.note.*)
    }
}
