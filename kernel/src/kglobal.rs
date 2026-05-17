use spin::Once;

use crate::{memory::bmp_alloc::BitmapFrameAllocator, utils::irq_lock::IrqMutex};

static BMP_ALLOC: Once<IrqMutex<BitmapFrameAllocator>> = Once::new();

pub fn init_bmp_alloc(alloc: BitmapFrameAllocator) {
    BMP_ALLOC.call_once(|| IrqMutex::new(alloc));
}

pub fn bmp_alloc() -> &'static IrqMutex<BitmapFrameAllocator> {
    BMP_ALLOC.get().expect("bmp allocator not initialized")
}
