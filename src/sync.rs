//! Synchronization

use core::{
    cell::UnsafeCell,
    mem::MaybeUninit,
    ops::Deref,
    sync::atomic::{AtomicBool, Ordering},
};

/// A lock which "spins" when contended.
pub struct KSpinLock<T: ?Sized> {
    /// The lock state.
    ///
    /// `false` means the lock is not held, and `true` means the lock is held.
    flag: AtomicBool,
    /// The value stored in the lock.
    value: UnsafeCell<T>,
}
#[expect(dead_code, reason = "I'll use this eventually")]
impl<T> KSpinLock<T> {
    /// Construct a [`KSpinLock`] to wrap the given value.
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

impl<T: ?Sized> KSpinLock<T> {
    /// Lock the mutex, returning an RAII guard.
    ///
    /// If the mutex is already locked, then this method will yield in a loop until the task
    /// holding the lock releases it.
    pub fn lock(&self) -> KSpinLockGuard<'_, T> {
        loop {
            if let Some(guard) = self.try_lock() {
                return guard;
            }
            crate::proc::sched_yield();
        }
    }

    /// Attempt to lock the mutex without blocking.
    pub fn try_lock(&self) -> Option<KSpinLockGuard<'_, T>> {
        self.flag
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .ok()
            .map(|_| KSpinLockGuard {
                // SAFETY:
                // We've locked `flag`, so we have exclusive access.
                data: unsafe { &mut *self.value.get() },
                flag: &self.flag,
            })
    }
}
impl<T: Default> Default for KSpinLock<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

// UnsafeCell implements `Send` as appropriate, so we only need `Sync`.

/// # Safety
/// Sharing the mutex between threads corresponds to sending the value to whichever thread locks
/// the mutex.
unsafe impl<T: Send> Sync for KSpinLock<T> {}

/// An RAII guard for a [`KSpinLock`].
///
/// This value is constructed by calling [`KSpinLock::lock`] and related methods.
pub struct KSpinLockGuard<'a, T: ?Sized> {
    data: &'a mut T,
    flag: &'a AtomicBool,
}
impl<T: ?Sized> core::ops::Deref for KSpinLockGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        self.data
    }
}
impl<T: ?Sized> core::ops::DerefMut for KSpinLockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.data
    }
}
impl<T: ?Sized> Drop for KSpinLockGuard<'_, T> {
    fn drop(&mut self) {
        self.flag.store(false, Ordering::Release);
    }
}

pub struct LazyLock<T, F = fn() -> T> {
    value: UnsafeCell<MaybeUninit<T>>,
    init_func: UnsafeCell<MaybeUninit<F>>,
    started: AtomicBool,
    finished: AtomicBool,
}
impl<T, F> LazyLock<T, F> {
    pub const fn new(f: F) -> Self {
        Self {
            value: UnsafeCell::new(MaybeUninit::uninit()),
            init_func: UnsafeCell::new(MaybeUninit::new(f)),
            started: AtomicBool::new(false),
            finished: AtomicBool::new(false),
        }
    }

    pub fn force(&self) -> &T
    where
        F: FnOnce() -> T,
        T: core::fmt::Debug,
    {
        if self.finished.load(Ordering::Acquire) {
            let value = unsafe { &*self.value.get() };
            unsafe { value.assume_init_ref() }
        } else {
            if self.started.swap(true, Ordering::AcqRel) {
                panic!("TODO Deconflict concurrent initialization attempts");
            }
            let init_func = unsafe { self.init_func.get().read() };
            let value =
                unsafe { &mut *self.value.get() }.write(unsafe { init_func.assume_init() }());
            self.finished.store(true, Ordering::Release);
            value
        }
    }
}
impl<T, F> Deref for LazyLock<T, F>
where
    F: FnOnce() -> T,
    T: core::fmt::Debug,
{
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.force()
    }
}
impl<T, F> Drop for LazyLock<T, F> {
    fn drop(&mut self) {
        let started = self.started.load(Ordering::Acquire);
        let finished = self.started.load(Ordering::Acquire);
        match (started, finished) {
            (false, false) => {
                let init_func = unsafe { &mut *self.init_func.get() };
                unsafe { init_func.assume_init_drop() };
            }
            (true, true) => {
                let value = unsafe { &mut *self.value.get() };
                unsafe { value.assume_init_drop() };
            }
            _ => {
                unreachable!("dropping lazy lock but started != finished");
            }
        }
    }
}

unsafe impl<T: Sync, F: Send> Sync for LazyLock<T, F> {}
unsafe impl<T: Send, F: Send> Send for LazyLock<T, F> {}
