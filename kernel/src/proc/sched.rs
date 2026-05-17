use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use alloc::collections::VecDeque;
use x86_64::{
    instructions::interrupts,
    registers::model_specific::{FsBase, KernelGsBase},
    VirtAddr,
};

use crate::{
    arch::{gdt, percpu},
    proc::{
        context::{context_switch, KernelContext},
        proc_manager::ProcessManager,
        task_manager::{TaskManager, TaskState, WaitReason},
    },
    syscall::{helpers, syscall_dispatch::TrapFrame},
    utils::irq_lock::IrqMutex,
};

pub type Tid = u64;

static RUN_QUEUE: IrqMutex<VecDeque<Tid>> = IrqMutex::new(VecDeque::new());
pub static NEED_RESCHED: AtomicBool = AtomicBool::new(false);

pub const QUANTUM_TICKS: u64 = 30;
pub static TICKS_LEFT: AtomicU64 = AtomicU64::new(QUANTUM_TICKS);

static mut SCHED_CTX: KernelContext = KernelContext {
    rsp: 0,
    r15: 0,
    r14: 0,
    r13: 0,
    r12: 0,
    rbx: 0,
    rbp: 0,
};

#[inline(always)]
pub fn in_user(tf: &TrapFrame) -> bool {
    (tf.iret.cs & 3) == 3
}

pub fn enqueue_ready(tid: Tid) {
    let mut tm = TaskManager::get().lock();
    enqueue_ready_locked(&mut tm, tid);
}

pub fn enqueue_ready_locked(tm: &mut TaskManager, tid: Tid) {
    let Some(t) = tm.tasks.get_mut(&tid) else {
        return;
    };

    if t.queued {
        return;
    }
    if t.state != TaskState::Ready {
        return;
    }

    t.queued = true;
    RUN_QUEUE.lock().push_back(tid);
}

fn dequeue_next_ready() -> Option<Tid> {
    let mut tm = TaskManager::get().lock();
    let mut rq = RUN_QUEUE.lock();

    while let Some(tid) = rq.pop_front() {
        let Some(t) = tm.tasks.get_mut(&tid) else {
            continue;
        };

        t.queued = false;

        if t.state == TaskState::Ready {
            return Some(tid);
        }
    }

    None
}

pub fn wake_tty_readers() {
    let mut tm = TaskManager::get().lock();
    let mut rq = RUN_QUEUE.lock();

    for task in tm.tasks.values_mut() {
        if task.state != TaskState::Waiting || !matches!(task.wait, Some(WaitReason::IO)) {
            continue;
        }

        task.wait = None;
        task.state = TaskState::Ready;

        if task.queued {
            continue;
        }
        task.queued = true;
        rq.push_back(task.tid);
    }
}

pub fn wake_sleepers(tick: u64) {
    let mut tm = TaskManager::get().lock();
    let mut rq = RUN_QUEUE.lock();

    for task in tm.tasks.values_mut() {
        if task.state != TaskState::Waiting {
            continue;
        }

        let Some(WaitReason::SleepUntil(t)) = task.wait else {
            continue;
        };

        if t > tick {
            continue;
        }

        task.wait = None;
        task.state = TaskState::Ready;

        if task.queued {
            continue;
        }
        task.queued = true;
        rq.push_back(task.tid);
    }
}

pub fn on_timer_tick(_tf: &mut TrapFrame, tick: u64) {
    wake_sleepers(tick);

    if let Some((_tid, pid)) = helpers::current_task_ids() {
        let mut pm = ProcessManager::procman().lock();
        let proc = pm.get_mut(pid).unwrap();
        proc.ticks_run += 1;
    }

    if TaskManager::current_tid() == 0 {
        return;
    }

    let left = TICKS_LEFT.fetch_sub(1, Ordering::AcqRel);
    if left > 1 {
        return;
    }

    TICKS_LEFT.store(QUANTUM_TICKS, Ordering::Release);
    NEED_RESCHED.store(true, Ordering::Release);
}

pub fn schedule_irq(tf: &mut TrapFrame) {
    let cur = TaskManager::current_tid();
    if cur == 0 {
        return;
    }

    if !NEED_RESCHED.load(Ordering::Acquire) {
        return;
    }

    if !in_user(tf) {
        return;
    }

    if !NEED_RESCHED.swap(false, Ordering::AcqRel) {
        return;
    }

    {
        let mut tm = TaskManager::get().lock();
        let Some(t) = tm.tasks.get_mut(&cur) else {
            return;
        };

        if t.state == TaskState::Running {
            t.state = TaskState::Ready;
            t.queued = false;
            enqueue_ready_locked(&mut tm, cur);
        }
    }

    switch_to_scheduler(cur);
}

fn switch_to_task(next_tid: Tid) {
    TICKS_LEFT.store(QUANTUM_TICKS, Ordering::Release);
    let (pid, kstack_top, kctx_ptr, user_gs_base, user_fs_base) = {
        let mut tm = TaskManager::get().lock();
        let Some(t) = tm.tasks.get_mut(&next_tid) else {
            return;
        };

        t.queued = false;
        t.state = TaskState::Running;

        TaskManager::set_current_tid(next_tid);

        (
            t.pid,
            t.kstack_top,
            &raw const t.kctx,
            t.user_gs_base,
            t.user_fs_base,
        )
    };

    gdt::set_rsp0(kstack_top);
    percpu::set_kernel_rsp(kstack_top);
    KernelGsBase::write(VirtAddr::new(user_gs_base));
    FsBase::write(VirtAddr::new(user_fs_base));

    {
        let mut pm = ProcessManager::procman().lock();
        let Some(p) = pm.procs.get_mut(&pid) else {
            return;
        };
        p.address_space.activate();
    }

    unsafe {
        context_switch(&raw mut SCHED_CTX, kctx_ptr as *const KernelContext);
    }
}

fn switch_to_scheduler(cur: Tid) {
    let kctx_ptr = {
        let mut tm = TaskManager::get().lock();
        let Some(t) = tm.tasks.get_mut(&cur) else {
            return;
        };
        &raw mut t.kctx
    };

    KernelGsBase::write(VirtAddr::new(0));
    unsafe {
        context_switch(kctx_ptr, &raw const SCHED_CTX);
    }
}

pub fn start() -> ! {
    TaskManager::set_current_tid(0);

    loop {
        TaskManager::set_current_tid(0);

        interrupts::disable();

        if let Some(next) = dequeue_next_ready() {
            switch_to_task(next);
            continue;
        }

        interrupts::enable_and_hlt();
    }
}

pub fn block_current() {
    let cur = TaskManager::current_tid();
    if cur == 0 {
        return;
    }

    interrupts::without_interrupts(|| {
        switch_to_scheduler(cur);
    });
}

pub fn exit_current() -> ! {
    let cur = TaskManager::current_tid();
    if cur == 0 {
        panic!("exit_current called from scheduler");
    }

    interrupts::without_interrupts(|| {
        switch_to_scheduler(cur);
    });

    panic!("exited task {} resumed unexpectedly", cur);
}
