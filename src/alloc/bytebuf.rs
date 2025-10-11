use core::{ops::Deref, ptr::NonNull};

use crate::error::{OutOfMemory, Result};

/// An in-kernel buffer allocated for some number of bytes.
pub struct KByteBuf {
    /// The allocated buffer.
    buf: NonNull<[u8]>,
}
impl KByteBuf {
    /// The alignment the allocated buffer will have.
    ///
    /// This is chosen to be aligned for `u64` on any reasonable platform.
    const BUFFER_ALIGN: usize = 8;

    pub fn new_zeroed(length: usize) -> Result<Self, OutOfMemory> {
        if length == 0 {
            return Ok(Self::new());
        }
        let layout = core::alloc::Layout::from_size_align(length, Self::BUFFER_ALIGN)
            // If this returns an error, then `length` rounded up by `Self::BUFFER_ALIGN` is bigger
            // than `isize::MAX`, which is a bigger allocation than we should hand out.
            .map_err(|_| OutOfMemory)?;
        let buf = super::ALLOCATOR.allocate_inner(layout)?;
        // SAFETY: Newly-allocated memory is known to be safe for writing.
        unsafe { buf.cast::<u8>().write_bytes(0, length) };
        // And now we've initialized the memory, so we can treat it like any other slice of bytes.
        Ok(Self {
            buf: NonNull::slice_from_raw_parts(buf.cast(), length),
        })
    }

    pub fn new() -> Self {
        Self {
            buf: NonNull::from(&[]),
        }
    }

    /// Get a pointer to the start of the allocation.
    ///
    /// This pointer is valid for reading the whole buffer for the lifetime of `self`.
    pub fn as_ptr(&self) -> *const u8 {
        self.buf.as_ptr().cast()
    }
}
impl Deref for KByteBuf {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        // SAFETY:
        // This memory is initialized in the constructor, so we can read it.
        unsafe { self.buf.as_ref() }
    }
}
impl core::ops::DerefMut for KByteBuf {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY:
        // This memory is initialized in the constructor, so we can read it.
        unsafe { self.buf.as_mut() }
    }
}
impl AsRef<[u8]> for KByteBuf {
    fn as_ref(&self) -> &[u8] {
        self
    }
}
impl AsMut<[u8]> for KByteBuf {
    fn as_mut(&mut self) -> &mut [u8] {
        self
    }
}
impl Drop for KByteBuf {
    fn drop(&mut self) {
        if !self.buf.is_empty() {
            let layout =
                core::alloc::Layout::from_size_align(self.buf.len(), Self::BUFFER_ALIGN).unwrap();
            // SAFETY: For nonempty buffers, we allocated from this allocator, so we can free here,
            // too.
            unsafe { super::ALLOCATOR.deallocate_inner(self.buf.cast(), layout) };
        }
    }
}
// SAFETY: Raw bytes are always sendable.
unsafe impl Send for KByteBuf {}
// SAFETY: Raw bytes are always shareable.
unsafe impl Sync for KByteBuf {}
