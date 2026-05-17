#![allow(unused_variables)]
#![no_std]
#![no_main]

extern crate alloc;

use core::ptr;

use user_rt as _;

#[inline(never)]
fn not_tail_recursive(depth: usize, seed: usize) -> usize {
    // Volatile prevents "this does nothing" optimization
    let v = unsafe { ptr::read_volatile(&seed) };

    if depth == 0 {
        return v ^ 0x9e37_79b9;
    }

    // --- recursive call is NOT in tail position ---
    let x = not_tail_recursive(depth - 1, v.wrapping_add(1));
    // work after the call => not a tail call
    x.wrapping_mul(1664525).wrapping_add(1013904223) ^ v
}

#[unsafe(no_mangle)]
pub fn main() -> i64 {
    let mut iter: usize = 0;
    let mut acc: usize = 0x1234_5678;

    for i in 0..500000u64 {
        // Do some CPU work that can't be elided.
        // Depth is small to avoid stack overflow.
        acc = not_tail_recursive(64, acc);

        iter = iter.wrapping_add(1);
    }
    return 0;
}
