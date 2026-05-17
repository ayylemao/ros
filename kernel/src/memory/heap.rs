use linked_list_allocator::LockedHeap;
use shared::{KERNEL_HEAP_BASE, KERNEL_HEAP_SIZE, PAGE_SIZE};
use x86_64::{
    structures::paging::{
        mapper::MapToError, FrameAllocator, Mapper, OffsetPageTable, Page, PageTableFlags, Size4KiB,
    },
    VirtAddr,
};

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

pub fn init_heap(
    mapper: &mut OffsetPageTable,
    alloc: &mut impl FrameAllocator<Size4KiB>,
) -> Result<(), MapToError<Size4KiB>> {
    let page_count = KERNEL_HEAP_SIZE / PAGE_SIZE;

    for i in 0..page_count {
        let va = VirtAddr::new(KERNEL_HEAP_BASE + PAGE_SIZE * i);
        unsafe {
            let pa = alloc.allocate_frame().unwrap();
            mapper
                .map_to(
                    Page::containing_address(va),
                    pa,
                    PageTableFlags::PRESENT | PageTableFlags::WRITABLE,
                    alloc,
                )
                .unwrap()
                .flush();
        }
    }

    unsafe {
        ALLOCATOR
            .lock()
            .init(KERNEL_HEAP_BASE as *mut u8, KERNEL_HEAP_SIZE as usize);
    }

    Ok(())
}
