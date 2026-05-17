use core::sync::atomic::{AtomicU64, Ordering};

use alloc::collections::btree_map::BTreeMap;
use shared::{
    PAGE_SIZE, PROC_KERNEL_STACK_PAGES, PROC_KERNEL_STACK_REGION_TOP, PROC_KERNEL_STACK_STRIDE,
};
use spin::Once;
use sys::syscall::errors::MapElfError;
use x86_64::{
    structures::paging::{FrameAllocator, Mapper, OffsetPageTable, Page, PageTableFlags, Size4KiB},
    VirtAddr,
};

use crate::{
    proc::{context::KernelContext, process::UserContext},
    syscall::syscall_dispatch::TrapFrame,
    utils::irq_lock::IrqMutex,
};

pub static TASK_MANAGER: Once<IrqMutex<TaskManager>> = Once::new();
static CURRENT_TID: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    Ready,
    Running,
    Waiting,
    Exited(i64),
    Faulted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitReason {
    AnyChild,
    ChildPid(u64),
    SleepUntil(u64),
    IO,
}

#[derive(Debug)]
pub struct Task {
    pub tid: u64,
    pub pid: u64,
    pub state: TaskState,
    pub ctx: UserContext,
    pub kctx: KernelContext,
    pub queued: bool,
    pub wait: Option<WaitReason>,
    pub wake_tick: u64,
    pub kstack_top: u64,
    pub user_gs_base: u64,
    pub user_fs_base: u64,
    pub clear_child_tid: u64,
}

impl Task {
    pub fn init_user_entry_kctx(&mut self) {
        let mut sp = self.kstack_top;

        sp = sp
            .checked_sub(core::mem::size_of::<TrapFrame>() as u64)
            .expect("kstack overflow building initial trap frame");

        let tf_ptr = sp as *mut TrapFrame;
        unsafe {
            tf_ptr.write(TrapFrame {
                regs: self.ctx.regs,
                vector: 0,
                error_code: 0,
                iret: self.ctx.iret,
            });
        }

        sp = sp
            .checked_sub(8)
            .expect("kstack overflow pushing trampoline");
        unsafe {
            (sp as *mut u64).write(crate::proc::context::iret_trampoline as *const () as u64);
        }

        self.kctx = KernelContext {
            rsp: sp,
            ..KernelContext::default()
        };
    }
}

#[derive(Debug)]
pub struct TaskManager {
    pub tasks: BTreeMap<u64, Task>,
    pub next_tid: u64,
}

impl TaskManager {
    pub fn new() -> Self {
        TaskManager {
            tasks: BTreeMap::new(),
            next_tid: 1,
        }
    }

    pub fn get() -> &'static IrqMutex<TaskManager> {
        TASK_MANAGER.call_once(|| IrqMutex::new(TaskManager::new()))
    }

    pub fn get_next_tid(&mut self) -> u64 {
        let tid = self.next_tid;
        self.next_tid += 1;
        tid
    }

    pub fn current_tid() -> u64 {
        CURRENT_TID.load(Ordering::Acquire)
    }

    pub fn set_current_tid(tid: u64) {
        CURRENT_TID.store(tid, Ordering::Release)
    }

    pub fn alloc_and_map_kstack_for_task(
        tid: u64,
        mapper: &mut OffsetPageTable,
        alloc: &mut impl FrameAllocator<Size4KiB>,
    ) -> Result<u64, MapElfError> {
        let top = Self::kstack_top_for_tid(tid);
        let bottom = top - PROC_KERNEL_STACK_PAGES * PAGE_SIZE;

        for va in (bottom..top).step_by(PAGE_SIZE as usize) {
            let frame = alloc.allocate_frame().ok_or(MapElfError::OutOfFrames)?;
            unsafe {
                mapper
                    .map_to(
                        Page::containing_address(VirtAddr::new(va)),
                        frame,
                        PageTableFlags::PRESENT
                            | PageTableFlags::WRITABLE
                            | PageTableFlags::NO_EXECUTE,
                        alloc,
                    )
                    .map_err(|_| MapElfError::MapFailed)?
                    .flush();
            }
        }

        Ok(top)
    }

    #[inline]
    pub fn kstack_top_for_tid(tid: u64) -> u64 {
        PROC_KERNEL_STACK_REGION_TOP - tid * PROC_KERNEL_STACK_STRIDE
    }
}
