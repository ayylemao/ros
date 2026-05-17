use num_enum::TryFromPrimitive;

use crate::{FileOpenFlags, IOCTL_TTY_GET_FLAGS, IOCTL_TTY_SET_FLAGS, syscall::errors::Errno};

#[repr(u64)]
#[derive(TryFromPrimitive, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sysno {
    Read = 0,
    Write = 1,
    Open = 2,
    Close = 3,
    Mmap = 9,
    Brk = 12,
    RtSigAction = 13,
    RtSigProcMask = 14,
    Ioctl = 16,
    Writev = 20,
    Sleep = 35,
    GetPid = 39,
    Spawn = 59,
    Exit = 60,
    Getdents = 78,
    Getcwd = 79,
    Chdir = 80,
    Umask = 95,
    GetUid = 102,  // STUBBED TO ZERO
    GeteUid = 107, // STUBBED TO ZERO
    ArchPrctl = 158,
    SetTidAddress = 218,
    ExitGroup = 231, // STUBBED TO EXIT
    Waitid = 247,
    Ps = 500,
}

#[inline(always)]
unsafe fn syscall0(n: u64) -> Result<i64, Errno> {
    let ret: i64;
    let mut _rcx: u64;
    let mut _r11: u64;

    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") n,
            lateout("rax") ret,
            lateout("rcx") _rcx,
            lateout("r11") _r11,
            options(nostack),
        );
    }

    if ret >= 0 {
        Ok(ret)
    } else {
        Err(Errno::try_from(ret).unwrap())
    }
}

#[inline(always)]
unsafe fn syscall1(n: u64, a1: u64) -> Result<i64, Errno> {
    let ret: i64;
    let mut _rcx: u64;
    let mut _r11: u64;

    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") n,
            in("rdi") a1,
            lateout("rax") ret,
            lateout("rcx") _rcx,
            lateout("r11") _r11,
            options(nostack),
        );
    }

    if ret >= 0 {
        Ok(ret)
    } else {
        Err(Errno::try_from(ret).unwrap())
    }
}

#[allow(dead_code)]
#[inline(always)]
unsafe fn syscall2(n: u64, a1: u64, a2: u64) -> Result<i64, Errno> {
    let ret: i64;
    let mut _rcx: u64;
    let mut _r11: u64;

    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") n,
            in("rdi") a1,
            in("rsi") a2,
            lateout("rax") ret,
            lateout("rcx") _rcx,
            lateout("r11") _r11,
            options(nostack),
        );
    }

    if ret >= 0 {
        Ok(ret)
    } else {
        Err(Errno::try_from(ret).unwrap())
    }
}

#[inline(always)]
unsafe fn syscall3(n: u64, a1: u64, a2: u64, a3: u64) -> Result<i64, Errno> {
    let ret: i64;
    let mut _rcx: u64;
    let mut _r11: u64;

    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") n,
            in("rdi") a1,
            in("rsi") a2,
            in("rdx") a3,
            lateout("rax") ret,
            lateout("rcx") _rcx,
            lateout("r11") _r11,
            options(nostack),
        );
    }

    if ret >= 0 {
        Ok(ret)
    } else {
        Err(Errno::try_from(ret).unwrap())
    }
}

#[inline(always)]
unsafe fn syscall4(n: u64, a1: u64, a2: u64, a3: u64, a4: u64) -> Result<i64, Errno> {
    let ret: i64;
    let mut _rcx: u64;
    let mut _r11: u64;

    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") n,
            in("rdi") a1,
            in("rsi") a2,
            in("rdx") a3,
            in("r10") a4,
            lateout("rax") ret,
            lateout("rcx") _rcx,
            lateout("r11") _r11,
            options(nostack),
        );
    }

    if ret >= 0 {
        Ok(ret)
    } else {
        Err(Errno::try_from(ret).unwrap())
    }
}

pub fn read(fd: u64, buf: &mut [u8]) -> Result<i64, Errno> {
    unsafe {
        syscall3(
            Sysno::Read as u64,
            fd,
            buf.as_mut_ptr() as u64,
            buf.len() as u64,
        )
    }
}

pub fn write(fd: u64, buf: &[u8]) -> Result<i64, Errno> {
    unsafe {
        syscall3(
            Sysno::Write as u64,
            fd,
            buf.as_ptr() as u64,
            buf.len() as u64,
        )
    }
}

pub fn open(path: &str, flags: FileOpenFlags) -> Result<i64, Errno> {
    let path = path.as_bytes();
    unsafe {
        syscall3(
            Sysno::Open as u64,
            path.as_ptr() as u64,
            path.len() as u64,
            flags.bits(),
        )
    }
}

pub fn ioctl(fd: u32, req: u64, arg: u64) -> Result<i64, Errno> {
    unsafe { syscall3(Sysno::Ioctl as u64, fd as u64, req, arg) }
}

pub fn tty_set_flags(fd: u32, flags: u64) -> Result<i64, Errno> {
    ioctl(fd, IOCTL_TTY_SET_FLAGS, flags)
}

pub fn tty_get_flags(fd: u32) -> Result<i64, Errno> {
    ioctl(fd, IOCTL_TTY_GET_FLAGS, 0)
}

pub fn sleep(ms: u64) -> Result<i64, Errno> {
    unsafe { syscall1(Sysno::Sleep as u64, ms) }
}

pub fn spawn(path: &[u8], argc: u64, argv_ptr: u64) -> Result<i64, Errno> {
    unsafe {
        syscall4(
            Sysno::Spawn as u64,
            path.as_ptr() as u64,
            path.len() as u64,
            argc,
            argv_ptr,
        )
    }
}

pub fn exit(exit_code: i64) -> ! {
    unsafe { _ = syscall1(Sysno::Exit as u64, exit_code as u64) };
    loop {}
}

pub fn brk(addr: u64) -> Result<u64, Errno> {
    // syscall returns new/current break; you can’t rely on -errno for failure
    Ok(unsafe { syscall1(Sysno::Brk as u64, addr).unwrap() } as u64)
}

pub fn chdir(path: &str) -> Result<i64, Errno> {
    let path = path.as_bytes();
    unsafe { syscall2(Sysno::Chdir as u64, path.as_ptr() as u64, path.len() as u64) }
}

pub fn getdents(fd: u64, buf: &mut [u8]) -> Result<i64, Errno> {
    unsafe {
        syscall3(
            Sysno::Getdents as u64,
            fd,
            buf.as_mut_ptr() as u64,
            buf.len() as u64,
        )
    }
}

pub fn close(fd: i64) -> Result<i64, Errno> {
    unsafe { syscall1(Sysno::Close as u64, fd as u64) }
}

pub fn getcwd(buf: &mut [u8]) -> Result<i64, Errno> {
    unsafe { syscall2(Sysno::Getcwd as u64, buf.as_ptr() as u64, buf.len() as u64) }
}

pub fn waitpid(pid: u64) -> Result<i64, Errno> {
    unsafe { syscall1(Sysno::Waitid as u64, pid) }
}

pub fn ps() -> Result<i64, Errno> {
    unsafe { syscall0(Sysno::Ps as u64) }
}

pub fn getpid() -> Result<i64, Errno> {
    unsafe { syscall0(Sysno::GetPid as u64) }
}
