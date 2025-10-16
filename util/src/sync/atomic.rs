//! An implementation of an atomic type.

pub use core::sync::atomic::Ordering;

use core::{
    mem::{align_of, size_of},
    sync::atomic as std_atomic,
};

use core::cell::UnsafeCell;

/// An atomic wrapper around custom types.
///
/// ```
/// # use util::sync::atomic::{Atomic, Ordering};
/// # use bytemuck::NoUninit;
/// #[derive(NoUninit, Debug, PartialEq, Eq, Copy, Clone)]
/// #[repr(u8)]
/// enum Values {
///     A, B, C,
/// };
/// let atomic = Atomic::new(Values::A);
/// assert_eq!(atomic.load(Ordering::Relaxed), Values::A);
/// assert_eq!(atomic.swap(Values::B, Ordering::Relaxed), Values::A);
/// assert_eq!(atomic.load(Ordering::Relaxed), Values::B);
/// ```
///
/// This implementation requires that the inner type have no uninitialized bytes in its memory
/// layout (which is checked by the [`bytemuck::NoUninit`] requirement on the relevant methods) and
/// that the target machine have atomic operations for the given size and alignment (which can only
/// be checked post-monomorphization for the specific type and target).
///
/// ```compile_fail
/// # use util::sync::atomic::{Atomic, Ordering};
/// let atomic = Atomic::new([0_u8; 256]);
/// // Doesn't compile because it's too big.
/// let _ = atomic.load(Ordering::Relaxed);
/// ```
///
/// Of note, composite types will likely need a more strict alignment enforced to be used here:
/// ```
/// # use util::sync::atomic::{Atomic, Ordering};
/// # use bytemuck::NoUninit;
/// #[derive(NoUninit, Clone, Copy)]
/// // Wider alignment needed to match `u32` on most targets
/// #[repr(C, align(4))]
/// struct TestDatum {
///    a: u16,
///    b: u8,
///    c: u8,
/// }
/// let atomic = Atomic::from(TestDatum { a: 0, b: 0, c: 0 });
/// // Requires the larger alignment to compile:
/// let _ = atomic.load(Ordering::Relaxed);
/// ```
pub struct Atomic<T> {
    /// The inner value.
    inner: UnsafeCell<T>,
}

impl<T> Atomic<T> {
    /// Construct a new value starting in the given state.
    pub const fn new(value: T) -> Self {
        Self {
            inner: UnsafeCell::new(value),
        }
    }

    /// Deconstruct the value.
    pub fn into_inner(self) -> T {
        self.inner.into_inner()
    }

    /// Convert to an exclusive reference to the inner value.
    ///
    /// Calling this method requires the borrow checker prove exclusive access, so this method
    /// doesn't involve atomicity operations.
    pub fn get_mut(&mut self) -> &mut T {
        self.inner.get_mut()
    }

    /// Get a pointer to the inner value.
    const fn as_ptr(&self) -> *const T {
        self.inner.get()
    }

    /// Get the inner value as a reference to the given type.
    ///
    /// # Safety
    /// The result of casting [`Self::as_ptr`] to `U` must result in a pointer which can be safely
    /// converted into a shared reference.
    unsafe fn inner_as_ref<U>(&self) -> &U {
        // SAFETY:
        // By function precondition, this casted pointer is valid to turn into a reference
        unsafe { &*self.as_ptr().cast::<U>() }
    }
}

impl<T: bytemuck::NoUninit> Atomic<T> {
    defer_to_inner!(
        /// Load a value from the atomic.
        ///
        /// # Panics
        /// Panics if `order` is `Release` or `AcqRel`.
        pub fn load(&self, ordering: Ordering) -> T;

        /// Store a value into the atomic.
        ///
        /// # Panics
        /// Panics if `order` is `Aquire` or `AcqRel`.
        pub fn store(&self; value: T, ordering: Ordering);

        /// Store the given value into the atomic and return the old value.
        ///
        /// All `ordering` values are allowed, and the parts that apply to the load and store part are
        /// applied to those parts.
        pub fn swap(&self; value: T, ordering: Ordering) -> T;
    );

