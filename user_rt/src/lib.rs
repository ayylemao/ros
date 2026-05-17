#![no_std]
#![feature(alloc_error_handler)]
#![feature(lang_items)]
#![allow(internal_features)]

extern crate alloc;

use core::arch::global_asm;

use sys::println;
use sys::syscall::wrappers::exit;

pub mod env;
pub mod fs;
pub mod heap;
pub mod process;

unsafe extern "Rust" {
    fn main(argc: u64, argv: *const *const u8) -> i64;
}

global_asm!(
    r#"
.global _start
.type _start, @function
_start:
    xor rbp, rbp
    mov rdi, [rsp]
    lea rsi, [rsp + 8]
    and rsp, -16
    call __user_rt_start
    ud2
"#
);

#[unsafe(no_mangle)]
pub extern "C" fn __user_rt_start(argc: u64, argv: *const *const u8) -> ! {
    let ret = unsafe { main(argc, argv) };
    exit(ret);
}

#[panic_handler]
fn panic(panic_info: &core::panic::PanicInfo) -> ! {
    println!("{:?}", panic_info);
    _ = exit(128);
}
