//! Page-based allocation routines.
use core::{
    ptr::NonNull,
    sync::atomic::{AtomicPtr, Ordering},
};

use crate::{
    alloc::PAGE_SIZE,
    error::{OutOfMemory, Result},
    sync::{KSpinLock, LazyLock},
};

#[expect(
    improper_ctypes,
    reason = "We only use these symbols for their addresses."
)]
unsafe extern "C" {
    safe static mut __free_ram: ();
    safe static mut __free_ram_end: ();
}

static NEXT_PTR: LazyLock<AtomicPtr<()>> =
    LazyLock::new(|| AtomicPtr::new(core::ptr::addr_of_mut!(__free_ram)));

static FREED_PAGES: FreePageList = FreePageList::new();

/// Allocate some pages, and erase the memory.
pub fn alloc_pages_zeroed(num_pages: usize) -> Result<*mut (), OutOfMemory> {
    let ptr = alloc_pages(num_pages)?;
    unsafe { ptr.write_bytes(0, num_pages * crate::page_table::PAGE_SIZE) };
    Ok(ptr)
}

/// Allocate some pages.
pub fn alloc_pages(num_pages: usize) -> Result<*mut (), OutOfMemory> {
    if let Some(alloc) = FREED_PAGES.try_pop(num_pages) {
        return Ok(alloc.as_ptr());
    }
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

/// Mark some pages as freed for later use.
pub unsafe fn free_pages(ptr: *mut (), num_pages: usize) {
    assert!(ptr.addr() % PAGE_SIZE == 0);
    FREED_PAGES.insert(ptr, num_pages);
}

struct FreePageList {
    head: KSpinLock<Option<NonNull<FreePageListNode>>>,
}
impl FreePageList {
    const fn new() -> Self {
        Self {
            head: KSpinLock::new(None),
        }
    }

    fn insert(&self, page_addr: *mut (), num_pages: usize) {
        let mut head = self.head.lock();
        let page_addr = NonNull::new(page_addr).expect("Given null page").cast();
        unsafe {
            page_addr.write(FreePageListNode {
                num_pages,
                next: *head,
            })
        };
        *head = Some(page_addr);
    }

    fn try_pop(&self, num_pages: usize) -> Option<NonNull<()>> {
        let mut head = self.head.try_lock()?;
        let mut head = &mut *head;
        loop {
            let mut page = (*head)?;
            if unsafe { page.read() }.num_pages == num_pages {
                todo!("Return these pages");
            }
            head = &mut unsafe { page.as_mut() }.next;
        }
    }
}
unsafe impl Send for FreePageList {}
unsafe impl Sync for FreePageList {}

#[repr(align(4096))]
struct FreePageListNode {
    num_pages: usize,
    next: Option<NonNull<FreePageListNode>>,
}
