use core::prelude::*;

use aspace::mapflag;
use con;
use con::write;
use cpu;
use dump_runqueue;
use process;
use process::Handle;
use process::Process;
use start32::kernel_base;
use util::abort;

static log_syscall : bool = false;
static log_unknown_syscall : bool = false;
static log_transfer_message : bool = false;
static log_portio : bool = false;
static log_hmod : bool = false;
static log_map : bool = false;
static log_pfault : bool = false;
static log_grant : bool = false;

static log_recv : bool = false;
static log_ipc : bool = false;
static log_pulse : bool = false;

pub mod nr {
    #![allow(dead_code)]
    pub static RECV : uint = 0;
    pub static MAP : uint = 1;
    pub static PFAULT : uint = 2;
    pub static UNMAP : uint = 3;
    pub static HMOD : uint = 4;
    pub static NEWPROC : uint = 5;
    pub static WRITE : uint = 6;
    pub static PORTIO : uint = 7;
    pub static GRANT : uint = 8;
    pub static PULSE : uint = 9;

    pub static USER : uint = 16;

    pub static MSG_MASK : uint = 0xff;
    pub static MSG_KIND_MASK : uint = 0x300;
    pub static MSG_KIND_SEND : uint = 0x000;
    pub static MSG_KIND_CALL : uint = 0x100;

    pub fn call(msg: uint) -> uint {
        msg | MSG_KIND_CALL
    }
}

// Note: tail-called from the syscall code, "return" by switching to a process.
#[no_mangle]
pub fn syscall(
    // Note weird ordering: syscall uses di,si,dx,8,9,10 but normal calls use
    // di,si,dx,cx,8,9. The stub puts r10 in rcx.
    arg0: uint,
    arg1: uint,
    arg2: uint,
    arg5: uint, // rcx = syscall r10
    arg3: uint,
    arg4: uint,
    nr : uint, // saved_rax
    // TODO: since arg5 is almost unused, swap with rax
) -> ! {
    use syscall::nr::*;

    let p = cpu().get_process().unwrap();
    p.unset(process::Running);
    p.set(process::FastRet);

    match nr {
    RECV => ipc_recv(p, arg0),
    MAP => syscall_map(p, arg0, arg1, arg2, arg3, arg4),
    PFAULT => syscall_pfault(p, arg1, arg2), // arg0 is always 0
    // unmap
    HMOD => syscall_hmod(p, arg0, arg1, arg2),
    //newproc
    WRITE => {
        con::putc(arg0 as u8 as char);
        syscall_return(p, 0);
    },
    PORTIO => syscall_portio(p, arg0, arg1, arg2),
    GRANT => syscall_grant(p, arg0, arg1, arg2),
    PULSE => syscall_pulse(p, arg0, arg1),
    _ if nr >= USER => {
        match nr & MSG_KIND_MASK {
            MSG_KIND_CALL => ipc_call(p, nr, arg0, arg1, arg2, arg3, arg4, arg5),
            MSG_KIND_SEND => ipc_send(p, nr, arg0, arg1, arg2, arg3, arg4, arg5),
            _ => abort("Unknown IPC kind")
        }
    },
    _ => {
        if log_syscall || log_unknown_syscall {
            write("syscall! nr=");
            con::writeUInt(nr);
            write(" from process ");
            con::writeMutPtr(p);
            con::newline();
        }
        abort("Unhandled syscall")
    },
    }

    if p.is_runnable() {
        abort("process not blocked at return");
    }

    unsafe { cpu().run(); }
}

