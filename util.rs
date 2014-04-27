use start32::kernel_base;
use con::Console;
use con::Writer;

#[no_mangle]
pub fn abort() -> ! {
	unsafe {
		let mut con = Console::new((kernel_base + 0xb8000) as *mut u16, 80, 25);
		con.write("aborted.");
		loop {
		asm!("cli; hlt");
		}
	}
}

#[no_mangle] #[allow(dead_code)]
pub fn breakpoint() {
	unsafe { asm!("int3") }
}
