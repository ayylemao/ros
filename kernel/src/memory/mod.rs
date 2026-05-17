use shared::{align_up_2mb, MIBI};
use x86_64::{
    structures::paging::{Mapper, OffsetPageTable, Page, Size2MiB},
    VirtAddr,
};

pub mod bmp_alloc;
pub mod heap;
pub mod paging;
pub mod phys_frame_bump_alloc;

pub fn unmap_identity_region_2mib(mapper: &mut OffsetPageTable, identity_limit: u64) {
    let start = 0u64;
    let end = align_up_2mb(identity_limit);
    let mut va = start;
    while va < end {
        let page = Page::<Size2MiB>::containing_address(VirtAddr::new(va));

        if let Ok((_frame, flush)) = mapper.unmap(page) {
            flush.flush();
        }
        va += 2 * MIBI;
    }
}
