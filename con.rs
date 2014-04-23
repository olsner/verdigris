use core::cmp::min;
use core::container::Container;
use core::ptr::offset;
use core::option::*;
use core::iter::*;

unsafe fn mut_offset<T>(dst: *mut T, off: int) -> *mut T {
	offset(dst as *T, off) as *mut T
}

pub fn debugc(c : char) {
	unsafe { asm!("outb %al,$$0xe9": :"{al}"(c as u8) :: "volatile"); }
}

pub trait Writer {
	fn putc(&mut self, c : char);

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
		for i in range(0, self.width - (self.position % self.width)) {
			self.putchar(self.position + i, 0);
		}
	}

	fn copy_back(&mut self, to : uint, from : uint, n : uint) {
		for i in range(0, n) {
			unsafe {
				*mut_offset(self.buffer, (to + i) as int) =
					*mut_offset(self.buffer, (from + i) as int);
			}
		}
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

	#[cfg(not(no_console))]
	pub fn newline(&mut self) {
		debugc('\n');
		if self.position > self.width * (self.height - 1) {
			self.scroll(1);
		}
		self.position += self.width - (self.position % self.width);
		self.clear_eol();
	}

	#[inline(always)]
	#[cfg(no_console)]
	pub fn newline(&mut self) {
	}
}

#[cfg(no_console)]
impl Writer for Console {
	#[inline(always)]
	fn putc(&mut self, c : char) {
	}
}

#[cfg(not(no_console))]
impl Writer for Console {
	#[inline(never)]
	fn putc(&mut self, c : char) {
		if c == '\n' {
			self.newline();
			return;
		}
		debugc(c);
		let value = (c as u8) as u16 | self.color;
		self.putchar(self.position, value);
		self.position += 1;
		if self.position == self.width * self.height {
			self.scroll(1);
		}
	}
}

