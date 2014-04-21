use core::iter::Iterator;
use core::iter::range_step;
use core::option::*;
use core::ptr::offset;

use con::Console;
use con::Writer;
use mboot;
use mboot::MemoryMapItem;
use start32::PhysAddr;

struct FreeFrame {
	next : Option<*FreeFrame>
}
type FreeFrameP = Option<*FreeFrame>;

fn make_free_frame<T>(p : *T, next : FreeFrameP) -> FreeFrameP {
	let free = p as *mut FreeFrame;
	unsafe { (*free).next = next; }
	return Some(free as *FreeFrame);
}

pub struct Global {
	// Frames that are uninitialized (except for the first word)
	garbage : FreeFrameP,
	// Frames that are all zeroes except for the first word
	free : FreeFrameP,
	num_used : uint,
	num_total : uint,
}

pub struct PerCpu {
	free : *FreeFrame
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

impl Global {
	pub fn new() -> Global {
		return Global { garbage : None, free : None, num_used : 0, num_total : 0 };
	}

	pub fn init(&mut self, info : &mboot::Info, min_addr : uint, con : &mut Console) {
		if !info.has(mboot::MemoryMap) {
			return;
		}

		let mut mmap = MemoryMap::new(PhysAddr(info.mmap_addr as uint), info.mmap_length as uint);
		let mut count = 0;
		for item in mmap {
			con.newline();
			con.writePHex(item.start as uint);
			con.newline();
			con.writePHex(item.length as uint);
			con.newline();
			con.writeUInt(item.item_type as uint);
			con.newline();
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
		self.garbage = make_free_frame(vpaddr, self.garbage);
	}

	pub fn free_pages(&self) -> uint {
		self.num_total - self.num_used
	}

	pub fn used_pages(&self) -> uint {
		self.num_used
	}
}
