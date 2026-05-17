use alloc::vec::Vec;
use shared::PHYS_OFFSET;
use x86_64::{
    registers::control::Cr3,
    structures::paging::{FrameAllocator, OffsetPageTable, PageTable, PhysFrame, Size4KiB},
    VirtAddr,
};

#[derive(Debug, Clone)]
pub struct AddressSpace {
    pml4: PhysFrame<Size4KiB>,
    page_table_frames: Vec<PhysFrame<Size4KiB>>,
}

impl AddressSpace {
    pub fn new_user(alloc: &mut impl FrameAllocator<Size4KiB>) -> Result<Self, &'static str> {
        let pml4 = alloc.allocate_frame().ok_or("out of frames (pml4)")?;

        let new_pml4: &mut PageTable = unsafe { phys_frame_as_mut_table(pml4) };
        new_pml4.zero();

        let (cur_pml4_frame, _) = Cr3::read();
        let cur_pml4: &PageTable = unsafe { phys_frame_as_table(cur_pml4_frame) };

        for i in 256..512 {
            new_pml4[i] = cur_pml4[i].clone();
        }

        Ok(AddressSpace {
            pml4,
            page_table_frames: Vec::new(),
        })
    }

    #[allow(dead_code)]
    pub fn pml4_frame(&self) -> PhysFrame<Size4KiB> {
        self.pml4
    }

    pub fn activate(&self) -> PhysFrame<Size4KiB> {
        let (old, flags) = Cr3::read();
        unsafe { Cr3::write(self.pml4, flags) };
        old
    }

    pub fn restore(old: PhysFrame<Size4KiB>) {
        let (_, flags) = Cr3::read();
        unsafe { Cr3::write(old, flags) };
    }

    pub unsafe fn mapper(&self) -> OffsetPageTable<'static> {
        let pml4_table: &'static mut PageTable = phys_frame_as_mut_table(self.pml4);
        OffsetPageTable::new(pml4_table, VirtAddr::new(PHYS_OFFSET))
    }

    pub fn owned_frames(&self) -> impl Iterator<Item = PhysFrame<Size4KiB>> + '_ {
        core::iter::once(self.pml4).chain(self.page_table_frames.iter().copied())
    }
}

unsafe fn phys_frame_as_table(frame: PhysFrame<Size4KiB>) -> &'static PageTable {
    let phys = frame.start_address().as_u64();
    let virt = VirtAddr::new(PHYS_OFFSET + phys);
    &*virt.as_ptr::<PageTable>()
}

unsafe fn phys_frame_as_mut_table(frame: PhysFrame<Size4KiB>) -> &'static mut PageTable {
    let phys = frame.start_address().as_u64();
    let virt = VirtAddr::new(PHYS_OFFSET + phys);
    &mut *virt.as_mut_ptr::<PageTable>()
}
