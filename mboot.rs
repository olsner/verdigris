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

pub struct Info {
    flags       : u32,
// if flags[0]:
    mem_lower   : u32,
    mem_upper   : u32,
// if flags[1]:
    boot_devices: u32,

// if (flags & MBI_FLAG_CMDLINE)
    cmdline     : u32,

// if (flags & MBI_FLAG_MODULES)
    mods_count  : u32,
    mods_addr   : u32,

    syms        : [u32, ..4],

// if (flags & MBI_FLAG_MMAP)
    mmap_length : u32,
    mmap_addr   : u32,

    drives_length   : u32,
    drives_addr : u32,

    config_table: u32,

    boot_loader : u32,
    apm_table   : u32,

    vbe         : VBE,
    fb          : FB,
}

pub struct Module {
    start       : u32,
    end         : u32,
    string      : u32,
    res    : u32,
}

