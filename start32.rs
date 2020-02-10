#![allow(dead_code)]

use mboot;
use x86;

extern {
    static mbi_pointer : u32;
    static memory_start : u32;
    //static orig_mbi_pointer : u32;
    static gdtr : x86::Gdtr;
    static kernel_pdp : [u64; 512];
}

pub static kernel_base : u64 = -(1i64 << 30) as u64;

pub fn HighAddr<T>(obj : &T) -> &T {
    PhysAddrRef(obj as *const T as u64)
}

pub fn MutPhysAddr<T>(addr : u64) -> *mut T {
    (addr + kernel_base) as *mut T
}

pub fn PhysAddr<T>(addr: u64) -> *const T {
    (addr + kernel_base) as *const T
}

pub fn PhysAddrRef<'a, T>(addr : u64) -> &'a T {
    unsafe { &*PhysAddr(addr) }
}

pub fn MultiBootInfo() -> &'static mboot::Info {
    unsafe { PhysAddrRef(*HighAddr(&mbi_pointer) as u64) }
}

pub fn MemoryStart() -> u64 {
    unsafe { *HighAddr(&memory_start) as u64 }
}

// End of (physical) memory usable is fixed by kernel_base. More memory and we
// wrap around to null.
pub fn MemoryEnd() -> u64 {
    -(kernel_base as i64) as u64
}

pub fn Gdtr() -> &'static x86::Gdtr {
    unsafe { PhysAddrRef(&gdtr as *const x86::Gdtr as u64) }
}

//pub fn OrigMultiBootInfo() -> *mboot::Info {
//  PhysAddr(orig_mbi_pointer as uint)
//}

// TOOD: Remove hardcoded lower-half mappings from start32.o
pub fn CleanPageMappings() {
}

pub fn kernel_pdp_addr() -> u64 {
    unsafe { &kernel_pdp as *const [u64; 512] as u64 }
}
