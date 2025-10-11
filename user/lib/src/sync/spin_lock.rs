//! A spin-lock implementation.
//!
//! This lock supports only the most basic spinning, with yielding the thread to the kernel on
//! contention. TODO Add support for a smarter lock with futex-like functionality once the kernel
//! implements an appropriate set of syscalls.

use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicBool, Ordering},
};

/// A lock which "spins" when contended.
pub struct SpinLock<T: ?Sized> {
    /// The lock state.
    ///
    /// `false` means the lock is not held, and `true` means the lock is held.
    flag: AtomicBool,
    /// The value stored in the lock.
    value: UnsafeCell<T>,
}
impl<T> SpinLock<T> {
    /// Construct a [`Mutex`] to wrap the given value.
    pub const fn new(value: T) -> Self {
        Self {
            flag: AtomicBool::new(false),
            value: UnsafeCell::new(value),
        }
    }

    /// Destruct the mutex and return the inner value.
    ///
    /// This function does not have to lock because consuming the value means it cannot be in use
    /// anywhere else.
    pub fn into_inner(self) -> T {
        self.value.into_inner()
    }

    /// Get an exclusive reference to the inner value from an exclusive reference to the outer
    /// value.
    ///
    /// This function does not have to lock because the exclusive reference to the value means it
    /// cannot be in use anywhere else.
    pub fn get_mut(&mut self) -> &mut T {
        self.value.get_mut()
    }
}

impl<T: ?Sized> SpinLock<T> {
    /// Lock the mutex, returning an RAII guard.
    ///
    /// If the mutex is already locked, then this method will yield in a loop until the task
    /// holding the lock releases it.
    pub fn lock(&self) -> SpinLockGuard<'_, T> {
        loop {
            if let Some(guard) = self.try_lock() {
                return guard;
            }
            crate::sys::sched_yield();
        }
    }

    /// Attempt to lock the mutex without blocking.
    pub fn try_lock(&self) -> Option<SpinLockGuard<'_, T>> {
        self.flag
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .ok()
            .map(|_| SpinLockGuard {
                // SAFETY:
                // We've locked `flag`, so we have exclusive access.
                data: unsafe { &mut *self.value.get() },
                flag: &self.flag,
            })
    }
}
impl<T: Default> Default for SpinLock<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

// UnsafeCell implements `Send` as appropriate, so we only need `Sync`.

// SAFETY:
// Sharing the mutex between threads corresponds to sending the value to whichever thread locks
// the mutex.
unsafe impl<T: Send> Sync for SpinLock<T> {}

/// An RAII guard for a [`SpinLock`].
///
/// This value is constructed by calling [`SpinLock::lock`] and related methods.
pub struct SpinLockGuard<'a, T: ?Sized> {
    data: &'a mut T,
    flag: &'a AtomicBool,
}
impl<T: ?Sized> core::ops::Deref for SpinLockGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        self.data
    }
}
impl<T: ?Sized> core::ops::DerefMut for SpinLockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.data
    }
}
impl<T: ?Sized> Drop for SpinLockGuard<'_, T> {
    fn drop(&mut self) {
        self.flag.store(false, Ordering::Release);
    }
}
