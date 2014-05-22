use core::prelude::*;
use core::mem::transmute;

use con;
use con::write;
use cpu;
use dict::*;
use dlist::*;
use mem::heap_copy;
use start32;
use util::abort;

static log_add_pte : bool = false;

pub mod mapflag {
    pub static X : uint = 1;
    pub static W : uint = 2;
    pub static R : uint = 4;
    pub static RWX : uint = 7;
    // Anonymous memory allocated to zeroes on first use.
    pub static Anon : uint = 8;
    // handle is 0; offset is (paddr - vaddr)
    pub static Phys : uint = 16;
    // Physical memory allocated and locked at map time; and deallocated when
    // unmapped.
    pub static DMA : uint = Anon | Phys;
    pub static UserAllowed : uint = DMA | RWX;
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
pub struct MapCard {
    as_node : DictNode<uint, MapCard>,
    pub handle : uint,
    // .vaddr + .offset = handle-offset to be sent to backer on fault
    // For a direct physical mapping, paddr = .vaddr + .offset
    // .offset = handle-offset - vaddr
    // .offset = paddr - .vaddr
    // The low 12 bits contain flags.
    pub offset : uint,
}

impl DictItem<uint> for MapCard {
    fn node<'a>(&'a mut self) -> &'a mut DictNode<uint, MapCard> {
        return &mut self.as_node;
    }
}

impl MapCard {
    fn new(vaddr : uint, handle : uint, offset : uint) -> MapCard {
        MapCard { as_node: DictNode::new(vaddr), handle: handle, offset: offset }
    }

    pub fn vaddr(&self) -> uint {
        return self.as_node.key;
    }

    pub fn paddr(&self, vaddr : uint) -> uint {
        return vaddr + (self.offset & !0xfff);
    }

    pub fn flags(&self) -> uint {
        return self.offset & 0xfff;
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
    as_node : DictNode<uint, Backing>,
    // Pointer to parent sharing. Needed to unlink self when unmapping.
    // Could have room for flags (e.g. to let it be a paddr when we don't
    // need the parent - we might have a direct physical address mapping)
    parent_paddr : uint,
    // Space to participate in parent's list of remappings.
    child_node : DListNode<Backing>
}

impl DictItem<uint> for Backing {
    fn node<'a>(&'a mut self) -> &'a mut DictNode<uint, Backing> {
        return &mut self.as_node;
    }
}

impl DListItem for Backing {
    fn node<'a>(&'a mut self) -> &'a mut DListNode<Backing> {
        return &mut self.child_node;
    }
}

fn alloc_frame_paddr() -> uint {
    let p : *mut u8 = cpu().memory.alloc_frame_panic();
    p as uint - start32::kernel_base
}

impl Backing {
    fn new_phys(vaddrFlags : uint, phys_addr : uint) -> Backing {
        if phys_addr & 0xfff != 0 {
            abort("Bad physical address in new_phys");
        }
        Backing {
            as_node: DictNode::new(vaddrFlags),
            parent_paddr : phys_addr,
            child_node: DListNode::new(),
        }
    }

    fn new_anon(vaddrFlags : uint) -> Backing {
        Backing {
            as_node: DictNode::new(vaddrFlags | mapflag::Phys),
            parent_paddr : alloc_frame_paddr(),
            child_node: DListNode::new(),
        }
    }

    pub fn has_vaddr(&self, vaddr : uint) -> bool {
        return self.vaddr() == vaddr & !0xfff;
    }

    pub fn vaddr(&self) -> uint {
        return self.as_node.key & !0xfff;
    }

    pub fn flags(&self) -> uint {
        return self.as_node.key & 0xfff;
    }

    fn pte_flags(&self) -> uint {
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

    pub fn pte(&self) -> uint {
        if (self.flags() & mapflag::Phys) != 0 {
            return self.parent_paddr | self.pte_flags();
        } else {
            abort("pte: non-physical backings unimplemented");
        }
    }
}

// sharing: mapping one page to every place it's been shared to
// 7 words!
pub struct Sharing {
    as_node : DictNode<uint, Sharing>,
    paddr : uint,
    aspace : *mut AddressSpace,
    children : DList<Backing>,
}

impl DictItem<uint> for Sharing {
    fn node<'a>(&'a mut self) -> &'a mut DictNode<uint, Sharing> {
        return &mut self.as_node;
    }
}

