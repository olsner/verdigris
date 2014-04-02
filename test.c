#include <stddef.h>

typedef unsigned char u8;
typedef unsigned short u16;

u8* const kernel_start = NULL - (1 << 30);

void start64() {
	u16* target = (u16*)(kernel_start + 0xb8000);
	for (int i = 80*25; i--; ) {
		*target++ = 'A' | 0xf00;
	}
	__asm__ __volatile__( "cli;hlt" );
}
