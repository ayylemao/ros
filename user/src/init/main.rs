#![allow(unused_variables)]
#![no_std]
#![no_main]

extern crate alloc;

use sys::syscall::wrappers::waitpid;
use user_rt::{self as _, process};

#[unsafe(no_mangle)]
pub fn main(_argc: u64, _argv: *const *const u8) -> i64 {
    let shell_path = "/usr/bin/shell";
    let r = process::spawn(shell_path, &[]).unwrap();
    _ = waitpid(r as u64).unwrap();
    loop {
        let shell_path = "/usr/bin/shell";
        let r = process::spawn(shell_path, &[]).unwrap();
        _ = waitpid(r as u64).unwrap();
    }
}
