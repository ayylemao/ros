use core::mem::size_of;
use core::ptr;
use shared::{
    BOOTINFO_VIRT_BASE, BootInfo, FRAMEBUFFER_VIRT_BASE, KERNEL_STACK_STRIDE,
    KERNEL_STACK_VIRT_BASE, MemoryRegion, PAGE_SIZE_2M, PHYS_OFFSET, PhysicalAllocator, align_down,
    align_down_2mb, align_up, align_up_2mb, early_usable,
};

use crate::kernel_load::KernelLoadInfo;

const PRESENT: u64 = 1 << 0;
const WRITE: u64 = 1 << 1;
const PTE_PWT: u64 = 1 << 3;
const PTE_PCD: u64 = 1 << 4;
#[allow(dead_code)]
const NX: u64 = 1 << 63;

const HUGE_PAGE: u64 = 1 << 7;
const PDE_PAT: u64 = 1 << 12;
const PTE_PAT: u64 = 1 << 7;

const MMIO_FLAGS: u64 = PRESENT | WRITE | PTE_PWT | PTE_PCD;
const DEFAULT_FLAG: u64 = PRESENT | WRITE;

const PAGE_SIZE: u64 = 4096;
const PAGE_TABLE_ADDR_MASK: u64 = 0x000F_FFFF_FFFF_F000;
const HUGE_PAGE_ADDR_MASK: u64 = 0x000F_FFFF_FFE0_0000;
const FLAGS_MASK: u64 = 0xFFF0_0000_0000_0FFF;

#[repr(C, align(4096))]
pub struct PageTable {
    pub entries: [u64; 512],
}

unsafe fn clear(pt: *mut PageTable) {
    unsafe { ptr::write_bytes(pt, 0, 1) };
}

unsafe fn mk_table(alloc: &mut PhysicalAllocator) -> (*mut PageTable, u64) {
    let phys = alloc.alloc_frame();
    let virt = phys as *mut PageTable;
    unsafe { clear(virt) };
    (virt, phys)
}

fn map_physical_range(
    alloc: &mut PhysicalAllocator,
    pml4: *mut PageTable,
    phys_addr: u64,
    size: u64,
    virt_base: u64,
    flags: u64,
) -> u64 {
    let phys_start = align_down(phys_addr);
    let phys_end = align_up(phys_addr + size);
    let offset = phys_addr - phys_start;

    let mut pa = phys_start;
    let mut va = virt_base;

    while pa < phys_end {
        unsafe {
            map_page(alloc, pml4, va, pa, flags);
        }
        pa += PAGE_SIZE;
        va += PAGE_SIZE;
    }

    virt_base + offset
}

fn map_mmio_region(
    alloc: &mut PhysicalAllocator,
    pml4: *mut PageTable,
    phys_addr: u64,
    size: usize,
) -> u64 {
    map_physical_range(
        alloc,
        pml4,
        phys_addr,
        size as u64,
        FRAMEBUFFER_VIRT_BASE,
        MMIO_FLAGS,
    )
}

fn map_boot_info_region(
    alloc: &mut PhysicalAllocator,
    pml4: *mut PageTable,
    phys_addr: u64,
) -> u64 {
    map_physical_range(
        alloc,
        pml4,
        phys_addr,
        size_of::<BootInfo>() as u64,
        BOOTINFO_VIRT_BASE,
        DEFAULT_FLAG,
    )
}

fn map_memory_map(
    alloc: &mut PhysicalAllocator,
    pml4: *mut PageTable,
    slice: &[MemoryRegion],
    virt_base: u64,
) -> u64 {
    let ptr = slice.as_ptr() as u64;
    let len = slice.len() as u64;
    let size = len * size_of::<MemoryRegion>() as u64;

    map_physical_range(alloc, pml4, ptr, size, virt_base, DEFAULT_FLAG)
}

pub struct BootPageTablesInfo {
    pub pml4_phys: u64,
    pub bootinfo_virt: u64,
    pub kernel_stack_top: u64,
}

pub unsafe fn build_boot_page_tables(
    alloc: &mut PhysicalAllocator,
    kernel: &KernelLoadInfo,
    identity_limit: u64,
    bootinfo: &mut BootInfo, // CHANGED: was &BootInfo
) -> BootPageTablesInfo {
    let (pml4, pml4_phys) = unsafe { mk_table(alloc) };

    identity_map_upto_2mib(alloc, pml4, identity_limit);

    for r in bootinfo.memory_regions {
        if !early_usable(r.kind) {
            continue;
        }

        let start = align_down_2mb(r.start);
        let end = align_up_2mb(r.end);

        let mut pa = start;
        while pa < end {
            unsafe {
                map_page_2mib(alloc, pml4, PHYS_OFFSET + pa, pa, PRESENT | WRITE);
            }
            pa += PAGE_SIZE_2M;
        }
    }

    let bootinfo_phys = bootinfo as *const BootInfo as u64;
    let bootinfo_virt = map_boot_info_region(alloc, pml4, bootinfo_phys);

    let mem_regions = bootinfo.memory_regions;
    let memmap_virt_base = BOOTINFO_VIRT_BASE + 0x10000;

    let memmap_virt_ptr =
        map_memory_map(alloc, pml4, mem_regions, memmap_virt_base) as *const MemoryRegion;

    unsafe {
        bootinfo.memory_regions = core::slice::from_raw_parts(memmap_virt_ptr, mem_regions.len());
    }

    if let Some(ref mut fb) = bootinfo.framebuffer {
        let fb_size_bytes = (fb.height * fb.stride * 4) as u64;

        let fb_virt = map_mmio_region(alloc, pml4, fb.addr, fb_size_bytes as usize);
        fb.addr = fb_virt;
    }

    for seg in kernel.segments.iter() {
        let seg_va = seg.virt_addr;
        let seg_pa = seg.phys_addr;
        let seg_size = seg.mem_size as u64;

        let page_offset = seg_va & (PAGE_SIZE - 1);
        let va_base = align_down(seg_va);
        let pa_base = align_down(seg_pa);

        let span = page_offset + seg_size;
        let page_count = ((span + PAGE_SIZE - 1) / PAGE_SIZE) as usize;

        for i in 0..page_count {
            let va = va_base + (i as u64) * PAGE_SIZE;
            let pa = pa_base + (i as u64) * PAGE_SIZE;
            unsafe { map_page(alloc, pml4, va, pa, DEFAULT_FLAG) };
        }
    }

    // kernel stack
    let page_count = KERNEL_STACK_STRIDE / PAGE_SIZE;

    // map rest of stack
    for i in 1..page_count {
        let pa = alloc.alloc_frame();
        let va = KERNEL_STACK_VIRT_BASE + PAGE_SIZE * i;
        unsafe {
            map_page(alloc, pml4, va, pa, DEFAULT_FLAG);
        }
    }

    let kernel_stack_top = KERNEL_STACK_VIRT_BASE + KERNEL_STACK_STRIDE;

    BootPageTablesInfo {
        pml4_phys,
        bootinfo_virt,
        kernel_stack_top,
    }
}