#[inline(never)]
fn ipc_call(p : &mut Process, msg : uint, to : uint, arg1: uint, arg2: uint,
    arg3: uint, arg4: uint, arg5: uint) {
    let log = log_ipc && to != 3;
    if log {
        con::writeMutPtr(p);
        write(" ipc_call to ");
        con::writeUInt(to);
    }

    let handle = p.find_handle(to);
    match handle {
    Some(h) => {
        if log {
            write("==> process ");
            con::writeMutPtr(h.process());
            con::newline();
        }

        p.set(process::InSend);
        p.set(process::InRecv);
        p.regs().rdi = to;
        send_or_block(p, h, msg, arg1, arg2, arg3, arg4, arg5);
    },
    None => abort("ipc_call: no recipient")
    }
    if log {
        write("ipc_call: blocked\n");
    }
}

fn transfer_set_handle(target: &mut Process, source: &mut Process) {
    let mut rcpt = target.regs().rdi;
    let from = source.regs().rdi;

    let h = source.find_handle(from).unwrap();
    if rcpt == 0 {
        rcpt = h.other().unwrap().id();
    } else if !target.find_handle(rcpt).is_some() {
        match h.other() {
            // Already associated, "junk" the fresh handle and just update the
            // recipient-side handle.
            Some(g) => {
                rcpt = g.id();
                if g.other != Some(h as *mut Handle) {
                    abort("other.other != self");
                }
                if log_transfer_message {
                    write("transfer_set_handle: g=");
                    con::writeMutPtr(g);
                    write(", g.id()=");
                    con::writeHex(rcpt);
                    con::newline();
                }
            },
            // Associate handles now.
            None => {
                let g = target.new_handle(rcpt, source);
                g.associate(h);
                if g.id() != rcpt {
                    abort("weird");
                }
            },
        }
    } else {
        if rcpt != h.other().unwrap().id() {
            abort("rcpt handle mismatch");
        }
        // TODO Assert that rcpt <-> from. (But the caller is responsible for
        // checking that first.)
    }
    if log_transfer_message {
        write("transfer_set_handle: rcpt=");
        con::writeHex(rcpt);
        write(" for ");
        con::writeHex(target.regs().rdi);
        write(" from ");
        con::writeHex(from);
        con::newline();
    }
    target.regs().rdi = rcpt;
}

fn transfer_message(target: &mut Process, source: &mut Process) -> ! {
    transfer_set_handle(target, source);

    if log_transfer_message {
        write("transfer_message ");
        con::writeMutPtr(target);
        write(" <- ");
        con::writeMutPtr(source);
        con::newline();
    }

    // FIXME Should use special fastret for message passing instead of
    // unsetting fastret and setting everything via the process struct.
    // Probably something like:
    // c.ipc_return(target, transfer_set_handle(...), source)
    // after updating source process runnability and state.

    target.regs().rax = source.regs().rax;
    // rdi is set by transfer_set_handle
    target.regs().rsi = source.regs().rsi;
    target.regs().rdx = source.regs().rdx;
    target.regs().r8 = source.regs().r8;
    target.regs().r9 = source.regs().r9;
    target.regs().r10 = source.regs().r10;

    target.unset(process::InRecv);
    target.unset(process::FastRet);
    source.unset(process::InSend);

    let c = cpu();
    if false && log_transfer_message {
        dump_runqueue(&c.runqueue);
        target.dump();
        source.dump();
    }
    source.remove_waiter(target);
    c.queue(target);

    if source.ipc_state() == 0 {
        target.remove_waiter(source);
        c.queue(source);
    }

    if false && log_transfer_message {
        dump_runqueue(&c.runqueue);
        target.dump();
        source.dump();
    }
    unsafe { c.run(); }
}

// TODO This and remaining IPC functions should probably be moved to a separate
// ipc module.
pub fn try_deliver_irq(p : &mut Process) {
    let c = cpu();
    let irqs = c.irq_delayed;
    if can_deliver_pulse(p, 0) {
        c.irq_delayed = 0;
        deliver_pulse(p, 0, irqs);
    }
    c.irq_delayed = irqs;
}

pub fn can_deliver_pulse(p : &mut Process, rcpt: uint) -> bool {
    let rdi = p.regs().rdi;
    p.ipc_state() == process::InRecv.mask() &&
    // If it's a receive from (wrong) specific, we can't deliver now.
    // Otherwise, both fresh and 0 is OK.
    (rdi == rcpt || !p.find_handle(rdi).is_some())
}

