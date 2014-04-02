ENTRY(start32_mboot)
OUTPUT_FORMAT(binary)

MEMORY {
    ram : org = 0x100000, l = 12K
}

SECTIONS {
    . = 0x100000;

    .text : {
        /*runtime.o(.text)*/
		start32.o(.text)
        *(.text)
    } >ram
	
	.rodata : {
		*(.rodata);
		PROVIDE(section.data.end = .);
	} >ram

    /DISCARD/ : {
        *(.comment)
        *(.eh_frame)
        *(.rel.eh_frame)
    }
}
