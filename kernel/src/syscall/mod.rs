use x86_64::registers::model_specific::Msr;

use crate::arch::{
    gdt::{kernel_code_selector, user_code_selector},
    interrupt::syscall_entry::syscall_entry,
};

pub mod helpers;
pub mod syscall_dispatch;
pub mod syscall_impl;

const IA32_EFER: u32 = 0xC000_0080;
const IA32_STAR: u32 = 0xC000_0081;
const IA32_LSTAR: u32 = 0xC000_0082;
const IA32_SFMASK: u32 = 0xC000_0084;

const EFER_SCE: u64 = 1 << 0;

const RFLAGS_IF: u64 = 1 << 9;
const RFLAGS_DF: u64 = 1 << 10;

const ARCH_SET_GS: i32 = 0x1001;
const ARCH_SET_FS: i32 = 0x1002;
const ARCH_GET_FS: i32 = 0x1003;
const ARCH_GET_GS: i32 = 0x1004;

const IOV_MAX: usize = 1024;

// ioctl
const TIOCGWINSZ: i32 = 0x5413;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Iovec {
    base: u64,
    len: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Winsize {
    pub rows: i16,
    pub cols: i16,
    pub xpixel: i16,
    pub ypixel: i16,
}

pub fn init_syscall_msrs() {
    unsafe {
        let mut efer_reg = Msr::new(IA32_EFER);
        let efer = efer_reg.read();
        efer_reg.write(efer | EFER_SCE);
        let mut lstar_reg = Msr::new(IA32_LSTAR);
        lstar_reg.write(syscall_entry as *const () as u64);
        let mut star_reg = Msr::new(IA32_STAR);
        let star =
            ((user_code_selector().0 as u64) << 48) | ((kernel_code_selector().0 as u64) << 32);
        star_reg.write(star);
        let mut sfmask_reg = Msr::new(IA32_SFMASK);
        sfmask_reg.write(RFLAGS_IF | RFLAGS_DF);
    }
}
