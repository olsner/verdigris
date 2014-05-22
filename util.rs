use start32::kernel_base;
use con::Console;
use con::Writer;

#[link_name="abort"] #[allow(dead_code)]
pub extern "C" fn abort2(msg: &'static str) -> ! {
    let mut con = Console::new((kernel_base + 0xb8000) as *mut u16);
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
