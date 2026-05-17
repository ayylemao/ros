#![no_std]
#![no_main]

extern crate kernel_lib;

use core::panic::PanicInfo;
use kernel_lib::kprintln;
use shared::BootInfo;

#[no_mangle]
pub extern "C" fn kstart(bootinfo: *const BootInfo) -> ! {
    let bootinfo = unsafe { &*bootinfo };
    kernel_lib::kmain(bootinfo)
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    kprintln!("PANIC: {}", _info);
    loop {}
}
