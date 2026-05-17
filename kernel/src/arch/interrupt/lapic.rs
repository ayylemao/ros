use core::{
    ptr::{read_volatile, write_volatile},
    sync::atomic::{AtomicU32, AtomicU64, Ordering},
};

use shared::{HZ, PHYS_OFFSET};
use x86_64::{
    instructions::port::Port,
    registers::model_specific::Msr,
    structures::paging::{
        FrameAllocator, Mapper, OffsetPageTable, Page, PageTableFlags, PhysFrame, Size4KiB,
    },
    PhysAddr, VirtAddr,
};

pub static LAPIC_BASE: AtomicU64 = AtomicU64::new(0);
pub static LAPIC_RELOAD: AtomicU32 = AtomicU32::new(0);

#[allow(dead_code)]
pub struct LocalApic {
    virt_base: VirtAddr,
}

impl LocalApic {
    pub fn init(mapper: &mut OffsetPageTable, allocator: &mut impl FrameAllocator<Size4KiB>) {
        Self::disable_pic();
        Self::enable_apic_msr();
        let lapic_phys = Self::enable_apic_msr();
        LAPIC_BASE.store(lapic_phys, Ordering::Release);
        let base = Self::map(mapper, allocator, lapic_phys);
        Self::enable_spurious_interrupt(base);
        Self::setup_timer(base);
    }

    fn map(
        mapper: &mut OffsetPageTable,
        allocator: &mut impl FrameAllocator<Size4KiB>,
        lapic_phys: u64,
    ) -> VirtAddr {
        let virt = VirtAddr::new(lapic_phys + PHYS_OFFSET);

        unsafe {
            mapper
                .map_to(
                    Page::<Size4KiB>::containing_address(VirtAddr::new(lapic_phys + PHYS_OFFSET)),
                    PhysFrame::<Size4KiB>::containing_address(PhysAddr::new(lapic_phys)),
                    PageTableFlags::PRESENT
                        | PageTableFlags::WRITABLE
                        | PageTableFlags::NO_CACHE
                        | PageTableFlags::WRITE_THROUGH
                        | PageTableFlags::NO_EXECUTE,
                    allocator,
                )
                .expect("Could not map LocalApic")
                .flush();
        };
        virt
    }

    fn disable_pic() {
        unsafe {
            Port::<u8>::new(0x21).write(0xFF);
            Port::<u8>::new(0xA1).write(0xFF);
        }
    }

    fn enable_apic_msr() -> u64 {
        const IA32_APIC_BASE: u32 = 0x1B;

        let mut apic_base = unsafe { Msr::new(IA32_APIC_BASE).read() };
        apic_base |= 1 << 11;
        unsafe { Msr::new(IA32_APIC_BASE).write(apic_base) };
        apic_base & 0xFFFF_FFFF_F000
    }

    fn enable_spurious_interrupt(base: VirtAddr) {
        const LAPIC_SVR: u64 = 0xF0;

        unsafe {
            let svr = (base.as_u64() + LAPIC_SVR) as *mut u32;
            svr.write_volatile(svr.read_volatile() | 0x100 | 0xFF);
        }
    }

    unsafe fn write(base: VirtAddr, offset: u64, val: u32) {
        let reg = (base.as_u64() + offset) as *mut u32;
        reg.write_volatile(val);
    }

    fn setup_timer(base: VirtAddr) {
        let tpm = Self::calibrate_timer_ticks_per_ms(base);
        let ticks_per_tick = (tpm * 1000) / HZ;
        let ticks_per_tick = ticks_per_tick.max(1);

        unsafe {
            // Divide by 16
            Self::write(base, 0x3E0, 0b11);

            // Periodic timer, vector 0x40
            Self::write(base, 0x320, 0x40 | (1 << 17));

            // Initial count
            Self::write(base, 0x380, ticks_per_tick);
        }
        LAPIC_RELOAD.store(ticks_per_tick, Ordering::Relaxed);
    }

    #[inline(always)]
    unsafe fn mmio_read(offset: u32) -> u32 {
        let lapic_phys = LAPIC_BASE.load(Ordering::Acquire);
        let base = VirtAddr::new(lapic_phys + PHYS_OFFSET);
        let p = (base.as_u64() + offset as u64) as *const u32;
        read_volatile(p)
    }

    #[inline(always)]
    unsafe fn mmio_write(offset: u32, val: u32) {
        let lapic_phys = LAPIC_BASE.load(Ordering::Acquire);
        let base = VirtAddr::new(lapic_phys + PHYS_OFFSET);
        let p = (base.as_u64() + offset as u64) as *mut u32;
        write_volatile(p, val);
    }

    /// LAPIC ID register (0x20), bits 24..31
    pub fn id() -> u8 {
        let id_reg = unsafe { Self::mmio_read(0x20) };
        ((id_reg >> 24) & 0xFF) as u8
    }

    pub fn eoi() {
        unsafe { Self::mmio_write(0xB0, 0) };
    }

    #[allow(dead_code)]
    fn enable_imcr_apic_mode() {
        unsafe {
            // IMCR select port 0x22, data port 0x23
            // Select register 0x70, then set bit 0 (route INTR to APIC)
            let mut sel = Port::<u8>::new(0x22);
            let mut dat = Port::<u8>::new(0x23);

            sel.write(0x70);
            let v = dat.read();
            dat.write(v | 0x01);
        }
    }

    #[inline(always)]
    unsafe fn read_current_count() -> u32 {
        Self::mmio_read(0x390)
    }

    fn calibrate_timer_ticks_per_ms(base: VirtAddr) -> u32 {
        unsafe {
            Self::write(base, 0x3E0, 0b11);
        }

        unsafe {
            Self::write(base, 0x320, 0x40 | (1 << 16));
        }

        unsafe {
            Self::write(base, 0x380, 0xFFFF_FFFF);
        }

        crate::arch::timer::pit::wait_ms(50);

        let cur = unsafe { Self::read_current_count() };
        let elapsed = 0xFFFF_FFFFu32.wrapping_sub(cur);

        elapsed / 50
    }
}
