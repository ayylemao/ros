use core::{i64, ptr, slice, sync::atomic::Ordering};

use alloc::vec::Vec;
use shared::{align_down, align_up, PAGE_SIZE, USER_SPACE_BOTTOM, USER_SPACE_TOP};
use spin::mutex::Mutex;
use sys::{
    syscall::{errors::Errno, DirentHeader, FileKindTag},
    FileOpenFlags,
};
use x86_64::{
    registers::model_specific::Msr,
    structures::paging::{
        FrameAllocator, FrameDeallocator, Mapper, Page, PageTableFlags, Size4KiB,
    },
    VirtAddr,
};

use crate::{
    arch::{
        interrupt::idt::TICKS,
        percpu::{IA32_FS_BASE, IA32_GS_BASE},
    },
    console::{
        console::CONSOLE,
        tty::{self},
    },
    fs::{
        file_descriptor::{FileDescriptor, FileKind, OpenFile},
        vfs::Vfs,
    },
    kglobal::bmp_alloc,
    kprintln,
    memory::bmp_alloc::TrackingAlloc,
    proc::{
        mmap_region::{MmapRegion, MAP_ANONYMOUS, MAP_FIXED, MAP_PRIVATE, PROT_EXEC, PROT_WRITE},
        proc_manager::ProcessManager,
        process::UserContext,
        sched::{self},
        signal::{UserSigAction, NSIG, SIGKILL, SIGSTOP, SIG_BLOCK, SIG_SETMASK, SIG_UNBLOCK},
        task_manager::{TaskManager, TaskState, WaitReason},
    },
    syscall::{
        helpers::{self, get_fd_for_curr_proc, sig_bit, zero_user_page},
        syscall_dispatch::TrapFrame,
        Iovec, Winsize, ARCH_GET_FS, ARCH_GET_GS, ARCH_SET_FS, ARCH_SET_GS, IOV_MAX, TIOCGWINSZ,
    },
};

#[inline(always)]
fn load_ctx_into_live_trap(tf: &mut TrapFrame, ctx: &UserContext) {
    tf.regs = ctx.regs;
    tf.iret = ctx.iret;
}

#[no_mangle]
pub extern "C" fn __sys_write(fd: u32, ptr: *const u8, len: usize) -> i64 {
    if ptr.is_null() {
        return Errno::BadAdress as i64;
    }
    if len == 0 {
        return 0;
    }

    let f = match get_fd_for_curr_proc(fd) {
        Some(val) => val,
        None => {
            return Errno::BadFileDescriptor as i64;
        }
    };

    // TODO: Add validation of ptr..ptr+len being mapped/accesible
    let bytes = unsafe { slice::from_raw_parts(ptr, len) };

    let (flags, kind) = {
        let fd_lock = f.file.lock();
        (fd_lock.flags, fd_lock.kind)
    };

    if !flags.contains(FileOpenFlags::WRITE) {
        return Errno::OperationNotPermitted as i64;
    }

    match kind {
        FileKind::Tty => {
            if let Ok(s) = core::str::from_utf8(bytes) {
                crate::kprint!("{}", s);
            } else {
                for &b in bytes {
                    crate::kprint!("{}", b as char);
                }
            }
            return len as i64;
        }
        FileKind::File { node } => {
            let vfs = Vfs::get().lock();
            let mut file_lock = f.file.lock();
            let mut offset = file_lock.offset;

            if flags.contains(FileOpenFlags::TRUNCATE) {
                offset = 0;
            } else if flags.contains(FileOpenFlags::APPEND) {
                offset = match vfs.file_len(node) {
                    Ok(v) => v,
                    Err(_) => {
                        return Errno::IoError as i64;
                    }
                }
            }

            match vfs.write_at(node, offset, bytes) {
                Ok(v) => {
                    file_lock.offset += v;
                    len as i64
                }
                Err(_e) => Errno::IoError as i64,
            }
        }
        FileKind::Dir { .. } => {
            return Errno::IsADirectory as i64;
        }
    }
}

