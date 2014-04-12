ENTRY(start32_mboot)
OUTPUT_FORMAT(binary)

MEMORY {
    ram : org = 0x100000, l = 8K
}

SECTIONS {
    . = 0x100000;

    .text : {
		start32.o(.text*);
        *(.text*);
		*(.rodata*);
		*(.data*);
		PROVIDE(section.data.end = .);
	} >ram

    /DISCARD/ : {
        *(.comment)
        *(.eh_frame)
        *(.rel.eh_frame)
    }
}
