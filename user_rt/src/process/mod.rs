use alloc::vec::Vec;
use sys::syscall::errors::Errno;

use crate::alloc;

fn to_cstr_bytes(s: &str) -> Vec<u8> {
    let mut v = Vec::with_capacity(s.len() + 1);
    v.extend_from_slice(s.as_bytes());
    v.push(0);
    v
}

pub fn spawn(path: &str, args: &[&str]) -> Result<i64, Errno> {
    let prog = path.rsplit('/').next().unwrap_or(path);
    // 1) Build owned NUL-terminated byte buffers
    let mut cstrs: Vec<Vec<u8>> = Vec::with_capacity(args.len() + 1);
    cstrs.push(to_cstr_bytes(prog));
    for &a in args {
        cstrs.push(to_cstr_bytes(a));
    }

    let mut argv: Vec<*const u8> = Vec::with_capacity(cstrs.len() + 1);
    for s in &cstrs {
        argv.push(s.as_ptr());
    }
    argv.push(core::ptr::null());

    let path_bytes = path.as_bytes();
    sys::syscall::wrappers::spawn(path_bytes, argv.len() as u64 - 1, argv.as_ptr() as u64)
}
