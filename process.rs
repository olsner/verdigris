use core::prelude::*;

use alloc;
use aspace::AddressSpace;
use con;
use con::write;
use dlist::DList;
use dlist::DListNode;
use dlist::DListItem;
use dict::*;

pub enum FlagBit {
// The process is currently queued on the run queue.
    Queued = 0,

// Can return to user-mode with sysret, only some registers will be restored:
// rsp, rip: restored to previous values
// rcx, r11: rip and rflags, respectively
// rax: syscall return value
// Remaining registers will be 0 (?)
    FastRet = 1,
// IN_RECV: Similar to FASTRET, when waiting for a message-send rendezvous
// When set together with IN_SEND, it's a sendrcv and the SEND needs to finish
// first.
// At any time when IN_RECV is set, the proc's saved rdi contains a pointer to
// the handle being received from.
// When a process starts a receive, until it becomes properly blocked on some
// process or finishes the receive immediately, it will be both RUNNING and
// IN_RECV.
    InRecv = 2,
// Process is trying to do a synchronous send or sendrcv, blocking on the
// waiting_for process to reach a PROC_IN_RECV state. Both IN_SEND and IN_RECV
// can be set at the same time.
// At any time when IN_SEND is set, the proc's saved rdi contains a pointer to
// the handle being sent to.
// When a process starts a send, until it becomes properly blocked on some
// process or finishes the operation, it will be both RUNNING and IN_SEND.
    InSend = 3,
// Is the currently running process
    Running = 4,
// Process has had a page fault that requires a response from a backer, or has
// requested a page paged in.
// proc.fault_addr is the address that faulted/was requested.
    PFault = 5
}

impl FlagBit {
    #[inline]
    pub fn mask(self) -> u32 {
        return 1 << (self as u32);
    }
}
// TODO Implement OR for FlagBit

pub struct FXSaveRegs {
    space : [u8, ..512]
}

pub struct Handle {
    node : DictNode<uint, Handle>,
    process : *mut Process,
    // pointer to other handle if any. Its 'key' field is the other-name that
    // we need when e.g. sending it a message. If null this is not associated
    // in other-proc yet.
    other : Option<*mut Handle>,
    events : uint,
}

impl DictItem<uint> for Handle {
    fn node<'a>(&'a mut self) -> &'a mut DictNode<uint, Handle> {
        &mut self.node
    }
}

impl Handle {
    pub fn new(id : uint, process : *mut Process) -> Handle {
        Handle {
            node : DictNode::new(id),
            process : process,
            other : None,
            events : 0
        }
    }

    pub fn id(&self) -> uint { self.node.key }
    pub fn process(&self) -> &mut Process { unsafe { &mut *self.process } }
    pub fn other(&mut self) -> Option<&mut Handle> {
        match self.other {
        Some(h) => Some(unsafe { &mut *h }),
        None => None,
        }
    }

    pub fn associate(&mut self, other: &mut Handle) {
        self.other = Some(other as *mut Handle);
        other.other = Some(self as *mut Handle);
    }
}

pub struct PendingPulse {
    node : DictNode<uint, PendingPulse>,
    handle : *mut Handle,
}

impl DictItem<uint> for PendingPulse {
    fn node<'a>(&'a mut self) -> &'a mut DictNode<uint, PendingPulse> {
        return &mut self.node;
    }
}

impl FXSaveRegs {
    fn new() -> FXSaveRegs {
        FXSaveRegs { space : [0, ..512] }
    }
}

pub struct Regs {
    pub rax : uint,
    pub rcx : uint,
    pub rdx : uint,
    pub rbx : uint,
    pub rsp : uint,
    pub rbp : uint,
    pub rsi : uint,
    pub rdi : uint,

    pub r8 : uint,
    pub r9 : uint,
    pub r10 : uint,
    pub r11 : uint,
    pub r12 : uint,
    pub r13 : uint,
    pub r14 : uint,
    pub r15 : uint,

    pub rip : uint,
    pub rflags : uint,
}

impl Regs {
    fn new() -> Regs {
        use x86::rflags;
        Regs {
            rax: 0, rcx: 0, rdx: 0, rbx: 0, rsp: 0, rbp: 0, rsi: 0, rdi: 0,
            r8: 0, r9: 0, r10: 0, r11: 0, r12: 0, r13: 0, r14: 0, r15: 0,
            rip : 0, rflags : rflags::IF
        }
    }
}

