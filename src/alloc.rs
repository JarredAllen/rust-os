//! Memory allocator for the kernel.

mod bytebuf;
mod page;
mod raw;
mod rc;

pub use bytebuf::KByteBuf;
pub use page::{alloc_pages, alloc_pages_zeroed, free_pages};
pub use rc::KrcBox;

/// The size of a single page in memory.
const PAGE_SIZE: usize = 4096;

pub static ALLOCATOR: raw::KAllocator = raw::KAllocator::new();
