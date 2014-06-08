#![allow(ctypes)]
#![no_std]
#![feature(globs)]
#![feature(asm)]
// Adding pub mod (another fix for this warning) increases footprint, so just
// disble it instead.
#![allow(visible_private_types)]

extern crate core;

use core::prelude::*;
use core::mem::*;

use aspace::AddressSpace;
use con::write;
use dlist::DList;
use process::Process;
use start32::MultiBootInfo;
//use start32::OrigMultiBootInfo;
use start32::PhysAddr;
use start32::MutPhysAddr;
use util::abort;
use x86::idt;
pub use x86::idt::irq_entry;
pub use syscall::syscall;

mod aspace;
#[allow(dead_code)]
mod con;
mod dict;
mod dlist;
mod mboot;
mod mem;
mod process;
mod start32;
mod syscall;
pub mod util;
mod x86;

static log_assoc_procs : bool = false;
static log_page_fault : bool = false;
static log_switch : bool = false;
static log_queue : bool = false;
static log_irq : bool = false;
static log_idle : bool = false;

static mem_test : bool = false;

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

#[inline(never)]
pub fn generic_irq_handler(vec : u8) {
    if log_irq {
        write("IRQ! vec=");
        con::writeUInt(vec as uint);
        con::newline();
    }
    // 32..47 are IRQ interrupts, 48 is APIC, others are unused.
    if vec > 48 {
        abort("Invalid IRQ vector");
    }

    let c = cpu();
    let mask = 1 << (vec - 32);
    if c.irq_delayed & mask != 0 {
        write("IRQ: already delayed\n");
        return;
    }
    c.irq_delayed |= mask;
    if !c.irq_process().is_some() {
        abort("IRQ: no irq process");
    }
    let p = c.irq_process().unwrap();

    if log_irq {
        write("IRQ: proc=");
        con::writeMutPtr(p);
        con::newline();
    }

    syscall::try_deliver_irq(p);
}

pub fn page_fault(p : &mut Process, error : uint) -> ! {
    mod pf_errors {
        #![allow(dead_code)]
        // Error code flags.
        pub static PRESENT : uint = 1;
        pub static WRITE : uint = 2;
        pub static USER : uint = 4;
        pub static RSVD : uint = 8;
        pub static INSTR : uint = 16;
    }

    if log_page_fault {
        write("page fault ");
        con::writeHex(error);
        write(" cr2=");
        con::writePHex(x86::cr2());
        write(" rip=");
        con::writePHex(p.rip as uint);
        write(" in process ");
        con::writeMutPtr(p);
        con::newline();
    }

    if (error & pf_errors::USER) == 0 {
        abort("kernel page fault\n");
    }

    let fault_addr = x86::cr2();
    if (fault_addr as int) < 0 {
        abort("fault with kernel-space addr");
    }

    let back = p.aspace().find_add_backing(fault_addr & !0xfff);
    // FIXME should return e.g. Option<> so we can detect failures better than
    // just aborting in find_add_backing.
    p.aspace().add_pte(back.vaddr(), back.pte());

    unsafe { cpu().switch_to(p); }
}

pub fn handler_NM() {
    abort("NM");
}

pub fn idle() -> ! {
    loop {
        if log_idle {
            write("idle\n");
        }
        cpu().process = None;
        unsafe { asm!("sti; hlt; cli" :::: "volatile"); }
    }
}

pub struct PerCpu {
    selfp : *mut PerCpu,
    stack : *mut u8,
    process : Option<&'static mut Process>,

    // End of assembly-fixed fields.
    memory : mem::PerCpu,
    runqueue : DList<Process>,

    irq_process : Option<&'static mut Process>,
    irq_delayed : uint,
}

impl PerCpu {
    unsafe fn new() -> *mut PerCpu {
        let mut mem = mem::PerCpu::new();
        let p : *mut PerCpu = mem.alloc_frame_panic();
        let stack : *mut u8 = mem.alloc_frame_panic();
        *p = PerCpu {
            selfp : p,
            stack : stack,
            memory : mem,
            runqueue : DList::empty(),
            process : None,
            irq_process : None,
            irq_delayed : 0,
        };
        return p
    }

    unsafe fn start(&mut self) {
        setup_msrs(self.selfp as uint);
    }

    fn queue(&mut self, p: &mut Process) {
        if log_queue {
            write("queue ");
            con::writeMutPtr(p);
            if p.is(process::Queued) {
                write(" already queued");
            }
            con::newline();
        }
        if !p.is_queued() {
            p.set(process::Queued);
            self.runqueue.append(p);
        }
    }

    unsafe fn run(&mut self) -> ! {
        match self.runqueue.pop() {
            Some(p) => {
                let r = &mut *p;
                if !r.is_queued() {
                    abort("popped unqueued item?");
                }
                r.unset(process::Queued);
                self.switch_to(r);
            },
            None => idle()
        }
    }

    unsafe fn switch_to(&mut self, p: &mut Process) -> ! {
        if log_switch {
            write("switch_to ");
            con::writeMutPtr(p);
            write(" rip=");
            con::writeHex(p.rip as uint);
            if p.is(process::FastRet) {
                write(" fastret");
            }
            if p.is(process::Queued) {
                write(" queued");
            }
            con::newline();
        }
        p.set(process::Running);
        self.process = transmute(p as *mut Process);
        // TODO Check fpu_process, see if we need to set/reset TS bit in cr0
        x86::set_cr3(p.cr3);
        extern "C" {
            fn fastret(p : &mut Process, rax : uint) -> !;
            fn slowret(p : &mut Process) -> !;
        }
        if p.is(process::FastRet) {
            p.unset(process::FastRet);
            let rax = p.regs().rax;
            fastret(p, rax);
        } else {
            slowret(p);
        }
    }

