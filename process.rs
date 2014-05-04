use core::prelude::*;

use aspace::AddressSpace;
use dlist::DList;
use dlist::DListNode;
use dlist::DListItem;
use dict::*;
use mem::heap_copy;

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
    fn mask(self) -> uint {
        return 1 << (self as uint);
    }
}

pub struct FXSaveRegs {
    space : [u8, ..512]
}

pub struct Handle {
    node : DictNode<uint, Handle>,
    pub process : *mut Process,
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

enum RegIndex {
    RAX = 0,
    RCX = 1,
    RDX = 2,
    RBX = 3,
    RSP = 4,
    RBP = 5,
    RSI = 6,
    RDI = 7
}

pub struct Regs {
	pub gps : [u64, ..16],
	pub rip : u64,
	pub rflags : u64,
}

impl Regs {
    fn new() -> Regs {
        use x86::rflags;
        Regs { gps : [0, ..16], rip : 0, rflags : rflags::IF as u64 }
    }

    pub fn set_rsp(&mut self, rsp : uint) {
        self.gps[RSP as uint] = rsp as u64;
    }
    pub fn set_rip(&mut self, rip : uint) {
        self.rip = rip as u64;
    }
    pub fn set_rax(&mut self, rax: uint) {
        self.gps[RAX as uint] = rax as u64;
    }
}

type Flags = uint;

pub struct Process {
    pub regs : Regs,
    // Physical address of PML4 to put in CR3
    pub cr3 : uint,

    // Bitwise OR of flags values
    flags : Flags,
    count : uint,

    // Pointer to the process we're waiting for (if any). See flags.
    waiting_for : *mut Process, // Option

    // List of processes waiting on this process.
    waiters : DList<Process>,
    node : DListNode<Process>,

    aspace : *mut AddressSpace,

    // TODO: move this into address space so handles can be shared between
    // threads.
    handles : Dict<Handle>,
    pending : Dict<PendingPulse>,

    // When PROC_PFAULT is set, the virtual address that faulted.
    // Note that we lose a lot of data about the mapping that we looked up
    // in PFAULT, and have to look up again in GRANT. This is intentional,
    // since we have to verify and match the GRANT to the correct page, we
    // simply don't save anything that might be wrong.
    // The lower bits are access flags for the fault/request.
    fault_addr: uint,

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

    pub fn find_handle<'a>(&mut self, id : uint) -> Option<&'a mut Handle> {
        let res = self.handles.find(id);
        match res {
            Some(ref h) if h.id() != id => None,
            _ => res
        }
    }

    pub fn new_handle(&mut self, id : uint, other : *mut Process) {
        match self.handles.find(id) {
            Some(ref h) if h.id() != id => (),
            Some(h) => self.delete_handle(h),
            None => ()
        }
        self.handles.insert(heap_copy(Handle::new(id, other)));
    }

    pub fn delete_handle(&mut self, handle : &mut Handle) {
    }

    pub fn rename_handle(&mut self, handle : &mut Handle, new_id: uint) {
        handle.node.key = new_id;
        // TODO self.handles.unlink/relink/key_changed
    }
}
