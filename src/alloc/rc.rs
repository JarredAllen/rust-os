//! A reference-counted shared pointer to a heap allocation.
//!
//! See [`KrcBox`].

use core::{
    alloc::Layout,
    mem::MaybeUninit,
    ops::Deref,
    ptr::NonNull,
    sync::atomic::{AtomicUsize, Ordering},
};

use crate::error::OutOfMemory;

/// A reference-counted shared pointer to a heap allocation.
///
/// The number of pointers to this memory is closely tracked, and the memory is automatically freed
/// when no more references to this allocation exist.
///
/// If `usize::MAX` copies of this allocation exist concurrently, then the reference counter will
/// saturate and the memory will be leaked.
pub struct KrcBox<T: ?Sized> {
    /// The inner pointer.
    ///
    /// # Safety Invariant
    /// This points to a real value until the destructor of the last [`KrcBox`] pointed at this
    /// allocation.
    ptr: NonNull<KrcBoxInner<T>>,
}
impl<T> KrcBox<T> {
    /// Construct a new reference-counted pointer for a given value.
    pub fn new(value: T) -> Result<Self, OutOfMemory> {
        Self::for_init_func(|slot| {
            slot.write(value);
        })
    }

    /// Construct a new reference-counted pointer in-place.
    ///
    /// If you're willing to construct the entire value on the stack and then copy it to
    /// heap-allocated memory, consider [`Self::new`] instead.
    pub fn for_init_func(init_func: impl FnOnce(&mut MaybeUninit<T>)) -> Result<Self, OutOfMemory> {
        let ptr = super::ALLOCATOR
            .allocate_inner(Layout::new::<KrcBoxInner<T>>())?
            .cast::<KrcBoxInner<T>>();
        // SAFETY:
        // We just allocated the value and haven't shared it, so we can write to it.
        unsafe {
            ptr.as_ptr()
                .cast::<AtomicUsize>()
                .wrapping_byte_add(core::mem::offset_of!(KrcBoxInner<T>, refcount))
                .write(AtomicUsize::new(1));
        }
        // SAFETY:
        // We just allocated the value and haven't shared it, so we have exclusive access.
        let value_memory = unsafe {
            &mut *ptr
                .as_ptr()
                .cast::<MaybeUninit<T>>()
                .wrapping_byte_add(core::mem::offset_of!(KrcBoxInner<T>, value))
        };
        init_func(value_memory);
        Ok(Self { ptr })
    }
}

impl<T: ?Sized> KrcBox<T> {
    /// Get the inner, shared value.
    fn inner(&self) -> &KrcBoxInner<T> {
        // SAFETY:
        // By the type invariant, this is valid so we can read it.
        unsafe { self.ptr.as_ref() }
    }

    /// Get whether this pointer has unique access to the underlying allocation.
    ///
    /// If this method returns true, then various methods for aquiring mutable access to the inner
    /// value will succeed.
    ///
    /// # Memory Ordering
    /// If this method returns `true`, then it synchronizes with any previous drops of other
    /// pointers to the same memory.
    pub fn is_unique(this: &Self) -> bool {
        this.inner().refcount.load(Ordering::Acquire) == 1
    }
}

impl<T: ?Sized> Clone for KrcBox<T> {
    fn clone(&self) -> Self {
        increment_atomic_saturating(&self.inner().refcount);
        Self { ptr: self.ptr }
    }
}

impl<T: ?Sized> Deref for KrcBox<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner().value
    }
}

impl<T: ?Sized> Drop for KrcBox<T> {
    fn drop(&mut self) {
        if decrement_if_unsaturated(&self.inner().refcount) == 0 {
            // SAFETY:
            // The allocation is about to be freed, so we can free the allocated value.
            unsafe { self.ptr.drop_in_place() };
            // SAFETY:
            // We allocated using this layout, so we can free with this layout.
            unsafe {
                super::ALLOCATOR.deallocate_inner(self.ptr.cast(), Layout::for_value(self.inner()));
            }
        }
    }
}

// SAFETY;
// Sending a `KrcBox` between threads can be sending or sharing the inner value, depending on
// whether other pointers exist to it.
unsafe impl<T: Send + Sync + ?Sized> Send for KrcBox<T> {}
// SAFETY:
// Sharing a `KrcBox` between threads shares the inner value, but also the reference can be cloned
// to potentially send the inner value.
unsafe impl<T: Send + Sync + ?Sized> Sync for KrcBox<T> {}

/// The heap memory a [`KrcBox`] points at.
struct KrcBoxInner<T: ?Sized> {
    /// The number of live allocations.
    ///
    /// Note that this value saturates at `usize::MAX`, at which point the memory is leaked.
    refcount: AtomicUsize,
    /// The value being stored here.
    value: T,
}

/// Saturating increment an atomic, returning the new value.
fn increment_atomic_saturating(counter: &AtomicUsize) -> usize {
    let mut old_count = counter.load(Ordering::Relaxed);
    loop {
        let new_count = old_count.saturating_add(1);
        match counter.compare_exchange_weak(
            old_count,
            new_count,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => {
                return new_count;
            }
            Err(updated_count) => {
                old_count = updated_count;
            }
        }
    }
}

/// Decrement an atomic if it hasn't saturated, returning the new value.
fn decrement_if_unsaturated(counter: &AtomicUsize) -> usize {
    let mut old_count = counter.load(Ordering::Relaxed);
    loop {
        if old_count == usize::MAX {
            return old_count;
        }
        let new_count = old_count.saturating_sub(1);
        match counter.compare_exchange_weak(
            old_count,
            new_count,
            Ordering::Release,
            Ordering::Relaxed,
        ) {
            Ok(_) => {
                return new_count;
            }
            Err(updated_count) => {
                old_count = updated_count;
            }
        }
    }
}