    fn syscall_return(&mut self, p: &mut Process, rax : uint) -> ! {
        p.regs().rax = rax;
        unsafe { self.switch_to(p); }
    }

    fn get_process<'a>(&'a mut self) -> Option<&'a mut Process> {
        match self.process {
            Some(ref p) => Some(unsafe { &mut *(*p as *mut Process) }),
            None => None,
        }
    }

    fn leave_proc(&mut self) {
        match self.process {
            Some(ref mut p) => p.unset(process::Running),
            None => (),
        };
    }

    fn irq_process<'a>(&'a mut self) -> Option<&'a mut Process> {
        match self.irq_process {
            Some(ref p) => Some(unsafe { &mut *(*p as *mut Process) }),
            None => None,
        }
    }

    fn is_irq_process(&self, p: &mut Process) -> bool {
        match self.irq_process {
        None => false,
        Some(ref q) => (p as *mut Process) == (*q as *mut Process),
        }
    }
}

// NB: One of the funky guarantees that Rust gives/requires is that there is
// at most one &mut reference to the same thing at any one time. This function
// can't quite guarantee that...
pub fn cpu() -> &mut PerCpu {
    unsafe {
        let mut ret : *mut PerCpu;
        asm!("movq %gs:($1), $0" : "=r"(ret) : "r"(0));
        return &mut *ret;
    }
}

#[lang="exchange_malloc"]
#[inline(always)]
pub fn xmalloc(size : uint, _align: uint) -> *mut u8 {
    malloc(size)
}

pub fn malloc(size : uint) -> *mut u8 {
    if size > 4096 {
        abort("oversized malloc");
    }
    #[inline(never)]
    fn malloc_() -> *mut u8 {
        cpu().memory.alloc_frame_panic()
    }
    malloc_()
}

#[inline(always)]
pub fn alloc<T>() -> &mut T {
    unsafe { &mut *(malloc(size_of::<T>()) as *mut T) }
}

pub fn free<T>(p : *mut T) {
    #[inline(never)]
    fn free_(p: *mut u8) {
        cpu().memory.free_frame(p);
    }
    if p.is_not_null() {
        free_(p as *mut u8);
    }
}

#[lang="exchange_free"]
#[inline(always)]
pub fn xfree(p : *mut u8, _size: uint, _align: uint) {
    free(p);
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
    // FIXME: We want to clear a lot more flags - Direction for instance.
    // FreeBSD sets PSL_NT|PSL_T|PSL_I|PSL_C|PSL_D
    wrmsr(FMASK, rflags::IF | rflags::VM);
    wrmsr(EFER, rdmsr(EFER) | efer::SCE | efer::NXE);
    wrmsr(GSBASE, gs);
}

#[lang="eh_personality"]
fn dummy() {}

#[inline(never)]
fn new_proc_simple(start : uint, end_unaligned : uint) -> *mut Process {
    let end = (end_unaligned + 0xfff) & !0xfff;
    let start_page = start & !0xfff;
    let aspace : *mut AddressSpace = AddressSpace::new();
    let ret : *mut Process = Process::new(aspace);
    unsafe {
        (*ret).regs().rsp = 0x100000;
        (*ret).rip = 0x100000 + (start & 0xfff);
    }

    unsafe {
        use aspace::mapflag::*;
        // Stack at 1MB - 4kB (not executable)
        (*aspace).mapcard_set(0x0ff000, 0, 0, Anon | R | W);
        (*aspace).mapcard_set(0x100000, 0, start_page - 0x100000, Phys | R | X);
        (*aspace).mapcard_set(0x100000 + (end - start_page), 0, 0, 0);
        /*match (*ret).aspace().mapcard_find(0x100000) {
            Some(c) => {
                write("0x100000: ");
                con::writePHex(c.vaddr());
                write(" -> paddr ");
                con::writePHex(c.paddr(0x100000));
                con::newline();
            },
            None => {
                abort("mapcard we added is not there anymore");
            }
        }*/
    }
    return ret;
}

fn assoc_procs(p : &mut Process, i : uint, q : &mut Process, j : uint) {
    if log_assoc_procs {
        con::writeMutPtr(p);
        con::putc(':');
        con::writeUInt(i);
        write(" <-> ");
        con::writeUInt(j);
        con::putc(':');
        con::writeMutPtr(q);
        con::newline();
    }
    p.assoc_handles(j, q, i);
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
        if i == 0 {
            cpu.irq_process = Some(unsafe { &mut *(p as *mut Process) });
        }
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

pub fn dump_runqueue(queue: &DList<Process>) {
    let mut count = 0;
    for _ in queue.iter() {
        count += 1;
    }
    con::write("runqueue: ");
    con::writeUInt(count);
    con::newline();
    for p in queue.iter() {
        p.dump();
    }
}

#[no_mangle]
pub unsafe fn start64() -> ! {
    con::init(MutPhysAddr(0xb80a0));
    con::clear();
    write("Hello World!\n");
    //writeMBInfo(start32::MultiBootInfo());

    x86::lgdt(start32::Gdtr());
    x86::ltr(x86::seg::tss64);

    idt::init();

    mem::global.init(start32::MultiBootInfo(), start32::MemoryStart());
    write("Memory initialized. ");
    mem::global.stat();

    let pcpu = PerCpu::new();
    let ref mut cpu = *pcpu;
    cpu.start();
    if mem_test {
        cpu.memory.test();
        mem::global.stat();
    }

    init_modules(cpu);
    //dump_runqueue(&cpu.runqueue);

//  let mut i = 0;
//  loop {
//      con.writeUInt(i);
//      con.putc('\n');
//      i += 1;
//  }
    cpu.run();
}
