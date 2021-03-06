use core::ptr;

use alloc;
use free;

use aspace::AddressSpace;
use con;
use con::write;
use dlist::DList;
use dlist::DListNode;
use dlist::DListItem;
use dict::*;

pub use self::FlagBit::*;

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
    pub fn mask(self) -> Flags {
        return 1 << (self as Flags);
    }
}
// TODO Implement OR for FlagBit

//pub struct FXSaveRegs {
//    space : [u8; 512]
//}

pub struct Handle {
    node : DictNode<u64, Handle>,
    process : *mut Process,
    // pointer to other handle if any. Its 'key' field is the other-name that
    // we need when e.g. sending it a message. If null this is not associated
    // in other-proc yet.
    pub other : Option<*mut Handle>,
    pulses : u64,
}

impl DictItem for Handle {
    type Key = u64;
    fn node<'a>(&'a mut self) -> &'a mut DictNode<u64, Handle> {
        &mut self.node
    }
}

impl Handle {
    fn init(&mut self, id : u64, process : *mut Process) {
        self.node.init(id);
        self.process = process;
    }

    pub fn new(id : u64, process : *mut Process) -> *mut Handle {
        let res = alloc::<Handle>();
        res.init(id, process);
        res as *mut Handle
    }

    pub fn id(&self) -> u64 { self.node.key }
    pub fn process<'a>(&self) -> &'a mut Process {
        unsafe { &mut *self.process }
    }
    pub fn other<'a>(&mut self) -> Option<&'a mut Handle> {
        match self.other {
        Some(h) => Some(unsafe { &mut *h }),
        None => None,
        }
    }

    pub fn associate(&mut self, other: &mut Handle) {
        self.other = Some(other as *mut Handle);
        other.other = Some(self as *mut Handle);
    }

    pub fn dissociate(&mut self) {
        match self.other() {
        Some(o) => o.other = None,
        None => (),
        }
        self.other = None;
    }

    pub fn add_pulses(&mut self, pulses: u64) -> u64 {
        let res = self.pulses;
        self.pulses |= pulses;
        return res;
    }

    pub fn pop_pulses(&mut self) -> u64 {
        let res = self.pulses;
        self.pulses = 0;
        return res;
    }
}

pub struct PendingPulse {
    node : DictNode<u64, PendingPulse>,
    handle : *mut Handle,
}

impl DictItem for PendingPulse {
    type Key = u64;
    fn node<'a>(&'a mut self) -> &'a mut DictNode<u64, PendingPulse> {
        return &mut self.node;
    }
}

impl PendingPulse {
    fn new(handle: &mut Handle) -> *mut PendingPulse {
        let res = alloc::<PendingPulse>();
        res.node.init(handle.id());
        res.handle = handle as *mut Handle;
        res as *mut PendingPulse
    }
}

pub struct Regs {
    pub rax : u64,
    pub rcx : u64,
    pub rdx : u64,
    pub rbx : u64,
    pub rsp : u64,
    pub rbp : u64,
    pub rsi : u64,
    pub rdi : u64,

    pub r8 : u64,
    pub r9 : u64,
    pub r10 : u64,
    pub r11 : u64,
    pub r12 : u64,
    pub r13 : u64,
    pub r14 : u64,
    pub r15 : u64,
}

impl Regs {
}

type Flags = u8;

pub struct Process {
    // Regs must be first since it's used by assembly code.
    regs : Regs,
    pub rip : u64,
    pub rflags : u64,
    // Physical address of PML4 to put in CR3
    pub cr3 : u64,
    // Fields up until cr3 are shared with assembly code.

    // Bitwise OR of flags values
    flags : Flags,

    // Pointer to the process we're waiting for (if any). See flags.
    waiting_for : *mut Process, // Option

    // List of processes waiting on this process.
    pub waiters : DList<Process>,
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
    pub fault_addr: u64,

    //fxsave : FXSaveRegs,
}

impl DListItem for Process {
    fn node<'a>(&'a mut self) -> &'a mut DListNode<Process> {
        return &mut self.node;
    }
}

impl Process {
    fn init(&mut self, aspace: *mut AddressSpace) {
        use x86::rflags;
        let init_flags = FastRet.mask();
        self.flags = init_flags;
        self.aspace = aspace;
        self.cr3 = self.aspace().cr3();
        self.rflags = rflags::IF;
    }

    pub fn new(aspace : *mut AddressSpace) -> *mut Process {
        let res = alloc::<Process>();
        res.init(aspace);
        res as *mut Process
    }

    pub fn regs<'a>(&'a mut self) -> &'a mut Regs {
        &mut self.regs
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

    pub fn is_runnable(&self) -> bool {
        self.ipc_state() == 0
    }

    pub fn aspace<'a>(&'a mut self) -> &'a mut AddressSpace {
        unsafe {
            return &mut *self.aspace;
        }
    }

    #[inline(never)]
    pub fn find_handle<'a>(&mut self, id : u64) -> Option<&'a mut Handle> {
        let res = self.handles.find(id);
        match res {
            Some(ref h) if h.id() != id => None,
            _ => res
        }
    }

    pub fn assoc_handles(&mut self, id: u64, other : &mut Process, other_id: u64) {
        let x = self.new_handle(id, other);
        let y = other.new_handle(other_id, self);
        x.other = Some(y as *mut Handle);
        y.other = Some(x as *mut Handle);
    }

    #[inline(never)]
    pub fn new_handle<'a>(&mut self, id : u64, other : *mut Process) -> &'a mut Handle {
        match self.handles.find(id) {
            Some(ref h) if h.id() != id => (),
            Some(h) => self.delete_handle(h),
            None => ()
        }
        self.handles.insert(Handle::new(id, other))
    }

    pub fn delete_handle(&mut self, handle : &mut Handle) {
        handle.dissociate();
        self.pending.remove(handle.node.key);
        self.handles.remove(handle.node.key);
    }

    pub fn rename_handle(&mut self, handle : &mut Handle, new_id: u64) {
        handle.node.key = new_id;
        // TODO self.handles.unlink/relink/key_changed
    }

    pub fn add_pending_handle(&mut self, handle: &mut Handle) {
        self.pending.insert(PendingPulse::new(handle));
    }

    pub fn pop_pending_handle<'a>(&mut self) -> Option<&'a mut Handle> {
        match self.pending.pop() {
        Some(p) => unsafe {
            let h = (*p).handle;
            free(p);
            Some(&mut *h)
        },
        None => None,
        }
    }

    pub fn add_waiter(&mut self, other : &mut Process) {
        if other.waiting_for.is_null() {
            self.waiters.append(other);
            other.waiting_for = self as *mut Process;
        }
    }

    pub fn remove_waiter(&mut self, other : &mut Process) {
        if other.waiting_for == (self as *mut Process) {
            self.waiters.remove(other);
            other.waiting_for = ptr::null_mut();
        }
    }

    pub fn dump(&self) {
        write("proc ");
        con::writePtr(self);
        write(" f=");
        con::writeHex(self.flags);
        write(":\n");

        for (id,h) in self.handles.iter() {
            write("  handle ");
            con::writeUInt(id);
            write(" -> proc ");
            con::writeMutPtr(h.process());
            con::newline();
        }

        for p in self.waiters.iter() {
            write("  waiter ");
            con::writeMutPtr(p);
            con::newline();
        }
    }
}
