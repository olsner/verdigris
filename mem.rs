use core::iter::Iterator;
use core::iter::range_step;
use core::option::*;
use core::ptr::offset;

use con;
use con::newline;
use con::write;
use con::writePHex;
use con::writePtr;
use con::writeUInt;
use mboot;
use mboot::MemoryMapItem;
use start32::PhysAddr;
use util::abort;

extern {
fn memset(dst : *mut u8, v : u8, count : uint);
}

struct FreeFrame {
	next : FreeFrameP
}
type FreeFrameP = Option<*mut FreeFrame>;

fn make_free_frame<T>(p : *T, next : FreeFrameP) -> FreeFrameP {
	let free = p as *mut FreeFrame;
	unsafe { (*free).next = next; }
	return Some(free);
}

pub struct Global {
	// Frames that are uninitialized (except for the first word)
	garbage : FreeFrameP,
	// Frames that are all zeroes except for the first word
	free : FreeFrameP,
	num_used : uint,
	num_total : uint,
}

pub static empty_global : Global = Global { garbage : None, free : None, num_used : 0, num_total : 0 };
pub static mut global : Global = empty_global;

pub struct PerCpu {
	free : FreeFrameP
}

struct MemoryMap {
	addr : *u8,
	end : *u8
}

impl MemoryMap {
	fn new(addr : *u8, length : uint) -> MemoryMap {
		return MemoryMap { addr : addr, end : unsafe { offset(addr, length as int) } }
	}
}

impl Iterator<MemoryMapItem> for MemoryMap {
	fn next(&mut self) -> Option<MemoryMapItem> {
		if self.addr < self.end { unsafe {
			let item = *(self.addr as *MemoryMapItem);
			self.addr = offset(self.addr, 4 + item.item_size as int);
			Some(item)
		} } else {
			None
		}
	}
}

fn push_frame<T>(head : &mut FreeFrameP, frame : *mut T) {
	let free = frame as *mut FreeFrame;
	unsafe { (*free).next = *head; }
	*head = Some(free);
}

fn clear(page : *mut FreeFrame) {
	unsafe { memset(page as *mut u8, 0, 4096); }
}

fn pop_frame(head : &mut FreeFrameP) -> FreeFrameP {
	match *head {
		Some(page) => {
			unsafe {
				*head = (*page).next;
				(*page).next = None;
			}
			Some(page)
		},
		None => None
	}
}

fn from_option<T>(x : Option<T>, def : T) -> T {
	match x {
		Some(val) => val,
		None => def
	}
}

impl Global {
	pub fn init(&mut self, info : &mboot::Info, min_addr : uint) {
		if !info.has(mboot::MemoryMap) {
			return;
		}

		let mut mmap = MemoryMap::new(PhysAddr(info.mmap_addr as uint), info.mmap_length as uint);
		let mut count = 0;
		for item in mmap {
			newline();
			writePHex(item.start as uint);
			newline();
			writePHex(item.length as uint);
			newline();
			writeUInt(item.item_type as uint);
			newline();
			for p in range_step(item.start, item.start + item.length, 4096) {
				if (p as uint > min_addr) {
					self.free_frame(PhysAddr(p as uint));
					count += 1;
				}
			}
		}
		self.num_used = 0;
		self.num_total = count;
	}

	pub fn free_frame(&mut self, vpaddr : *u8) {
		self.num_used -= 1;
		push_frame(&mut self.garbage, vpaddr as *mut FreeFrame);
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

	pub fn alloc_frame_panic(&mut self) -> *mut FreeFrame {
		match self.alloc_frame() {
			Some(page) => page,
			None => abort()
		}
	}

	pub fn free_pages(&self) -> uint {
		self.num_total - self.num_used
	}

	pub fn used_pages(&self) -> uint {
		self.num_used
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
		PerCpu { free : None }
	}

	pub fn alloc_frame(&mut self) -> Option<*mut u8> {
		match pop_frame(&mut self.free) {
			Some(page) => { return Some(page as *mut u8); }
			None => {}
		}
		self.free = get().alloc_frame();
		return self.steal_frame();
	}

	pub fn steal_frame(&mut self) -> Option<*mut u8> {
		match get().alloc_frame() {
			Some(page) => Some(page as *mut u8),
			None => None
		}
	}

	pub fn alloc_frame_panic(&mut self) -> *mut u8 {
		match self.alloc_frame() {
			Some(page) => page,
			None => abort()
		}
	}

	pub fn free_frame(&mut self, page : *u8) {
		get().free_frame(page);
	}

	pub fn test(&mut self) {
		let mut head = None;
		let mut count = 0;
		loop {
			let p = self.alloc_frame();
//			write("Allocation #");
//			writeUInt(count);
//			write(": ");
//			writePtr(from_option(p, 0 as *mut u8) as *u8);
//			newline();
//			get().stat();
			match p {
				Some(pp) => push_frame(&mut head, pp),
				None => break
			}
			count += 1;
		}
		write("Allocated everything: ");
		writeUInt(count);
		write(" pages\n");
		get().stat();
		loop {
//			write("Allocation #");
//			writeUInt(count);
//			write(": ");
//			writePtr(from_option(head, 0 as *mut FreeFrame) as *u8);
//			newline();
//			get().stat();
			match pop_frame(&mut head) {
				Some(p) => self.free_frame(p as *u8),
				None => break
			}
			count -= 1;
		}
	}
}