    /// Stores a value if the current value is the same as `current`.
    ///
    /// The return value is a result indicating whether the new value was written and containing
    /// the previous value. On success, this value is guaranteed to match `current`.
    ///
    /// Unlike [`Self::compare_exchange`], this function is allowed to spuriously fail even if the
    /// comparison succeeds, which can result in more efficient code on some platforms.
    ///
    /// `compare_exchange` takes two [`Ordering`] arguments to describe the memory ordering of this
    /// operation. `success` describes the required ordering for the read-modify-write operation that
    /// takes place if the comparison. `failure` describes the required ordering for the load
    /// operation that takes place when the comparison fails.
    pub fn compare_exchange_weak(
        &self,
        current: T,
        new: T,
        success: Ordering,
        failure: Ordering,
    ) -> Result<T, T> {
        dispatch_atomic!(
            || T => Atomic {
                // SAFETY:
                // `Atomic` is chosen to make this safe.
                unsafe { self.inner_as_ref::<Atomic>() }
                    .compare_exchange_weak(
                        bytemuck::cast(current),
                        bytemuck::cast(new),
                        success,
                        failure,
                    )
                    // SAFETY:
                    // `Atomic` is chosen to make this safe.
                    .map(|res| unsafe { core::mem::transmute_copy(&res) })
                    // SAFETY:
                    // `Atomic` is chosen to make this safe.
                    .map_err(|err_res| unsafe { core::mem::transmute_copy(&err_res) })
            }
        )
    }

    /// Stores a value if the current value is the same as `current`.
    ///
    /// The return value is a result indicating whether the new value was written and containing
    /// the previous value. On success, this value is guaranteed to match `current`.
    ///
    /// `compare_exchange` takes two [`Ordering`] arguments to describe the memory ordering of this
    /// operation. `success` describes the required ordering for the read-modify-write operation that
    /// takes place if the comparison. `failure` describes the required ordering for the load
    /// operation that takes place when the comparison fails.
    pub fn compare_exchange(
        &self,
        current: T,
        new: T,
        success: Ordering,
        failure: Ordering,
    ) -> Result<T, T> {
        dispatch_atomic!(
            || T => Atomic {
                // SAFETY:
                // `Atomic` is chosen to make this safe.
                unsafe { self.inner_as_ref::<Atomic>() }
                    .compare_exchange(
                        bytemuck::cast(current),
                        bytemuck::cast(new),
                        success,
                        failure,
                    )
                    // SAFETY:
                    // `Atomic` is chosen to make this safe.
                    .map(|res| unsafe { core::mem::transmute_copy(&res) })
                    // SAFETY:
                    // `Atomic` is chosen to make this safe.
                    .map_err(|err_res| unsafe { core::mem::transmute_copy(&err_res) })
            }
        )
    }

    /// Attempt to update the value according to a function, if no other thread changes it in the
    /// meantime.
    ///
    /// The return value is a result indicating whether the new value was written and containing
    /// the previous value. On success, this value is guaranteed to match `current`.
    ///
    /// This function will only return `Err` if another thread modifies the value while the
    /// function is running.
    ///
    /// This method is not magic; it is not provided by the hardware, and does not act like a
    /// critical section or mutex. In particular, if another thread changes the value one or more
    /// times, but the value ends up matching what it was when this thread read it, then the write
    /// will still go through.
    pub fn update_weak(
        &self,
        set_order: Ordering,
        fetch_order: Ordering,
        update: impl FnOnce(T) -> T,
    ) -> Result<T, T> {
        let fetched = self.load(fetch_order);
        let new = update(fetched);
        self.compare_exchange(fetched, new, set_order, set_order)
    }

    /// Update the value according to a function, returning the old value.
    ///
    /// Unlike [`Self::update_weak`], the function will be called as many times as it takes for the
    /// `compare_exchange` after the function to succeed. Notably, this means multiple threads
    /// calling this function at the same time can cause a quadratic amount of work.
    ///
    /// This method is not magic; it is not provided by the hardware, and does not act like a
    /// critical section or mutex. In particular, if another thread changes the value one or more
    /// times, but the value ends up matching what it was when this thread read it, then the write
    /// will still go through.
    pub fn update(
        &self,
        set_order: Ordering,
        fetch_order: Ordering,
        mut update: impl FnMut(T) -> T,
    ) -> T {
        loop {
            if let Ok(old_value) = self.update_weak(set_order, fetch_order, &mut update) {
                return old_value;
            }
        }
    }
}

