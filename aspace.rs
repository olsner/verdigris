use alloc;

use con;
use con::write;
use cpu;
use dict::*;
use dlist::*;
use mem::heap_copy;
use start32;
use util::abort;
pub use self::mapflag::MapFlag;

static log_add_pte : bool = false;

pub mod mapflag {
    pub type MapFlag = u8;

    pub const X : MapFlag = 1;
    pub const W : MapFlag = 2;
    pub const R : MapFlag = 4;
    pub const RWX : MapFlag = 7;
    // Anonymous memory allocated to zeroes on first use.
    pub const Anon : MapFlag = 8;
    // handle is 0; offset is (paddr - vaddr)
    pub const Phys : MapFlag = 16;
    // Physical memory allocated and locked at map time; and deallocated when
    // unmapped.
    pub const DMA : MapFlag = Anon | Phys;
    pub const UserAllowed : MapFlag = DMA | RWX;
}

// mapcard: the handle, offset and flags for the range of virtual addresses until
// the next card.
// 5 words:
// - 3 for dict_node w/ vaddr
// - 1 handle
// - 1 offset+flags
//   (since offsets must be page aligned we have 12 left-over bits)
// This structure is completely unrelated to the physical pages backing virtual
// memory - it represents each process' wishful thinking about how their memory
// should look. backings and sharings control physical memory.
#[derive(Clone, Copy)]
pub struct MapCard {
    as_node : DictNode<u64, MapCard>,
    pub handle : u64,
    // .vaddr + .offset = handle-offset to be sent to backer on fault
    // For a direct physical mapping, paddr = .vaddr + .offset
    // .offset = handle-offset - vaddr
    // .offset = paddr - .vaddr
    // The low 12 bits contain flags.
    pub offset : u64,
}

impl DictItem for MapCard {
    type Key = u64;

    fn node<'a>(&'a mut self) -> &'a mut DictNode<u64, MapCard> {
        return &mut self.as_node;
    }
}

impl MapCard {
    fn new(vaddr : u64, handle : u64, offset : u64, access : MapFlag) -> MapCard {
        MapCard { as_node: DictNode::new(vaddr), handle: handle,
            offset: offset | (access as u64) }
    }

    /*fn init(&mut self, vaddr: u64, handle: u64, offset: u64) {
        self.as_node.init(vaddr);
        self.handle = handle;
        self.offset = offset;
    }*/

    pub fn vaddr(&self) -> u64 {
        return self.as_node.key;
    }

    pub fn paddr(&self, vaddr : u64) -> u64 {
        return vaddr + (self.offset & !0xfff);
    }

    pub fn flags(&self) -> MapFlag {
        (self.offset & 0xfff) as MapFlag
    }

    fn same_addr(&self, other : &MapCard) -> bool {
        self.vaddr() == other.vaddr()
    }

    fn same(&self, other: &MapCard) -> bool {
        return self.handle == other.handle && self.offset == other.offset;
    }

    fn set(&mut self, other : &MapCard) {
        self.handle = other.handle;
        self.offset = other.offset;
    }
}

// backing: mapping *one page* to the place that page came from.
// Indexed by vaddr for the process that maps it. The vaddr includes flags, so
// look up by vaddr|0xfff.
// This is likely to exist once per physical page per process. Should be
// minimized.
// 6 words:
// - 3 words for dict_node w/ vaddr
// - 1 word for parent
// - 2 words for child-list links
//
// Could be reduced to 4 words: flags, parent, child-list links, if moving to
// an external dictionary. 32 bytes per page gives 128 entries in the last-level
// table. Flags could indicate how many levels that are required, and e.g. a
// very small process could have only one level, and map 128 pages at 0..512kB.
pub struct Backing {
    // Flags stored in low bits of vaddr/key!
    as_node : DictNode<u64, Backing>,
    // Pointer to parent sharing. Needed to unlink self when unmapping.
    // Could have room for flags (e.g. to let it be a paddr when we don't
    // need the parent - we might have a direct physical address mapping)
    parent_paddr : u64,
    // Space to participate in parent's list of remappings.
    child_node : DListNode<Backing>
}

impl DictItem for Backing {
    type Key = u64;

    fn node<'a>(&'a mut self) -> &'a mut DictNode<u64, Backing> {
        return &mut self.as_node;
    }
}

impl DListItem for Backing {
    fn node<'a>(&'a mut self) -> &'a mut DListNode<Backing> {
        return &mut self.child_node;
    }
}

fn alloc_frame_paddr() -> u64 {
    let p : *mut u8 = cpu().memory.alloc_frame_panic();
    p as u64 - start32::kernel_base
}

