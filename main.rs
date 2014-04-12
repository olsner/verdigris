#![allow(ctypes)]
#![no_std]
#![no_main]
#![feature(globs)]
#![feature(asm)]

extern crate core;

use core::cmp::min;
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
	fn putc(&mut self, c : char);

	fn write(&mut self, string : &str) {
		range(0, string.len(), |i| {
			self.putc(string[i] as char);
		});
	}

	fn writeUInt(&mut self, x : uint) {
		self.writeNumber(0, false, 10, false, x);
	}

	fn writeNumber(&mut self, width : uint, leading_zero : bool, base : uint, show_base : bool, num : uint)
	{
		if show_base && base == 16 {
			self.write("0x");
		}
		let mut buf : [u8, ..32] = [0, ..32];
		let mut len = 0;
		let mut num_ = num;
		loop {
			let i = num_ % base;
			buf[len] = "0123456789abcdef"[i];
			len += 1;
			num_ /= base;
			if num_ == 0 { break; }
		}
		if width > 0 {
			let c = if leading_zero { '0' } else { ' ' };
			range(0, min(len, width), |_| {
				self.putc(c);
			});
		}
		while len > 0 {
			len -= 1;
			self.putc(buf[len] as char);
		}
	}

	fn writeSNumber(&mut self, width : uint, leading_zero : bool, num : int) {
		let abs = if num < 0 {
			self.putc('-');
			-num
		} else {
			num
		} as uint;
		self.writeNumber(width, leading_zero, 10, false, abs);
	}

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
		unsafe {
			*mut_offset(self.buffer, position as int) = c;
		}
	}

	fn clear(&mut self) {
		range(0, 80*25, |i| { unsafe {
			*mut_offset(self.buffer, i as int) = 0;
		}});
		self.position = 0;
	}

	fn clear_eol(&mut self) {
		range(0, self.width - (self.position % self.width), |i| {
			self.putchar(self.position + i, 0);
		});
	}

	fn copy_back(&mut self, to : uint, from : uint, n : uint) {
		range(0, n, |i| {
			unsafe {
				*mut_offset(self.buffer, (to + i) as int) =
					*mut_offset(self.buffer, (from + i) as int);
			}
		});
	}

	fn scroll(&mut self, lines : uint) {
		if lines >= self.height {
			self.clear();
			return;
		}
		let dist = self.width * lines;
		self.copy_back(0, dist, self.width * self.height - dist);
		self.position -= dist;
	}

	fn newline(&mut self) {
		if self.position > self.width * (self.height - 1) {
			self.scroll(1);
		}
		self.position += self.width - (self.position % self.width);
		self.clear_eol();
	}
}

impl Writer for Console {
	fn putc(&mut self, c : char) {
		if c == '\n' {
			self.newline();
			return;
		}
		let value = (c as u8) as u16 | self.color;
		self.putchar(self.position, value);
		self.position += 1;
		if self.position == self.width * self.height {
			self.scroll(1);
		}
	}
}

#[no_mangle]
pub unsafe fn start64() {
	let mut con = Console::new((kernel_base + 0xb8000) as *mut u16, 80, 25);
	con.clear();
	con.write("Hello World!\n");
	con.writeUInt((*MultiBootInfo()).mem_upper as uint);
	let mut i = 0;
	loop {
		con.writeUInt(i);
		con.putc('\n');
		i += 1;
	}
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