type PageTable = [u64, ..512];
type PML4 = PageTable;

pub struct AddressSpace {
    // Upon setup, pml4 is set to a freshly allocated frame that is empty
    // except for the mapping to the kernel memory area (which, as long as
    // it's less than 4TB is only a single entry in the PML4).
    // (This is the virtual address in the higher half. proc.cr3 is a
    // physical address.)
    pml4 : *mut PML4,
    count : uint,
    // Do we need a list of processes that share an address space?
    // (That would remove the need for .count, I think.)
    //.procs    resq 1
//  .handles    restruc dict
//  .pending    restruc dict

    mapcards : Dict<uint, MapCard>,
    backings : Dict<uint, Backing>,
    sharings : Dict<uint, Sharing>
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

fn get_alloc_pt(table : *mut PML4, mut index : uint, flags : uint) -> *mut PageTable {
    index &= 0x1ff;
    unsafe {
        let existing = (*table)[index];
        if (existing & 1) == 0 {
            let new : *mut PML4 = cpu().memory.alloc_frame_panic();
            (*table)[index] = ((new as uint - start32::kernel_base) | flags) as u64;
            return new;
        } else {
            return ((existing & !0xfff) + start32::kernel_base as u64) as *mut PageTable;
        }
    }
}

impl AddressSpace {
    pub fn new() -> AddressSpace {
        AddressSpace {
            pml4 : alloc_pml4(),
            count : 1,
            mapcards : Dict::empty(),
            backings : Dict::empty(),
            sharings : Dict::empty()
        }
    }

    pub fn cr3(&self) -> uint {
        let vaddr : *PML4 = unsafe { transmute(self.pml4) };
        return vaddr as uint - start32::kernel_base;
    }

    // FIXME This should return by-value and have a default value instead.
    pub fn mapcard_find<'a>(&mut self, vaddr : uint) -> Option<&'a mut MapCard> {
        return self.mapcards.find(vaddr);
    }

    pub fn mapcard_find_def(&self, vaddr: uint) -> MapCard {
        match self.mapcards.find_const(vaddr) {
            Some(card) => *card,
            None => MapCard::new(vaddr, 0, 0),
        }
    }

    fn mapcard_add(&mut self, card : &MapCard) {
        self.mapcards.insert(heap_copy(*card));
    }

    pub fn mapcard_set(&mut self, vaddr : uint, handle : uint, offset : uint, access : uint) {
        // TODO Assert that vaddr & 0xfff == offset & 0xfff == 0
        self.mapcard_set_(&MapCard::new(vaddr, handle, offset | access));
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

    pub fn map_range(&mut self, start: uint, end: uint, handle: uint, offset: uint) {
        let end_card = self.mapcard_find_def(end);
        let new_end_card = MapCard::new(end, end_card.handle, end_card.offset);
        let start_card = MapCard::new(start, handle, offset);
        if start_card.same(&end_card) {
            self.mapcards.remove(end_card.vaddr());
        } else {
            // Insert a new card 
            self.mapcard_set_(&new_end_card);
        }
        self.mapcards.remove_range_exclusive(start, end);
        self.mapcard_set_(&start_card);
    }

    fn add_phys_backing<'a>(&mut self, card : MapCard, vaddr : uint)
    -> &'a Backing {
        let b = heap_copy(Backing::new_phys(vaddr | card.flags(), card.paddr(vaddr)));
        self.backings.insert(b);
        return unsafe { &*b };
    }

    fn add_anon_backing<'a>(&mut self, card : MapCard, vaddr : uint)
    -> &'a Backing {
        let b = heap_copy(Backing::new_anon(vaddr | card.flags()));
        self.backings.insert(b);
        return unsafe { &*b };
    }

    pub fn find_add_backing<'a>(&mut self, vaddr : uint) -> &'a Backing {
        use aspace::mapflag::*;
        use util::abort;

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
                    if (card.flags() & Anon) != 0 {
                        return self.add_anon_backing(*card, vaddr);
                    } else if (card.flags() & Phys) != 0 {
                        return self.add_phys_backing(*card, vaddr);
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

    pub fn add_pte(&mut self, vaddr : uint, pte : uint) {
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
        unsafe { (*pt)[(vaddr >> 12) & 0x1ff] = pte as u64; }
    }
}
