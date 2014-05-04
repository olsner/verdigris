//use core::prelude::*;

use con;
use con::write;
use cpu;
use process;

// Note: tail-called from the syscall code, "return" by switching to a process.
#[no_mangle]
pub fn syscall(
    _arg0: uint,
    _arg1: uint,
    _arg2: uint,
    _arg3: uint,
    _arg4: uint,
    _arg5: uint,
    nr : uint, // saved_rax
) -> ! {
    let c = cpu();
    let p = unsafe { c.get_process() };
    // FIXME cpu.leave_proc?
    p.unset(process::Running);
    p.set(process::FastRet);

    write("syscall! nr=");
    con::writeUInt(nr);
    write(" from process ");
    con::writeMutPtr(p);
    con::newline();

    unsafe { c.run(); }
}

