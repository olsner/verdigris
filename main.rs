#![allow(ctypes)]
#![no_std]
#![no_main]
#![feature(globs)]
#![feature(asm)]

extern crate core;

use core::container::Container;
use start32::MultiBootInfo;

mod mboot;
mod start32;

static kernel_base : uint = - (1 << 30);

extern "rust-intrinsic" {
    fn offset<T>(dst: *T, offset: int) -> *T;
}
unsafe fn mut_offset<T>(dst: *mut T, off: int) -> *mut T {
	offset(dst as *T, off) as *mut T
}

fn range(lo: uint, hi: uint, it: |uint| -> ()) {
    let mut iter = lo;
    while iter < hi {
        it(iter);
        iter += 1;
    }
}

trait Writer {
	fn write(&mut self, string : &str);
}

fn writeUInt<T : Writer>(out : &mut T, x : uint) {
	out.write("<num>");
}

struct Console {
	buffer : *mut u16,
	position : uint,
	width : uint,
	height : uint,
	color : u16
}

impl Console {
	fn new(buffer : *mut u16, width : uint, height : uint) -> Console {
		Console {
			buffer : buffer,
			position : 0,
			width : width,
			height : height,
			color : 0x0f00,
		}
	}

	fn putchar(&mut self, position : uint, c : u16) {
	}

	fn clear(&mut self) {
		range(0, 80*25, |i| { unsafe {
			*mut_offset(self.buffer, i as int) = 0;
		}});
		self.position = 0;
	}
}

impl Writer for Console {
	fn write(&mut self, string : &str) {
		range(0, string.len(), |i| {
			let c = string[i] as u16;
			unsafe {
				*mut_offset(self.buffer, self.position as int) = self.color | c;
			}
			self.position += 1;
			if self.position > self.width * self.height {
				// TODO scroll
				self.position = 0;
			}
		});
	}
}

#[no_mangle]
pub unsafe fn start64() {
	let mut con = Console::new((kernel_base + 0xb8000) as *mut u16, 80, 25);
	con.clear();
	con.write("Hello World! ");
	writeUInt(&mut con, (*MultiBootInfo()).mem_upper as uint);
	loop {}
}

#[no_mangle]
pub unsafe fn abort() {
	let mut con = Console::new((kernel_base + 0xb8000) as *mut u16, 80, 25);
	con.write("aborted.");
	loop{} // asm!("cli; hlt")
}

#[no_mangle]
pub unsafe fn breakpoint() {
	asm!("int3")
}
