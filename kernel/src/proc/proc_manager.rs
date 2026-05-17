#![allow(dead_code)]
use alloc::collections::btree_map::BTreeMap;
use spin::Once;
use sys::syscall::errors::ProcessError;
use x86_64::structures::paging::FrameDeallocator;

use crate::{
    fs::vfs::VfsNode,
    kglobal::{self},
    proc::{
        context::KernelContext,
        process::Process,
        sched,
        task_manager::{Task, TaskManager, TaskState},
    },
    utils::irq_lock::IrqMutex,
};

static PROCESS_MANAGER: Once<IrqMutex<ProcessManager>> = Once::new();

#[derive(Debug)]
pub struct ProcessManager {
    next_pid: u64,
    pub procs: BTreeMap<u64, Process>,
}

impl ProcessManager {
    pub fn new() -> Self {
        Self {
            next_pid: 1,
            procs: BTreeMap::new(),
        }
    }

    pub fn get_next_pid(&mut self) -> u64 {
        let pid = self.next_pid;
        self.next_pid += 1;
        pid
    }

    pub fn procman() -> &'static IrqMutex<ProcessManager> {
        PROCESS_MANAGER.call_once(|| IrqMutex::new(ProcessManager::new()))
    }

    pub fn spawn(
        &mut self,
        path: &str,
        parent_pid: Option<u64>,
        args: &[&str],
        cwd_node: VfsNode,
    ) -> Result<u64, ProcessError> {
        let pid = self.get_next_pid();
        let tid = { TaskManager::get().lock().get_next_tid() };
        let (proc, iti) = Process::new(pid, path, parent_pid, args, cwd_node, tid)?;

        let mut task = Task {
            tid,
            pid,
            state: TaskState::Ready,
            ctx: Process::make_initial_user_ctx(
                iti.entry_rip,
                iti.user_rsp,
                iti.argc,
                iti.argv_ptr,
            ),
            kctx: KernelContext::default(),
            queued: false,
            wait: None,
            wake_tick: 0,
            kstack_top: iti.kstack_top,
            user_gs_base: 0,
            user_fs_base: 0,
            clear_child_tid: 0,
        };

        task.init_user_entry_kctx();

        self.procs.insert(proc.pid, proc);
        {
            TaskManager::get().lock().tasks.insert(tid, task)
        };

        sched::enqueue_ready(tid);

        Ok(pid)
    }

    pub fn get(&self, pid: u64) -> Option<&Process> {
        self.procs.get(&pid)
    }

    pub fn get_mut(&mut self, pid: u64) -> Option<&mut Process> {
        self.procs.get_mut(&pid)
    }

    pub fn remove(&mut self, pid: u64) -> Option<Process> {
        self.procs.remove(&pid)
    }

    pub fn reap_proc(&mut self, pid: u64) {
        let proc = self.procs.remove(&pid).unwrap();

        let mut bmp_alloc_guard = kglobal::bmp_alloc().lock();
        for frame in proc.allocated_frames {
            unsafe { bmp_alloc_guard.deallocate_frame(frame) };
        }
    }
}