#[no_mangle]
pub extern "C" fn __sys_read(_tf: &mut TrapFrame, fd: u32, ptr_out: *mut u8, len: usize) -> i64 {
    if ptr_out.is_null() {
        return Errno::BadAdress as i64;
    }
    if len == 0 {
        return 0;
    }

    let f = match get_fd_for_curr_proc(fd) {
        Some(val) => val,
        None => return Errno::BadFileDescriptor as i64,
    };
    let (flags, kind) = {
        let fd_lock = f.file.lock();
        (fd_lock.flags, fd_lock.kind)
    };

    if !flags.contains(FileOpenFlags::READ) {
        return Errno::OperationNotPermitted as i64;
    }

    match kind {
        FileKind::Tty => loop {
            tty::Tty::pump_tty();

            let mut n = 0usize;
            while n < len {
                match tty::Tty::tty_in().pop() {
                    Some(b) => {
                        unsafe {
                            ptr_out.add(n).write(b);
                        }
                        n += 1;
                    }
                    None => break,
                }
            }

            if n > 0 {
                return n as i64;
            }

            let cur = TaskManager::current_tid();
            {
                let mut tm = TaskManager::get().lock();
                if let Some(t) = tm.tasks.get_mut(&cur) {
                    t.state = TaskState::Waiting;
                    t.wait = Some(WaitReason::IO);
                    t.queued = false;
                }
            }

            sched::block_current();
        },

        FileKind::File { node } => {
            let offset = { f.file.lock().offset };

            let buf = {
                let vfs = Vfs::get().lock();
                let mut openfile = f.file.lock();
                match vfs.read_at(node, &mut openfile, len) {
                    Ok(b) => b,
                    Err(_) => return Errno::IoError as i64,
                }
            };

            if buf.is_empty() {
                return 0;
            }

            unsafe {
                core::ptr::copy_nonoverlapping(buf.as_ptr(), ptr_out, buf.len());
            }
            {
                f.file.lock().offset = offset + buf.len();
            }
            buf.len() as i64
        }
        FileKind::Dir { .. } => Errno::IsADirectory as i64,
    }
}

pub extern "C" fn __sys_open(ptr_in: *const u8, len: usize, flags: u64) -> i64 {
    if ptr_in.is_null() {
        return Errno::InvalidArgument as i64;
    }
    if len == 0 {
        return Errno::InvalidArgument as i64;
    }

    // TODO: Add validation of ptr..ptr+len being mapped/accesible
    let path_bytes = unsafe { slice::from_raw_parts(ptr_in, len) };
    let path: &str = match str::from_utf8(path_bytes) {
        Ok(v) => v,
        Err(_e) => {
            return Errno::InvalidArgument as i64;
        }
    };

    let pid = match helpers::current_pid() {
        Some(p) => p,
        None => return Errno::NoSuchProcess as i64,
    };

    let cwd = {
        let pm = ProcessManager::procman().lock();
        let proc = pm.procs.get(&pid).unwrap();
        proc.cwd_node
    };

    let fo_flags = FileOpenFlags::from_bits(flags).unwrap();

    // optimize me
    let vnode = {
        let vfs = Vfs::get().lock();
        match vfs.resolve(cwd, path) {
            Ok(n) => n,
            Err(_) => {
                if fo_flags.contains(FileOpenFlags::CREATE) {
                    match vfs.create_path(cwd, path) {
                        Ok(v) => v,
                        Err(_) => {
                            return Errno::IoError as i64;
                        }
                    }
                } else {
                    return Errno::IoError as i64;
                }
            }
        }
    };

    let kind_tag = {
        let vfs = Vfs::get().lock();
        match vfs.node_type(vnode) {
            Ok(t) => t,
            Err(_) => return Errno::IoError as i64,
        }
    };

    let filekind = match kind_tag {
        FileKindTag::File => FileKind::File { node: vnode },
        FileKindTag::Dir => FileKind::Dir { node: vnode },
        _ => unimplemented!("DO NOT OPEN TTY OR UNKNOWN KIND OF FILES"),
    };

    if matches!(filekind, FileKind::Dir { .. })
        && (fo_flags.contains(FileOpenFlags::WRITE) || fo_flags.contains(FileOpenFlags::EXECUTE))
    {
        return Errno::OperationNotPermitted as i64;
    }

    let fd: FileDescriptor = FileDescriptor {
        file: Mutex::new(OpenFile {
            kind: filekind,
            offset: 0,
            flags: fo_flags,
            cache: None,
        }),
    };

    let fd = {
        let mut pm = ProcessManager::procman().lock();
        let proc = pm.get_mut(pid).unwrap();
        let fd = proc.alloc_fd(fd);
        fd
    };

    fd as i64
}

