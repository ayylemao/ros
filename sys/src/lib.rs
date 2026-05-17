#![no_std]
pub mod stdio;
pub mod syscall;

pub const IOCTL_TTY_GET_FLAGS: u64 = 1;
pub const IOCTL_TTY_SET_FLAGS: u64 = 2;

pub const MAX_ARGS: usize = 12;
pub const MAX_ARG_LEN: usize = 128;

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    pub struct FileOpenFlags: u64 {
        const READ = 1;
        const WRITE = 1 << 1;
        const EXECUTE = 1 << 2;
        const CREATE = 1 << 3;
        const APPEND = 1 << 4;
        const TRUNCATE = 1 << 5;
    }
}
