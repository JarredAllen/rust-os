//! Cell types.

use crate::sync::atomic::Atomic;
use core::{cell::UnsafeCell, mem::MaybeUninit};

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
        UnsafeCell::raw_get(this as *const UnsafeCell<T>)
    }
}

// SAFETY: Safe construction only permits `Sync` values.
unsafe impl<T> Sync for SyncUnsafeCell<T> {}
// SAFETY: Safe construction only permits `Send` values.
unsafe impl<T> Send for SyncUnsafeCell<T> {}

/// A locked value which can only be written to once.
pub struct OnceLock<T> {
    /// Flags indicating the inner state.
    flags: Atomic<OnceLockFlags>,
    /// The inner value.
    value: UnsafeCell<MaybeUninit<T>>,
}
impl<T> OnceLock<T> {
    /// Construct a new lock, without a written value.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            flags: Atomic::new(OnceLockFlags::empty()),
            value: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }

    /// Get the value, if it has already been initialized.
    pub fn get(&self) -> Option<&T> {
        self.flags
            .load(core::sync::atomic::Ordering::Acquire)
            .locked()
            .then(|| {
                // SAFETY:
                // Because `self.initialized` is set, no more exclusive access can exist.
                let value = unsafe { &*self.value.get() };
                // SAFETY:
                // Because `self.initialized` is set, the value must be initialized.
                unsafe { value.assume_init_ref() }
            })
    }

    /// Attempt to set the value.
    ///
    /// If the value has already been set, then the given value is returned in an `Err`.
    pub fn set(&self, value: T) -> Result<(), T> {
        if self
            .flags
            .fetch_or(OnceLockFlags::LOCKED, core::sync::atomic::Ordering::AcqRel)
            .locked()
        {
            return Err(value);
        }
        // SAFETY:
        // Becuase we set `self.locked`, we have exclusive access until we mark `self.initialized`.
        unsafe { &mut *self.value.get() }.write(value);
        self.flags.fetch_or(
            OnceLockFlags::INITIALIZED,
            core::sync::atomic::Ordering::Release,
        );
        Ok(())
    }
}
impl<T> Default for OnceLock<T> {
    fn default() -> Self {
        Self::new()
    }
}
/// Construct a [`OnceLock`] with the value already inside.
impl<T> From<T> for OnceLock<T> {
    fn from(value: T) -> Self {
        Self {
            flags: Atomic::new(OnceLockFlags::LOCKED | OnceLockFlags::INITIALIZED),
            value: UnsafeCell::new(MaybeUninit::new(value)),
        }
    }
}
// SAFETY:
// A `OnceLock<T>` is equivalent to a `T`.
unsafe impl<T: Send> Send for OnceLock<T> {}
// SAFETY:
// A `OnceLock<T>` is equivalent to a `T`.
unsafe impl<T: Sync> Sync for OnceLock<T> {}

bitset::bitset!(
    /// Flags for the state of a `OnceLock`.
    OnceLockFlags(u8) {
        /// Whether the construction has been locked.
        ///
        /// If this value is not set, then no access to [`Self::value`] can exist.
        Locked,
        /// Whether [`Self::value`] has been initialized.
        ///
        /// If this value is set, then no exclusive access to [`Self::value`] exists anymore, and
        /// the value has been initialized.
        Initialized,
    }
);