impl Backing {
    fn new(vaddr: u64, flags: MapFlag, parent_paddr: u64) -> *mut Backing {
        let res = alloc::<Backing>();
        res.as_node.init(vaddr | (flags as u64));
        res.parent_paddr = parent_paddr;
        res as *mut Backing
    }

    fn new_phys(vaddr : u64, flags : MapFlag, phys_addr : u64) -> *mut Backing {
        if phys_addr & 0xfff != 0 {
            abort("Bad physical address in new_phys");
        }
        Backing::new(vaddr, flags, phys_addr)
    }

    fn new_anon(vaddr : u64, flags : MapFlag) -> *mut Backing {
        Backing::new(vaddr, flags | mapflag::Phys, alloc_frame_paddr())
    }

    fn new_share(vaddr: u64, flags : MapFlag, share: &mut Sharing) -> *mut Backing {
        Backing::new(vaddr, flags, (share as *mut Sharing) as u64)
    }

    pub fn has_vaddr(&self, vaddr : u64) -> bool {
        return self.vaddr() == vaddr & !0xfff;
    }

    pub fn vaddr(&self) -> u64 {
        return self.as_node.key & !0xfff;
    }

    pub fn flags(&self) -> MapFlag {
        (self.as_node.key & 0xfff) as MapFlag
    }

    fn pte_flags(&self) -> u64 {
        let flags = self.flags();
        let mut pte = 5; // Present, User-accessible
        if (flags & mapflag::X) == 0 {
            // Set bit 63 to *disable* execute permission
            pte |= 1 << 63;
        }
        if (flags & mapflag::W) != 0 {
            pte |= 2;
        }
        return pte;
    }

    pub fn pte(&self) -> u64 {
        self.paddr() | self.pte_flags()
    }

    pub fn paddr(&self) -> u64 {
        if (self.flags() & mapflag::Phys) != 0 {
            self.parent_paddr
        } else {
            self.parent().paddr
        }
    }

    pub fn parent<'a>(&self) -> &Sharing {
        if (self.flags() & mapflag::Phys) == 0 {
            unsafe { &*(self.parent_paddr as *const Sharing) }
        } else {
            abort("no parent: direct backing");
        }
    }
}

// sharing: mapping one page to every place it's been shared to
// 7 words!
pub struct Sharing {
    as_node : DictNode<u64, Sharing>,
    paddr : u64,
    aspace : *mut AddressSpace,
    children : DList<Backing>,
}

impl DictItem for Sharing {
    type Key = u64;

    fn node<'a>(&'a mut self) -> &'a mut DictNode<u64, Sharing> {
        return &mut self.as_node;
    }
}

impl Sharing {
    fn new(aspace: *mut AddressSpace, back: &Backing) -> *mut Sharing {
        let res = alloc::<Sharing>();
        res.as_node.init(back.vaddr());
        res.paddr = back.paddr();
        res.aspace = aspace;
        res as *mut Sharing
    }
}

type PageTable = [u64; 512];
type PML4 = PageTable;

pub struct AddressSpace {
    // Upon setup, pml4 is set to a freshly allocated frame that is empty
    // except for the mapping to the kernel memory area (which, as long as
    // it's less than 4TB is only a single entry in the PML4).
    // (This is the virtual address in the higher half. proc.cr3 is a
    // physical address.)
    pml4 : *mut PML4,
    count : u64,
    // Do we need a list of processes that share an address space?
    // (That would remove the need for .count, I think.)
    //.procs    resq 1
//  .handles    restruc dict
//  .pending    restruc dict

    mapcards : Dict<MapCard>,
    backings : Dict<Backing>,
    sharings : Dict<Sharing>
}

fn alloc_pml4() -> *mut PML4 {
    let res : *mut PML4 = cpu().memory.alloc_frame_panic();
    // Copy a reference to the kernel memory range into the new PML4.
    // Since this currently is at most one 4TB range, this is easy: only a
    // single PML4 entry maps everything by sharing the kernel's lower
    // page tables between all processes.
    unsafe { (*res)[511] = start32::kernel_pdp_addr() | 3; }
    return res;
}

fn get_alloc_pt(table : *mut PML4, index_ : u64, flags : u64) -> *mut PageTable {
    let index = (index_ & 0x1ff) as usize;
    unsafe {
        let existing = (*table)[index];
        if (existing & 1) == 0 {
            let new : *mut PML4 = cpu().memory.alloc_frame_panic();
            (*table)[index] = ((new as u64 - start32::kernel_base) | flags) as u64;
            return new;
        } else {
            return ((existing & !0xfff) + start32::kernel_base as u64) as *mut PageTable;
        }
    }
}

fn paddr_for_vpaddr<T>(vpaddr: *mut T) -> u64 {
    return vpaddr as u64 - start32::kernel_base;
}

