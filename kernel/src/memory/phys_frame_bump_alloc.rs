use shared::{align_up, MemoryRegion, MemoryRegionKind, PAGE_SIZE};
use x86_64::{
    structures::paging::{FrameAllocator, PhysFrame, Size4KiB},
    PhysAddr,
};

pub struct PhysicalFrameBumpAllocator {
    pub regions: &'static [MemoryRegion],
    pub current: usize,
    pub next: u64,
}

impl PhysicalFrameBumpAllocator {
    pub fn restore_from(regions: &'static [MemoryRegion], current: usize, next: u64) -> Self {
        Self {
            regions,
            current,
            next,
        }
    }

    pub fn alloc_frame(&mut self) -> u64 {
        loop {
            let region = &self.regions[self.current];

            if region.kind == MemoryRegionKind::Usable && self.next + 4096 <= region.end {
                let p = self.next;
                self.next += 4096;
                return p;
            }

            self.current += 1;
            if self.current >= self.regions.len() {
                panic!("Out of physical Memory!")
            }

            if self.regions[self.current].kind == MemoryRegionKind::Usable {
                self.next = self.regions[self.current].start;
            }
        }
    }

    pub fn alloc_frames(&mut self, num_pages: usize) -> u64 {
        let size = (num_pages as u64) * PAGE_SIZE;

        loop {
            let region = &self.regions[self.current];
            let next = align_up(self.next);

            if region.kind == MemoryRegionKind::Usable && next + size <= region.end {
                let p = next;
                self.next = next + size;
                return p;
            }

            self.current += 1;
            if self.current >= self.regions.len() {
                panic!("Out of physical memory!");
            }

            self.next = align_up(self.regions[self.current].start);
        }
    }
}

unsafe impl FrameAllocator<Size4KiB> for PhysicalFrameBumpAllocator {
    fn allocate_frame(&mut self) -> Option<x86_64::structures::paging::PhysFrame<Size4KiB>> {
        let frame = self.alloc_frame();
        let frame = PhysFrame::<Size4KiB>::from_start_address(PhysAddr::new(frame)).unwrap();
        Some(frame)
    }
}