pub extern "C" fn __sys_ioctl(fd: u32, op: i32, argp: u64) -> i64 {
    let f = match get_fd_for_curr_proc(fd) {
        Some(v) => v,
        None => {
            return Errno::BadFileDescriptor as i64;
        }
    };

    let kind = { f.file.lock().kind };

    if kind != FileKind::Tty {
        return Errno::BadFileDescriptor as i64;
    }

    if op != TIOCGWINSZ {
        kprintln!("IOCTL: op 0x{:x} not supported!", op);
        return 0;
    }

    // TODO: Validate ptr
    let argp = argp as *mut u8;
    let winsize = { CONSOLE.lock().as_ref().unwrap().get_winsize() };
    let winsize_p = &winsize as *const _ as *const u8;
    let count = size_of::<Winsize>();
    unsafe {
        core::ptr::copy_nonoverlapping(winsize_p, argp, count);
    }

    0
}

pub extern "C" fn __sys_sleep(ms: u64, _tf: &mut TrapFrame) -> i64 {
    let tid = TaskManager::current_tid();
    let now = TICKS.load(Ordering::Acquire);

    let wake = now + ms;

    {
        let mut tm = TaskManager::get().lock();
        let task = tm.tasks.get_mut(&tid).unwrap();
        task.state = TaskState::Waiting;
        task.wait = Some(WaitReason::SleepUntil(wake));
        task.wake_tick = wake;
        task.queued = false;
    }
    sched::block_current();
    0
}

pub extern "C" fn __sys_spawn(
    ptr_in: *const u8,
    len: usize,
    _tf: &mut TrapFrame,
    argv_ptr: u64,
) -> i64 {
    if ptr_in.is_null() {
        return Errno::BadAdress as i64;
    }
    if len == 0 {
        return 0;
    }

    let path_bytes = unsafe { slice::from_raw_parts(ptr_in, len) };
    let path: &str = match str::from_utf8(path_bytes) {
        Ok(v) => v,
        Err(_e) => {
            return Errno::InvalidArgument as i64;
        }
    };

    let argv_vec = helpers::copy_user_argv(argv_ptr as *const *const u8).unwrap();
    let argv_refs: Vec<&str> = argv_vec.iter().map(|s| s.as_str()).collect();

    let parent_pid = match helpers::current_pid() {
        Some(p) => p,
        None => return Errno::NoSuchProcess as i64,
    };

    let parent_cwd = {
        let pm = ProcessManager::procman().lock();
        let parent = match pm.get(parent_pid) {
            Some(p) => p,
            None => return Errno::NoSuchProcess as i64,
        };
        parent.cwd_node
    };

    let child_pid = {
        let mut pm = ProcessManager::procman().lock();
        match pm.spawn(path, Some(parent_pid), &argv_refs, parent_cwd) {
            Ok(pid) => pid,
            Err(e) => return Errno::from(e) as i64,
        }
    };

    child_pid as i64
}