fn deliver_pulse(p: &mut Process, rcpt: uint, pulses: uint) -> ! {
    p.regs().rdi = rcpt;
    p.regs().rsi = pulses;
    // See comment in transfer_message about special ipc-return
    p.unset(process::FastRet);
    p.unset(process::InRecv);
    syscall_return(p, nr::PULSE);
}

fn send_or_block(sender : &mut Process, h : &mut Handle, msg: uint,
        arg1: uint, arg2: uint, arg3: uint, arg4: uint, arg5: uint) {
    let p = h.process();

    // Save regs - either we'll copy these in transfer_message or we'll
    // need to store them until later on when the transfer can finish.
    // FIXME: The instant transfer_message path should be able to avoid this.
    sender.regs().rax = msg;
    sender.regs().rdi = h.id();
    sender.regs().rsi = arg1;
    sender.regs().rdx = arg2;
    sender.regs().r10 = arg3;
    sender.regs().r8 = arg4;
    sender.regs().r9 = arg5;

    let other_id = match h.other() {
        Some(g) => g.id(),
        None => 0,
    };

    // p is the recipient, the sender is in g.process().
    if p.ipc_state() == process::InRecv.mask() {
        let rcpt = h.process().regs().rdi;
        // Check the receiving process' receipt handle
        //   0 ==> transfer
        //   !0, connected to our handle ==> transfer
        //   !0, fresh ==> transfer
        //   !0 otherwise ==> block
        if rcpt == other_id || !p.find_handle(rcpt).is_some() {
            transfer_message(p, sender);
        }
    }

    if log_ipc {
        write("send_or_block: ");
        con::writeMutPtr(sender);
        write(" waits for ");
        con::writeMutPtr(p);
        con::newline();
    }
    p.add_waiter(sender)
}

#[inline(never)]
fn ipc_send(p : &mut Process, msg : uint, to : uint, arg1: uint, arg2: uint,
        arg3: uint, arg4: uint, arg5: uint) {
    if log_ipc && to != 3 {
        con::writeMutPtr(p);
        write(" ipc_send to ");
        con::writeHex(to);
//        write(" ==>");
//        con::writeMutPtr(p.find_handle(to).unwrap().process());
        con::newline();
    }

    let handle = p.find_handle(to);
    match handle {
    Some(h) => {
        p.set(process::InSend);
        send_or_block(p, h, msg, arg1, arg2, arg3, arg4, arg5);
    },
    None => {
        p.dump();
        abort("ipc_send: no recipient")
    }
    }
}

#[inline(never)]
fn ipc_recv(p : &mut Process, from : uint) {
    let mut handle = None;
    if from != 0 {
        handle = p.find_handle(from);
    }

    if log_recv {
        con::writeMutPtr(p);
        write(" recv from ");
        con::writeUInt(from);
    }

    p.set(process::InRecv);
    p.regs().rdi = from;
    match handle {
        Some(h) => {
            if log_recv {
                write(" ==> process ");
                con::writeMutPtr(h.process());
                con::newline();
            }
            recv(p, h)
        },
        None => {
            if log_recv && from != 0 {
                write(" ==> fresh\n");
            }
            recv_from_any(p, from)
        }
    }
}

fn recv(p: &mut Process, handle: &mut Handle) {
    let rcpt = handle.process();
    if rcpt.is(process::InSend) && handle.other().unwrap().id() == rcpt.regs().rdi {
        transfer_message(p, rcpt);
    } else {
        rcpt.add_waiter(p);
    }
}

