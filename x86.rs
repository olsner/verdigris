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
pub unsafe fn ltr(tr : uint) {
    asm!("ltr %ax" :: "{ax}"(tr));
}

pub fn cr2() -> uint {
    let mut cr2 : uint;
    unsafe { asm!("mov %cr2, $0": "=r" (cr2)); }
    return cr2;
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

use core::prelude::*;

use process::Process;
use x86::seg;
use x86::Idtr;
use x86::lidt;

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
    Ignore,
    Error(fn(u64)),
    NoError(fn()),
    IRQ(fn(u8)),
}

impl Handler {
}

pub type BuildEntry = (u8, extern "C" unsafe fn());
pub type Entry = (u64,u64);
pub type Table = [Entry, ..49];

pub fn build(target : &mut [Entry, ..49], entries : &[BuildEntry]) {
    let table_size : uint = 49;
    for &(vec,handler) in entries.iter() {
        target[vec as uint] = entry(handler as *u8);
    }
}

pub fn limit(_table : &[Entry, ..49]) -> u16 {
    return 49 * 16 - 1;
}

pub unsafe fn load(table: *[Entry, ..49]) {
    static mut idtr : Idtr = Idtr { base : 0, limit : 0 };
    idtr.base = (table as *u8) as uint;
    idtr.limit = limit(&*table);
    lidt(&idtr);
}

#[no_mangle]
pub fn irq_entry(vec : u8, err : uint) -> ! {
    use page_fault;
    use handler_NM;
    use generic_irq_handler;
    use cpu;
    unsafe { cpu().leave_proc(); }
    if vec == 7 {
        handler_NM();
    } else if vec == 14 {
        page_fault(err);
    } else if vec >= 32 {
        generic_irq_handler(vec);
    }
    unsafe { cpu().run(); }
}

pub unsafe fn init() {
    extern {
        fn handler_PF_stub();
        fn handler_NM_stub();
        // We can generate this, probably in less than 68 bytes?
        static irq_handlers : [u32, ..17];
    }
    let handlers = [(14, handler_PF_stub), (7, handler_NM_stub)];
    static mut idt_table : [Entry, ..49] = [null_entry, ..49];
    build(&mut idt_table, handlers);
    for i in range(32 as uint,49) {
        idt_table[i] = entry((&irq_handlers[i - 32]) as *u32 as *u8);
    }
    load(&idt_table);
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
        let mut h : uint;
        let mut l : uint;
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

pub unsafe fn set_cr3(cr3 : uint) {
    let mut old_cr3 : uint;
    asm!("movq %cr3, $0" : "=r"(old_cr3));
    if old_cr3 != cr3 {
        asm!("movq $0, %cr3" :: "r"(cr3));
    }
}
