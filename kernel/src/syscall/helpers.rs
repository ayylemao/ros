use core::ptr;

use alloc::{
    slice,
    string::{String, ToString},
    sync::Arc,
    vec::Vec,
};
use shared::{HZ, PAGE_SIZE};
use sys::{MAX_ARGS, MAX_ARG_LEN};

use crate::{
    fs::file_descriptor::FileDescriptor,
    proc::{proc_manager::ProcessManager, task_manager::TaskManager},
};

pub fn current_pid() -> Option<u64> {
    let tid = TaskManager::current_tid();
    if tid == 0 {
        return None;
    }

    let tm = TaskManager::get().lock();
    tm.tasks.get(&tid).map(|t| t.pid)
}

pub fn current_task_ids() -> Option<(u64, u64)> {
    let tid = TaskManager::current_tid();
    let tm = TaskManager::get().lock();
    let task = tm.tasks.get(&tid)?;
    Some((tid, task.pid))
}

pub fn current_task_pid() -> Option<u64> {
    let tid = TaskManager::current_tid();
    let tm = TaskManager::get().lock();
    let task = tm.tasks.get(&tid)?;
    Some(task.pid)
}

pub fn get_fd_for_curr_proc(fd: u32) -> Option<Arc<FileDescriptor>> {
    let f = {
        let pid = match current_pid() {
            Some(p) => p,
            None => return None,
        };
        let mut pm = ProcessManager::procman().lock();
        let proc = pm.get_mut(pid).ok_or(-1isize).unwrap();
        match proc.fds.get(fd as usize).and_then(|x| x.as_ref()) {
            Some(f) => Arc::clone(f),
            None => return None,
        }
    };
    Some(f)
}

unsafe fn cstr_len(mut p: *const u8) -> usize {
    let mut n = 0usize;
    while n < MAX_ARG_LEN && ptr::read(p) != 0 {
        n += 1;
        p = p.add(1);
    }
    n
}

pub fn copy_user_argv(argv_ptr: *const *const u8) -> Result<Vec<String>, ()> {
    if argv_ptr.is_null() {
        return Ok(Vec::new());
    }

    let mut owned = Vec::new();

    unsafe {
        for i in 0..MAX_ARGS {
            let p = ptr::read(argv_ptr.add(i));
            if p.is_null() {
                break;
            }

            let len = cstr_len(p);
            if len == MAX_ARG_LEN {
                return Err(());
            }

            let bytes = slice::from_raw_parts(p, len);
            let s = str::from_utf8(bytes).map_err(|_| ())?;
            owned.push(s.to_string());
        }
    }

    Ok(owned)
}

pub fn ms_to_ticks(ms: u64) -> u64 {
    (ms * HZ as u64) / 1000
}

#[inline]
pub fn sig_bit(sig: usize) -> u64 {
    1u64 << (sig - 1) // signal 1 => bit 0
}

pub fn zero_user_page(va: u64) {
    debug_assert_eq!(va & (PAGE_SIZE - 1), 0);
    unsafe {
        core::ptr::write_bytes(va as *mut u8, 0, PAGE_SIZE as usize);
    }
}
