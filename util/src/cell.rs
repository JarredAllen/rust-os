use core::{cell::UnsafeCell, mem::MaybeUninit, sync::atomic::AtomicBool};

/// A wrapper for [`UnsafeCell`] which is also [`Sync`].
///
/// This value is designed primarily for putting mutable values in static memory.
#[repr(transparent)]
pub struct SyncUnsafeCell<T: ?Sized> {
    /// The inner cell.
    inner: UnsafeCell<T>,
}

impl<T: ?Sized> SyncUnsafeCell<T> {
    /// Construct a new [`SyncUnsafeCell`].
    ///
    /// This constructor requires the value be `Send` and `Sync` because it can be used to share or
    /// send values between threads. For values which aren't, see [`Self::new_unchecked`].
    pub const fn new(value: T) -> Self
    where
        T: Sized + Send + Sync,
    {
        // SAFETY: We've checked that this value is `Send + Sync`.
        unsafe { Self::new_unchecked(value) }
    }

    /// Construct a new [`SyncUnsafeCell`].
    ///
    /// # Safety
    /// This type can be used to share or send values between threads. For types which aren't
    /// `Send` and/or `Sync`, it's on you to ensure that this cell doesn't get used in such a way
    /// that causes UB.
    pub const unsafe fn new_unchecked(value: T) -> Self
    where
        T: Sized,
    {
        Self {
            inner: UnsafeCell::new(value),
        }
    }

    /// Convert back into the original value.
    pub fn into_inner(self) -> T
    where
        T: Sized,
    {
        self.inner.into_inner()
    }

    /// Get a pointer to the inner value.
    ///
    /// This method is always safe to call, and the resulting pointer is safe to dereference so
    /// long as you comply with normal aliasing rules.
    pub const fn get(&self) -> *mut T {
        self.inner.get()
    }

    /// Get an exclusive reference to the inner value (safely).
    ///
    /// Having an exclusive reference to `self` ensures that no one else can access the inner
    /// value, so this is always a safe operation.
    ///
    /// If the type checker isn't able to verify exclusive access, use [`Self::get`] instead.
    pub const fn get_mut(&mut self) -> &mut T {
        self.inner.get_mut()
    }

    /// Get a pointer to the inner value.
    ///
    /// This method is always safe to call, and the resulting pointer is safe to dereference so
    /// long as you comply with normal aliasing rules and `this` points to a validly-constructed
    /// instance of `SyncUnsafeCell<T>`.
    pub const fn raw_get(this: *const Self) -> *mut T {
        // NOTE: `repr(transparent)` means the same address works for both `self` and `inner`
        UnsafeCell::raw_get(this as *const _)
    }
}

unsafe impl<T> Sync for SyncUnsafeCell<T> {}
unsafe impl<T> Send for SyncUnsafeCell<T> {}

pub struct OnceLock<T> {
    locked: AtomicBool,
    initialized: AtomicBool,
    value: UnsafeCell<MaybeUninit<T>>,
}
impl<T> OnceLock<T> {
    pub const fn new() -> Self {
        Self {
            locked: AtomicBool::new(false),
            initialized: AtomicBool::new(false),
            value: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }
    pub fn get(&self) -> Option<&T> {
        self.initialized
            .load(core::sync::atomic::Ordering::Acquire)
            .then(|| {
                let value = unsafe { &*self.value.get() };
                unsafe { value.assume_init_ref() }
            })
    }

    pub fn set(&self, value: T) -> Result<(), T> {
        if self.locked.swap(true, core::sync::atomic::Ordering::AcqRel) {
            return Err(value);
        }
        unsafe { &mut *self.value.get() }.write(value);
        self.initialized
            .store(true, core::sync::atomic::Ordering::Release);
        Ok(())
    }
}
impl<T> Default for OnceLock<T> {
    fn default() -> Self {
        Self::new()
    }
}
unsafe impl<T: Send> Send for OnceLock<T> {}
unsafe impl<T: Sync> Sync for OnceLock<T> {}
