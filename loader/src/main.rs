#![no_std]
#![no_main]

extern crate alloc;

mod kernel_load;
mod paging;

use shared::{
    BootInfo, FramebufferInfo, MemoryRegion, MemoryRegionKind, PhysicalAllocator, early_usable,
};
use uefi::{
    Identify,
    boot::{MemoryType, exit_boot_services},
    mem::memory_map::{MemoryMap, MemoryMapOwned},
    prelude::*,
    println,
    proto::console::gop::GraphicsOutput,
    system::with_config_table,
    table::cfg::ConfigTableEntry,
};

use crate::paging::{BootPageTablesInfo, build_boot_page_tables};

#[entry]
fn main() -> Status {
    uefi::helpers::init().unwrap();

    println!("Loader started!");

    println!("Firmware vendor: {}", uefi::system::firmware_vendor());
    println!("UEFI revision:   {:?}", uefi::system::uefi_revision());

    let rspd_addr = get_acpi2_rsdp_addr();
    println!("ACPI2 RSDP Address: 0x{:x}", rspd_addr);
    let mmap = boot::memory_map(MemoryType::LOADER_DATA).expect("Failed to retrieve memory map");

    println!("Memory map has {} entries.", mmap.len());

    println!("\nBuilding BootInfo...");
    let bootinfo: &mut BootInfo = build_bootinfo(&mmap).expect("BootInfo failed");

    println!("{} memory regions.", bootinfo.memory_regions.len());

    let mut kernel_load_info = kernel_load::load_kernel().unwrap();

    let identity_limit = calc_identity_limit(bootinfo.memory_regions);
    bootinfo.identity_limit = identity_limit;

    println!("Identity Limit: 0x{:x}", identity_limit);

    let mut allocator = PhysicalAllocator::new(bootinfo.memory_regions);

    for segment in kernel_load_info.segments.iter_mut() {
        let n_pages = (segment.mem_size + 0xFFF) / 4096;

        let phys_addr = allocator.alloc_frames(n_pages as usize);

        for (i, byte) in segment.data.iter().enumerate() {
            unsafe { core::ptr::write((phys_addr as *mut u8).add(i), *byte) };
        }
        for i in segment.file_size as usize..segment.mem_size as usize {
            unsafe { core::ptr::write((phys_addr as *mut u8).add(i), 0) };
        }
        segment.phys_addr = phys_addr;
    }

    for segment in kernel_load_info.segments.iter() {
        println!(
            "Phys: 0x{:x}, virt: 0x{:x}, fs: 0x{:x}, mems: 0x{:x}",
            segment.phys_addr, segment.virt_addr, segment.file_size, segment.mem_size,
        );
    }

    let entry_addr = kernel_load_info.entry_point as u64;
    println!("Kernel Entry Point: 0x{:x}", entry_addr);

    println!("Exiting boot services!");
    let _final_memmap = unsafe { exit_boot_services(None) };

    let BootPageTablesInfo {
        pml4_phys,
        bootinfo_virt,
        kernel_stack_top,
    } = unsafe {
        build_boot_page_tables(&mut allocator, &kernel_load_info, identity_limit, bootinfo)
    };

    bootinfo.phys_alloc_current = allocator.current;
    bootinfo.phys_alloc_next = allocator.next;
    bootinfo.kernel_stack_top = kernel_stack_top;
    bootinfo.rsdp = rspd_addr;

    unsafe {
        core::arch::asm!(
            "cli",
            "cld",
            "mov cr3, {pml4}",
            "mov rsp, {stack}",

            // emulate call ABI (RSP%16 == 8 at entry)
            "sub rsp, 8",
            "mov qword ptr [rsp], 0",
            "xor rbp, rbp",

            "mov rdi, {bootinfo}",
            "jmp {entry}",

            pml4     = in(reg) pml4_phys,
            stack    = in(reg) kernel_stack_top,
            bootinfo = in(reg) bootinfo_virt,
            entry    = in(reg) entry_addr,
            options(noreturn)
        );
    }
}

