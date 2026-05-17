use sys::syscall::wrappers::Sysno;

use crate::{
    kprintln,
    syscall::{helpers, syscall_impl},
    utils::ringbuffer::SpscRing,
};

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct GprFrame {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rbp: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rbx: u64,
    pub rax: u64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct IretFrame {
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct TrapFrame {
    pub regs: GprFrame,
    pub vector: u64,
    pub error_code: u64,
    pub iret: IretFrame,
}

#[derive(Debug, Clone, Copy)]
pub struct Strace {
    pub pid: u64,
    pub sysno: u64,
}

pub static STRACE_BUF: SpscRing<Strace, 2048> = SpscRing::new();

#[no_mangle]
pub fn syscall_dispatch(trap_frame: *mut TrapFrame) {
    let frame = unsafe { &mut *(trap_frame) };

    let pid = helpers::current_task_pid().unwrap();
    _ = STRACE_BUF.push(Strace {
        pid: pid,
        sysno: frame.regs.rax,
    });

    let sysno = match Sysno::try_from(frame.regs.rax) {
        Ok(v) => v,
        Err(_e) => {
            kprintln!("Syscall {} not implemented, exiting proc!", frame.regs.rax);
            syscall_impl::__sys_exit(1, frame);
            return;
        }
    };

    match sysno {
        Sysno::Read => {
            let fd = frame.regs.rdi as u32;
            let ptr = frame.regs.rsi as *mut u8;
            let len = frame.regs.rdx as usize;
            frame.regs.rax = syscall_impl::__sys_read(frame, fd, ptr, len) as u64;
        }
        Sysno::Write => {
            let fd = frame.regs.rdi as u32;
            let ptr = frame.regs.rsi as *const u8;
            let len = frame.regs.rdx as usize;
            frame.regs.rax = syscall_impl::__sys_write(fd, ptr, len) as u64;
        }
        Sysno::Open => {
            let ptr_in = frame.regs.rdi as *const u8;
            let len = frame.regs.rsi as usize;
            let flags = frame.regs.rdx as u64;
            frame.regs.rax = syscall_impl::__sys_open(ptr_in, len, flags) as u64;
        }
        Sysno::Close => {
            let fd = frame.regs.rdi;
            frame.regs.rax = syscall_impl::__sys_close(fd) as u64;
        }
        Sysno::Brk => {
            let addr = frame.regs.rdi;
            frame.regs.rax = syscall_impl::__sys_brk(addr) as u64;
        }
        Sysno::Sleep => {
            let duration = frame.regs.rdi;
            frame.regs.rax = syscall_impl::__sys_sleep(duration, frame) as u64;
        }
        Sysno::Spawn => {
            let ptr_in = frame.regs.rdi as *const u8;
            let len = frame.regs.rsi as usize;
            let argv_ptr = frame.regs.r10;

            let rc = syscall_impl::__sys_spawn(ptr_in, len, frame, argv_ptr);
            frame.regs.rax = rc as u64;
        }
        Sysno::Exit => {
            let exit_code = frame.regs.rdi as i64;
            syscall_impl::__sys_exit(exit_code, frame);
        }
        Sysno::Chdir => {
            let ptr_in = frame.regs.rdi as *const u8;
            let len = frame.regs.rsi as usize;
            frame.regs.rax = syscall_impl::__sys_chdir(ptr_in, len) as u64;
        }
        Sysno::Getdents => {
            let fd = frame.regs.rdi;
            let ptr_out = frame.regs.rsi;
            let len = frame.regs.rdx as usize;
            frame.regs.rax = syscall_impl::__sys_getdents(fd, ptr_out as *mut u8, len) as u64;
        }
        Sysno::Getcwd => {
            let ptr_out = frame.regs.rdi;
            let len = frame.regs.rsi as usize;
            frame.regs.rax = syscall_impl::__sys_getcwd(ptr_out as *mut u8, len) as u64;
        }
        Sysno::Waitid => {
            let pid = frame.regs.rdi;
            frame.regs.rax = syscall_impl::__sys_waitid(pid, frame) as u64;
        }
        Sysno::Ps => {
            syscall_impl::__sys_ps();
            frame.regs.rax = 0 as u64;
        }
        Sysno::GetPid => {
            frame.regs.rax = syscall_impl::__sys_get_pid() as u64;
        }
        Sysno::ArchPrctl => {
            let op = frame.regs.rdi as i32;
            let addr = frame.regs.rsi as u64;
            frame.regs.rax = syscall_impl::__sys_arch_prctl(op, addr) as u64;
        }
        Sysno::SetTidAddress => {
            let tidptr = frame.regs.rdi;
            frame.regs.rax = syscall_impl::__sys_set_tid_address(tidptr);
        }
        Sysno::ExitGroup => {
            let exit_code = frame.regs.rdi as i64;
            syscall_impl::__sys_exit(exit_code, frame);
        }
        Sysno::Writev => {
            let fd = frame.regs.rdi;
            let iovec_addr = frame.regs.rsi;
            let iovcnt = frame.regs.rdx;
            frame.regs.rax = syscall_impl::__sys_writev(fd, iovec_addr, iovcnt) as u64;
        }
        Sysno::Ioctl => {
            let fd = frame.regs.rdi as u32;
            let op = frame.regs.rsi as i32;
            let argp = frame.regs.rdx;
            frame.regs.rax = syscall_impl::__sys_ioctl(fd, op, argp) as u64;
        }
        Sysno::RtSigAction => {
            let signum = frame.regs.rdi as i32;
            let act_ptr = frame.regs.rsi;
            let old_act_ptr = frame.regs.rdx;
            let sigsetsize = frame.regs.r10;
            frame.regs.rax =
                syscall_impl::__sys_rt_sigaction(signum, act_ptr, old_act_ptr, sigsetsize) as u64;
        }
        Sysno::RtSigProcMask => {
            let how = frame.regs.rdi as i32;
            let set_ptr = frame.regs.rsi;
            let oldset_ptr = frame.regs.rdx;
            let sigsetsize = frame.regs.r10;
            frame.regs.rax =
                syscall_impl::__sys_rt_sigprocmask(how, set_ptr, oldset_ptr, sigsetsize) as u64;
        }
        Sysno::GetUid => {
            frame.regs.rax = 0;
        }
        Sysno::GeteUid => {
            frame.regs.rax = 0;
        }
        Sysno::Mmap => {
            let addr = frame.regs.rdi as u64;
            let len = frame.regs.rsi as u64;
            let prot = frame.regs.rdx as i32;
            let flags = frame.regs.r10 as i32;
            let fd = frame.regs.r8 as i32;
            let offset = frame.regs.r9 as u64;
            frame.regs.rax = syscall_impl::__sys_mmap(addr, len, prot, flags, fd, offset) as u64;
        }
        Sysno::Umask => {
            let mask = frame.regs.rdi as u16;
            frame.regs.rax = syscall_impl::__sys_umask(mask) as u64;
        }
    }
}

//frame.regs.rdi/rsi/rdx/r10/r8/r9 are args 1-6
