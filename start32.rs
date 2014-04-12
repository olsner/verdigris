#![allow(dead_code)]

use mboot;

extern {
    static mbi_pointer : u32;
}

static kernel_base : uint = - (1 << 30);

pub unsafe fn MultiBootInfo() -> *mboot::Info {
	(mbi_pointer as uint + kernel_base) as *mboot::Info
}
