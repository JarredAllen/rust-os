//! Memory allocator for the kernel.

mod bytebuf;
mod raw;

pub use bytebuf::KByteBuf;

use core::{
    cell::UnsafeCell,
    mem::MaybeUninit,
    ops::Deref,
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

pub static ALLOCATOR: raw::KAllocator = raw::KAllocator::new();

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
