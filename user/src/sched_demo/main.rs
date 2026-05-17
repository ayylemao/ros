#![allow(unused_variables)]
#![no_std]
#![no_main]

extern crate alloc;

use sys::{
    println,
    syscall::{self},
};
use user_rt::{self as _};

#[unsafe(no_mangle)]
pub fn main() -> i64 {
    //let path = "/usr/bin/task_demo";
    //for i in 0..5 {
    //    let r = process::spawn(path, &[]).unwrap();
    //}
    let res = syscall::wrappers::getpid().unwrap();
    println!("{res}");
    return 0;
}