impl AddressSpace {
    fn init(&mut self) {
        self.pml4 = alloc_pml4();
        self.count = 1;
    }

    pub fn new() -> *mut AddressSpace {
        let res = alloc::<AddressSpace>();
        res.init();
        res as *mut AddressSpace
    }

    pub fn cr3(&self) -> u64 {
        return paddr_for_vpaddr(self.pml4);
    }

    // FIXME This should return by-value and have a default value instead.
    pub fn mapcard_find<'a>(&mut self, vaddr : u64) -> Option<&'a mut MapCard> {
        return self.mapcards.find(vaddr);
    }

    pub fn mapcard_find_def(&self, vaddr: u64) -> MapCard {
        match self.mapcards.find_const(vaddr) {
            Some(card) => *card,
            None => MapCard::new(vaddr, 0, 0, 0),
        }
    }

    fn mapcard_add(&mut self, card : &MapCard) {
        self.mapcards.insert(heap_copy(card));
    }

    pub fn mapcard_set(&mut self, vaddr : u64, handle : u64, offset : u64, access : MapFlag) {
        // TODO Assert that vaddr & 0xfff == offset & 0xfff == 0
        self.mapcard_set_(&MapCard::new(vaddr, handle, offset, access));
    }

    fn mapcard_set_(&mut self, new : &MapCard) {
        match self.mapcard_find(new.vaddr()) {
            Some(card) => {
                if card.same_addr(new) {
                    card.set(new);
                    return;
                }
            },
            None => {}
        }
        self.mapcard_add(new);
    }

    pub fn map_range(&mut self, start: u64, end: u64, handle: u64, offset: u64) {
        let end_card = self.mapcard_find_def(end);
        let new_end_card = MapCard::new(end, end_card.handle, end_card.offset, 0);
        let start_card = MapCard::new(start, handle, offset, 0);
        if start_card.same(&end_card) {
            self.mapcards.remove(end_card.vaddr());
        } else {
            // Insert a new card 
            self.mapcard_set_(&new_end_card);
        }
        self.mapcards.remove_range_exclusive(start, end);
        self.mapcard_set_(&start_card);
    }

    pub fn add_shared_backing<'a>(&mut self, vaddr: u64, prot: MapFlag,
            share: &mut Sharing) -> &'a mut Backing {
        let b = Backing::new_share(vaddr, prot, share);
        share.children.append(b);
        self.backings.insert(b)
    }

    fn add_phys_backing<'a>(&mut self, card : &MapCard, vaddr : u64)
    -> &'a Backing {
        let b = Backing::new_phys(vaddr, card.flags(), card.paddr(vaddr));
        &*self.backings.insert(b)
    }

    fn add_anon_backing<'a>(&mut self, card : &MapCard, vaddr : u64)
    -> &'a Backing {
        let b = Backing::new_anon(vaddr, card.flags());
        &*self.backings.insert(b)
    }

    pub fn find_add_backing<'a>(&mut self, vaddr : u64) -> &'a Backing {
        use aspace::mapflag::*;

        match self.backings.find_const(vaddr | 0xfff) {
            Some(ref back) if back.has_vaddr(vaddr) => { return &**back; },
            _ => ()
        }

        match self.mapcard_find(vaddr) {
            Some(card) => {
                if (card.flags() & RWX) == 0 {
                    abort("No access!");
                }
                if card.handle == 0 {
                    if (card.flags() & DMA) == Anon {
                        return self.add_anon_backing(card, vaddr);
                    } else if (card.flags() & Phys) != 0 {
                        return self.add_phys_backing(card, vaddr);
                    } else {
                        abort("not anon or phys for handle==0");
                    }
                } else {
                    abort("user mappings unimplemented");
                }
            },
            None => {
                abort("No mapping found!");
            }
        }
    }

    pub fn share_backing<'a>(&mut self, vaddr: u64) -> &'a mut Sharing {
        let back = self.find_add_backing(vaddr);
        let s = Sharing::new(self, back);
        self.sharings.insert(s)
    }

    pub fn add_pte(&mut self, vaddr : u64, pte : u64) {
        if log_add_pte {
            write("Mapping ");
            con::writePHex(vaddr);
            write(" to ");
            con::writePHex(pte);
            con::newline();
        }
        let pdp = get_alloc_pt(self.pml4, vaddr >> 39, 7);
        let pd = get_alloc_pt(pdp, vaddr >> 30, 7);
        let pt = get_alloc_pt(pd, vaddr >> 21, 7);
        unsafe { (*pt)[(vaddr as usize >> 12) & 0x1ff] = pte as u64; }
    }
}
