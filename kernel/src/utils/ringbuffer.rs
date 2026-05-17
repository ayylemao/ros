use core::{
    cell::UnsafeCell,
    mem::MaybeUninit,
    sync::atomic::{AtomicUsize, Ordering},
};

#[derive(Debug)]
pub struct SpscRing<T: Copy, const N: usize> {
    head: AtomicUsize,
    tail: AtomicUsize,
    buf: [UnsafeCell<MaybeUninit<T>>; N],
}

unsafe impl<T: Copy, const N: usize> Sync for SpscRing<T, N> {}

impl<T: Copy, const N: usize> SpscRing<T, N> {
    pub const fn new() -> Self {
        Self {
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            buf: unsafe { MaybeUninit::<[UnsafeCell<MaybeUninit<T>>; N]>::uninit().assume_init() },
        }
    }

    #[inline(always)]
    fn mask() -> usize {
        N - 1
    }

    #[inline]
    pub fn push(&self, val: T) -> Result<(), T> {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);

        if head.wrapping_sub(tail) == N {
            return Err(val);
        }

        let idx = head & Self::mask();
        unsafe {
            (*self.buf[idx].get()).write(val);
        };

        self.head.store(head.wrapping_add(1), Ordering::Relaxed);
        Ok(())
    }

    #[allow(dead_code)]
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.head.load(Ordering::Acquire) == self.tail.load(Ordering::Acquire)
    }

    #[inline]
    pub fn pop(&self) -> Option<T> {
        let tail = self.tail.load(Ordering::Relaxed);
        let head = self.head.load(Ordering::Acquire);

        if head == tail {
            return None; // empty
        }

        let idx = tail & Self::mask();
        let val = unsafe { (*self.buf[idx].get()).assume_init_read() };

        self.tail.store(tail.wrapping_add(1), Ordering::Release);
        Some(val)
    }
    #[inline]
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        let head = self.head.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Acquire);
        head.wrapping_sub(tail)
    }
}
