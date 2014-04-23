use core::cmp::min;
use core::container::Container;
use core::ptr::offset;
use core::option::*;
use core::iter::*;

unsafe fn mut_offset<T>(dst: *mut T, off: int) -> *mut T {
	offset(dst as *T, off) as *mut T
}

// NOTE: memcpy is assumed to copy from the beginning (and will be used on
// overlapping ranges)
extern {
	fn memcpy(dst : *mut u8, src : *u8, count : uint);
}

pub fn debugc(c : char) {
	unsafe { asm!("outb %al,$$0xe9": :"{al}"(c as u8) :: "volatile"); }
}

pub trait Writer {
	fn putc(&mut self, c : char);

	fn newline(&mut self) {
		self.putc('\n');
	}

	fn write(&mut self, string : &str) {
		for i in range(0, string.len()) {
			self.putc(string[i] as char);
		}
	}

	fn writeUInt(&mut self, x : uint) {
		self.writeNumber(0, false, 10, false, x);
	}

	fn writeHex(&mut self, x : uint) {
		self.writeNumber(0, false, 16, true, x);
	}

	fn writePtr<T>(&mut self, x : *T) {
		self.writePHex(x as uint);
	}
	fn writePHex(&mut self, x : uint) {
		self.writeNumber(16, true, 16, true, x);
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
			for _ in range(0, min(width - len, width)) {
				self.putc(c);
			}
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

	#[inline(never)]
	fn writeCStr(&mut self, c_str : *u8) {
		unsafe {
			let mut p = c_str;
			while *p != 0 {
				self.putc(*p as char);
				p = offset(p, 1);
			}
		}
	}
}

pub static mut con : Console = Console { buffer : 0 as *mut u16, position : 0, width : 0, height : 0, color : 0 };

pub struct Console {
	buffer : *mut u16,
	position : uint,
	width : uint,
	height : uint,
	color : u16
}

pub fn init(buffer : *mut u16, width : uint, height : uint) {
	unsafe { con = Console::new(buffer, width, height); }
}

pub fn get() -> &'static mut Console {
	unsafe { &mut con }
}

impl Console {
	pub fn new(buffer : *mut u16, width : uint, height : uint) -> Console {
		Console {
			buffer : buffer,
			position : 0,
			width : width,
			height : height,
			color : 0x0f00,
		}
	}

	pub fn putchar(&mut self, position : uint, c : u16) {
		unsafe {
			*mut_offset(self.buffer, position as int) = c;
		}
	}

	pub fn clear(&mut self) {
		for i in range(0, 80*25 as uint) {
			self.putchar(i, 0);
		}
		self.position = 0;
	}

	fn clear_eol(&mut self) {
		let count = self.width - (self.position % self.width);
		self.clear_range(self.position, count);
		self.position += count;
	}

	pub fn clear_range(&mut self, start : uint, length : uint) {
		for i in range(start, start + length) {
			self.putchar(i, self.color);
		}
	}

	#[inline(never)]
	fn copy_back(&mut self, to : uint, from : uint, n : uint) {
		unsafe {
			let b = self.buffer as *mut u8;
			let dst = mut_offset(b, 2 * to as int);
			let src = offset(b as *u8, 2 * from as int);
			memcpy(dst, src, 2 * n);
		}
	}

	#[inline(never)]
	fn scroll(&mut self) {
		debugc('S');
		let dist = self.width;
		let count = self.width * (self.height - 1);
		self.copy_back(0, dist, count);
		self.clear_range(count, dist);
		self.position = count;
	}
}

#[cfg(no_console)]
impl Writer for Console {
	#[inline(always)]
	fn putc(&mut self, c : char) {
		debugc(c);
	}
}

#[cfg(not(no_console))]
impl Writer for Console {
	#[inline(never)]
	fn putc(&mut self, c : char) {
		debugc(c);
		if c == '\n' {
			self.clear_eol();
		} else {
			let value = (c as u8) as u16 | self.color;
			self.putchar(self.position, value);
			self.position += 1;
		}
		if self.position >= self.width * self.height {
			self.scroll();
		}
	}
}
