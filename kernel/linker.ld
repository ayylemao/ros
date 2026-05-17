ENTRY(kstart)

KERNEL_BASE = 0xffffffff80000000;

PHDRS {
    text PT_LOAD FLAGS(0x5); /* R + X */
    rodata PT_LOAD FLAGS(0x4); /* R */
    data PT_LOAD FLAGS(0x6); /* R + W */
}

SECTIONS
{
    . = KERNEL_BASE;

    .text : ALIGN(4K) {
        *(.text .text.*)
    } :text

    .rodata : ALIGN(4K) {
        *(.rodata .rodata.*)
    } :rodata

    .data : ALIGN(4K) {
        *(.data .data.*)
    } :data

    .bss : ALIGN(4K) {
        *(.bss .bss.* COMMON)
    } :data

    __kernel_end = ALIGN(4K);

    /DISCARD/ : {
        *(.eh_frame*)
        *(.comment)
    }
}
