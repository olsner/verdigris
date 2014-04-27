#![allow(ctypes)]
#![no_std]
#![no_main]
#![feature(globs)]
#![feature(asm)]
// Adding pub mod (another fix for this warning) increases footprint, so just
// disble it instead.
#![allow(visible_private_types)]

extern crate core;

use con::write;
use start32::MultiBootInfo;
//use start32::OrigMultiBootInfo;
use start32::PhysAddr;
use start32::MutPhysAddr;
use x86::idt;

#[allow(dead_code)]
mod con;
mod mboot;
mod mem;
mod start32;
mod util;
mod x86;

static mut idt_table : [idt::Entry, ..48] = [idt::null_entry, ..48];

#[allow(dead_code)]
fn writeMBInfo(infop : *mboot::Info) {
	con::write("Multiboot info at ");
	con::writePtr(infop);
	con::putc('\n');

	let &info = unsafe { &*infop };
	con::write("Flags: ");
	con::writeHex(info.flags as uint);
	con::newline();

	if info.has(mboot::MemorySize) {
		con::writeUInt(info.mem_lower as uint);
		con::write("kB lower memory, ");
		con::writeUInt(info.mem_upper as uint);
		con::write("kB upper memory, ");
		con::writeUInt(((info.mem_lower + info.mem_upper + 1023) / 1024) as uint);
		con::write("MB total.\n");
	}
	// FIXME start32 doesn't copy this
	if info.has(mboot::CommandLine) {
		let cmdline : *u8 = PhysAddr(info.cmdline as uint);
		con::write("Command line @");
		con::writePtr(cmdline);
		con::write(" (");
		con::writeHex(info.cmdline as uint);
		con::write(") \"");
		con::writeCStr(cmdline);
		con::write("\"\n");
	}
}

pub fn generic_irq_handler(_vec : u8) {
}

pub fn page_fault(_error : u64) {
}

pub fn idle() -> ! {
	loop { unsafe { asm!("hlt"); } }
}

pub struct PerCpu {
	selfp : *mut PerCpu,
	memory : mem::PerCpu,
}

impl PerCpu {
	// TODO Always do a heap allocation so we can populate selfp with a proper
	// value
	fn new() -> PerCpu {
		PerCpu { selfp : 0 as *mut PerCpu, memory : mem::PerCpu::new() }
	}

	fn run(&mut self) -> ! {
		// TODO: Pop something from run queue, run it
		idle();
	}
}

// NB: One of the funky guarantees that Rust gives/requires is that there is
// at most one &mut reference to the same thing at any one time. This function
// can't quite guarantee that...
// This function also returns garbage as long as PerCpu::new doesn't fill in
// the selfp pointer.
pub fn cpu() -> &mut PerCpu {
	unsafe {
		let mut ret = 0;
		asm!("movq %gs:($0), $0" : "=r"(ret) : "0"(ret));
		return &mut *(ret as *mut PerCpu);
	}
}

#[lang="exchange_malloc"]
pub fn malloc(_size : uint) -> *mut u8 {
	return cpu().memory.alloc_frame_panic();
}

#[lang="exchange_free"]
pub fn free(p : *mut u8) {
	cpu().memory.free_frame(p);
}

#[no_mangle]
pub unsafe fn start64() -> ! {
	con::init(MutPhysAddr(0xb8000), 80, 25);
	con::clear();
	write("Hello World!\n");

	x86::lgdt(start32::Gdtr());

	let handlers = [(14, idt::Error(page_fault))];
	idt::build(&mut idt_table, handlers, generic_irq_handler);
	idt::load(&idt_table);

	mem::global.init(&*start32::MultiBootInfo(), start32::memory_start as uint);
	write("Memory initialized. ");
	mem::global.stat();

	let mut cpu = PerCpu::new();
	cpu.memory.test();
	mem::global.stat();

//	let mut i = 0;
//	loop {
//		con.writeUInt(i);
//		con.putc('\n');
//		i += 1;
//	}
	cpu.run();
}
