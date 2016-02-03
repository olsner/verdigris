use core::prelude::*;

#[repr(C, packed)]
#[allow(dead_code)]
#[derive(Clone, Copy)]
pub struct Gdtr {
    limit : u16,
    base : u64,
}
pub unsafe fn lgdt(gdtr : &Gdtr) {
    asm!("lgdt $0" :: "*m" (*gdtr));
}
pub unsafe fn ltr(tr : u16) {
    asm!("ltr %ax" :: "{ax}"(tr));
}

pub fn cr2() -> u64 {
    let mut cr2 : u64;
    unsafe { asm!("mov %cr2, $0": "=r" (cr2)); }
    return cr2;
}

pub mod seg {
    #![allow(dead_code)]
    pub const code32 : u16 = 8;
    pub const data32 : u16 = 16;
    pub const code : u16 = 24;
    pub const data : u16 = 32;
    pub const tss64 : u16 = 40;
    pub const user_code32_base : u16 = 56;
    pub const user_data32_base : u16 = 64;
    pub const user_code64_base : u16 = user_code32_base + 16;
    pub const user_data64_base : u16 = user_code64_base + 8;
    pub const user_cs : u16 = user_code64_base | 3;
    pub const user_ds : u16 = user_cs + 8;
}

pub mod idt {

use core::prelude::*;
use core::iter::range_inclusive;

use x86::seg;
use util::concat;

static GatePresent : u8 = 0x80;
static GateTypeInterrupt : u8 = 0x0e;

pub fn entry(handler_ptr : *const u8) -> Entry {
    let handler = handler_ptr as u64;
    let low = concat(handler as u16, seg::code);
    let flags = (GatePresent | GateTypeInterrupt) as u16;
    let high = concat((handler >> 16) as u16, flags << 8);

    (concat(high, low), handler as u64 >> 32)
}

pub const null_entry : Entry = (0,0);

pub type Entry = (u64,u64);
pub type Table = [Entry; 49];

#[repr(packed)]
#[allow(dead_code)]
#[derive(Clone, Copy)]
pub struct Idtr {
    limit : u16,
    base : *const Table,
}

pub unsafe fn lidt(idtr : &Idtr) {
    asm!("lidt $0" :: "*m" (*idtr));
}
pub fn limit(_table : &[Entry; 49]) -> u16 {
    return 49 * 16 - 1;
}

pub unsafe fn load(table: *const [Entry; 49]) {
    let idtr = Idtr {
        limit : limit(&*table),
        base : table,
    };
    lidt(&idtr);
}

#[no_mangle]
pub fn irq_entry(vec : u8, err : u64) -> ! {
    use page_fault;
    use handler_NM;
    use generic_irq_handler;
    use cpu;
    cpu().leave_proc();
    let p = cpu().get_process();
    if vec == 7 {
        handler_NM();
    } else if vec == 14 {
        page_fault(p.unwrap(), err);
    } else if vec >= 32 {
        match p {
            Some(p) => cpu().queue(p),
            None => (),
        }
        generic_irq_handler(vec);
    }
    unsafe { cpu().run(); }
}

pub unsafe fn init() {
    extern {
        fn handler_PF_stub();
        fn handler_NM_stub();
        // We can generate this, probably in less than 68 bytes?
        static irq_handlers : [u32; 17];
    }
    static mut idt_table : [Entry; 49] = [null_entry; 49];
    idt_table[7] = entry(handler_NM_stub as *const u8);
    idt_table[14] = entry(handler_PF_stub as *const u8);
    for i in range_inclusive(32, 48) {
        idt_table[i] = entry((&irq_handlers[i - 32]) as *const u32 as *const u8);
    }
    load(&idt_table);
}

} // mod idt

pub mod msr {
    pub use self::MSR::*;

    pub enum MSR {
        EFER = 0xc000_0080,
        STAR = 0xc000_0081,
        LSTAR = 0xc000_0082,
        CSTAR = 0xc000_0083,
        FMASK = 0xc000_0084,
        GSBASE = 0xc000_0101
    }

    pub unsafe fn wrmsr(msr : MSR, val : u64) {
        asm!("wrmsr":
        : "{ecx}" (msr as u32),
          "{edx}" ((val >> 32) as u32),
          "{eax}" (val as u32)
        :
        : "volatile");
    }

    pub unsafe fn rdmsr(msr : MSR) -> u64 {
        let mut h : u32;
        let mut l : u32;
        asm!("rdmsr"
        : "={edx}" (h),
          "={eax}" (l)
        : "{ecx}" (msr as u32));
        return ((h as u64) << 32) | (l as u64);
    }

}

pub mod rflags {
    pub static IF : u64 = 1 << 9;
    pub static VM : u64 = 1 << 17;
}

pub mod efer {
    pub static SCE : u64 = 1;
    pub static NXE : u64 = 1 << 11;
}

pub unsafe fn set_cr3(cr3 : u64) {
    let mut old_cr3 : u64;
    asm!("movq %cr3, $0" : "=r"(old_cr3));
    if old_cr3 != cr3 {
        asm!("movq $0, %cr3" :: "r"(cr3));
    }
}
