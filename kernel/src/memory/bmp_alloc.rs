use alloc::vec::Vec;
use shared::{early_usable, PAGE_SIZE, PHYS_OFFSET};
use x86_64::{
    structures::paging::{FrameAllocator, FrameDeallocator, PhysFrame, Size4KiB},
    PhysAddr,
};

use crate::memory::phys_frame_bump_alloc::PhysicalFrameBumpAllocator;

pub struct BitmapFrameAllocator {
    bmp: &'static mut [u8],
    usable_frames: u64,
    free_frames: u64,
    next_free_frame: u64,
    max_frame: u64,
}
impl BitmapFrameAllocator {
    pub fn new(bootstrap_alloc: &mut PhysicalFrameBumpAllocator) -> Self {
        let max_addr = bootstrap_alloc
            .regions
            .iter()
            .filter(|x| early_usable(x.kind))
            .map(|x| x.end)
            .max()
            .unwrap();

        let bmp_size = ((max_addr / PAGE_SIZE) + 7) / 8;
        let bmp_pages = (bmp_size + PAGE_SIZE - 1) / PAGE_SIZE;

        let phys_addr = bootstrap_alloc.alloc_frames(bmp_pages as usize);
        let bmp_virt = phys_addr + PHYS_OFFSET;

        let bmp: &mut [u8] =
            unsafe { core::slice::from_raw_parts_mut(bmp_virt as *mut u8, bmp_size as usize) };
        bmp.fill(0xFF);

        let mut free_frames: u64 = 0;
        let mut usable_frames: u64 = 0;
        let mut next_free_frame: u64 = 0;
        let mut max_frame: u64 = 0;

        // shitty logic iterating 3 times over the map but it is what it is
        for r in bootstrap_alloc
            .regions
            .iter()
            .filter(|r| early_usable(r.kind))
        {
            let start = (r.start + PAGE_SIZE - 1) / PAGE_SIZE;
            let end = r.end / PAGE_SIZE;
            usable_frames += end - start;
            free_frames += end - start;

            for frame in start..end {
                clear_bit(bmp, frame);
            }
            max_frame = max_frame.max(end);
        }

        for r in &bootstrap_alloc.regions[..bootstrap_alloc.current] {
            if !early_usable(r.kind) {
                continue;
            }
            let start = (r.start + PAGE_SIZE - 1) / PAGE_SIZE;
            let end = r.end / PAGE_SIZE;
            free_frames -= end - start;
            for f in start..end {
                set_bit(bmp, f);
            }
        }

        let r = &bootstrap_alloc.regions[bootstrap_alloc.current];
        if early_usable(r.kind) {
            let start = (r.start + PAGE_SIZE - 1) / PAGE_SIZE;
            let end = (bootstrap_alloc.next + PAGE_SIZE - 1) / PAGE_SIZE;
            for f in start..end {
                free_frames -= 1;
                set_bit(bmp, f);
                next_free_frame = f + 1;
            }
        }

        Self {
            bmp,
            usable_frames,
            free_frames,
            next_free_frame,
            max_frame,
        }
    }

    fn find_next_free_frame(&mut self) -> Option<u64> {
        let mut frame = self.next_free_frame;
        while frame < self.max_frame {
            if !test_bit(&self.bmp, frame) {
                self.next_free_frame = frame + 1;
                return Some(frame);
            }
            frame += 1;
        }
        None
    }

    fn alloc_frame(&mut self) -> Option<u64> {
        let frame = self.find_next_free_frame()?;
        set_bit(&mut self.bmp, frame);
        self.free_frames -= 1;
        Some(frame)
    }

    fn dealloc_frame(&mut self, frame: u64) {
        debug_assert!(test_bit(self.bmp, frame));
        clear_bit(self.bmp, frame);
        self.free_frames += 1;
        if frame < self.next_free_frame {
            self.next_free_frame = frame;
        }
    }

    pub fn get_available_memory(&self) -> u64 {
        self.free_frames * PAGE_SIZE
    }

    pub fn get_total_memory(&self) -> u64 {
        self.usable_frames * PAGE_SIZE
    }
}

unsafe impl FrameAllocator<Size4KiB> for BitmapFrameAllocator {
    fn allocate_frame(&mut self) -> Option<x86_64::structures::paging::PhysFrame<Size4KiB>> {
        let frame_idx = self.alloc_frame()?;
        let phys = frame_idx * PAGE_SIZE;
        PhysFrame::from_start_address(PhysAddr::new(phys)).ok()
    }
}

impl FrameDeallocator<Size4KiB> for BitmapFrameAllocator {
    unsafe fn deallocate_frame(&mut self, frame: PhysFrame<Size4KiB>) {
        self.dealloc_frame(frame.start_address().as_u64() / PAGE_SIZE);
    }
}

#[inline]
fn set_bit(bitmap: &mut [u8], frame: u64) {
    let byte = (frame / 8) as usize;
    let bit = (frame % 8) as u8;
    bitmap[byte] |= 1 << bit;
}

#[inline]
fn clear_bit(bitmap: &mut [u8], frame: u64) {
    let byte = (frame / 8) as usize;
    let bit = (frame % 8) as u8;
    bitmap[byte] &= !(1 << bit);
}

#[inline]
fn test_bit(bitmap: &[u8], frame: u64) -> bool {
    let byte = (frame / 8) as usize;
    let bit = (frame % 8) as u8;
    bitmap[byte] & (1 << bit) != 0
}

pub struct TrackingAlloc<'a, A: FrameAllocator<Size4KiB>> {
    inner: &'a mut A,
    pub frames: Vec<PhysFrame<Size4KiB>>,
}

impl<'a, A: FrameAllocator<Size4KiB>> TrackingAlloc<'a, A> {
    pub fn new(inner: &'a mut A) -> Self {
        Self {
            inner,
            frames: Vec::new(),
        }
    }
}

unsafe impl<'a, A: FrameAllocator<Size4KiB>> FrameAllocator<Size4KiB> for TrackingAlloc<'a, A> {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        let f = self.inner.allocate_frame()?;
        self.frames.push(f);
        Some(f)
    }
}
