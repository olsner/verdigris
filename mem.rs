use core::prelude::*;
use core::iter::range_step;
use core::intrinsics::set_memory;

use con;
use con::Console;
use con::Writer;
use con::write;
use con::writeUInt;
use mboot;
use mboot::MemoryMapItem;
use start32::PhysAddr;
use start32::MutPhysAddr;
use util::abort;

use mem::framestack::*;

static log_alloc : bool = false;
static log_memory_map : bool = false;
static log_memtest : bool = false;
static mem_stats : bool = true;

pub mod framestack {
    use core::prelude::*;

    struct FreeFrame {
        next : FreeFrameS
    }
    pub type FreeFrameS = *mut FreeFrame;
    pub type FreeFrameP = Option<*mut FreeFrame>;

    pub const none : FreeFrameS = 0 as *mut FreeFrame;

    fn from_option<T>(x : Option<T>, def : T) -> T {
        match x {
            Some(val) => val,
            None => def
        }
    }

    pub fn store<T>(p : Option<*mut T>) -> *mut T {
        from_option(p, 0 as *mut T)
    }

    pub fn push_frame<T>(head : &mut FreeFrameS, frame : *mut T) {
        let free = frame as *mut FreeFrame;
        unsafe { (*free).next = *head; }
        *head = free;
    }

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
    num_used : uint,
    num_total : uint,
}

pub const empty_global : Global = Global { garbage : none, free : none, num_used : 0, num_total : 0 };
pub static mut global : Global = empty_global;

pub struct PerCpu {
    free : FreeFrameS
}

struct MemoryMap {
    addr : *const u8,
    end : *const u8
}

impl MemoryMap {
    fn new(addr : *const u8, length : uint) -> MemoryMap {
        return MemoryMap { addr : addr, end : unsafe { addr.offset(length as int) } }
    }
}

impl Iterator<MemoryMapItem> for MemoryMap {
    fn next(&mut self) -> Option<MemoryMapItem> {
        if self.addr < self.end { unsafe {
            let item = *(self.addr as *const MemoryMapItem);
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
    pub fn init(&mut self, info : &mboot::Info, min_addr : uint, max_addr : uint) {
        if !info.has(mboot::MemoryMap) {
            return;
        }

        let mut mmap = MemoryMap::new(PhysAddr(info.mmap_addr as uint), info.mmap_length as uint);
        let mut count = 0;
        for item in mmap {
            if log_memory_map {
                write("start=");
                con::writePHex(item.start as uint);
                write(" length=");
                con::writePHex(item.length as uint);
                write(" type=");
                con::writeUInt(item.item_type as uint);
                con::newline();
            }
            if item.item_type != mboot::MemoryTypeMemory as u32 {
                continue;
            }
            for p in range_step(item.start, item.start + item.length, 4096) {
                let addr = p as uint;
                if min_addr <= addr && addr < max_addr {
                    self.num_used = 1;
                    self.free_frame(MutPhysAddr(p as uint));
                    count += 1;
                }
            }
        }
        self.num_used = 0;
        self.num_total = count;
    }

    pub fn free_frame(&mut self, vpaddr : *mut u8) {
        self.num_used -= 1;
        if log_alloc {
            write("free_frame: ");
            con::writeMutPtr(vpaddr);
            con::newline();
        }
        if mem_stats {
            self.stat_line();
        }
        push_frame(&mut self.garbage, vpaddr);
    }

    pub fn alloc_frame(&mut self) -> FreeFrameP {
        let res = match pop_frame(&mut self.free) {
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
        };
        if log_alloc {
            write("alloc_frame: ");
            con::writeMutPtr(store(res));
            con::newline();
        }
        if mem_stats {
            self.stat_line();
        }
        res
    }

    pub fn free_pages(&self) -> uint {
        self.num_total - self.num_used
    }

    pub fn used_pages(&self) -> uint {
        self.num_used
    }

    #[inline(never)]
    fn stat_line(&self) {
        use start32::kernel_base;
        let mut con = Console::new((kernel_base + 0xb8000) as *mut u16);
        con.debug = false;
        con.color = 0x2f00;
        con.write("Memory: ");
        self.stat_(&mut con);
    }

    #[inline(never)]
    pub fn stat(&self) {
        self.stat_(con::get());
    }

    fn stat_(&self, con: &mut Console) {
        con.write("Free: ");
        con.writeUInt(self.free_pages() * 4);
        con.write("KiB, Used: ");
        con.writeUInt(self.used_pages() * 4);
        con.write("KiB\n");
    }
}

#[inline(always)]
pub fn get<'a>() -> &'a mut Global {
    unsafe { &mut global }
}

impl PerCpu {
    pub fn new() -> PerCpu {
        PerCpu { free : none }
    }

    #[inline(never)]
    pub fn alloc_frame_(&mut self) -> *mut u8 {
        match pop_frame(&mut self.free) {
            Some(page) => return page as *mut u8,
            None => {}
        }
        self.free = store(get().alloc_frame());
        return self.steal_frame();
    }

    pub fn steal_frame(&mut self) -> *mut u8 {
        match get().alloc_frame() {
            Some(page) => page as *mut u8,
            None => RawPtr::null()
        }
    }

    #[inline(always)]
    pub fn alloc_frame(&mut self) -> Option<*mut u8> {
        let res = self.alloc_frame_();
        if res.is_null() {
            None
        } else {
            Some(res)
        }
    }

    #[inline(never)]
    pub fn alloc_frame_panic<T>(&mut self) -> *mut T {
        let res = self.alloc_frame_();
        if res.is_null() { abort("OOM") }
        res as *mut T
    }

    pub fn free_frame(&mut self, page : *mut u8) {
        get().free_frame(page);
    }

    pub fn test(&mut self) {
        let mut head = none;
        let mut count = 0;
        loop {
            let p = self.alloc_frame_();
            if log_memtest {
                write("Allocation #");
                writeUInt(count);
                write(": ");
                con::writeMutPtr(p);
                con::newline();
                get().stat();
            }
            if p.is_null() {
                break;
            }
            push_frame(&mut head, p);
            count += 1;
        }
        if log_memtest {
            write("Allocated everything: ");
            writeUInt(count);
            write(" pages\n");
            get().stat();
        }
        loop {
            if log_memtest {
                write("Allocation #");
                writeUInt(count);
                write(": ");
                con::writeMutPtr(head);
                con::newline();
                get().stat();
            }
            match pop_frame(&mut head) {
                Some(p) => self.free_frame(p as *mut u8),
                None => break
            }
        }
    }
}

extern "rust-intrinsic" {
    fn copy_nonoverlapping_memory<T>(dst: *mut T, src: *const T, count: uint);
}

pub fn heap_copy<T>(x : T) -> *mut T {
    use alloc;
    unsafe {
        let res : *mut T = alloc();
        copy_nonoverlapping_memory(res, &x, 1);
        return res;
    }
}

