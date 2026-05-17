#![allow(unused_variables)]
#![no_std]
#![no_main]

extern crate alloc;

use sys::syscall::wrappers::read;
use user_rt as _;

use crate::shell::Shell;
mod shell;

#[unsafe(no_mangle)]
pub fn main(argc: u64, argv: *const *const u8) -> i64 {
    let mut shell = Shell::new();

    loop {
        shell.prompt();
        let mut b = [0u8; 128];
        let r = read(0, &mut b).unwrap();
        let cmd = &b[..r as usize];
        _ = shell.run_line(str::from_utf8(cmd).unwrap());
    }
}
