use core::prelude::*;

use aspace::mapflag;
use con;
use con::write;
use cpu;
use process;
use process::Handle;
use process::Process;
use start32::kernel_base;
use util::abort;

static log_syscall : bool = false;
static log_transfer_message : bool = false;

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
}

// Note: tail-called from the syscall code, "return" by switching to a process.
#[no_mangle]
pub fn syscall(
    arg0: uint,
    arg1: uint,
    arg2: uint,
    arg3: uint,
    arg4: uint,
    arg5: uint,
    nr : uint, // saved_rax
) -> ! {
    use syscall::nr::*;

    let p = unsafe { cpu().get_process() };
    // FIXME cpu.leave_proc?
    p.unset(process::Running);
    p.set(process::FastRet);

    if log_syscall {
        write("syscall! nr=");
        con::writeUInt(nr);
        write(" from process ");
        con::writeMutPtr(p);
        con::newline();
    }

    match nr {
    RECV => ipc_recv(p, arg0),
    MAP => syscall_map(p, arg0, arg1, arg2, arg3, arg4),
    PFAULT => syscall_pfault(p, arg1, arg2), // arg0 is always 0
    HMOD => syscall_hmod(p, arg0, arg1, arg2),
    PORTIO => syscall_portio(p, arg0, arg1, arg2),
    WRITE => {
        con::putc(arg0 as u8 as char);
        cpu().syscall_return(p, 0);
    },
    _ if nr >= USER => {
        match nr & MSG_KIND_MASK {
            MSG_KIND_CALL => ipc_call(p, nr & MSG_MASK, arg0, arg1, arg2, arg3, arg4, arg5),
            MSG_KIND_SEND => ipc_send(p, nr & MSG_MASK, arg0, arg1, arg2, arg3, arg4, arg5),
            _ => abort("Unknown IPC kind")
        }
    },
    _ => abort("Unhandled syscall"),
    }

    if p.is_runnable() {
        abort("process not blocked at return");
    }

    unsafe { cpu().run(); }
}

fn ipc_call(p : &mut Process, msg : uint, to : uint, arg1: uint, arg2: uint,
    arg3: uint, arg4: uint, arg5: uint) {
    write("ipc_call to ");
    con::writeUInt(to);
    con::newline();

    let handle = p.find_handle(to);
    match handle {
    Some(h) => {
        write("==> process ");
        con::writeMutPtr(h.process());
        con::newline();
        p.set(process::InSend);
        p.set(process::InRecv);
        p.regs.rdi = to;
        send_or_block(h, msg, arg1, arg2, arg3, arg4, arg5);
    },
    None => abort("ipc_call: no recipient")
    }
    write("ipc_call: blocked\n");
}

fn transfer_set_handle(target: &mut Process, source: &mut Process) {
    let mut rcpt = target.regs.rdi;
    let from = source.regs.rdi;

    let h = source.find_handle(from).unwrap();
    if rcpt == 0 {
        rcpt = h.other().unwrap().id();
    } else if !target.find_handle(rcpt).is_some() {
        match h.other() {
            // Already associated, "junk" the fresh handle and just update the
            // recipient-side handle.
            Some(g) => rcpt = g.id(),
            // Associate handles now.
            None => {
                let g = target.new_handle(rcpt, source);
                g.associate(h);
            },
        }
    } else {
        // TODO Assert that rcpt <-> from. (But the caller is responsible for
        // checking that first.)
    }
    target.regs.rdi = rcpt;
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

    target.regs.rax = source.regs.rax;
    target.regs.rdi = source.regs.rdi;
    target.regs.rsi = source.regs.rsi;
    target.regs.rdx = source.regs.rdx;
    target.regs.r8 = source.regs.r8;
    target.regs.r9 = source.regs.r9;
    target.regs.r10 = source.regs.r10;

    target.unset(process::InRecv);
    source.unset(process::InSend);

    let c = cpu();
    c.queue(target);
    if source.ipc_state() == 0 {
        c.queue(source);
    }
    unsafe { c.run(); }
}

