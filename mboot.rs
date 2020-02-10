use core::slice;
pub use self::MemoryTypes::*;
pub use self::InfoFlags::*;

pub struct VBE {
    control_info: u32,
    mode_info   : u32,
    mode        : u16,
    iface_seg   : u16,
    iface_off   : u16,
    iface_len   : u16,
}

pub struct FB {
    addr        : u64,
    pitch       : u32,
    width       : u32,
    height      : u32,
    bpp         : u8,
    fbtype      : u8,
    colors      : [u8; 6],
}

#[allow(dead_code)]
pub enum FBType {
    Indexed = 0,
    RGB = 1,
    Text = 2,
}

#[allow(dead_code)]
pub struct FBPalette {
    addr        : u32,
    count       : u16,
}

#[allow(dead_code)]
pub struct FBPixelFormat {
    red_shift   : u8,
    red_mask    : u8,
    green_shift : u8,
    green_mask  : u8,
    blue_shift  : u8,
    blue_mask   : u8,
}

pub enum InfoFlags {
    MemorySize = 1,
    BootDevice = 2,
    CommandLine = 4,
    Modules = 8,
    // There are two formats for the symbol bit of the info struct, not sure
    // what they are though.
    Symbols = 16 | 32,
    Symbols1 = 16,
    Symbols2 = 32,
    MemoryMap = 64,
    Drives = 128,
    ConfigTable = 256,
    LoaderName = 512,
    APMTable = 1024,
    VBEInfo = 2048
}

pub struct Info {
    pub flags   : u32,
// if has(MemorySize)
    pub mem_lower   : u32,
    pub mem_upper   : u32,
// if flags[1]:
    pub boot_devices: u32,

// if has(CommandLine)
    pub cmdline     : u32,

// if has(Modules)
    pub mods_count  : u32,
    pub mods_addr   : u32,

// 
    syms        : [u32; 4],

// if has(MMap)
    pub mmap_length : u32,
    pub mmap_addr   : u32,

    drives_length: u32,
    drives_addr : u32,

    config_table: u32,

    boot_loader : u32,
    apm_table   : u32,

    vbe         : VBE,
    fb          : FB,
}

impl Info {
    pub fn has(&self, flag : InfoFlags) -> bool {
        self.flags & (flag as u32) != 0
    }

    pub fn modules(&self, make_ptr : fn(u64) -> *const Module) -> &[Module] {
        unsafe { slice::from_raw_parts(make_ptr(self.mods_addr as u64), self.mods_count as usize) }
    }
}

#[allow(dead_code)] // Spurious dead-code warning?
pub struct Module {
    pub start   : u32,
    pub end     : u32,
    pub string  : u32,
    reserved    : u32,
}

#[repr(packed)]
#[derive(Clone, Copy)]
pub struct MemoryMapItem {
    // Size of item (bytes), *not* including the item_size field
    pub item_size   : u32,
    pub start       : u64,
    pub length      : u64,
    // See values from MemoryTypes
    pub item_type   : u32,
}

pub enum MemoryTypes {
    MemoryTypeMemory = 1,
    MemoryTypeReserved = 2,
    MemoryTypeACPIRCL = 3,
    MemoryTypeACPISomething = 4,
}
