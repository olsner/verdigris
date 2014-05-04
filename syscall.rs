use core::prelude::*;

use con;
use con::write;
use cpu;
use process;

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
    let c = cpu();
    let p = unsafe { c.get_process() };
    p.unset(process::Running);
    p.set(process::FastRet);

    write("syscall! nr=");
    con::writeUInt(nr);
    write(" from process ");
    con::writeMutPtr(p);
    con::newline();

    unsafe { c.run(); }
}

