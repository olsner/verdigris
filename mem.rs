use core::prelude::*;
use core::iter::range_step;
use core::intrinsics::set_memory;

use con::write;
use con::writeUInt;
use mboot;
use mboot::MemoryMapItem;
use start32::PhysAddr;
use start32::MutPhysAddr;
use util::abort;

use mem::framestack::*;

pub mod framestack {
    use core::prelude::*;

    struct FreeFrame {
        next : FreeFrameS
    }
    pub type FreeFrameS = *mut FreeFrame;
    pub type FreeFrameP = Option<*mut FreeFrame>;

    pub static none : FreeFrameS = 0 as *mut FreeFrame;

    fn from_option<T>(x : Option<T>, def : T) -> T {
        match x {
            Some(val) => val,
            None => def
        }
    }

    pub fn store(p : FreeFrameP) -> FreeFrameS {
        from_option(p, none)
    }

    #[inline]
    pub fn push_frame<T>(head : &mut FreeFrameS, frame : *mut T) {
        let free = frame as *mut FreeFrame;
        unsafe {
            let h = *head;
            (*free).next = h;
            *head = free;
        }
    }

    #[inline]
    pub fn pop_frame(head : &mut FreeFrameS) -> FreeFrameP {
        if *head == none {
            None
        } else {
            let page = *head;
            unsafe {
                *head = (*page).next;
                (*page).next = none;
            }
            Some(page)
        }
    }

}

pub struct Global {
    // Frames that are uninitialized (except for the first word)
    garbage : FreeFrameS,
    // Frames that are all zeroes except for the first word
    free : FreeFrameS,
    // 2^32 pages ~= 16TB
    num_used : u32,
    num_total : u32,
}

pub static empty_global : Global = Global { garbage : none, free : none, num_used : 0, num_total : 0 };
pub static mut global : Global = empty_global;

pub struct PerCpu {
    free : FreeFrameS
}

struct MemoryMap {
    addr : *u8,
    end : *u8
}

impl MemoryMap {
    fn new(addr : *u8, length : uint) -> MemoryMap {
        return MemoryMap { addr : addr, end : unsafe { addr.offset(length as int) } }
    }
}

impl Iterator<MemoryMapItem> for MemoryMap {
    fn next(&mut self) -> Option<MemoryMapItem> {
        if self.addr < self.end { unsafe {
            let item = *(self.addr as *MemoryMapItem);
            self.addr = self.addr.offset(4 + item.item_size as int);
            Some(item)
        } } else {
            None
        }
    }
}

fn clear<T>(page : *mut T) {
    unsafe { set_memory(page as *mut u8, 0, 4096); }
}

impl Global {
    pub fn init(&mut self, info : &mboot::Info, min_addr : uint) {
        if !info.has(mboot::MemoryMap) {
            return;
        }

        let mut mmap = MemoryMap::new(PhysAddr(info.mmap_addr as uint), info.mmap_length as uint);
        let mut count : u32 = 0;
        for item in mmap {
            if item.item_type != mboot::MemoryTypeMemory as u32 {
                continue;
            }
//          newline();
//          writePHex(item.start as uint);
//          newline();
//          writePHex(item.length as uint);
//          newline();
//          writeUInt(item.item_type as uint);
//          newline();
            for p in range_step(item.start, item.start + item.length, 4096) {
                if p as uint > min_addr {
                    self.free_frame(MutPhysAddr(p as uint));
                    count += 1;
                }
            }
        }
        self.num_used = 0;
        self.num_total = count;
    }

    #[inline(never)]
    pub fn free_frame(&mut self, vpaddr : *mut u8) {
        self.num_used -= 1;
        push_frame(&mut self.garbage, vpaddr);
    }

    pub fn alloc_frame(&mut self) -> FreeFrameP {
        match pop_frame(&mut self.free) {
            Some(page) => {
                self.num_used += 1;
                Some(page)
            },
            None => match pop_frame(&mut self.garbage) {
                Some(page) => {
                    self.num_used += 1;
                    clear(page);
                    Some(page)
                },
                None => { None }
            }
        }
    }

    pub fn free_pages(&self) -> uint {
        (self.num_total - self.num_used) as uint
    }

    pub fn used_pages(&self) -> uint {
        self.num_used as uint
    }

    #[inline(never)]
    pub fn stat(&self) {
        write("Free: ");
        writeUInt(self.free_pages() * 4);
        write("KiB, Used: ");
        writeUInt(self.used_pages() * 4);
        write("KiB\n");
    }
}

#[inline(always)]
pub fn get() -> &mut Global {
    unsafe { &mut global }
}

impl PerCpu {
    pub fn new() -> PerCpu {
        PerCpu { free : none }
    }

    pub fn alloc_frame(&mut self) -> Option<*mut u8> {
        match pop_frame(&mut self.free) {
            Some(page) => { return Some(page as *mut u8); }
            None => {}
        }
        self.free = store(get().alloc_frame());
        return self.steal_frame();
    }

    pub fn steal_frame(&mut self) -> Option<*mut u8> {
        match get().alloc_frame() {
            Some(page) => Some(page as *mut u8),
            None => None
        }
    }

    #[inline(never)]
    pub fn alloc_frame_panic<T>(&mut self) -> *mut T {
        match self.alloc_frame() {
            Some(page) => page as *mut T,
            None => abort("OOM")
        }
    }

    #[inline(always)]
    pub fn free_frame(&mut self, page : *mut u8) {
        get().free_frame(page);
    }

    pub fn test(&mut self) {
        let mut head = none;
//      let mut count = 0;
        loop {
            let p = self.alloc_frame();
//          write("Allocation #");
//          writeUInt(count);
//          write(": ");
//          writePtr(from_option(p, 0 as *mut u8) as *u8);
//          newline();
//          get().stat();
            match p {
                Some(pp) => push_frame(&mut head, pp),
                None => break
            }
//          count += 1;
        }
//      write("Allocated everything: ");
//      writeUInt(count);
//      write(" pages\n");
//      get().stat();
        loop {
//          write("Allocation #");
//          writeUInt(count);
//          write(": ");
//          writePtr(from_option(head, 0 as *mut FreeFrame) as *u8);
//          newline();
//          get().stat();
            match pop_frame(&mut head) {
                Some(p) => self.free_frame(p as *mut u8),
                None => break
            }
        }
    }
}
