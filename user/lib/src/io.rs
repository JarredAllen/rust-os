//! Utilities for input/output

use core::{fmt, sync::atomic::AtomicBool};

use crate::rd::BorrowedResourceDescriptor;

/// Write to standard output.
#[macro_export]
macro_rules! print {
    ($( $args:tt )*) => {{
        use ::core::fmt::Write;
        if let Some(mut writer) = $crate::io::Stdout::try_lock() {
            _ = ::core::write!(writer, $( $args )*);
        }
    }};
}

/// Write to standard output.
#[macro_export]
macro_rules! println {
    ($( $args:tt )*) => {{
        use ::core::fmt::Write;
        if let Some(mut writer) = $crate::io::Stdout::try_lock() {
            _ = ::core::writeln!(writer, $( $args )*);
        }
    }};
}

/// Temporary ownership over the standard output stream.
#[must_use = "`Stdout` objects are only useful for writing to"]
pub struct Stdout<'a> {
    rd: BorrowedResourceDescriptor<'a>,
}
impl Stdout<'_> {
    /// Lock the standard output stream so writing can happen.
    ///
    /// If another copy of `Self` exists anywhere, this method will panic. See [`Self::try_lock`]
    /// for a panic-free alternative.
    pub fn lock() -> Self {
        Self::try_lock().expect("Failed to lock stdout - is there another instance?")
    }

    /// Attempt to lock the standard output stream.
    ///
    /// This method returns `None` if the output stream is already locked. See [`Self::lock`] for
    /// an alternative that panics.
    pub fn try_lock() -> Option<Self> {
        if STDOUT_LOCK.swap(true, core::sync::atomic::Ordering::Acquire) {
            None
        } else {
            Some(Self {
                rd: BorrowedResourceDescriptor::from_raw(1),
            })
        }
    }

    /// Forcibly lock the standard output stream.
    ///
    /// # Safety
    /// Calling this method when other instances of [`Stdout`] exist may lead to undefined behavior
    /// if those other instances will have any methods called on them in the future (including the
    /// [`Drop::drop`] destructor).
    pub unsafe fn force_lock() -> Self {
        STDOUT_LOCK.store(true, core::sync::atomic::Ordering::Relaxed);
        Self {
            rd: BorrowedResourceDescriptor::from_raw(1),
        }
    }
}
impl Drop for Stdout<'_> {
    fn drop(&mut self) {
        STDOUT_LOCK.store(false, core::sync::atomic::Ordering::Release);
    }
}
impl fmt::Write for Stdout<'_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let mut s = s.as_bytes();
        while !s.is_empty() {
            let len = crate::sys::write(self.rd.raw(), s).map_err(|_| fmt::Error)?;
            s = &s[len..];
        }
        Ok(())
    }
}

/// A lock for [`Stdout`], to ensure there aren't conflicting claims.
static STDOUT_LOCK: AtomicBool = AtomicBool::new(false);
