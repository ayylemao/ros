#![allow(dead_code)]
#![no_std]
#![feature(abi_x86_interrupt)]

#[macro_use]
extern crate alloc;

use core::sync::atomic::Ordering;

use shared::{
    BootInfo, KERNEL_HEAP_BASE, KERNEL_HEAP_SIZE, KERNEL_STACK_SIZE, KERNEL_STACK_VIRT_BASE, KIBI,
    MIBI,
};

mod arch;
mod drivers;
mod fs;
mod kernel_cmds;
mod kglobal;
mod logging;
mod memory;
mod proc;
mod serial;
mod syscall;
mod utils;

pub mod console;

use memory::phys_frame_bump_alloc::PhysicalFrameBumpAllocator;

use x86_64::{
    instructions::interrupts,
    registers::{
        control::{Efer, EferFlags},
        rflags,
    },
    structures::paging::{FrameAllocator, OffsetPageTable, Size4KiB},
    VirtAddr,
};

use crate::{
    arch::{
        gdt::{self},
        pci::PCI,
        percpu, sys_desc,
    },
    console::{
        console::Console,
        gop_framebuffer::{FbInfo, GopFramebuffer},
    },
    fs::vfs::{Vfs, VfsNode},
    proc::{
        proc_manager::ProcessManager,
        sched::{QUANTUM_TICKS, TICKS_LEFT},
    },
    serial::serial_init,
};

pub fn kmain(bootinfo: &BootInfo) -> ! {
    enable_nx();
    let FbInfo { width, height } = GopFramebuffer::init_fb(bootinfo);
    let mut mapper = memory::paging::kernel_mapper();
    let mut init_allocator = PhysicalFrameBumpAllocator::restore_from(
        bootinfo.memory_regions,
        bootinfo.phys_alloc_current,
        bootinfo.phys_alloc_next,
    );
    let mut bmp_alloc = memory::bmp_alloc::BitmapFrameAllocator::new(&mut init_allocator);
    drop(init_allocator);

    init(&mut mapper, &mut bmp_alloc, &bootinfo, height, width);
    unsafe {
        serial_init();
    }

    // Init Vfs/Ramfs
    _ = Vfs::get();

    kglobal::init_bmp_alloc(bmp_alloc);

    kprintln!("Kernel Stack Base: 0x{:x}", KERNEL_STACK_VIRT_BASE);
    kprintln!("Kernel Stack Size: {}KiB", KERNEL_STACK_SIZE / KIBI);
    kprintln!();
    kprintln!("Kernel Heap initialized:");
    kprintln!("Kernel Heap Base: 0x{:x}", KERNEL_HEAP_BASE);
    kprintln!("Kernel Heap Size: {}MiB", KERNEL_HEAP_SIZE / MIBI);

    kprintln!("Enabling Interrupts...");
    interrupts::enable();

    kprintln!("Fetching pci...");
    let pci_devices = PCI::pci_init();
    kprintln!("{}", pci_devices);

    kprintln!();

    {
        let mut proc_man = ProcessManager::procman().lock();
        proc_man
            .spawn("/usr/sbin/init", None, &[], VfsNode { mount: 0, node: 0 })
            .unwrap();
    }

    TICKS_LEFT.store(QUANTUM_TICKS, Ordering::Release);
    proc::sched::start();
}

pub fn init(
    mapper: &mut OffsetPageTable,
    allocator: &mut impl FrameAllocator<Size4KiB>,
    bootinfo: &BootInfo,
    height: usize,
    width: usize,
) {
    arch::gdt::init(bootinfo.kernel_stack_top);
    percpu::init_this_cpu();
    syscall::init_syscall_msrs();
    memory::heap::init_heap(mapper, allocator).unwrap();
    Console::init_console(width, height);
    memory::unmap_identity_region_2mib(mapper, bootinfo.identity_limit);
    let sys_desc = sys_desc::SystemDescriptors::init(bootinfo.rsdp, mapper, allocator);
    let apic_info = sys_desc.parse_apic_info(mapper, allocator).unwrap();
    arch::interrupt::init_interrupts(&apic_info, mapper, allocator);
}

pub fn enable_nx() {
    unsafe {
        let mut efer = Efer::read();
        efer.insert(EferFlags::NO_EXECUTE_ENABLE);
        Efer::write(efer);
    }
}

pub unsafe fn enter_user_mode(user_rip: VirtAddr, user_rsp: VirtAddr, kstack_top: u64) -> ! {
    let user_cs = crate::arch::gdt::user_code_selector().0 as u64;
    let user_ss = crate::arch::gdt::user_data_selector().0 as u64;
    gdt::set_rsp0(kstack_top);
    percpu::set_kernel_rsp(kstack_top);

    let rflags = rflags::read().bits() | (1 << 9); // IF=1
    kprintln!(
        "Handing off to init proc\ninit rip: {:x}, init rsp {:x}",
        user_rip,
        user_rsp
    );
    core::arch::asm!(
        "cli",
        "push {ss}",
        "push {rsp}",
        "push {rflags}",
        "push {cs}",
        "push {rip}",
        "iretq",
        ss = in(reg) user_ss,
        rsp = in(reg) user_rsp.as_u64(),
        rflags = in(reg) rflags,
        cs = in(reg) user_cs,
        rip = in(reg) user_rip.as_u64(),
        options(noreturn)
    );
}
