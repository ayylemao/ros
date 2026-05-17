use core::sync::atomic::Ordering;

use shared::{fmt_bytes, HZ};
use x86_64::{
    instructions::hlt,
    structures::paging::{OffsetPageTable, Translate},
    VirtAddr,
};

use crate::{arch::interrupt::idt::TICKS, kprintln, memory::bmp_alloc::BitmapFrameAllocator};

#[allow(dead_code)]
pub struct KernelCtx<'a> {
    pub mapper: &'a mut OffsetPageTable<'a>,
    pub frame_alloc: &'a BitmapFrameAllocator,
}

pub fn cmd_mem(ctx: &mut KernelCtx<'_>) {
    let free = ctx.frame_alloc.get_available_memory() as u64;
    let total = ctx.frame_alloc.get_total_memory() as u64;
    let used = total.saturating_sub(free);

    let (t, tu) = fmt_bytes(total);
    let (u, uu) = fmt_bytes(used);
    let (f, fu) = fmt_bytes(free);

    kprintln!("Memory");
    kprintln!("  total: {:>6} {}", t, tu);
    kprintln!("  used : {:>6} {}", u, uu);
    kprintln!("  free : {:>6} {}", f, fu);
}

pub fn ticks() {
    let ticks = TICKS.load(Ordering::Relaxed);
    kprintln!("{}", ticks);
}

pub fn virt2phys(ctx: &mut KernelCtx<'_>, addr: u64) {
    let phys = ctx.mapper.translate(VirtAddr::new(addr));
    kprintln!("0x{:x} -> {:?}", addr, phys);
}

pub fn uptime_ms() -> u64 {
    let ticks: u64 = TICKS.load(Ordering::Relaxed);
    let hz: u64 = HZ as u64;

    if hz == 0 {
        return 0;
    }
    ticks.saturating_mul(1000) / hz
}

pub fn sleep_ms(ms: u64) {
    let hz = HZ as u64;
    let start = TICKS.load(Ordering::Relaxed);

    let delta = (ms * hz + 999) / 1000;
    let target = start + delta;

    while (TICKS.load(Ordering::Relaxed) as u64) < target {
        hlt();
    }
}