pub extern "C" fn __sys_exit(exit_code: i64, _tf: &mut TrapFrame) {
    let (tid, pid) = helpers::current_task_ids().expect("exit with no current task");
    {
        let mut tm = TaskManager::get().lock();
        let task = tm.tasks.get_mut(&tid).unwrap();
        task.state = TaskState::Exited(exit_code);
        task.queued = false;
        if task.clear_child_tid != 0 {
            let tidptr = task.clear_child_tid as *mut u32;
            unsafe {
                *tidptr = 0;
            }
        }
    }

    let mut pm = ProcessManager::procman().lock();

    let is_last_task = {
        let proc = pm.get_mut(pid).unwrap();
        proc.tasks.retain(|&t| t != tid);
        proc.tasks.is_empty()
    };

    // if not last task, just reschedule
    if !is_last_task {
        drop(pm);
        sched::exit_current();
    }

    // pm is locked here
    let parent_pid_opt = pm.get(pid).and_then(|p| p.parent_pid);

    if let Some(parent_pid) = parent_pid_opt {
        let parent_tids = match pm.get(parent_pid) {
            Some(pp) => pp.tasks.clone(),
            None => Vec::new(),
        };

        drop(pm);

        let mut tm = TaskManager::get().lock();

        for &ptid in &parent_tids {
            let Some(t) = tm.tasks.get_mut(&ptid) else {
                continue;
            };

            if t.state != TaskState::Waiting {
                continue;
            }

            let should_wake = match t.wait {
                Some(WaitReason::AnyChild) => true,
                Some(WaitReason::ChildPid(wanted)) => wanted == pid,
                _ => false,
            };

            if should_wake {
                t.wait = None;
                t.state = TaskState::Ready;

                sched::enqueue_ready_locked(&mut tm, ptid);

                break;
            }
        }
    } else {
        panic!("Process {} exited but has no parent", pid);
    }

    sched::exit_current();
}

pub extern "C" fn __sys_brk(addr: u64) -> i64 {
    let pid = match helpers::current_pid() {
        Some(p) => p,
        None => return 0,
    };

    let mut pm = ProcessManager::procman().lock();
    let proc = pm.get_mut(pid).unwrap();

    let old = proc.brk;

    // Linux: brk(0) = query
    if addr == 0 {
        return old as i64;
    }

    // Requested new break (absolute)
    let new = addr;

    // Range checks. On failure: return old unchanged.
    if new < proc.brk_start || new > proc.brk_max {
        return old as i64;
    }

    // No-op
    if new == old {
        return old as i64;
    }

    let mut mapper = unsafe { proc.address_space.mapper() };
    let mut alloc = bmp_alloc().lock();

    if new > old {
        let map_from = align_up(old);
        let map_to = align_up(new);

        if map_to > map_from {
            let mut talloc = TrackingAlloc::new(&mut *alloc);
            let mut va = map_from;

            while va < map_to {
                let frame = match talloc.allocate_frame() {
                    Some(f) => f,
                    None => return old as i64,
                };

                let res = unsafe {
                    mapper.map_to(
                        Page::<Size4KiB>::containing_address(VirtAddr::new(va)),
                        frame,
                        PageTableFlags::PRESENT
                            | PageTableFlags::WRITABLE
                            | PageTableFlags::USER_ACCESSIBLE,
                        &mut talloc,
                    )
                };

                match res {
                    Ok(flush) => flush.flush(),
                    Err(_) => return old as i64,
                }

                va += PAGE_SIZE;
            }

            let frames = core::mem::take(&mut talloc.frames);
            proc.allocated_frames.extend(frames);
        }

        proc.brk = new;
        return proc.brk as i64;
    } else {
        if new < proc.brk_start {
            return old as i64;
        }

        let unmap_to = align_up(new);
        let unmap_from = align_up(old);

        if unmap_from > unmap_to {
            let mut va = match unmap_from.checked_sub(PAGE_SIZE) {
                Some(v) => v,
                None => return old as i64,
            };

            loop {
                if va < unmap_to {
                    break;
                }

                let page = Page::<Size4KiB>::containing_address(VirtAddr::new(va));
                match mapper.unmap(page) {
                    Ok((frame, flush)) => {
                        flush.flush();

                        if let Some(idx) = proc.allocated_frames.iter().position(|p| *p == frame) {
                            proc.allocated_frames.swap_remove(idx);
                        }
                        unsafe {
                            alloc.deallocate_frame(frame);
                        }
                    }
                    Err(_) => {
                        return old as i64;
                    }
                }

                if va == unmap_to {
                    break;
                }
                va -= PAGE_SIZE;
            }
        }

        proc.brk = new;
        return proc.brk as i64;
    }
}

