//! Memory allocator for the kernel.

use core::{
    alloc::GlobalAlloc,
    cell::UnsafeCell,
    mem::MaybeUninit,
    ops::Deref,
    ptr::NonNull,
    sync::atomic::{AtomicBool, AtomicPtr, Ordering},
};

use crate::error::{OutOfMemory, Result};

#[expect(
    improper_ctypes,
    reason = "We only use these symbols for their addresses."
)]
unsafe extern "C" {
    safe static mut __free_ram: ();
    safe static mut __free_ram_end: ();
}

/// The size of a single page in memory.
const PAGE_SIZE: usize = 4096;

pub static ALLOCATOR: KPageAlloc = KPageAlloc::new();

/// An allocator which gives out by page.
///
/// This is very inefficient as every allocation uses 4kB of memory.
pub struct KPageAlloc {
    /// The most recently freed page.
    free_list_head: AtomicPtr<FreePageListEntry>,
}

impl KPageAlloc {
    /// Create a new allocator.
    pub const fn new() -> Self {
        Self {
            free_list_head: AtomicPtr::new(core::ptr::null_mut()),
        }
    }

    /// Attempt to allocate new memory.
    ///
    /// Unlike the [`GlobalAlloc::alloc`] trait, this method allows for ZSTs, in which case the
    /// returned allocation is aligned and has no other guarantees.
    ///
    /// On success, this returns the entire allocated slice of memory (which may be larger than the
    /// request). All of the returned memory is valid for reading and writing, but it may not be
    /// initialized.
    ///
    /// This method can only fail if there is no available memory to do the allocation.
    pub fn alloc(&self, layout: core::alloc::Layout) -> Result<NonNull<[u8]>, OutOfMemory> {
        if layout.size() > PAGE_SIZE || layout.align() > PAGE_SIZE {
            todo!("Support allocations larger than a page");
        }
        loop {
            let old_head = self.free_list_head.load(Ordering::Relaxed);
            let Some(old_head) = NonNull::new(old_head) else {
                let Ok(alloc) = alloc_pages(1) else {
                    return Err(OutOfMemory);
                };
                return Ok(NonNull::slice_from_raw_parts(
                    NonNull::new(alloc).unwrap().cast(),
                    PAGE_SIZE,
                ));
            };
            // Note: this is not actually thread-safe since another thread might take the same page
            // and start writing to it. I need some sort of locking structure before we can make
            // this kernel multi-threaded.
            let new_head = unsafe { old_head.read() }.next;
            if self
                .free_list_head
                .compare_exchange_weak(
                    old_head.as_ptr(),
                    new_head,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                )
                .is_ok()
            {
                return Ok(NonNull::slice_from_raw_parts(old_head.cast(), PAGE_SIZE));
            }
        }
    }

    /// Free the given allocation.
    ///
    /// # Safety
    /// `ptr` must point to the same address returned by [`Self::alloc`], and `layout` must be
    /// identical to the layout used for the initial allocation.
    unsafe fn dealloc(&self, ptr: NonNull<()>, layout: core::alloc::Layout) {
        if layout.size() > PAGE_SIZE || layout.align() > PAGE_SIZE {
            todo!("Allocations larger than a page");
        }
        loop {
            let old_head = self.free_list_head.load(Ordering::Relaxed);
            let ptr = ptr.cast::<FreePageListEntry>();
            unsafe { ptr.write(FreePageListEntry { next: old_head }) };
            if self
                .free_list_head
                .compare_exchange_weak(old_head, ptr.as_ptr(), Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
            {
                break;
            }
        }
    }
}

unsafe impl GlobalAlloc for KPageAlloc {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        let Ok(mem) = self.alloc(layout) else {
            return core::ptr::null_mut();
        };
        mem.as_ptr().cast()
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: core::alloc::Layout) {
        let ptr = NonNull::new(ptr)
            .expect("Tried to free null pointer")
            .cast();
        // SAFETY: The trait's preconditions match this method's.
        unsafe { self.dealloc(ptr, layout) };
    }
}

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
        let layout = core::alloc::Layout::from_size_align(length, Self::BUFFER_ALIGN).unwrap();
        let buf = ALLOCATOR.alloc(layout)?;
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
}
impl core::ops::Deref for KByteBuf {
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
impl core::convert::AsRef<[u8]> for KByteBuf {
    fn as_ref(&self) -> &[u8] {
        self.deref()
    }
}
impl core::convert::AsMut<[u8]> for KByteBuf {
    fn as_mut(&mut self) -> &mut [u8] {
        use core::ops::DerefMut;
        self.deref_mut()
    }
}
impl Drop for KByteBuf {
    fn drop(&mut self) {
        if !self.buf.is_empty() {
            let layout =
                core::alloc::Layout::from_size_align(self.buf.len(), Self::BUFFER_ALIGN).unwrap();
            // SAFETY: For nonempty buffers, we allocated from this allocator, so we can free here,
            // too.
            unsafe { ALLOCATOR.dealloc(self.buf.cast(), layout) };
        }
    }
}

/// An entry in the free page list for [`KPageAlloc`].
struct FreePageListEntry {
    /// The pointer to the next entry, if there is one.
    next: *mut FreePageListEntry,
}

/// Allocate some pages, and erase the memory.
pub fn alloc_pages_zeroed(num_pages: usize) -> Result<*mut (), OutOfMemory> {
    let ptr = alloc_pages(num_pages)?;
    unsafe { ptr.write_bytes(0, num_pages * crate::page_table::PAGE_SIZE) };
    Ok(ptr)
}

/// Allocate some pages.
pub fn alloc_pages(num_pages: usize) -> Result<*mut (), OutOfMemory> {
    static NEXT_PTR: LazyLock<AtomicPtr<()>> =
        LazyLock::new(|| AtomicPtr::new(core::ptr::addr_of_mut!(__free_ram)));

    loop {
        let head = NEXT_PTR.load(Ordering::Relaxed);
        log::debug!("Trying to allocate {num_pages} pages at {:X}", head.addr());
        let new_next =
            head.wrapping_byte_add(PAGE_SIZE.checked_mul(num_pages).expect("alloc too big"));
        if new_next > core::ptr::addr_of_mut!(__free_ram_end) {
            return Err(OutOfMemory);
        };
        if NEXT_PTR
            .compare_exchange_weak(head, new_next, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
        {
            break Ok(head);
        }
    }
}

struct LazyLock<T, F = fn() -> T> {
    value: UnsafeCell<MaybeUninit<T>>,
    init_func: UnsafeCell<MaybeUninit<F>>,
    started: AtomicBool,
    finished: AtomicBool,
}
impl<T, F> LazyLock<T, F> {
    const fn new(f: F) -> Self {
        Self {
            value: UnsafeCell::new(MaybeUninit::uninit()),
            init_func: UnsafeCell::new(MaybeUninit::new(f)),
            started: AtomicBool::new(false),
            finished: AtomicBool::new(false),
        }
    }

    fn force(&self) -> &T
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
