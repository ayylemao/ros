use shared::PHYS_OFFSET;
use x86_64::{
    registers::control::Cr3,
    structures::paging::{OffsetPageTable, PageTable},
    VirtAddr,
};

pub fn kernel_mapper() -> OffsetPageTable<'static> {
    let (frame, _) = Cr3::read();

    let pml4_phys = frame.start_address();
    let pml4_virt = PHYS_OFFSET + pml4_phys.as_u64();

    let pml4: *mut PageTable = pml4_virt as *mut PageTable;
    let pml4: &mut PageTable = unsafe { &mut *pml4 };

    let mapper: OffsetPageTable<'_> =
        unsafe { OffsetPageTable::new(pml4, VirtAddr::new(PHYS_OFFSET)) };
    mapper
}