pub extern "C" fn __sys_chdir(ptr_in: *const u8, len: usize) -> i64 {
    if ptr_in.is_null() {
        return Errno::BadAdress as i64;
    }
    if len == 0 {
        return 0;
    }

    // TODO: Add validation of ptr..ptr+len being mapped/accesible
    let path_bytes = unsafe { slice::from_raw_parts(ptr_in, len) };
    let path: &str = match str::from_utf8(path_bytes) {
        Ok(v) => v,
        Err(_e) => {
            return Errno::InvalidArgument as i64;
        }
    };

    let pid = match helpers::current_pid() {
        Some(p) => p,
        None => return Errno::NoSuchProcess as i64,
    };

    let curr_cwd = {
        let pm = ProcessManager::procman().lock();
        let proc = pm.get(pid).ok_or(-2isize).unwrap();
        proc.cwd_node
    };

    let new_cwd = {
        let vfs = Vfs::get().lock();

        let id = match vfs.resolve(curr_cwd, path) {
            Ok(id) => id,
            Err(_) => return Errno::NotFound as i64,
        };

        let ty = match vfs.node_type(id) {
            Ok(t) => t,
            Err(_) => return Errno::IoError as i64,
        };

        if ty != FileKindTag::Dir {
            return Errno::NotADirectory as i64;
        }
        id
    };

    let mut pm = ProcessManager::procman().lock();
    let proc = pm.get_mut(pid).unwrap();
    proc.set_cwd(new_cwd);
    0
}

pub extern "C" fn __sys_getdents(fd: u64, ptr_out: *mut u8, len: usize) -> i64 {
    if ptr_out.is_null() || len == 0 {
        return 0;
    }
    let f = match get_fd_for_curr_proc(fd as u32) {
        Some(f) => f,
        None => return Errno::BadFileDescriptor as i64,
    };

    let (node_id, start_idx) = {
        let f_lock = f.file.lock();
        let node_id = match f_lock.kind {
            FileKind::Dir { node } => node,
            _ => {
                return Errno::NotADirectory as i64;
            }
        };
        (node_id, f_lock.offset)
    };

    let out = unsafe { slice::from_raw_parts_mut(ptr_out, len) };

    let direntries = {
        let vfs = Vfs::get().lock();
        match vfs.readdir_node(node_id) {
            Ok(v) => v,
            Err(e) => return Errno::from(e) as i64,
        }
    };

    let mut buf_offset = 0usize;
    let mut written_entries = 0usize;

    for entry in direntries.iter().skip(start_idx) {
        let written = unsafe {
            DirentHeader::write_dirent(
                buf_offset,
                out.as_mut_ptr(),
                entry.id as u64,
                entry.ftype as u8,
                entry.name.as_bytes(),
                out.len(),
            )
        };

        match written {
            Some(sz) => {
                buf_offset += sz;
                written_entries += 1;
            }
            None => break,
        }
    }

    f.file.lock().offset += written_entries;
    buf_offset as i64
}

pub extern "C" fn __sys_close(fd: u64) -> i64 {
    if fd == 0 || fd == 1 {
        return Errno::InvalidArgument as i64;
    }

    let pid = match helpers::current_pid() {
        Some(p) => p,
        None => return Errno::NoSuchProcess as i64,
    };
    let mut pm = ProcessManager::procman().lock();
    let proc = pm.get_mut(pid).unwrap();
    let fd = fd as usize;

    if fd >= proc.fds.len() {
        return Errno::ValueOverflow as i64;
    }
    if proc.fds[fd].is_none() {
        return Errno::BadFileDescriptor as i64;
    }

    proc.fds[fd] = None;
    0
}

pub extern "C" fn __sys_getcwd(ptr_out: *mut u8, len: usize) -> i64 {
    if ptr_out.is_null() {
        return -1;
    }
    if len == 0 {
        return 0;
    }

    let pid = match helpers::current_pid() {
        Some(p) => p,
        None => return Errno::NoSuchProcess as i64,
    };

    let cwd_node = {
        let pm = ProcessManager::procman().lock();
        let proc = pm.get(pid).ok_or(-2isize).unwrap();
        proc.cwd_node
    };

    let mut cwd_str = { Vfs::get().lock().vnode_to_path(cwd_node).unwrap() };

    if cwd_str.len() + 1 > len {
        return Errno::BadAdress as i64;
    }
    cwd_str.push('\0');

    let cwd_buf = cwd_str.as_bytes();

    unsafe {
        core::ptr::copy_nonoverlapping(cwd_buf.as_ptr(), ptr_out, cwd_str.len() + 1);
    }

    cwd_buf.len() as i64
}

