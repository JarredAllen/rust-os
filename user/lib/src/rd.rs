//! Resource descriptor logic

use core::marker::PhantomData;

/// An RAII resource representing ownership over a resource descriptor.
///
/// Ownership means that this object has exclusive access (up to the borrow checker) and gets
/// closed when dropped.
pub struct OwnedResourceDescriptor {
    /// The number of this resource descriptor.
    raw: i32,
}

impl OwnedResourceDescriptor {
    /// Construct for a given raw resource descriptor.
    #[must_use]
    pub(crate) fn from_raw(raw: i32) -> Self {
        Self { raw }
    }

    /// Get the raw resource descriptor.
    #[must_use]
    pub(crate) fn raw(&self) -> i32 {
        self.raw
    }

    /// Borrow this resource descriptor.
    #[must_use]
    pub fn borrow(&self) -> BorrowedResourceDescriptor<'_> {
        BorrowedResourceDescriptor {
            raw: self.raw,
            _phantom: PhantomData,
        }
    }
}

impl Drop for OwnedResourceDescriptor {
    fn drop(&mut self) {
        crate::sys::close(self.raw);
    }
}

/// A borrow over a resource descriptor.
///
/// This type can be easily constructed from an `&OwnedResourceDescriptor`, but also might exist in
/// other contexts where the resource descriptor exists but doesn't have an
/// [`OwnedResourceDescriptor`] for the borrow checker to look at.
pub struct BorrowedResourceDescriptor<'a> {
    /// The number of this resource descriptor.
    raw: i32,
    /// Semantically, this type acts like a borrow on a resource descriptor.
    _phantom: PhantomData<&'a OwnedResourceDescriptor>,
}
impl BorrowedResourceDescriptor<'_> {
    /// Construct for a given raw resource descriptor.
    pub(crate) fn from_raw(raw: i32) -> Self {
        Self {
            raw,
            _phantom: PhantomData,
        }
    }

    /// Get the raw resource descriptor.
    pub(crate) fn raw(&self) -> i32 {
        self.raw
    }
}
impl<'a, 'b: 'a> From<&'b OwnedResourceDescriptor> for BorrowedResourceDescriptor<'a> {
    fn from(rd: &'b OwnedResourceDescriptor) -> Self {
        rd.borrow()
    }
}