fn calc_identity_limit(memory_regions: &[MemoryRegion]) -> u64 {
    let mut max = 0;

    for r in memory_regions {
        if early_usable(r.kind) {
            if r.end > max {
                max = r.end;
            }
        }
    }

    // add safety pad
    max + 0x200000
}

fn convert_type(ty: MemoryType) -> MemoryRegionKind {
    match ty {
        MemoryType::CONVENTIONAL => MemoryRegionKind::Usable,
        MemoryType::BOOT_SERVICES_CODE
        | MemoryType::BOOT_SERVICES_DATA
        | MemoryType::PERSISTENT_MEMORY => MemoryRegionKind::EarlyReclaimable,
        MemoryType::ACPI_RECLAIM => MemoryRegionKind::AcpiReclaimable,
        MemoryType::LOADER_CODE | MemoryType::LOADER_DATA => MemoryRegionKind::LateReclaimable,
        _ => MemoryRegionKind::Reserved,
    }
}

fn convert_regions(mmap: &MemoryMapOwned) -> Option<&'static [MemoryRegion]> {
    let count = mmap.len();

    let region_size = core::mem::size_of::<MemoryRegion>() * count;

    let raw = boot::allocate_pool(MemoryType::LOADER_DATA, region_size).ok()?;

    let ptr = raw.cast::<MemoryRegion>().as_ptr();

    let slice: &mut [MemoryRegion] = unsafe { core::slice::from_raw_parts_mut(ptr, count) };

    for (i, desc) in mmap.entries().enumerate() {
        slice[i] = MemoryRegion {
            start: desc.phys_start,
            end: desc.phys_start + (desc.page_count * 4096) as u64,
            kind: convert_type(desc.ty),
        };
    }

    Some(slice)
}

fn build_bootinfo(mmap: &MemoryMapOwned) -> Option<&'static mut shared::BootInfo> {
    let mem_regions = convert_regions(mmap)?;

    let bi_ptr =
        boot::allocate_pool(MemoryType::LOADER_DATA, core::mem::size_of::<BootInfo>()).ok()?;

    let frame_buffer = load_framebuffer_info();
    let bi: &mut BootInfo = unsafe { &mut *bi_ptr.cast().as_ptr() };

    bi.memory_regions = mem_regions;
    bi.framebuffer = Some(frame_buffer);

    Some(bi)
}

fn load_framebuffer_info() -> FramebufferInfo {
    let handles =
        boot::locate_handle_buffer(boot::SearchType::ByProtocol(&GraphicsOutput::GUID)).unwrap();

    let handle = handles[0];

    let params = boot::OpenProtocolParams {
        handle: handle,
        agent: boot::image_handle(),
        controller: None,
    };

    let mut gop = unsafe {
        boot::open_protocol::<GraphicsOutput>(params, boot::OpenProtocolAttributes::GetProtocol)
    }
    .expect("Failed to get GOP");

    let mode = gop.current_mode_info();

    let fb_ptr = gop.frame_buffer().as_mut_ptr();
    let fb_size = gop.frame_buffer().size();
    let (width, height) = mode.resolution();
    let stride = mode.stride();

    println!("Resolution: {} x {}", width, height);
    println!("FB address: {:p}", fb_ptr);
    println!("FB size:    {}", fb_size);
    println!("Stride:     {}", stride);
    println!("Format:     {:?}", mode.pixel_format());

    FramebufferInfo {
        addr: fb_ptr as u64,
        width,
        height,
        stride,
    }
}

fn get_acpi2_rsdp_addr() -> u64 {
    let mut rsdp: u64 = 0;
    with_config_table(|slice| {
        for e in slice {
            if e.guid == ConfigTableEntry::ACPI2_GUID {
                rsdp = e.address as u64;
                break;
            }
        }
    });
    rsdp
}
