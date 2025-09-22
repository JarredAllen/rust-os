ENTRY(start)

SECTIONS {
    . = 0x01000000;

    .text :{
        KEEP(*(.text.start));
        *(.text .text.*);
        *(.eh_frame);
    }

    .rodata : ALIGN(4) {
        *(.rodata .rodata.*);
    }

    .data : ALIGN(4) {
        *(.data .data.*);
    }

    .bss : ALIGN(4) {
        *(.bss .bss.* .sbss .sbss.*);

        . = ALIGN(16);
        . += 64 * 1024; /* 64KB */
        __stack_top = .;

       ASSERT(. < 0x01800000, "too large executable");
    }
}
