use core::ops::{Deref, DerefMut};

use spin::{Mutex, MutexGuard};
use x86_64::instructions::interrupts;

pub struct IrqMutex<T> {
    inner: Mutex<T>,
}

impl<T> IrqMutex<T> {
    pub const fn new(val: T) -> Self {
        Self {
            inner: Mutex::new(val),
        }
    }

    #[inline(always)]
    pub fn lock(&self) -> IrqMutexGuard<'_, T> {
        let was_enabled = interrupts::are_enabled();
        interrupts::disable();

        let guard = self.inner.lock();

        IrqMutexGuard {
            guard: Some(guard),
            restore_interrupts: was_enabled,
        }
    }
}

pub struct IrqMutexGuard<'a, T> {
    guard: Option<MutexGuard<'a, T>>,
    restore_interrupts: bool,
}

impl<'a, T> Deref for IrqMutexGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        self.guard.as_ref().unwrap().deref()
    }
}

impl<'a, T> DerefMut for IrqMutexGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.guard.as_mut().unwrap().deref_mut()
    }
}

impl<'a, T> Drop for IrqMutexGuard<'a, T> {
    fn drop(&mut self) {
        self.guard.take();
        if self.restore_interrupts {
            interrupts::enable();
        }
    }
}