pub extern "C" fn __sys_waitid(pid: u64, _tf: &mut TrapFrame) -> i64 {
    let (tid, _ppid) = helpers::current_task_ids().expect("no current task :(");

    loop {
        if pid == 0 {
            let parent_pid = match helpers::current_pid() {
                Some(p) => p,
                None => return Errno::NoSuchProcess as i64,
            };

            let pm = ProcessManager::procman().lock();
            let tm = TaskManager::get().lock();

            for task in tm.tasks.values() {
                let TaskState::Exited(code) = task.state else {
                    continue;
                };

                let Some(proc) = pm.get(task.pid) else {
                    continue;
                };

                if proc.parent_pid == Some(parent_pid) {
                    return code;
                }
            }
        } else {
            let tm = TaskManager::get().lock();
            for task in tm.tasks.values() {
                if task.pid != pid {
                    continue;
                }
                if let TaskState::Exited(code) = task.state {
                    return code;
                }
            }
        }

        {
            let mut tm = TaskManager::get().lock();
            let task = tm.tasks.get_mut(&tid).unwrap();
            task.state = TaskState::Waiting;
            task.wait = Some(match pid {
                0 => WaitReason::AnyChild,
                _ => WaitReason::ChildPid(pid),
            });
            task.queued = false;
        }

        sched::block_current();
    }
}

pub extern "C" fn __sys_get_pid() -> i64 {
    let curr = TaskManager::current_tid();
    let pid: u64 = {
        let tm = TaskManager::get().lock();
        if let Some(task) = tm.tasks.get(&curr) {
            task.pid
        } else {
            return Errno::NoSuchProcess as i64;
        }
    };
    pid as i64
}

pub extern "C" fn __sys_arch_prctl(op: i32, addr: u64) -> i64 {
    // TODO: Validate user pointer
    match op {
        ARCH_SET_GS => {
            let mut gs = Msr::new(IA32_GS_BASE);
            let curr_task = TaskManager::current_tid();
            TaskManager::get()
                .lock()
                .tasks
                .get_mut(&curr_task)
                .unwrap()
                .user_gs_base = addr;
            unsafe {
                gs.write(addr);
            }
        }
        ARCH_GET_GS => {
            if addr == 0 {
                return Errno::InvalidArgument as i64;
            }
            let curr_task = TaskManager::current_tid();
            let addr = addr as *mut u64;
            unsafe {
                *addr = TaskManager::get()
                    .lock()
                    .tasks
                    .get_mut(&curr_task)
                    .unwrap()
                    .user_gs_base;
            }
        }
        ARCH_SET_FS => {
            let mut fs = Msr::new(IA32_FS_BASE);
            let curr_task = TaskManager::current_tid();
            TaskManager::get()
                .lock()
                .tasks
                .get_mut(&curr_task)
                .unwrap()
                .user_fs_base = addr;
            unsafe {
                fs.write(addr);
            }
        }
        ARCH_GET_FS => {
            if addr == 0 {
                return Errno::InvalidArgument as i64;
            }
            let curr_task = TaskManager::current_tid();
            let addr = addr as *mut u64;
            unsafe {
                *addr = TaskManager::get()
                    .lock()
                    .tasks
                    .get_mut(&curr_task)
                    .unwrap()
                    .user_fs_base;
            }
        }
        _ => return Errno::InvalidArgument as i64,
    }

    0
}

pub extern "C" fn __sys_set_tid_address(tidptr: u64) -> u64 {
    if tidptr == 0 {
        return Errno::InvalidArgument as u64;
    }

    let curr_tid = TaskManager::current_tid();

    {
        let mut tm = TaskManager::get().lock();
        let task = tm.tasks.get_mut(&curr_tid).unwrap();
        task.clear_child_tid = tidptr;
    }

    let p = tidptr as *mut u32;

    // TODO validate/copy_to_user
    unsafe {
        *p = curr_tid as u32;
    }
    curr_tid as u64
}