unsafe fn map_page(
    alloc: &mut PhysicalAllocator,
    pml4: *mut PageTable,
    virt: u64,
    phys: u64,
    flags: u64,
) {
    let idx_pml4 = ((virt >> 39) & 0x1FF) as usize;
    let idx_pdpt = ((virt >> 30) & 0x1FF) as usize;
    let idx_pd = ((virt >> 21) & 0x1FF) as usize;
    let idx_pt = ((virt >> 12) & 0x1FF) as usize;

    unsafe {
        if (*pml4).entries[idx_pml4] == 0 {
            let (_tbl, phys_addr) = mk_table(alloc);
            (*pml4).entries[idx_pml4] = phys_addr | PRESENT | WRITE;
        }
        let pdpt = ((*pml4).entries[idx_pml4] & PAGE_TABLE_ADDR_MASK) as *mut PageTable;

        if (*pdpt).entries[idx_pdpt] == 0 {
            let (_tbl, phys_addr) = mk_table(alloc);
            (*pdpt).entries[idx_pdpt] = phys_addr | PRESENT | WRITE;
        }
        let pd = ((*pdpt).entries[idx_pdpt] & PAGE_TABLE_ADDR_MASK) as *mut PageTable;

        if (*pd).entries[idx_pd] == 0 {
            let (_tbl, phys_addr) = mk_table(alloc);
            (*pd).entries[idx_pd] = phys_addr | PRESENT | WRITE;
        }

        if (*pd).entries[idx_pd] & HUGE_PAGE != 0 {
            let pde_entry = (*pd).entries[idx_pd];
            let huge_phys_base = pde_entry & HUGE_PAGE_ADDR_MASK;
            let mut pte_flags = (pde_entry & FLAGS_MASK) & !HUGE_PAGE;

            if pde_entry & PDE_PAT != 0 {
                pte_flags |= PTE_PAT;
            }

            let (pt, pt_phys_addr) = mk_table(alloc);

            for i in 0..512 {
                (*pt).entries[i] = huge_phys_base + (i as u64) * PAGE_SIZE | pte_flags;
            }

            let new_pde_flags = (pde_entry & FLAGS_MASK) & !HUGE_PAGE;
            (*pd).entries[idx_pd] = pt_phys_addr | new_pde_flags;
        }

        let pt = ((*pd).entries[idx_pd] & PAGE_TABLE_ADDR_MASK) as *mut PageTable;

        (*pt).entries[idx_pt] = phys | flags;
    }
}

unsafe fn map_page_2mib(
    alloc: &mut PhysicalAllocator,
    pml4: *mut PageTable,
    virt: u64,
    phys: u64,
    flags: u64,
) {
    debug_assert!(virt % PAGE_SIZE_2M == 0);
    debug_assert!(phys % PAGE_SIZE_2M == 0);

    let idx_pml4 = ((virt >> 39) & 0x1FF) as usize;
    let idx_pdpt = ((virt >> 30) & 0x1FF) as usize;
    let idx_pd = ((virt >> 21) & 0x1FF) as usize;

    // PML4
    unsafe {
        if (*pml4).entries[idx_pml4] == 0 {
            let (_tbl, phys_addr) = mk_table(alloc);
            (*pml4).entries[idx_pml4] = phys_addr | PRESENT | WRITE;
        }
        let pdpt = ((*pml4).entries[idx_pml4] & PAGE_TABLE_ADDR_MASK) as *mut PageTable;

        // PDPT
        if (*pdpt).entries[idx_pdpt] == 0 {
            let (_tbl, phys_addr) = mk_table(alloc);
            (*pdpt).entries[idx_pdpt] = phys_addr | PRESENT | WRITE;
        }
        let pd = ((*pdpt).entries[idx_pdpt] & PAGE_TABLE_ADDR_MASK) as *mut PageTable;

        // PD entry maps 2 MiB directly
        (*pd).entries[idx_pd] = phys | flags | HUGE_PAGE;
    };
}

fn identity_map_upto_2mib(alloc: &mut PhysicalAllocator, pml4: *mut PageTable, limit: u64) {
    let end = align_up_2mb(limit);
    let mut pa = 0u64;
    while pa < end {
        unsafe {
            map_page_2mib(alloc, pml4, pa, pa, PRESENT | WRITE);
        } // no NX here
        pa += PAGE_SIZE_2M;
    }
}
