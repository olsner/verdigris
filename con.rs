use core::prelude::*;
use core::cmp::min;

// NOTE: We cheat here - we know the memcpy in runtime.s copies from the
// beginning so we use this on overlapping ranges too.
// (To avoid having to implement memmove.)
extern "rust-intrinsic" {
    fn copy_nonoverlapping_memory<T>(dst: *mut T, src: *const T, count: uint);
}

#[inline]
unsafe fn copy_memory<T>(dst: *mut T, src: *const T, count: uint) {
    copy_nonoverlapping_memory(dst, src, count);
}

pub fn debugc(c : char) {
    unsafe { asm!("outb %al,$$0xe9": :"{al}"(c as u8) :: "volatile"); }
}

unsafe fn memset16(dst : *mut u16, v : u16, count : uint) {
    asm!("rep stosw" : : "{rdi}"(dst), "{ax}"(v), "{rcx}"(count) : "rdi", "rcx", "memory");
}

pub trait Writer {
    fn putc(&mut self, c : char);

    fn newline(&mut self) {
        self.putc('\n');
    }

    #[inline(never)]
    fn write(&mut self, string : &str) {
        for i in range(0, string.len()) {
            self.putc(string.as_bytes()[i] as char);
        }
    }

    fn writeInt(&mut self, x : int) {
        self.writeSigned(0, false, x);
    }

    fn writeUInt(&mut self, x : uint) {
        self.writeUnsigned(0, false, 10, false, x);
    }

    fn writeHex(&mut self, x : uint) {
        self.writeUnsigned(0, false, 16, true, x);
    }

    fn writePtr<T>(&mut self, x : *const T) {
        self.writePHex(x as uint);
    }
    fn writeMutPtr<T>(&mut self, x : *mut T) {
        self.writePHex(x as uint);
    }
    #[inline(never)]
    fn writePHex(&mut self, x : uint) {
        self.writeUnsigned(16, true, 16, true, x);
    }

    #[cfg(no_console)]
    fn writeUnsigned(&mut self, width : uint, leading_zero : bool, base : uint, show_base : bool, num : uint) {
        // nothing
    }

    #[cfg(not(no_console))]
    fn writeUnsigned(&mut self, width : uint, leading_zero : bool, base : uint, show_base : bool, num : uint)
    {
        if show_base && base == 16 {
            self.write("0x");
        }
        let mut buf : [u8, ..32] = [0, ..32];
        let mut len = 0;
        let mut num_ = num;
        loop {
            let i = num_ % base;
            buf[len] = "0123456789abcdef".as_bytes()[i];
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

    fn writeSigned(&mut self, width : uint, leading_zero : bool, num : int) {
        let abs = if num < 0 {
            self.putc('-');
            -num
        } else {
            num
        } as uint;
        self.writeUnsigned(width, leading_zero, 10, false, abs);
    }

    #[inline(never)]
    fn writeCStr(&mut self, c_str : *const u8) {
        unsafe {
            let mut p = c_str;
            while *p != 0 {
                self.putc(*p as char);
                p = p.offset(1);
            }
        }
    }
}

pub struct DebugCon;

impl Writer for DebugCon {
    fn putc(&mut self, c : char) {
        debugc(c);
    }
}

static mut con : Console = Console { buffer : 0 as *mut u16, position : 0, color : 0, debug : true };

pub struct Console {
    buffer : *mut u16,
    position : uint,
    pub color : u16,
    pub debug : bool,
}

pub fn init(buffer : *mut u16) {
    unsafe { con = Console::new(buffer); }
}

pub fn get() -> &'static mut Console {
    unsafe { &mut con }
}

pub fn dbg() -> DebugCon {
    return DebugCon;
}

impl Console {
    pub fn new(buffer : *mut u16) -> Console {
        Console {
            buffer : buffer,
            position : 0,
            color : 0x0f00,
            debug : true,
        }
    }

    pub fn putchar(&self, position : uint, c : u16) {
        unsafe {
            *self.buffer.offset(position as int) = c;
        }
    }

    pub fn clear(&mut self) {
        for i in range(0, 80*24 as uint) {
            self.putchar(i, 0);
        }
        self.position = 0;
    }

    fn width(&self) -> uint { 80 }
    fn height(&self) -> uint { 24 }

    fn clear_eol(&mut self) {
        let count = self.width() - (self.position % self.width());
        self.clear_range(self.position, count);
        self.position += count;
    }

    #[inline(always)]
    pub fn clear_range(&self, start : uint, length : uint) {
        unsafe {
            memset16(self.buffer.offset(start as int), self.color, length);
        }
    }

    #[inline(always)]
    fn copy_back(&mut self, to : uint, from : uint, n : uint) {
        unsafe {
            let b = self.buffer;
            copy_memory(b.offset(to as int), b.offset(from as int) as *const u16, n);
        }
    }

    #[inline(always)]
    fn scroll(&mut self) {
        let dist = self.width();
        let count = self.width() * (self.height() - 1);
        self.copy_back(0, dist, count);
        self.clear_range(count, dist);
        self.position = count;
    }
}

#[cfg(no_console)]
impl Writer for Console {
    #[inline(always)]
    fn putc(&mut self, c : char) {
        if self.debug { debugc(c); }
    }
}

#[cfg(not(no_console))]
impl Writer for Console {
    #[inline(never)]
    fn putc(&mut self, c : char) {
        if self.debug { debugc(c); }
        if c == '\n' {
            self.clear_eol();
        } else {
            let value = (c as u8) as u16 | self.color;
            self.putchar(self.position, value);
            self.position += 1;
        }
        if self.position >= self.width() * self.height() {
            self.scroll();
        }
    }
}


pub fn clear() { get().clear(); }
#[inline(never)]
pub fn newline() { get().newline(); }
#[inline(never)]
pub fn putc(c : char) { get().putc(c); }
#[inline(never)]
pub fn write(string : &str) { get().write(string); }
pub fn writeCStr(c_str : *const u8) { get().writeCStr(c_str); }
pub fn writeHex(x : uint) { get().writeHex(x); }
pub fn writeInt(x : int) { get().writeInt(x); }
pub fn writePHex(x : uint) { get().writePHex(x); }
pub fn writePtr<T>(x : *const T) { get().writePtr(x); }
pub fn writeMutPtr<T>(x : *mut T) { get().writeMutPtr(x); }
#[inline(never)]
pub fn writeUInt(x : uint) { get().writeUInt(x); }
