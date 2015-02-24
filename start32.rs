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

#[allow(unsigned_negation)]
pub static kernel_base : uint = -(1u << 30) as uint;

pub fn HighAddr<T>(obj : &T) -> &T {
    PhysAddrRef(obj as *const T as uint)
}

pub fn MutPhysAddr<T>(addr : uint) -> *mut T {
    (addr + kernel_base) as *mut T
}

pub fn PhysAddr<T>(addr: uint) -> *const T {
    (addr + kernel_base) as *const T
}

pub fn PhysAddrRef<'a, T>(addr : uint) -> &'a T {
    unsafe { &*PhysAddr(addr) }
}

pub fn MultiBootInfo() -> &'static mboot::Info {
    PhysAddrRef(*HighAddr(&mbi_pointer) as uint)
}

pub fn MemoryStart() -> uint {
    *HighAddr(&memory_start) as uint
}

// End of (physical) memory usable is fixed by kernel_base. More memory and we
// wrap around to null.
#[allow(unsigned_negation)]
pub fn MemoryEnd() -> uint {
    -kernel_base
}

pub fn Gdtr() -> &'static x86::Gdtr {
    PhysAddrRef(&gdtr as *const x86::Gdtr as uint)
}

//pub fn OrigMultiBootInfo() -> *mboot::Info {
//  PhysAddr(orig_mbi_pointer as uint)
//}

// TOOD: Remove hardcoded lower-half mappings from start32.o
pub fn CleanPageMappings() {
}

pub fn kernel_pdp_addr() -> u64 {
    return &kernel_pdp as *const [u64; 512] as u64;
}
