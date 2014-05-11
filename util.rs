pub mod detail {
use start32::kernel_base;
use con::Console;
use con::Writer;

#[no_mangle] #[allow(dead_code)]
pub extern "C" fn abort(msg: &str) -> ! {
    let mut con = Console::new((kernel_base + 0xb8000) as *mut u16, 80, 25);
    con.color = 0x4f00;
    con.write(msg);
    con.newline();
    loop {
        unsafe { asm!("cli; hlt"); }
    }
}
}

pub fn abort(s : &str) -> ! {
    detail::abort(s);
}

#[no_mangle] #[allow(dead_code)]
pub fn breakpoint() {
    unsafe { asm!("int3") }
}
