use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::ops::{Deref, DerefMut};

use wdk_sys::KSPIN_LOCK;
use wdk_sys::ntddk::{KeAcquireSpinLockRaiseToDpc, KeInitializeSpinLock, KeReleaseSpinLock};

pub struct SpinLockGuard<'a, T> {
    _inner: &'a SpinLock<T>,
    _old_irql: u8,
}

impl<T> Drop for SpinLockGuard<'_, T> {
    fn drop(&mut self) {
        let lock = &self._inner._lock as *const KSPIN_LOCK as *mut KSPIN_LOCK;
        unsafe {
            KeReleaseSpinLock(lock, self._old_irql);
        }
    }
}

impl<T> Deref for SpinLockGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        let ptr = self._inner._inner.get();
        unsafe { ptr.as_ref().unwrap_unchecked() }
    }
}

impl<T> DerefMut for SpinLockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        let ptr = self._inner._inner.get();
        unsafe { ptr.as_mut().unwrap_unchecked() }
    }
}

/// Wrapper around a Windows kernel
/// [spin lock](https://learn.microsoft.com/en-us/windows-hardware/drivers/kernel/introduction-to-spin-locks).
pub struct SpinLock<T> {
    _lock: KSPIN_LOCK,
    _inner: UnsafeCell<T>,
}

impl<T> SpinLock<T> {
    /// Construct a new spin lock synchronizing access to an inner value.
    pub fn new(inner: T) -> Self {
        let mut lock = MaybeUninit::<KSPIN_LOCK>::uninit();
        unsafe {
            KeInitializeSpinLock(lock.as_mut_ptr());
            Self {
                _lock: lock.assume_init(),
                _inner: UnsafeCell::new(inner),
            }
        }
    }

    /// Acquire the spin lock. While the guard is held, IRQL is raised to at least
    /// `DISPATCH_LEVEL`.
    pub fn lock(&self) -> SpinLockGuard<'_, T> {
        let lock = &self._lock as *const KSPIN_LOCK as *mut KSPIN_LOCK;
        let irql = unsafe { KeAcquireSpinLockRaiseToDpc(lock) };

        SpinLockGuard {
            _inner: self,
            _old_irql: irql,
        }
    }
}

unsafe impl<T: Send> Send for SpinLock<T> {}
unsafe impl<T: Send> Sync for SpinLock<T> {}
