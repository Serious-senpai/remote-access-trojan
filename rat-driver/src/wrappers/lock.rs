use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::ops::{Deref, DerefMut};

use wdk_sys::ntddk::{
    ExAcquireSpinLockExclusive, ExAcquireSpinLockShared, ExReleaseSpinLockExclusive,
    ExReleaseSpinLockShared, KeAcquireSpinLockRaiseToDpc, KeInitializeSpinLock, KeReleaseSpinLock,
};
use wdk_sys::{EX_SPIN_LOCK, KIRQL, KSPIN_LOCK};

#[allow(unused)] // TODO: Remove this in the future
pub struct SpinLockGuard<'a, T> {
    _inner: &'a SpinLock<T>,
    _old_irql: KIRQL,
}

impl<T> Drop for SpinLockGuard<'_, T> {
    fn drop(&mut self) {
        let lock = self._inner._lock.get();
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
    _lock: UnsafeCell<KSPIN_LOCK>,
    _inner: UnsafeCell<T>,
}

#[allow(unused)] // TODO: Remove this in the future
impl<T> SpinLock<T> {
    /// Construct a new spin lock synchronizing access to an inner value.
    pub fn new(inner: T) -> Self {
        let mut lock = MaybeUninit::<KSPIN_LOCK>::uninit();
        unsafe {
            KeInitializeSpinLock(lock.as_mut_ptr());
            Self {
                _lock: UnsafeCell::new(lock.assume_init()),
                _inner: UnsafeCell::new(inner),
            }
        }
    }

    /// Acquire the spin lock. While the guard is held, IRQL is raised to at least
    /// `DISPATCH_LEVEL`.
    pub fn lock(&self) -> SpinLockGuard<'_, T> {
        let lock = self._lock.get();
        let irql = unsafe { KeAcquireSpinLockRaiseToDpc(lock) };

        SpinLockGuard {
            _inner: self,
            _old_irql: irql,
        }
    }
}

unsafe impl<T: Send> Send for SpinLock<T> {}
unsafe impl<T: Send + Sync> Sync for SpinLock<T> {}

pub struct ExSpinLockReadGuard<'a, T> {
    _inner: &'a ExSpinLock<T>,
    _old_irql: KIRQL,
}

impl<T> Drop for ExSpinLockReadGuard<'_, T> {
    fn drop(&mut self) {
        let lock = self._inner._lock.get();
        unsafe {
            ExReleaseSpinLockShared(lock, self._old_irql);
        }
    }
}

impl<T> Deref for ExSpinLockReadGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        let ptr = self._inner._inner.get();
        unsafe { ptr.as_ref().unwrap_unchecked() }
    }
}

pub struct ExSpinLockWriteGuard<'a, T> {
    _inner: &'a ExSpinLock<T>,
    _old_irql: KIRQL,
}

impl<T> Drop for ExSpinLockWriteGuard<'_, T> {
    fn drop(&mut self) {
        let lock = self._inner._lock.get();
        unsafe {
            ExReleaseSpinLockExclusive(lock, self._old_irql);
        }
    }
}

impl<T> Deref for ExSpinLockWriteGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        let ptr = self._inner._inner.get();
        unsafe { ptr.as_ref().unwrap_unchecked() }
    }
}

impl<T> DerefMut for ExSpinLockWriteGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        let ptr = self._inner._inner.get();
        unsafe { ptr.as_mut().unwrap_unchecked() }
    }
}

pub struct ExSpinLock<T> {
    _lock: UnsafeCell<EX_SPIN_LOCK>,
    _inner: UnsafeCell<T>,
}

impl<T> ExSpinLock<T> {
    /// Construct a new spin lock synchronizing access to an inner value.
    pub fn new(inner: T) -> Self {
        Self {
            _lock: UnsafeCell::new(EX_SPIN_LOCK::default()),
            _inner: UnsafeCell::new(inner),
        }
    }

    pub fn read(&self) -> ExSpinLockReadGuard<'_, T> {
        let lock = self._lock.get();
        let irql = unsafe { ExAcquireSpinLockShared(lock) };

        ExSpinLockReadGuard {
            _inner: self,
            _old_irql: irql,
        }
    }

    pub fn write(&self) -> ExSpinLockWriteGuard<'_, T> {
        let lock = self._lock.get();
        let irql = unsafe { ExAcquireSpinLockExclusive(lock) };

        ExSpinLockWriteGuard {
            _inner: self,
            _old_irql: irql,
        }
    }
}

unsafe impl<T: Send> Send for ExSpinLock<T> {}
unsafe impl<T: Send + Sync> Sync for ExSpinLock<T> {}
