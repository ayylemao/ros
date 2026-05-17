use core::sync::atomic::{AtomicU64, Ordering};

use x86_64::registers::model_specific::Msr;

pub const IA32_GS_BASE: u32 = 0xC000_0101;
pub const IA32_FS_BASE: u32 = 0xC000_0100;
pub const IA32_KERNEL_GS_BASE: u32 = 0xC000_0102;

static PERCPU0: PerCpu = PerCpu {
    kernel_rsp: AtomicU64::new(0),
    user_rsp: AtomicU64::new(0),
};

#[repr(C)]
pub struct PerCpu {
    pub kernel_rsp: AtomicU64,
    pub user_rsp: AtomicU64,
}

pub fn set_kernel_rsp(rsp: u64) {
    PERCPU0.kernel_rsp.store(rsp, Ordering::Release);
}

pub fn init_this_cpu() {
    unsafe {
        let mut gs = Msr::new(IA32_GS_BASE);
        gs.write(&PERCPU0 as *const _ as u64);

        let mut kgs = Msr::new(IA32_KERNEL_GS_BASE);
        kgs.write(0);
    }
}