/// Methods involving bit-wise manipulation.
///
/// These methods require the type to be [`bytemuck::Pod`] because they
impl<T: bytemuck::Pod> Atomic<T> {
    defer_to_inner!(
        /// Bitwise and with the current value.
        ///
        /// The stored value is set to the result, and the old value is returned.
        pub fn fetch_and(&self; value: T, ordering: Ordering) -> T;

        /// Bitwise nand with the current value.
        ///
        /// The stored value is set to the result, and the old value is returned.
        pub fn fetch_nand(&self; value: T, ordering: Ordering) -> T;

        /// Bitwise or with the current value.
        ///
        /// The stored value is set to the result, and the old value is returned.
        pub fn fetch_or(&self; value: T, ordering: Ordering) -> T;

        /// Bitwise xor with the current value.
        ///
        /// The stored value is set to the result, and the old value is returned.
        pub fn fetch_xor(&self; value: T, ordering: Ordering) -> T;
    );
}

/// Defer an up-toone-input and up-to-one-output function to the inner atomic.
///
/// This macro abstracts over a common pattern used by many, but not all, methods, and is meant to
/// strike a balance of having a simple implementation with also being useful to a large amount of
/// these methods.
macro_rules! defer_to_inner {
    ($(
        $( #[$meta:meta] )*
        $pub:vis fn $func_name:ident(&self $( ; $value_arg:ident : T )?, ordering: Ordering $(,)?) $(-> $ret_ty:ty)?;
    )*) => {$(
        $( #[$meta] )*
        $pub fn $func_name(&self, $($value_arg: T,)? ordering: Ordering) $( -> $ret_ty)? {
            dispatch_atomic!(
                || T => Atomic {
                    #[allow(unused, reason = "Only used for some macro arguments")]
                    // SAFETY:
                    // `Atomic` is chosen to make this safe.
                    let raw = unsafe { self.inner_as_ref::<Atomic>() }
                        .$func_name(
                            $( bytemuck::cast($value_arg), )?
                            ordering,
                        );
                    $(
                        // SAFETY:
                        // We ensure the inner value is always a valid `T`, so we can cast the
                        // return value back.
                        unsafe { core::mem::transmute_copy::<_, $ret_ty>(&raw) }
                    )?
                }
            )
        }
    )*};
}
use defer_to_inner;

impl<T> From<T> for Atomic<T> {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

/// A helper macro for dispatching to an atomic.
///
/// The body of `$impl` must compile and type check for all possible atomic values, but will only
/// run (and is only expected to be safe) for the one that matches.
macro_rules! dispatch_atomic {
    // Perform the given operation.
    (|| $user_ty:ty => $atomic:ident $impl:expr) => {{
        const { dispatch_atomic!(ASSERT_ATOMIC_ABLE $user_ty) };
        match size_of::<$user_ty>() {
            #[cfg(target_has_atomic = "8")]
            1 if align_of::<$user_ty>() >= align_of::<std_atomic::AtomicU8>() => {
                type $atomic = std_atomic::AtomicU8;
                $impl
            }
            #[cfg(target_has_atomic = "16")]
            2 if align_of::<$user_ty>() >= align_of::<std_atomic::AtomicU16>() => {
                type $atomic = std_atomic::AtomicU16;
                $impl
            }
            #[cfg(target_has_atomic = "32")]
            4 if align_of::<$user_ty>() >= align_of::<std_atomic::AtomicU32>() => {
                type $atomic = std_atomic::AtomicU32;
                $impl
            }
            #[cfg(target_has_atomic = "64")]
            8 if align_of::<$user_ty>() >= align_of::<std_atomic::AtomicU64>() => {
                type $atomic = std_atomic::AtomicU64;
                $impl
            }
            _ => unreachable!("Atomic operations for type not available, should have been caught at compile time"),

        }
    }};
    // Assert that the given type can be used as an atomic (this is automatically used by the other
    // branch).
    (ASSERT_ATOMIC_ABLE $user_ty:ty) => {
        match size_of::<$user_ty>() {
            #[cfg(target_has_atomic = "8")]
            1 if align_of::<$user_ty>() >= align_of::<std_atomic::AtomicU8>() => {}
            #[cfg(target_has_atomic = "16")]
            2 if align_of::<$user_ty>() >= align_of::<std_atomic::AtomicU16>() => {}
            #[cfg(target_has_atomic = "32")]
            4 if align_of::<$user_ty>() >= align_of::<std_atomic::AtomicU32>() => {}
            #[cfg(target_has_atomic = "64")]
            8 if align_of::<$user_ty>() >= align_of::<std_atomic::AtomicU64>() => {}
            _ => panic!("Atomic operations for type not available on current target"),

        }
    };
}
use dispatch_atomic;
