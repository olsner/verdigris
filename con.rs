use core::cmp::min;

// NOTE: We cheat here - we know the memcpy in runtime.s copies from the
// beginning so we use this on overlapping ranges too.
// (To avoid having to implement memmove.)
#[inline]
unsafe fn copy_memory<T>(dst: *mut T, src: *const T, count: usize) {
    use core::intrinsics::copy_nonoverlapping;
    copy_nonoverlapping(src, dst, count);
}

pub fn debugc(c : char) {
    unsafe { asm!("outb %al,$$0xe9": :"{al}"(c as u8) :: "volatile"); }
}

unsafe fn memset16(dst : *mut u16, v : u16, count : usize) {
    asm!("rep stosw" : : "{rdi}"(dst), "{ax}"(v), "{rcx}"(count) : "rdi", "rcx", "memory");
}

pub trait Writer {
    fn putc(&mut self, c : char);

    fn newline(&mut self) {
        self.putc('\n');
    }

    #[inline(never)]
    fn write(&mut self, string : &str) {
        for b in string.as_bytes() {
            self.putc(*b as char);
        }
    }

    fn writeInt(&mut self, x : isize) {
        self.writeSigned(0, false, x);
    }

    fn writeUInt(&mut self, x : usize) {
        self.writeUnsigned(0, false, 10, false, x);
    }

    fn writeHex(&mut self, x : usize) {
        self.writeUnsigned(0, false, 16, true, x);
    }

    fn writePtr<T>(&mut self, x : *const T) {
        self.writePHex(x as usize);
    }
    fn writeMutPtr<T>(&mut self, x : *mut T) {
        self.writePHex(x as usize);
    }
    #[inline(never)]
    fn writePHex(&mut self, x : usize) {
        self.writeUnsigned(16, true, 16, true, x);
    }

    #[cfg(no_console)]
    fn writeUnsigned(&mut self, width : usize, leading_zero : bool, base : usize, show_base : bool, num : usize) {
        // nothing
    }

    #[cfg(not(no_console))]
    fn writeUnsigned(&mut self, width : usize, leading_zero : bool, base : usize, show_base : bool, num : usize)
    {
        if show_base && base == 16 {
            self.write("0x");
        }
        let mut buf : [u8; 32] = [0; 32]; // TODO redundant type annotation?
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
            for _ in 0..min(width - len, width) {
                self.putc(c);
            }
        }
        while len > 0 {
            len -= 1;
            self.putc(buf[len] as char);
        }
    }

    fn writeSigned(&mut self, width : usize, leading_zero : bool, num : isize) {
        let abs = if num < 0 {
            self.putc('-');
            -num
        } else {
            num
        } as usize;
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
    position : usize,
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

    pub fn putchar(&self, position : usize, c : u16) {
        unsafe {
            *self.buffer.offset(position as isize) = c;
        }
    }

    pub fn clear(&mut self) {
        for i in 0..(80*24) {
            self.putchar(i, 0);
        }
        self.position = 0;
    }

    fn width(&self) -> usize { 80 }
    fn height(&self) -> usize { 24 }

    fn clear_eol(&mut self) {
        let count = self.width() - (self.position % self.width());
        self.clear_range(self.position, count);
        self.position += count;
    }

    #[inline(always)]
    pub fn clear_range(&self, start : usize, length : usize) {
        unsafe {
            memset16(self.buffer.offset(start as isize), self.color, length);
        }
    }

    #[inline(always)]
    fn copy_back(&mut self, to : usize, from : usize, n : usize) {
        unsafe {
            let b = self.buffer;
            copy_memory(b.offset(to as isize), b.offset(from as isize) as *const u16, n);
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
pub fn writeHex<T : Unsigned>(x : T) { x.writeHex(); }
pub fn writeInt<T : Signed>(x : T) { x.writeInt(); }
pub fn writePHex<T : Unsigned>(x : T) { x.writePHex(); }
pub fn writePtr<T>(x : *const T) { get().writePtr(x); }
pub fn writeMutPtr<T>(x : *mut T) { get().writeMutPtr(x); }
#[inline(never)]
pub fn writeUInt<T : Unsigned>(x : T) { x.writeUInt(); }

pub trait Unsigned {
    fn writeHex(self);
    fn writePHex(self);
    fn writeUInt(self);
}
pub trait Signed {
    fn writeInt(self);
}
impl Unsigned for usize {
    fn writeHex(self) { get().writeHex(self); }
    fn writePHex(self) { get().writePHex(self); }
    fn writeUInt(self) { get().writeUInt(self); }
}
impl Signed for isize {
    fn writeInt(self) { get().writeInt(self); }
}
macro_rules! unsigned {
    ($up:ident, $( $x:ident ),* ) => {
        $(
        impl Unsigned for $x {
            fn writeHex(self) { (self as $up).writeHex(); }
            fn writePHex(self) { (self as $up).writePHex(); }
            fn writeUInt(self) { (self as $up).writeUInt(); }
        }
        )*
    };
}
macro_rules! signed {
    ($up:ident, $( $x:ident ),* ) => {
        $(
        impl Signed for $x {
            fn writeInt(self) { (self as $up).writeInt(); }
        }
        )*
    };
}
unsigned!(usize, u64, u32, u16, u8);
unsigned!(usize, i64, i32, i16, i8);
unsigned!(usize, isize);
signed!(isize, i64, i32, i16, i8);
signed!(isize, u64, u32, u16, u8);
signed!(isize, usize);
