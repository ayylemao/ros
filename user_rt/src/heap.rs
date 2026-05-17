use core::alloc::Layout;

use spin::Mutex;
use talc::{OomHandler, Span, Talc, Talck};

use sys::syscall::{
    errors::Errno,
    wrappers::{brk, exit},
};

const INIT_HEAP: usize = 16 * 1024;
const GROW_HEAP: usize = 64 * 1024;
const PAGE_SIZE: usize = 4096;

pub fn sbrk(delta: i64) -> Result<i64, Errno> {
    let old = brk(0)? as i64;
    if delta == 0 {
        return Ok(old);
    }

    let new = old.checked_add(delta).ok_or(Errno::ValueOverflow)?;
    if new < 0 {
        return Err(Errno::InvalidArgument);
    }

    let after = brk(new as u64)? as i64;

    if after != new {
        return Err(Errno::OutOfMemory);
    }

    Ok(old)
}

const fn align_up(x: usize, a: usize) -> usize {
    (x + a - 1) & !(a - 1)
}

#[derive(Debug, Clone, Copy)]
pub struct SbrkOom {
    heap: Span,
}

impl SbrkOom {
    pub const fn new() -> Self {
        Self {
            heap: Span::empty(),
        }
    }
}

impl OomHandler for SbrkOom {
    fn handle_oom(talc: &mut talc::Talc<Self>, layout: core::alloc::Layout) -> Result<(), ()> {
        let need = layout.size().max(INIT_HEAP as usize);
        let grow = align_up(need.max(GROW_HEAP), PAGE_SIZE as usize);

        let old_brk = sbrk(grow as i64).unwrap();

        let new_region = Span::from_base_size(old_brk as *mut u8, grow);

        unsafe {
            if talc.oom_handler.heap.is_empty() {
                let actual = talc.claim(new_region)?;
                talc.oom_handler.heap = actual;
                Ok(())
            } else {
                let heap = talc.oom_handler.heap;
                let (_base, acme) = heap.get_base_acme().ok_or(())?;
                if (old_brk as *mut u8) != acme {
                    let actual = talc.claim(new_region)?;
                    talc.oom_handler.heap = actual;
                    return Ok(());
                }
                let req_heap = heap.extend(0, grow);
                let actual = talc.extend(heap, req_heap);
                talc.oom_handler.heap = actual;
                Ok(())
            }
        }
    }
}

#[global_allocator]
static ALLOCATOR: Talck<Mutex<()>, SbrkOom> = Talc::new(SbrkOom::new()).lock();

#[alloc_error_handler]
fn oom(_: Layout) -> ! {
    _ = exit(127);
}