fn recv_from_any(p : &mut Process, _id: uint) {
    let mut sender = None;
    for waiter in p.waiters.iter() {
        if waiter.is(process::InSend) {
            sender = Some(waiter);
            break;
        }
    }
    match sender {
        Some(s) => {
            p.remove_waiter(s);
            transfer_message(p, s);
        },
        None => ()
    }

    match p.pop_pending_handle() {
    Some(h) => deliver_pulse(p, h.id(), h.pop_pulses()),
    None => (),
    }

    let c = cpu();
    if c.is_irq_process(p) && c.irq_delayed != 0 {
        let irqs = c.irq_delayed;
        c.irq_delayed = 0;
        deliver_pulse(p, 0, irqs);
    }

    if log_recv {
        con::writeMutPtr(p);
        write(" recv: nothing to receive\n");
    }

    // Nothing to receive, run something else.
    unsafe { c.run(); }
}

#[inline(never)]
fn syscall_pulse(p: &mut Process, handle: uint, pulses: uint) -> ! {
    if log_pulse {
        con::writeMutPtr(p);
        write(" send pulse ");
        con::writeHex(pulses);
        write(" to ");
        con::writeHex(handle);
        con::newline();
    }

    let maybe_h = p.find_handle(handle);
    if maybe_h.is_none() {
        syscall_return(p, 0);
    }
    let h = maybe_h.unwrap();
    let q = h.process();
    if h.other().is_none() {
        syscall_return(p, 0);
    }
    let g = h.other().unwrap();
    if can_deliver_pulse(q, g.id()) {
        cpu().queue(p);
        deliver_pulse(q, g.id(), pulses);
    }
    if g.add_pulses(pulses) == 0 {
        if log_pulse {
            con::writeMutPtr(q);
            write(" can't receive pulse right now, pending\n");
        }

        // First pulse added to this process
        q.add_pending_handle(g);
    }
    syscall_return(p, 0);
}

#[inline(never)]
fn syscall_map(p: &mut Process, handle: uint, mut prot: uint, addr: uint, mut offset: uint, size: uint) {
    prot &= mapflag::UserAllowed;
    // TODO Check (and return failure) on:
    // * unaligned addr, offset, size (must be page-aligned)
    if (prot & mapflag::DMA) == mapflag::DMA {
        offset = match cpu().memory.alloc_frame() {
            None => {
                prot = 0;
                addr
            },
            Some(p) => p as uint - kernel_base,
        }
    }

    if log_map {
        write("map: handle=");
        con::writeHex(handle);
        write(" prot=");
        con::writeHex(prot);
        write(" addr=");
        con::writeHex(addr);
        write(" size=");
        con::writeHex(size);
        write(" offset=");
        con::writeHex(offset);
        con::newline();
    }

    p.aspace().map_range(addr, addr + size, handle, (offset - addr) | prot);

    if (prot & mapflag::Phys) == 0 {
        offset = 0;
    }
    syscall_return(p, offset);
}

#[inline(never)]
fn syscall_pfault(p : &mut Process, mut vaddr: uint, access: uint) {
    vaddr &= !0xfff;

    // set fault address
    p.fault_addr = vaddr;
    p.set(process::PFault);
    let prot = access & mapflag::RWX;
    // Look up vaddr, get handle, offset and flags
    let card = p.aspace().mapcard_find_def(vaddr);
    let offset = card.paddr(vaddr);

    if log_pfault {
        con::writeMutPtr(p);
        write(" fault: vaddr=");
        con::writeHex(vaddr);
        write(" handle=");
        con::writeHex(card.handle);
        write(" offset=");
        con::writeHex(offset);
        write(" prot=");
        con::writeHex(prot);
        con::newline();
    }

    // Now do the equivalent of sendrcv with rdi=handle, rsi=offset, rdx=flags
    ipc_call(p, nr::call(nr::PFAULT), card.handle, offset, prot, 0, 0, 0);
}