pub extern "C" fn __sys_writev(fd: u64, iovec_addr: u64, iovcnt: u64) -> i64 {
    if iovcnt == 0 {
        return 0;
    }

    if iovec_addr == 0 {
        return Errno::BadAdress as i64;
    }

    if iovcnt as usize > IOV_MAX {
        return Errno::InvalidArgument as i64;
    }

    let iovs = vec![Iovec { base: 0, len: 0 }; iovcnt as usize];

    let bytes = iovcnt as usize * size_of::<Iovec>();
    unsafe {
        ptr::copy_nonoverlapping(iovec_addr as *const u8, iovs.as_ptr() as *mut u8, bytes);
    }

    let mut total: i64 = 0;

    for iov in iovs {
        if iov.len == 0 {
            continue;
        }

        let n = __sys_write(fd as u32, iov.base as *const u8, iov.len as usize);
        if n < 0 {
            return if total > 0 { total } else { n };
        }
        if n == 0 && iov.len != 0 {
            return if total > 0 {
                total
            } else {
                Errno::IoError as i64
            };
        }
        total += n;

        if (n as u64) < iov.len {
            break;
        }
    }
    total
}

pub extern "C" fn __sys_rt_sigaction(
    signum: i32,
    act_ptr: u64,
    old_act_ptr: u64,
    sigsetsize: u64,
) -> i64 {
    if sigsetsize != 8 {
        return Errno::InvalidArgument as i64;
    }
    if signum <= 0 || signum as usize >= NSIG {
        return Errno::InvalidArgument as i64;
    }
    let sig = signum as usize;
    if sig == SIGKILL || sig == SIGSTOP {
        if act_ptr != 0 {
            return Errno::InvalidArgument as i64;
        }
    }

    let curr_proc = helpers::current_pid().unwrap();
    let mut pm = ProcessManager::procman().lock();
    if let Some(proc) = pm.procs.get_mut(&curr_proc) {
        if old_act_ptr != 0 {
            let old = proc.sigactions[sig];
            unsafe {
                core::ptr::copy_nonoverlapping(
                    &old as *const _ as *const u8,
                    old_act_ptr as *mut u8,
                    size_of::<UserSigAction>(),
                );
            }
        }

        if act_ptr != 0 {
            let mut newact: UserSigAction = UserSigAction::default();
            unsafe {
                core::ptr::copy_nonoverlapping(
                    act_ptr as *const u8,
                    &mut newact as *mut _ as *mut u8,
                    size_of::<UserSigAction>(),
                );
            }
            proc.sigactions[sig - 1] = newact;
        }
    }
    0
}

pub extern "C" fn __sys_rt_sigprocmask(
    how: i32,
    set_ptr: u64,
    oldset_ptr: u64,
    sigsetsize: u64,
) -> i64 {
    if sigsetsize != 8 {
        return Errno::InvalidArgument as i64;
    }

    let curr_proc = match helpers::current_pid() {
        Some(pid) => pid,
        None => return Errno::NoSuchProcess as i64,
    };

    let mut pm = ProcessManager::procman().lock();
    let proc = match pm.procs.get_mut(&curr_proc) {
        Some(p) => p,
        None => return Errno::NoSuchProcess as i64,
    };

    // Return old mask if requested
    if oldset_ptr != 0 {
        let oldmask = proc.sigmask;
        unsafe {
            core::ptr::copy_nonoverlapping(
                &oldmask as *const _ as *const u8,
                oldset_ptr as *mut u8,
                size_of::<u64>(),
            );
        }
    }

    // No new mask provided: query only
    if set_ptr == 0 {
        return 0;
    }

    // Read requested mask
    let mut setmask: u64 = 0;
    unsafe {
        core::ptr::copy_nonoverlapping(
            set_ptr as *const u8,
            &mut setmask as *mut _ as *mut u8,
            size_of::<u64>(),
        );
    }

    // Apply operation
    let mut newmask = proc.sigmask;
    match how {
        SIG_BLOCK => newmask |= setmask,
        SIG_UNBLOCK => newmask &= !setmask,
        SIG_SETMASK => newmask = setmask,
        _ => return Errno::InvalidArgument as i64,
    }

    // SIGKILL and SIGSTOP may not be blocked
    newmask &= !(sig_bit(SIGKILL) | sig_bit(SIGSTOP));

    proc.sigmask = newmask;
    0
}

