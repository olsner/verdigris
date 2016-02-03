use start32::kernel_base;
use con::Console;
use con::Writer;

#[link_name="abort"] #[allow(dead_code)]
pub extern "C" fn abort2(msg: &'static str) -> ! {
    let mut con = Console::new((kernel_base + 0xb80a0) as *mut u16);
    con.color = 0x4f00;
    con.write(msg);
    con.newline();
    loop {
        unsafe { asm!("cli; hlt"); }
    }
}

pub fn abort(s : &'static str) -> ! {
    abort2(s);
}

#[no_mangle] #[allow(dead_code)]
pub fn breakpoint() {
    unsafe { asm!("int3") }
}

#[no_mangle]
pub extern "C" fn rust_begin_unwind() -> ! {
    abort("rust_begin_unwind");
}
#[no_mangle]
pub extern "C" fn rust_fail_bounds_check() -> ! {
    abort("rust_fail_bounds_check");
}

pub fn concat<U, T : Concat<U>>(h: T, l : T) -> U {
    h.concat(l)
}

pub trait Concat<Full> {
    fn concat(self, low : Self) -> Full;
}
impl Concat<u32> for u16 {
    fn concat(self, l: u16) -> u32 {
        ((self as u32) << 16) | (l as u32)
    }
}
impl Concat<u64> for u32 {
    fn concat(self, l: u32) -> u64 {
        ((self as u64) << 32) | (l as u64)
    }
}