#[inline(never)]
fn syscall_grant(p: &mut Process, id: uint, mut vaddr: uint, mut prot: uint) {
    vaddr &= !0xfff;
    prot &= mapflag::RWX;

    if log_grant {
        con::writeMutPtr(p);
        write(" grant: id=");
        con::writeHex(id);
        write(" vaddr=");
        con::writeHex(vaddr);
        write(" prot=");
        con::writeHex(prot);
        con::newline();
    }

    let handle = p.find_handle(id).unwrap();
    let other_proc = handle.process();
    let other_handle = handle.other().unwrap();

    if !other_proc.is(process::PFault) {
        abort("Grant to non-faulting process");
    }

    let card = other_proc.aspace().mapcard_find_def(other_proc.fault_addr);

	// check that our handle's remote handle's key matched the one in the
	// mapping
    if card.handle != other_handle.id() {
        abort("Wrong handle granted");
    }

    prot &= card.flags();

	// check that our offset matches what it should? we'd need to pass on
	// the offset that we think we're granting, and compare that to the
	// vaddr+offset on the recipient side....

	// proc.rdi = our handle
	// proc.rsi = our vaddr
	// proc.rdx = our flags (& MAPFLAG_RWX)
	// rbx + proc.fault_addr = their vaddr (+ flags?)

    // TODO Recursive faults: if granting a page that needs IPC to fulfil, do
    // something special.

    // TODO If the recipient already has a backing at fault_addr, we need to
    // release it.

    other_proc.aspace().add_shared_backing(
        other_proc.fault_addr,
        prot,
        p.aspace().share_backing(vaddr));

    other_proc.unset(process::PFault);
    if other_proc.is(process::InRecv) {
        // Explicit pfault, do a send to respond (rather than a resume-from-
        // interrupt as would otherwise be required).
        ipc_send(p, nr::GRANT, id, other_proc.fault_addr, prot, 0, 0, 0);
    } else {
        abort("non-explicit fault");
    }
}

#[inline(never)]
fn syscall_hmod(p : &mut Process, id: uint, rename: uint, copy: uint) {
    let handle = p.find_handle(id);
    if log_hmod {
        con::writeMutPtr(p);
        write(" hmod: id="); con::writeHex(id);
        write(" rename="); con::writeHex(rename);
        write(" copy="); con::writeHex(copy);
        con::newline();
    }
    match handle {
    None => (),
    Some(h) => {
        // Fresh/dissociated handle for the same process as the original
        if copy != 0 {
            p.new_handle(copy, h.process());
        }
        if rename == 0 {
            p.delete_handle(h);
        } else if rename != id {
            p.rename_handle(h, rename);
        }
    }
    }
    syscall_return(p, 0);
}

#[inline(never)]
fn syscall_portio(p : &mut Process, port : uint, op : uint, data: uint) -> ! {
    if log_portio {
        con::writeMutPtr(p);
        write(" portio: port="); con::writeHex(port & 0xffff);
        write(" op="); con::writeHex(op);
        if op & 0x10 != 0 {
            write(" write "); con::writeHex(data);
        }
    }
    let mut res : uint = 0;
    unsafe { match op {
    0x01 => asm!("inb %dx, %al" : "={al}"(res) : "{dx}"(port)),
    0x02 => asm!("inw %dx, %ax" : "={ax}"(res) : "{dx}"(port)),
    0x04 => asm!("inl %dx, %eax" : "={eax}"(res) : "{dx}"(port)),
    0x11 => asm!("outb %al, %dx" :: "{al}"(data), "{dx}"(port)),
    0x12 => asm!("outw %ax, %dx" :: "{ax}"(data), "{dx}"(port)),
    0x14 => asm!("outl %eax, %dx" :: "{eax}"(data), "{dx}"(port)),
    _ => abort("unhandled portio operation")
    } }
    if log_portio {
        if op & 0x10 == 0 {
            write(" res="); con::writeHex(res);
        }
        con::newline();
    }
    syscall_return(p, res);
}

#[inline(never)]
fn syscall_return(p : &mut Process, res : uint) -> ! {
    cpu().syscall_return(p, res);
}
