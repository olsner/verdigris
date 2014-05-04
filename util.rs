use start32::kernel_base;
use con::Console;
use con::Writer;

mod detail {
#[no_mangle]
fn abort() -> ! {
    use util;
    util::abort("aborted.");
}
}

pub fn abort(s : &str) -> ! {
    unsafe {
        let mut con = Console::new((kernel_base + 0xb8000) as *mut u16, 80, 25);
        con.color = 0x4f00;
        con.write(s);
        loop {
        asm!("cli; hlt");
        }
    }
}

#[no_mangle] #[allow(dead_code)]
pub fn breakpoint() {
    unsafe { asm!("int3") }
}
