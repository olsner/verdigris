use core::prelude::*;
use core::mem::transmute;

use dict::*;
use dlist::*;
use cpu;
use start32;
use alloc;

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
    handle : uint,
    // .vaddr + .offset = handle-offset to be sent to backer on fault
    // For a direct physical mapping, paddr = .vaddr + .offset
    // .offset = handle-offset - vaddr
    // .offset = paddr - .vaddr
    // The low 12 bits contain flags.
    offset : uint,
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

    fn vaddr(&self) -> uint {
        return self.as_node.key;
    }

    fn same_addr(&self, other : &MapCard) -> bool {
        self.vaddr() == other.vaddr()
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
    // Flags stored in low bits of vaddr!
    vaddr : uint,
    as_node : DictNode<uint, Backing>,
    // Pointer to parent sharing. Needed to unlink self when unmapping.
    // Could have room for flags (e.g. to let it be a paddr when we don't
    // need the parent - we might have a direct physical address mapping)
    parent : *mut Sharing,
    // Space to participate in parent's list of remappings.
    child_node : DListNode<Backing>
}

impl DictItem<uint> for Backing {
    fn node<'a>(&'a mut self) -> &'a mut DictNode<uint, Backing> {
        return &mut self.as_node;
    }
}

// sharing: mapping one page to every place it's been shared to
// 7 words!
pub struct Sharing {
    vaddr : uint,
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

type PML4 = [u64, ..512];

pub struct AddressSpace {
    // Upon setup, pml4 is set to a freshly allocated frame that is empty
    // except for the mapping to the kernel memory area (which, as long as
    // it's less than 4TB is only a single entry in the PML4).
    // (This is the virtual address in the higher half. proc.cr3 is a
    // physical address.)
    pml4 : *mut [u64, ..512],
    count : uint,
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

fn heap_copy<T>(x : T) -> *mut T {
    unsafe {
        let res : *mut T = alloc();
        *res = x;
        return res;
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

    pub fn mapcard_find<'a>(&'a mut self, vaddr : uint) -> Option<&'a mut MapCard> {
        match self.mapcards.find(vaddr) {
            Some(p) => unsafe { Some(&mut *p) },
            None => None
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
                    *card = *new;
                    return;
                }
            },
            None => {}
        }
        self.mapcard_add(new);
    }
}
