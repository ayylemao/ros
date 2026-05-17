#![no_std]

pub const KIBI: u64 = 0x400;
pub const MIBI: u64 = 0x100000;
pub const GIBI: u64 = 0x400000000;

pub const HZ: u32 = 1000;

pub const PAGE_SIZE: u64 = 0x1000;
pub const PAGE_SIZE_2M: u64 = 2 * 1024 * 1024;
pub const BOOTINFO_VIRT_BASE: u64 = 0xFFFF_FFFF_A000_0000;
pub const FRAMEBUFFER_VIRT_BASE: u64 = 0xFFFF_FFFF_B000_0000;
pub const PHYS_OFFSET: u64 = 0xFFFF_8000_0000_0000;
pub const KERNEL_STACK_SIZE: u64 = 0x100000;
pub const KERNEL_STACK_GUARD: u64 = 0x1000;
pub const KERNEL_STACK_STRIDE: u64 = KERNEL_STACK_SIZE + KERNEL_STACK_GUARD;
pub const KERNEL_STACK_VIRT_BASE: u64 = 0xFFFF_FFFF_9000_0000;
pub const KERNEL_HEAP_BASE: u64 = 0xFFFF_FFFF_C000_0000;
pub const KERNEL_HEAP_SIZE: u64 = 0x100_0000;
pub const MAX_BOOT_PHYS: u64 = 0x1_0000_0000;

pub const USER_SPACE_BOTTOM: u64 = 0x0000_0000_0020_0000;
pub const USER_SPACE_TOP: u64 = 0x0000_0000_8000_0000;
pub const PROCESS_STACK_PAGES: u64 = 8;
pub const USER_SPACE_MAX_HEAP_SIZE: u64 = 128 * MIBI;
pub const USER_MMAP_TOP: u64 = USER_SPACE_TOP - 0x0100_0000;

pub const PROC_KERNEL_STACK_REGION_TOP: u64 = 0xFFFF_FFFF_FF00_0000;
pub const PROC_KERNEL_STACK_PAGES: u64 = 8;
pub const PROC_KERNEL_STACK_STRIDE_PAGES: u64 = PROC_KERNEL_STACK_PAGES + 1;
pub const PROC_KERNEL_STACK_STRIDE: u64 = PROC_KERNEL_STACK_STRIDE_PAGES * PAGE_SIZE;

pub const LAPIC_EOI: u64 = 0xB0;

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub enum MemoryRegionKind {
    Usable,
    EarlyReclaimable,
    LateReclaimable,
    LoaderCode,
    LoaderData,
    Reserved,
    Unknown,
    AcpiReclaimable,
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct MemoryRegion {
    pub start: u64,
    pub end: u64,
    pub kind: MemoryRegionKind,
}

#[derive(Debug)]
#[repr(C)]
pub struct FramebufferInfo {
    pub addr: u64,
    pub width: usize,
    pub height: usize,
    pub stride: usize,
}

#[derive(Debug)]
#[repr(C)]
pub struct BootInfo {
    pub memory_regions: &'static [MemoryRegion],
    pub framebuffer: Option<FramebufferInfo>,
    pub kernel_stack_top: u64,
    pub phys_alloc_current: usize,
    pub phys_alloc_next: u64,
    pub rsdp: u64,
    pub identity_limit: u64,
}

pub struct PhysicalAllocator {
    pub regions: &'static [MemoryRegion],
    pub current: usize,
    pub next: u64,
}

impl PhysicalAllocator {
    pub fn new(regions: &'static [MemoryRegion]) -> Self {
        let mut first = 0;
        for (i, r) in regions.iter().enumerate() {
            if early_usable(r.kind) && r.start >= 0x0010_0000 {
                first = i;
                break;
            }
        }

        Self {
            regions,
            current: first,
            next: core::cmp::max(regions[first].start, 0x0010_0000),
        }
    }

    pub fn restore_from(regions: &'static [MemoryRegion], current: usize, next: u64) -> Self {
        let mut a = Self {
            regions,
            current,
            next,
        };
        a.next = align_up(next);
        a
    }

    pub fn alloc_frame(&mut self) -> u64 {
        loop {
            let r = &self.regions[self.current];

            if early_usable(r.kind) {
                self.next = align_up(self.next);
                let end = core::cmp::min(r.end, MAX_BOOT_PHYS);
                if self.next + PAGE_SIZE <= end {
                    let p = self.next;
                    self.next += PAGE_SIZE;
                    return p;
                }
            }

            self.current += 1;
            if self.current >= self.regions.len() {
                panic!("Out of physical memory (boot alloc)!");
            }

            let r = &self.regions[self.current];
            if early_usable(r.kind) {
                self.next = core::cmp::max(r.start, 0x0010_0000);
            }
        }
    }

    pub fn alloc_frames(&mut self, num_pages: usize) -> u64 {
        let size = (num_pages as u64) * PAGE_SIZE;

        loop {
            let region = &self.regions[self.current];

            if early_usable(region.kind) {
                self.next = align_up(self.next);
                let end = core::cmp::min(region.end, MAX_BOOT_PHYS);
                if self.next + size <= end {
                    let p = self.next;
                    self.next += size;
                    return p;
                }
            }

            self.current += 1;
            if self.current >= self.regions.len() {
                panic!("Out of physical memory!");
            }

            if early_usable(self.regions[self.current].kind) {
                self.next = core::cmp::max(self.regions[self.current].start, 0x0010_0000);
            }
        }
    }
}

#[inline(always)]
pub fn align_down(addr: u64) -> u64 {
    addr & !(PAGE_SIZE - 1)
}

#[inline(always)]
pub fn align_up(addr: u64) -> u64 {
    (addr + PAGE_SIZE - 1) & !(PAGE_SIZE - 1)
}

#[inline(always)]
pub fn align_down_2mb(addr: u64) -> u64 {
    addr & !(PAGE_SIZE_2M - 1)
}

#[inline(always)]
pub fn align_up_2mb(addr: u64) -> u64 {
    (addr + PAGE_SIZE_2M - 1) & !(PAGE_SIZE_2M - 1)
}

pub fn fmt_bytes(bytes: u64) -> (u64, &'static str) {
    if bytes >= GIBI {
        (bytes / GIBI, "GiB")
    } else if bytes >= MIBI {
        (bytes / MIBI, "MiB")
    } else if bytes >= KIBI {
        (bytes / KIBI, "KiB")
    } else {
        (bytes, "B")
    }
}

#[inline]
pub fn early_usable(kind: MemoryRegionKind) -> bool {
    matches!(
        kind,
        MemoryRegionKind::Usable
            | MemoryRegionKind::EarlyReclaimable
            | MemoryRegionKind::AcpiReclaimable
    )
}