fn send_or_block(h : &mut Handle, msg: uint, arg1: uint, arg2: uint,
        arg3: uint, arg4: uint, arg5: uint) {
    match h.other() {
        Some(g) => {
            let p = h.process();
            let sender = g.process();

            // Save regs - either we'll copy these in transfer_message or we'll
            // need to store them until later on when the transfer can finish.
            sender.regs.rax = msg;
            sender.regs.rdi = h.id();
            sender.regs.rsi = arg1;
            sender.regs.rdx = arg2;
            sender.regs.r10 = arg3;
            sender.regs.r8 = arg4;
            sender.regs.r9 = arg5;

            // p is the recipient, the sender is in g.process().
            if p.ipc_state() == process::InRecv.mask() {
                let rcpt = h.process().regs.rdi;
                // Check the receiving process' receipt handle
                //   0 ==> transfer
                //   !0, connected to our handle ==> transfer
                //   !0, fresh ==> transfer
                //   !0 otherwise ==> block
                if rcpt == 0
                || rcpt == g.id()
                || !p.find_handle(rcpt).is_some() {
                    transfer_message(p, sender);
                }
            }

            p.add_waiter(sender)
        },
        None => abort("sending to unconnected handle"),
    }
}

fn ipc_send(p : &mut Process, msg : uint, to : uint, arg1: uint, arg2: uint,
        arg3: uint, arg4: uint, arg5: uint) {
    let handle = p.find_handle(to);
    match handle {
    Some(h) => {
        p.set(process::InSend);
        send_or_block(h, msg, arg1, arg2, arg3, arg4, arg5);
    },
    None => abort("ipc_send: no recipient")
    }
}

fn ipc_recv(p : &mut Process, from : uint) {
    let mut handle = None;
    if from != 0 {
        handle = p.find_handle(from);
    }

    write("recv from ");
    con::writeUInt(from);
    con::newline();

    p.set(process::InRecv);
    p.regs.rdi = from;
    match handle {
        Some(h) => {
            write("==> process ");
            con::writeMutPtr(h.process());
            con::newline();
            recv(p, h)
        },
        None => {
            if from != 0 {
                write("==> fresh\n");
            }
            recv_from_any(p, from)
        }
    }
}

fn recv(p: &mut Process, handle: &mut Handle) {
    let rcpt = handle.process();
    if rcpt.is(process::InSend) {
        abort("recv-from-specific not implemented");
    } else {
        rcpt.add_waiter(p);
    }
}

fn recv_from_any(p : &mut Process, _id: uint) {
    for waiter in p.waiters.iter() {
        if waiter.is(process::InSend) {
            abort("found sender!");
        }
    }
    // TODO Look for pending pulse
    // 2. Look for pending pulses
    // 3. Switch next
}

fn syscall_map(p: &mut Process, handle: uint, mut prot: uint, addr: uint, mut offset: uint, size: uint) {
    prot &= mapflag::UserAllowed;
    // TODO Check (and return failure) on:
    // * unaligned addr, offset, size (must be page-aligned)
    if (prot & mapflag::DMA) == mapflag::DMA {
        offset = match cpu().memory.alloc_frame() {
            None => 0,
            Some(p) => p as uint - kernel_base,
        }
    }

    p.aspace().map_range(addr, addr + size, handle, (offset - addr) | prot);

    if (prot & mapflag::Phys) == 0 {
        offset = 0;
    }
    cpu().syscall_return(p, offset);
}

fn syscall_pfault(p : &mut Process, mut vaddr: uint, access: uint) {
    vaddr &= !0xfff;

    // set fault address
    p.fault_addr = vaddr;
    p.regs.rsi = vaddr;
    p.regs.rdx = access & mapflag::RWX;
    // Look up vaddr, get handle, offset and flags
    let card = p.aspace().mapcard_find_def(vaddr);
    p.regs.rsi += card.offset; // proc.rsi is now translated into offset
    p.regs.rdi = card.handle;
    p.set(process::PFault);

    // Now do the equivalent of sendrcv with rdi=handle, rsi=offset, rdx=flags
}

fn syscall_hmod(p : &mut Process, id: uint, rename: uint, copy: uint) {
    let handle = p.find_handle(id);
    match handle {
    None => return,
    Some(h) => {
        // Fresh/dissociated handle for the same process as the original
        if copy != 0 {
            p.new_handle(copy, h.process());
        }
        if rename != 0 {
            p.rename_handle(h, rename);
        } else {
            p.delete_handle(h);
        }
    }
    }
}

fn syscall_portio(p : &mut Process, port : uint, op : uint, data: uint) -> ! {
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
    cpu().syscall_return(p, res);
}