type Flags = u32;

pub struct Process {
    pub regs : Regs,
    // Physical address of PML4 to put in CR3
    pub cr3 : uint,

    // Bitwise OR of flags values
    flags : Flags,
    count : u32,

    // Pointer to the process we're waiting for (if any). See flags.
    waiting_for : *mut Process, // Option

    // List of processes waiting on this process.
    pub waiters : DList<Process>,
    node : DListNode<Process>,

    aspace : *mut AddressSpace,

    // TODO: move this into address space so handles can be shared between
    // threads.
    handles : Dict<uint, Handle>,
    pending : Dict<uint, PendingPulse>,

    // When PROC_PFAULT is set, the virtual address that faulted.
    // Note that we lose a lot of data about the mapping that we looked up
    // in PFAULT, and have to look up again in GRANT. This is intentional,
    // since we have to verify and match the GRANT to the correct page, we
    // simply don't save anything that might be wrong.
    // The lower bits are access flags for the fault/request.
    pub fault_addr: uint,

    fxsave : FXSaveRegs,
}

impl DListItem for Process {
    fn node<'a>(&'a mut self) -> &'a mut DListNode<Process> {
        return &mut self.node;
    }
}

impl Process {
    pub fn new(aspace : *mut AddressSpace) -> Process {
        let init_flags = FastRet.mask();
        Process {
            regs : Regs::new(),
            flags : init_flags, count : 0,
            waiting_for : RawPtr::null(),
            waiters : DList::empty(),
            node : DListNode::new(),
            cr3 : unsafe { (*aspace).cr3() },
            aspace : aspace,
            handles : Dict::empty(),
            pending : Dict::empty(),
            fault_addr : 0,
            fxsave : FXSaveRegs::new()
        }
    }

    #[inline]
    pub fn is(&self, f : FlagBit) -> bool {
        (self.flags & f.mask()) != 0
    }

    pub fn ipc_state(&self) -> Flags {
        self.flags & (InRecv.mask() | InSend.mask() | PFault.mask())
    }

    pub fn set(&mut self, f : FlagBit) {
        self.flags |= f.mask();
    }

    pub fn unset(&mut self, f : FlagBit) {
        self.flags &= !f.mask();
    }

    #[inline]
    pub fn is_queued(&self) -> bool { self.is(Queued) }

    pub fn aspace<'a>(&'a mut self) -> &'a mut AddressSpace {
        unsafe {
            return &mut *self.aspace;
        }
    }

    #[inline(never)]
    pub fn find_handle<'a>(&mut self, id : uint) -> Option<&'a mut Handle> {
        let res = self.handles.find(id);
        match res {
            Some(ref h) if h.id() != id => None,
            _ => res
        }
    }

    pub fn assoc_handles(&mut self, id: uint, other : &mut Process, other_id: uint) {
        let x = self.new_handle(id, other);
        let y = other.new_handle(other_id, self);
        x.other = Some(y as *mut Handle);
        y.other = Some(x as *mut Handle);
    }

    #[inline(never)]
    pub fn new_handle<'a>(&mut self, id : uint, other : *mut Process) -> &'a mut Handle {
        match self.handles.find(id) {
            Some(ref h) if h.id() != id => (),
            Some(h) => self.delete_handle(h),
            None => ()
        }
        unsafe {
            let h = alloc();
            *h = Handle::new(id, other);
            self.handles.insert(h)
        }
    }

    pub fn delete_handle(&mut self, handle : &mut Handle) {
        self.handles.remove(handle.node.key);
    }

    pub fn rename_handle(&mut self, handle : &mut Handle, new_id: uint) {
        handle.node.key = new_id;
        // TODO self.handles.unlink/relink/key_changed
    }

    pub fn add_waiter(&mut self, other : &mut Process) {
        self.waiters.append(other);
    }

    pub fn dump(&self) {
        write("proc ");
        con::writePtr(self);
        write(":\n");

        for (id,h) in self.handles.iter() {
            write("  handle ");
            con::writeUInt(id);
            write(" -> proc ");
            con::writeMutPtr(h.process());
            con::newline();
        }
    }
}
