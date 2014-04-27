#[packed]
pub struct Gdtr {
	limit : u16,
	base : uint,
}
#[packed]
pub struct Idtr {
	limit : u16,
	base : uint,
}

// FIXME Would like to use "m", but it seems to be impossible to get the right
// thing sent.
pub unsafe fn lgdt(gdtr : &Gdtr) {
	asm!("lgdt ($0)" :: "r" (gdtr));
}
pub unsafe fn lidt(idtr : &Idtr) {
	asm!("lidt ($0)" :: "r" (idtr));
}

pub mod seg {
	#![allow(dead_code)]
	pub static code32 : uint = 8;
	pub static data32 : uint = 16;
	pub static code : uint = 24;
	pub static data : uint = 32;
	pub static tss64 : uint = 40;
	pub static user_code32_base : uint = 56;
	pub static user_data32_base : uint = 64;
	pub static user_code64_base : uint = user_code32_base + 16;
	pub static user_data64_base : uint = user_code64_base + 8;
	pub static user_cs : uint = user_code64_base | 3;
	pub static user_ds : uint = user_cs + 8;
}

pub mod idt {

use x86::seg;
use x86::Idtr;
use x86::lidt;
use core::iter::range;
use core::iter::Iterator;
use core::slice::iter;
use core::option::*;

static GatePresent : uint = 0x80;
static GateTypeInterrupt : uint = 0x0e;

pub fn entry(handler_ptr : *u8) -> Entry {
	let handler = handler_ptr as uint;
	let low = (handler & 0xffff) | (seg::code << 16);
	let flags = GatePresent | GateTypeInterrupt;
	let high = (((handler >> 16) & 0xffff) << 16) | (flags << 8);

	(low as u64 | (high as u64 << 32), handler as u64 >> 32)
}

pub static null_entry : Entry = (0,0);

pub enum Handler {
	Error(fn(u64)),
	NoError(fn()),
}

impl Handler {
	fn fn_ptr(&self) -> *u8 {
		match *self {
			Error(f) => f as *u8,
			NoError(f) => f as *u8
		}
	}
}

pub type BuildEntry = (u8, Handler);
pub type Entry = (u64,u64);
pub type Table = [Entry, ..48];

pub fn build(target : &mut [Entry, ..48], entries : &[BuildEntry], default : fn(u8)) {
	let default_entry = entry(default as *u8);
	let table_size : uint = 48;
	for i in range(0, table_size) {
		target[i] = default_entry;
	}
	for &(vec,handler) in iter(entries) {
		target[vec as uint] = entry(handler.fn_ptr());
	}
}

pub fn limit(_table : &[Entry, ..48]) -> u16 {
	return 48 * 16 - 1;
}

pub unsafe fn load(table: *[Entry, ..48]) {
	static mut idtr : Idtr = Idtr { base : 0, limit : 0 };
	idtr.base = (table as *u8) as uint;
	idtr.limit = limit(&*table);
	lidt(&idtr);
}

} // mod idt

pub mod msr {
	pub enum MSR {
		EFER = 0xc000_0080,
		STAR = 0xc000_0081,
		LSTAR = 0xc000_0082,
		CSTAR = 0xc000_0083,
		FMASK = 0xc000_0084,
		GSBASE = 0xc000_0101
	}

	pub unsafe fn wrmsr(msr : MSR, val : uint) {
		asm!("wrmsr":
		: "{ecx}" (msr as u32),
		  "{edx}" ((val >> 32) as u32),
		  "{eax}" (val as u32)
		:
		: "volatile");
	}

	pub unsafe fn rdmsr(msr : MSR) -> uint {
		let mut h : uint = 0;
		let mut l : uint = 0;
		asm!("rdmsr"
		: "={edx}" (h),
		  "={eax}" (l)
		: "{ecx}" (msr as u32));
		return (h << 32) | l;
	}

}

pub mod rflags {
	pub static IF : uint = 1 << 9;
	pub static VM : uint = 1 << 17;
}

pub mod efer {
	pub static SCE : uint = 1;
	pub static NXE : uint = 1 << 11;
}
