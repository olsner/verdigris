use core::prelude::*;

use con;
use con::write;
use cpu;
use process;
use process::Handle;
use process::Process;
use util::abort;

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

    let c = cpu();
    let p = unsafe { c.get_process() };
    // FIXME cpu.leave_proc?
    p.unset(process::Running);
    p.set(process::FastRet);

    write("syscall! nr=");
    con::writeUInt(nr);
    write(" from process ");
    con::writeMutPtr(p);
    con::newline();

    if nr >= USER {
        match nr & MSG_KIND_MASK {
            MSG_KIND_CALL => ipc_call(p, nr & MSG_MASK, arg0, arg1, arg2, arg3, arg4, arg5),
            MSG_KIND_SEND => ipc_send(p, nr & MSG_MASK, arg0, arg1, arg2, arg3, arg4, arg5),
            _ => abort("Unknown IPC kind")
        }
        // IPC syscall
    }

    match nr {
    RECV => ipc_recv(p, arg0),
    HMOD => syscall_hmod(p, arg0, arg1, arg2),
    PORTIO => syscall_portio(p, arg0, arg1, arg2),
    _ => abort("Unhandled syscall"),
    }

    unsafe { c.run(); }
}

fn ipc_call(p : &mut Process, msg : uint, to : uint, arg1: uint, arg2: uint,
    arg3: uint, arg4: uint, arg5: uint) {
    write("ipc_call to ");
    con::writeUInt(to);
    con::newline();

    let mut handle = p.find_handle(to);
    match handle {
    Some(h) => {
        write("==> process ");
        con::writeMutPtr(h.process());
        con::newline();
        p.set(process::InSend);
        p.set(process::InRecv);
        send_or_block(h, msg, arg1, arg2, arg3, arg4, arg5);
    },
    None => abort("ipc_call: no recipient")
    }
    abort("ipc_call unimplemented");
}

fn send_or_block(h : &mut Handle, msg: uint, arg1: uint, arg2: uint,
        arg3: uint, arg4: uint, arg5: uint) {
    // h.process() is the recipient, the sender is the reverse
    if h.process().ipc_state() == process::InRecv.mask() {
        abort("actually sending is unimplemented");
    } else {
        match h.other() {
            Some(g) => {
                h.process().add_waiter(g.process())
            },
            None => abort("sending to unconnected handle"),
        }
    }
}

fn ipc_send(p : &mut Process, msg : uint, to : uint, arg1: uint, arg2: uint,
        arg3: uint, arg4: uint, arg5: uint) {
    let mut handle = p.find_handle(to);
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

fn recv_from_any(p : &mut Process, id: uint) {
    for waiter in p.waiters.iter() {
        if waiter.is(process::InSend) {
            abort("found sender!");
        }
    }
    // 1. Look for senders in waiters list
    // 2. Look for pending pulses
    // 3. Switch next
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
