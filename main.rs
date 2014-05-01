#![allow(ctypes)]
#![no_std]
#![no_main]
#![feature(globs)]
#![feature(asm)]
// Adding pub mod (another fix for this warning) increases footprint, so just
// disble it instead.
#![allow(visible_private_types)]

extern crate core;

use core::prelude::*;
use core::mem::transmute;

use con::write;
use dlist::DList;
use process::Process;
use start32::MultiBootInfo;
//use start32::OrigMultiBootInfo;
use start32::PhysAddr;
use start32::MutPhysAddr;
use util::abort;
use x86::idt;

#[allow(dead_code)]
mod con;
mod dlist;
mod mboot;
mod mem;
mod process;
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
	// Just after syscall entry, this will actually be the user process' rsp.
	stack : *mut u8,
	memory : mem::PerCpu,
	runqueue : DList<Process>,
}

impl PerCpu {
	unsafe fn new() -> *mut PerCpu {
		let p = mem::global.alloc_frame_panic() as *mut PerCpu;
		*p = PerCpu {
			selfp : p,
			stack : mem::global.alloc_frame_panic(),
			memory : mem::PerCpu::new(),
			runqueue : DList::empty(),
		};
		return p
	}

	unsafe fn start(&mut self) {
		setup_msrs(self.selfp as uint);
	}

	fn queue(&mut self, p: &mut Process) {
		if !p.is_queued() {
			p.set(process::Queued);
			self.runqueue.append(p);
		}
	}

	unsafe fn run(&mut self) -> ! {
		match self.runqueue.pop() {
			Some(p) => self.switch_to(&mut *p),
			None => idle()
		}
	}

	unsafe fn switch_to(&mut self, p: &mut Process) -> ! {
		write("switch_to ");
		con::writeMutPtr(p as *mut Process);
		con::newline();
		p.unset(process::Queued);
		p.set(process::Running);
		loop {}
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

// Note: tail-called from the syscall code, return by switching to a process.
#[no_mangle]
pub fn syscall(
	// Parameter list of doom :/ If we fix the relevant bits of the process
	// struct we can move some of this into the syscall.s asssembly instead.

	// syscall arguments. Quite annoying that rax isn't acessible though.
	_rdi: uint,
	_rsi: uint,
	_rdx: uint,
	_r10: uint,
	_r8: uint,
	_nr : uint, // saved_rax
	_r9: uint,

	// user-process' old flags and rip, needs to be saved in the process too
	_rip: uint,
	_rflags: uint,
	// callee-save registers we need to save in the process structure
	_saved_rbp: uint,
	_saved_rbx: uint,
	_saved_r12: uint,
	_saved_r13: uint,
	_saved_r14: uint,
	_saved_r15: uint
) -> ! {
	con::write("syscall!\n");
	abort();
}

unsafe fn setup_msrs(gs : uint) {
	use x86::msr::*;
	use x86::rflags;
	use x86::efer;
	use x86::seg;
	#[allow(dead_code)]
	extern {
		fn syscall_entry_stub();
		fn syscall_entry_compat();
	}

	wrmsr(STAR, (seg::user_code32_base << 16) | seg::code);
	wrmsr(LSTAR, syscall_entry_stub as uint);
	wrmsr(CSTAR, syscall_entry_compat as uint);
	wrmsr(FMASK, rflags::IF | rflags::VM);
	wrmsr(EFER, rdmsr(EFER) | efer::SCE | efer::NXE);
	wrmsr(GSBASE, gs);
}

#[lang="eh_personality"]
fn dummy() {}

fn new_proc_simple(start : uint, end_unaligned : uint) -> *mut Process {
	let end = (end_unaligned + 0xfff) & !0xfff;
	let start_page = start & !0xfff;
	let ret : *mut Process = unsafe { transmute(~Process::new()) };
	let &mut res = unsafe { &mut *ret };
	res.regs.set_rsp(0x100000);
	res.regs.set_rip(0x100000 + (start & 0xfff));
	// TODO more
	return ret;
}

fn assoc_procs(p : &mut Process, i : uint, q : &mut Process, j : uint) {
	con::writeMutPtr(p as *mut Process);
	con::putc(':');
	con::writeUInt(i);
	write(" <-> ");
	con::writeUInt(j);
	con::putc(':');
	con::writeMutPtr(q as *mut Process);
	con::newline();
}

fn init_modules(cpu : &mut PerCpu) {
	let &info = start32::MultiBootInfo();
	if !info.has(mboot::Modules) {
		return;
	}
	let mut head = DList::empty();
	let mut count = 0;
	for m in info.modules(start32::PhysAddr).iter() {
		write("Module ");
		con::writeHex(m.start as uint);
		write("..");
		con::writeHex(m.end as uint);
		write(": ");
		con::writeCStr(start32::PhysAddr(m.string as uint));
		con::newline();

		head.append(new_proc_simple(m.start as uint, m.end as uint));
		count += 1;
	}
	con::writeUInt(count);
	con::newline();
	// Now all processes are in our list. We need to remove them before it
	// gets possible to make them runnable.
	let mut i = 0;
	for p in head.iter() {
		i += 1;
		// start iterating at j = i + 1, and q = p.next...
		let mut j = 0;
		for q in head.iter() {
			j += 1;
			if i < j {
				assoc_procs(p, i, q, j);
			}
		}
	}
	while match head.pop() {
		Some(p) => { cpu.queue(unsafe { &mut *p }); true },
		None => false
	} {}
}

#[no_mangle]
pub unsafe fn start64() -> ! {
	con::init(MutPhysAddr(0xb8000), 80, 25);
	con::clear();
	write("Hello World!\n");
	//writeMBInfo(start32::MultiBootInfo());

	x86::lgdt(start32::Gdtr());

	let handlers = [(14, idt::Error(page_fault))];
	idt::build(&mut idt_table, handlers, generic_irq_handler);
	idt::load(&idt_table);

	mem::global.init(start32::MultiBootInfo(), start32::memory_start as uint);
	write("Memory initialized. ");
	mem::global.stat();

	let pcpu = PerCpu::new();
	let &mut cpu = &*pcpu;
	cpu.start();
	cpu.memory.test();
	mem::global.stat();

	init_modules(&mut cpu);

//	let mut i = 0;
//	loop {
//		con.writeUInt(i);
//		con.putc('\n');
//		i += 1;
//	}
	cpu.run();
}
