use alloc::{slice, string::String, vec::Vec};
use sys::{
    MAX_ARG_LEN, MAX_ARGS,
    syscall::{errors::Errno, wrappers::getcwd},
};

unsafe fn cstr_len(mut p: *const u8) -> usize {
    let mut n = 0usize;
    while n < MAX_ARG_LEN && unsafe { core::ptr::read(p) } != 0 {
        n += 1;
        p = unsafe { p.add(1) };
    }
    n
}

pub unsafe fn argv_bytes(argc: usize, argv: *const *const u8) -> Vec<&'static [u8]> {
    let mut out: Vec<&'static [u8]> = Vec::new();
    if argv.is_null() {
        return out;
    }

    let limit = core::cmp::min(argc, MAX_ARGS);
    out.reserve(limit);

    for i in 0..limit {
        let p = unsafe { core::ptr::read(argv.add(i)) };
        if p.is_null() {
            break;
        }

        let len = unsafe { cstr_len(p) };
        if len == MAX_ARG_LEN {
            break;
        }

        let b: &'static [u8] = unsafe { slice::from_raw_parts(p, len) };
        out.push(b);
    }

    out
}

unsafe fn argv_str(argc: usize, argv: *const *const u8) -> Result<Vec<&'static str>, ()> {
    let bytes = unsafe { argv_bytes(argc, argv) };
    let mut out: Vec<&'static str> = Vec::with_capacity(bytes.len());

    for b in bytes {
        let s: &'static str = str::from_utf8(b).map_err(|_| ())?;
        out.push(s);
    }

    Ok(out)
}

pub fn parse_argv(argc: u64, argv: *const *const u8) -> Result<Vec<&'static str>, ()> {
    unsafe { argv_str(argc as usize, argv) }
}

pub fn get_cwd() -> Result<String, Errno> {
    let mut b = [0u8; 128];
    getcwd(&mut b)?;

    let path_len = unsafe { cstr_len(b.as_ptr()) };
    Ok(String::from_utf8(b[..path_len].to_vec()).unwrap())
}
