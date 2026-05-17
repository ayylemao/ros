use alloc::vec::Vec;
use x86_64::{
    structures::paging::{FrameAllocator, OffsetPageTable, Size4KiB},
    VirtAddr,
};

use crate::arch::{
    interrupt::{
        ioapic::{find_ioapic_for_gsi, IoApicMmio},
        lapic::LocalApic,
    },
    ps2::enable_keyboard_irq,
    sys_desc::{map_mmio, ApicInfo, SystemDescriptors},
};

pub mod idt;
mod ioapic;
pub mod lapic;
pub mod syscall_entry;
mod timer_trap;
mod types;

/// Bring up IDT + LAPIC + IOAPIC routes (keyboard, etc.).
pub fn init_interrupts(
    apic: &ApicInfo,
    mapper: &mut OffsetPageTable,
    allocator: &mut impl FrameAllocator<Size4KiB>,
) {
    idt::init_idt(mapper, allocator);

    enable_keyboard_irq();

    route_keyboard_irq(apic, mapper, allocator);
}

fn route_keyboard_irq(
    apic: &ApicInfo,
    mapper: &mut OffsetPageTable,
    allocator: &mut impl FrameAllocator<Size4KiB>,
) {
    // Build MMIO IOAPIC list
    let mut mmios: Vec<IoApicMmio> = Vec::new();
    for e in &apic.ioapics {
        // sys_desc.rs MADT IOAPIC entry fields:
        // e.addr = phys MMIO base
        // e.global_sys_int_base = GSI base
        let base_virt = map_mmio(e.addr as u64, 0x1000, mapper, allocator);
        mmios.push(unsafe { IoApicMmio::new(VirtAddr::new(base_virt), e.global_sys_int_base) });
    }

    // ISA IRQ1 -> (GSI, flags)
    let (gsi, flags) = SystemDescriptors::resolve_isa_irq(apic, 1);

    let ioa = unsafe { find_ioapic_for_gsi(&mmios, gsi) }.expect("No IOAPIC covers keyboard GSI");
    // Deliver to BSP (current) LAPIC id
    let dest = LocalApic::id();

    unsafe {
        ioa.set_gsi_redirect(gsi, idt::KEYBOARD_VEC, dest, flags);
    }
}
