//! Page table code

use core::ptr::NonNull;

/// The size of a single memory page.
pub const PAGE_SIZE: usize = 4096;

#[expect(
    improper_ctypes,
    reason = "We only use these symbols for their addresses."
)]
unsafe extern "C" {
    safe static mut __kernel_base: ();
    safe static mut __free_ram_end: ();
}

/// The number of entries in a page table.
///
/// 1024 entries in 32-bit mode.
const PAGE_TABLE_LEGNTH: usize = {
    let len = PAGE_SIZE / core::mem::size_of::<PageTableEntry>();
    assert!(len * core::mem::size_of::<PageTableEntry>() == PAGE_SIZE);
    len
};

#[repr(transparent)]
#[derive(Clone, Copy)]
struct PageTableEntry(usize);
impl PageTableEntry {
    const FLAGS_MASK: usize = 0b11111;
    const ADDR_MASK: usize = {
        let mask = (!0) << Self::ADDR_SHIFT;
        assert!(mask & Self::FLAGS_MASK == 0);
        mask
    };

    const ADDR_SHIFT: usize = 10;

    const EMPTY: Self = Self(0);

    fn from_addr_flags(addr: *mut (), flags: PageTableFlags) -> Self {
        Self(
            ((addr.addr() / PAGE_SIZE) << Self::ADDR_SHIFT) & Self::ADDR_MASK
                | usize::from(flags) & Self::FLAGS_MASK,
        )
    }

    fn addr(self) -> *mut () {
        let page_num = (self.0 & Self::ADDR_MASK) >> Self::ADDR_SHIFT;
        core::ptr::with_exposed_provenance_mut(page_num * PAGE_SIZE)
    }

    fn flags(self) -> PageTableFlags {
        PageTableFlags::from(self.0 & Self::FLAGS_MASK)
    }
}

#[repr(align(4096))]
pub struct PageTable {
    entries: [PageTableEntry; PAGE_TABLE_LEGNTH],
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PhysicalAddress(pub usize);
impl PhysicalAddress {
    /// Check whether `self` is aligned to a given alignment.
    pub fn is_aligned(self, align: usize) -> bool {
        self.0.is_multiple_of(align)
    }

    /// Make a null address.
    pub const fn null() -> Self {
        Self(0)
    }
}

bitset::bitset!(
    pub PageTableFlags(usize) {
        Valid = 0,
        Readable = 1,
        Writable = 2,
        Executable = 3,
        UserAccessible = 4,
    }
);

pub unsafe fn map_kernel_memory(table: NonNull<PageTable>) {
    /// The flags to use for kernel memory allocations.
    ///
    /// TODO Use flags to catch bugs around wrong memory types.
    const KERNEL_MEM_FLAGS: PageTableFlags = PageTableFlags::VALID
        .bit_or(PageTableFlags::READABLE)
        .bit_or(PageTableFlags::WRITABLE)
        .bit_or(PageTableFlags::EXECUTABLE);

    for paddr in (core::ptr::addr_of_mut!(__kernel_base).addr()
        ..core::ptr::addr_of_mut!(__free_ram_end).addr())
        .step_by(PAGE_SIZE)
    {
        unsafe {
            map_page(
                table,
                core::ptr::with_exposed_provenance_mut(paddr),
                PhysicalAddress(paddr),
                KERNEL_MEM_FLAGS,
            )
        };
    }
}

pub unsafe fn map_page(
    mut table: NonNull<PageTable>,
    vaddr: *mut (),
    paddr: PhysicalAddress,
    flags: PageTableFlags,
) {
    assert!(
        paddr.is_aligned(PAGE_SIZE),
        "Unaligned physical address 0x{:X}",
        paddr.0,
    );
    assert!(
        vaddr.addr().is_multiple_of(PAGE_SIZE),
        "Unaligned virtual address 0x{:X}",
        vaddr.addr(),
    );

    let vpn1 = (vaddr.addr() >> 22) & 0x3ff;

    let table = unsafe { table.as_mut() };
    if !table.entries[vpn1].flags().valid() {
        let new_page = crate::alloc::alloc_pages(1);
        table.entries[vpn1] = PageTableEntry::from_addr_flags(new_page, PageTableFlags::VALID);
        unsafe {
            new_page.cast::<PageTable>().write(PageTable {
                entries: [PageTableEntry::EMPTY; PAGE_TABLE_LEGNTH],
            })
        };
    }
    let table0 = unsafe { &mut *table.entries[vpn1].addr().cast::<PageTable>() };

    let vpn0 = (vaddr.addr() >> 12) & 0x3ff;
    table0.entries[vpn0] = PageTableEntry::from_addr_flags(vaddr, flags | PageTableFlags::VALID);
}
