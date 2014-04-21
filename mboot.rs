#![allow(dead_code)]

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
    colors      : [u8, ..6],
}

pub enum FBType {
    Indexed = 0,
    RGB = 1,
    Text = 2,
}

pub struct FBPalette {
    addr        : u32,
    count       : u16,
}

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
	MBI_FLAG_BOOTDEV = 2,
	CommandLine = 4,
	Modules = 8,
	MBI_FLAGS_SYMS = 16 | 32,
	MemoryMap = 64,
	MBI_FLAG_DRIVES = 128,
	MBI_FLAG_CFGTBL = 256,
	MBI_FLAG_LOADER_NAME = 512,
	MBI_FLAG_APM_TABLE = 1024,
	MBI_FLAG_VBE = 2048
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
    mods_count  : u32,
    mods_addr   : u32,

// 
    syms        : [u32, ..4],

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
}

pub struct Module {
    start       : u32,
    end         : u32,
    string      : u32,
    res    : u32,
}

#[packed]
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
