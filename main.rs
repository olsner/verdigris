#![allow(ctypes)]
#![no_std]
#![no_main]
#![feature(globs)]
#![feature(asm)]

extern crate core;

use con::Console;
use con::Writer;
use start32::MultiBootInfo;
//use start32::OrigMultiBootInfo;
use start32::PhysAddr;
use start32::MutPhysAddr;
use x86::idt;

mod con;
mod mboot;
mod mem;
mod start32;
mod util;
mod x86;

static mut idt_table : [idt::Entry, ..48] = [idt::null_entry, ..48];

fn writeMBInfo(con : &mut Console, infop : *mboot::Info) {
	con.write("Multiboot info at ");
	con.writePtr(infop);
	con.putc('\n');

	let &info = unsafe { &*infop };
	con.write("Flags: ");
	con.writeHex(info.flags as uint);
	con.newline();

	if info.has(mboot::MemorySize) {
		con.writeUInt(info.mem_lower as uint);
		con.write("kB lower memory, ");
		con.writeUInt(info.mem_upper as uint);
		con.write("kB upper memory, ");
		con.writeUInt(((info.mem_lower + info.mem_upper + 1023) / 1024) as uint);
		con.write("MB total.\n");
	}
	// FIXME start32 doesn't copy this
	if info.has(mboot::CommandLine) {
		let cmdline : *u8 = PhysAddr(info.cmdline as uint);
		con.write("Command line @");
		con.writePtr(cmdline);
		con.write(" (");
		con.writeHex(info.cmdline as uint);
		con.write(") \"");
		con.writeCStr(cmdline);
		con.write("\"\n");
	}
}

pub fn generic_irq_handler(vec : u8) {
}

pub fn page_fault(error : u64) {
}

pub fn idle() -> ! {
	loop { unsafe { asm!("hlt"); } }
}

struct PerCpu {
	memory : mem::PerCpu,
}

impl PerCpu {
	fn new() -> PerCpu {
		PerCpu { memory : mem::PerCpu::new() }
	}

	fn run(&mut self) -> ! {
		// TODO: Pop something from run queue, run it
		idle();
	}
}

#[no_mangle]
pub unsafe fn start64() -> ! {
	let mut con = Console::new(MutPhysAddr(0xb8000), 80, 25);
	con.clear();
	con.write("Hello World!\n");

	x86::lgdt(start32::Gdtr());

	let handlers = [(14, idt::Error(page_fault))];
	idt::build(&mut idt_table, handlers, generic_irq_handler);
	idt::load(&idt_table);

	let &mut memory = &mut mem::global;
	memory.init(&*start32::MultiBootInfo(), start32::memory_start as uint, &mut con);
	con.write("Memory initialized. Free: ");
	con.writeUInt(memory.free_pages() * 4);
	con.write("KiB, Used: ");
	con.writeUInt(memory.used_pages() * 4);
	con.write("KiB\n");

	let mut cpu = PerCpu::new();
	cpu.memory.test(&mut con);

//	let mut i = 0;
//	loop {
//		con.writeUInt(i);
//		con.putc('\n');
//		i += 1;
//	}
	idle();
}
