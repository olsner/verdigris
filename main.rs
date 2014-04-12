#![allow(ctypes)]
#![no_std]
#![no_main]
#![feature(globs)]
#![feature(asm)]

extern crate core;

use core::container::Container;

mod mboot;
mod start32;

static kernel_base : uint = - (1 << 30);

extern "rust-intrinsic" {
    //fn transmute<T, U>(x: T) -> U;

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

struct Console {
	buffer : *mut u16,
	position : int,
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

	unsafe fn write(&mut self, string : &str) {
		range(0, string.len(), |i| {
			let c = string[i] as u16;
			*mut_offset(self.buffer, self.position) = self.color | c;
			self.position += 1;
		});
	}
}

#[no_mangle] #[no_split_stack]
pub unsafe fn start64() {
    range(0, 80*25, |i| {
        *((kernel_base + 0xb8000 + i * 2) as *mut u16) = 0;
    });
	let mut con = Console::new((kernel_base + 0xb8000) as *mut u16, 80, 25);
	con.write("Hello World!");
	loop {}
}

#[no_mangle] #[no_split_stack]
pub unsafe fn abort() {
	let mut con = Console::new((kernel_base + 0xb8000) as *mut u16, 80, 25);
	con.write("aborted.");
	loop{} // asm!("cli; hlt")
}

#[no_mangle] #[no_split_stack]
pub unsafe fn breakpoint() {
	asm!("int3")
}
