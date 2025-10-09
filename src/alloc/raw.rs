//! An allocator implementation.
//!
//! See [`ALLOCATOR`] for details on the global allocator.

use core::{alloc::GlobalAlloc, ptr::NonNull};

use crate::{alloc::PAGE_SIZE, error::OutOfMemory, sync::KSpinLock};

/// An implementation of an allocator.
///
/// This allocator has specific size classes for powers of two up to a page size, beyond which the
/// backing memory is `mmap`ed. This comes with a relatively high potential for overhead, since
/// almost half of the assigned memory can go unused if allocations at the wrong size are chosen.
///
/// This allocator is thread-safe, but may have poor performance if several threads attempt to use
/// it to allocate memory at the same time.
pub struct KAllocator {
    /// Each size class gets its own separate logic.
    classes: [KSpinLock<FixedSizeAllocator>; NUM_SIZE_CLASSES],
}
impl KAllocator {
    /// Create a new allocator.
    pub const fn new() -> Self {
        Self {
            classes: [const { KSpinLock::new(FixedSizeAllocator::new()) }; NUM_SIZE_CLASSES],
        }
    }

    /// Request to allocate for a given layout.
    ///
    /// The given allocation (which may be larger than requested) is returned as a slice.
    ///
    /// This function may return `None` if we're out of memory and attempting to allocate more
    /// fails.
    pub(super) fn allocate_inner(
        &self,
        layout: core::alloc::Layout,
    ) -> Result<NonNull<[u8]>, OutOfMemory> {
        if layout.size() == 0 {
            return Ok(NonNull::slice_from_raw_parts(
                NonNull::without_provenance(
                    // SAFETY:
                    // Alignments can't ever be zero.
                    unsafe { core::num::NonZero::new_unchecked(layout.align()) },
                ),
                0,
            ));
        }
        if layout.align() > PAGE_SIZE {
            todo!("Support allocations with > page alignment");
        }
        let size = layout.size().max(layout.align());
        let Some((size_class, raw_size)) = class_for_size(size) else {
            let num_pages = size.div_ceil(PAGE_SIZE);
            return Ok(NonNull::slice_from_raw_parts(
                unsafe { NonNull::new_unchecked(super::alloc_pages(num_pages)?) }.cast(),
                num_pages * PAGE_SIZE,
            ));
        };
        // SAFETY:
        // `class_for_size` always returns the same size for a given size class, so we meet the
        // precondition.
        let head_ptr = unsafe { self.classes[size_class].lock().allocate(raw_size) }?;
        Ok(NonNull::slice_from_raw_parts(head_ptr.cast(), raw_size))
    }

    /// Deallocate a given allocation.
    ///
    /// # Safety
    /// `ptr` must have been returned from [`Self::allocate_inner`] with the given layout.
    pub(super) unsafe fn deallocate_inner(&self, ptr: NonNull<()>, layout: core::alloc::Layout) {
        if layout.size() == 0 {
            return;
        }
        let size = layout.size().max(layout.align());
        let Some((size_class, _raw_size)) = class_for_size(size) else {
            todo!("Free an `mmap`-backed allocation");
        };
        // SAFETY:
        // We allocated from the same size class originally.
        unsafe { self.classes[size_class].lock().deallocate(ptr) };
    }
}

impl Default for KAllocator {
    fn default() -> Self {
        Self::new()
    }
}

unsafe impl GlobalAlloc for KAllocator {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        self.allocate_inner(layout)
            .map_or(core::ptr::null_mut(), |ptr| ptr.cast::<u8>().as_ptr())
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: core::alloc::Layout) {
        let ptr = unsafe { NonNull::new_unchecked(ptr) }.cast();
        // SAFETY:
        // By method precondition, this pointer came from `self.alloc(layout)`, so we can
        // deallocate it.
        unsafe { self.deallocate_inner(ptr, layout) };
    }
}

/// The smallest size class we make a separate allocation for.
///
/// Allocations smaller than this limit get rounded up to this value.
const MIN_SIZE_CLASS: usize = 16;

/// The largest size class we make a seprate allocation for.
///
/// Allocations larger than this limit get a direct `mmap` call.
const MAX_SIZE_CLASS: usize = 2048;

/// The number of distinct size classes to handle.
const NUM_SIZE_CLASSES: usize = {
    let num = (MAX_SIZE_CLASS / MIN_SIZE_CLASS).ilog2() as usize + 1;
    assert!(MIN_SIZE_CLASS << (num - 1) == MAX_SIZE_CLASS);
    num
};

/// Get the size class and raw allocation size for this pointer.
///
/// The first element is the size class index and the second number is the raw allocation size.
fn class_for_size(size: usize) -> Option<(usize, usize)> {
    if size > MAX_SIZE_CLASS {
        return None;
    }
    let rounded_size = size.next_power_of_two().max(MIN_SIZE_CLASS);
    Some((
        (rounded_size / MIN_SIZE_CLASS).ilog2() as usize,
        rounded_size,
    ))
}

/// An allocator which only ever allocates blocks of a given size.
struct FixedSizeAllocator {
    /// A pointer to a list of "freed" blocks which we can reuse.
    free_list: Option<NonNull<FreeListNode>>,
    /// A pointer to the next "fresh" address to allocate from.
    fresh_head: *mut (),
}
impl FixedSizeAllocator {
    /// Create a new fixed-size allocator with no backing memory yet.
    const fn new() -> Self {
        Self {
            free_list: None,
            fresh_head: core::ptr::null_mut(),
        }
    }

    /// Get a new allocation of the given size.
    ///
    /// This function may return `None` if we're out of memory and attempting to allocate more
    /// fails.
    ///
    /// # Safety
    /// This function may only be called with one value of `size` for a given
    /// [`FixedSizeAllocator`].
    unsafe fn allocate(&mut self, size: usize) -> Result<NonNull<()>, OutOfMemory> {
        assert!(size >= core::mem::size_of::<FreeListNode>());
        if let Some(free_head) = self.free_list {
            self.free_list = unsafe { free_head.as_ref() }.next;
            return Ok(free_head.cast());
        }
        if self.fresh_head.addr().is_multiple_of(4096) {
            self.fresh_head = super::alloc_pages(1)?;
        }
        // SAFETY:
        // Null pointers are a multiple of 4096, so we'd hit the above branch and grab a new
        // page to use.
        let ret_ptr = unsafe { NonNull::new_unchecked(self.fresh_head) };
        self.fresh_head = self.fresh_head.wrapping_byte_add(size);
        Ok(ret_ptr)
    }

    /// Free the given pointer.
    ///
    /// # Safety
    /// This pointer must have been returned by [`Self::allocate`] called on this object. This
    /// function takes ownership over the allocation, so the pointer must not be used again except
    /// through this allocator returning it again from [`Self::allocate`].
    unsafe fn deallocate(&mut self, ptr: NonNull<()>) {
        let ptr = ptr.cast::<FreeListNode>();
        unsafe {
            ptr.write(FreeListNode {
                next: self.free_list,
            })
        };
        self.free_list = Some(ptr);
    }
}
unsafe impl Send for FixedSizeAllocator {}

struct FreeListNode {
    next: Option<NonNull<Self>>,
}
