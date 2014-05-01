#![allow(dead_code)]

use mboot;
use x86;

extern {
    static mbi_pointer : u32;
	pub static memory_start : u32;
	//static orig_mbi_pointer : u32;
	static gdtr : x86::Gdtr;
	static kernel_pdp : [u64, ..512];
}

pub static kernel_base : uint = - (1 << 30);

pub fn MutPhysAddr<T>(addr : uint) -> *mut T {
	(addr + kernel_base) as *mut T
}

pub fn PhysAddr<T>(addr : uint) -> *T {
	(addr + kernel_base) as *T
}

pub fn MultiBootInfo() -> &'static mboot::Info {
	unsafe { &*PhysAddr(mbi_pointer as uint) }
}

pub fn Gdtr() -> &'static x86::Gdtr {
	unsafe { &*PhysAddr(&gdtr as *x86::Gdtr as uint) }
}

//pub fn OrigMultiBootInfo() -> *mboot::Info {
//	PhysAddr(orig_mbi_pointer as uint)
//}

// TOOD: Remove hardcoded lower-half mappings from start32.o
pub fn CleanPageMappings() {
}

pub fn kernel_pdp_addr() -> u64 {
	return &kernel_pdp as *[u64, ..512] as u64;
}