pub extern "C" fn __sys_ps() -> i64 {
    let tm = TaskManager::get().lock();
    let pm = ProcessManager::procman().lock();
    kprintln!("{:<10} {:<10} {:<10} {:<10}", "ID", "CMD", "TICKS", "STATE");
    for (pid, proc) in &pm.procs {
        if let Some(tid) = proc.tasks.get(0) {
            let task = tm.tasks.get(&tid).unwrap();
            kprintln!(
                "{:<10} {:<10} {:<10} {:?}",
                pid,
                proc.cmd,
                proc.ticks_run,
                task.state
            );
        }
    }
    drop(pm);
    drop(tm);
    0
}

pub extern "C" fn __sys_mmap(
    _addr: u64,
    len: u64,
    prot: i32,
    flags: i32,
    fd: i32,
    offset: u64,
) -> i64 {
    if len == 0 {
        return Errno::InvalidArgument as i64;
    }

    let want_anon = (flags & MAP_ANONYMOUS) != 0;
    let want_priv = (flags & MAP_PRIVATE) != 0;
    if !want_anon || !want_priv {
        return Errno::InvalidArgument as i64;
    }

    if fd != -1 || offset != 0 {
        return Errno::InvalidArgument as i64;
    }

    if (flags & MAP_FIXED) != 0 {
        return Errno::InvalidArgument as i64;
    }

    let len_rounded = (len + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);

    let curr = match helpers::current_pid() {
        Some(pid) => pid,
        None => return Errno::NoSuchProcess as i64,
    };

    let mut pm = ProcessManager::procman().lock();
    let proc = match pm.procs.get_mut(&curr) {
        Some(p) => p,
        None => return Errno::NoSuchProcess as i64,
    };

    let end = align_down(proc.mmap_top);
    let base = match end.checked_sub(len_rounded) {
        Some(v) => v,
        None => return Errno::OutOfMemory as i64,
    };

    let brk_top = (proc.brk + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
    if base < brk_top {
        return Errno::OutOfMemory as i64;
    }

    if base < USER_SPACE_BOTTOM || end > USER_SPACE_TOP {
        return Errno::OutOfMemory as i64;
    }

    let mut alloc = bmp_alloc().lock();
    let mut talloc = TrackingAlloc::new(&mut *alloc);
    let mut mapper = unsafe { proc.address_space.mapper() };

    for va in (base..end).step_by(PAGE_SIZE as usize) {
        let frame = match talloc.allocate_frame() {
            Some(f) => f,
            None => return Errno::OutOfMemory as i64,
        };

        let mut pte_flags = PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE;
        if (prot & PROT_WRITE) != 0 {
            pte_flags |= PageTableFlags::WRITABLE;
        }
        if (prot & PROT_EXEC) == 0 {
            pte_flags |= PageTableFlags::NO_EXECUTE;
        }

        let res = unsafe {
            mapper.map_to(
                Page::<Size4KiB>::containing_address(VirtAddr::new(va)),
                frame,
                pte_flags,
                &mut talloc,
            )
        };

        match res {
            Ok(flush) => flush.flush(),
            Err(_) => return Errno::OutOfMemory as i64,
        }

        zero_user_page(va);
    }

    proc.mmap_top = base;
    proc.mmaps.push(MmapRegion {
        start: base,
        len: len_rounded,
        prot,
        flags,
    });

    let frames = core::mem::take(&mut talloc.frames);
    proc.allocated_frames.extend(frames);

    base as i64
}

pub extern "C" fn __sys_umask(mask: u16) -> i64 {
    let curr = match helpers::current_pid() {
        Some(pid) => pid,
        None => return Errno::NoSuchProcess as i64,
    };
    let mut pm = ProcessManager::procman().lock();
    let proc = match pm.procs.get_mut(&curr) {
        Some(p) => p,
        None => return Errno::NoSuchProcess as i64,
    };
    let old_mask = proc.umask;
    proc.umask = mask & 0o777;
    old_mask as i64
}
