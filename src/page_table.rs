//! Page table code

use core::ptr::NonNull;

use crate::error::{OutOfMemory, Result};

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
    let len = PAGE_SIZE / size_of::<PageTableEntry>();
    assert!(len * size_of::<PageTableEntry>() == PAGE_SIZE);
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

    fn from_addr_flags(addr: PhysicalAddress, flags: PageTableFlags) -> Self {
        Self(
            ((addr.0 / PAGE_SIZE) << Self::ADDR_SHIFT) & Self::ADDR_MASK
                | usize::from(flags) & Self::FLAGS_MASK,
        )
    }

    fn physical_addr(self) -> PhysicalAddress {
        let page_num = (self.0 & Self::ADDR_MASK) >> Self::ADDR_SHIFT;
        PhysicalAddress(page_num * PAGE_SIZE)
    }

    fn flags(self) -> PageTableFlags {
        PageTableFlags::from(self.0 & Self::FLAGS_MASK)
    }
}

#[repr(align(4096))]
pub struct PageTable {
    entries: [PageTableEntry; PAGE_TABLE_LEGNTH],
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
#[repr(transparent)]
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

    pub const fn byte_add(self, offset: usize) -> Self {
        Self(self.0 + offset)
    }
}

bitset::bitset!(
    /// The flags on a [`PageTableEntry`].
    pub PageTableFlags(usize) {
        /// The page is valid.
        Valid = 0,
        /// The page is readable.
        Readable = 1,
        /// The page is writable.
        Writable = 2,
        /// The page is executable.
        Executable = 3,
        /// User-mode code should have access to this page.
        ///
        /// If the `SUM` bit isn't set, then this bit denies the kernel access to it (see
        /// [`crate::csr::AllowUserModeMemory`] for more details).
        UserAccessible = 4,
    }
);

/// Map kernel memory into the given page table.
///
/// # Safety
/// This writes to the given page table, which must not interfere with rust's understanding of
/// memory.
///
/// Also, because this method is in the memory that it maps itself into, you really should call
/// this method on a page table before setting it to be active.
pub unsafe fn map_kernel_memory(table: NonNull<PageTable>) -> Result<(), OutOfMemory> {
    /// The flags to use for kernel memory allocations.
    ///
    /// TODO Use flags to catch bugs around wrong memory types.
    const KERNEL_MEM_FLAGS: PageTableFlags = PageTableFlags::READABLE
        .bit_or(PageTableFlags::WRITABLE)
        .bit_or(PageTableFlags::EXECUTABLE);

    for paddr in (core::ptr::addr_of_mut!(__kernel_base).addr()
        ..core::ptr::addr_of_mut!(__free_ram_end).addr())
        .step_by(PAGE_SIZE)
    {
        // SAFETY: Outer method preconditions match inner method's.
        unsafe {
            map_page(
                table,
                core::ptr::with_exposed_provenance_mut(paddr),
                PhysicalAddress(paddr),
                KERNEL_MEM_FLAGS,
            )
        }?;
    }
    // Map the virtio block device
    // SAFETY: Outer method preconditions match inner method's.
    unsafe {
        map_page(
            table,
            core::ptr::with_exposed_provenance_mut(crate::virtio::BLOCK_DEVICE_ADDRESS),
            PhysicalAddress(crate::virtio::BLOCK_DEVICE_ADDRESS),
            PageTableFlags::READABLE.bit_or(PageTableFlags::WRITABLE),
        )
    }?;
    // Map the virtio entropy device
    // SAFETY: Outer method preconditions match inner method's.
    unsafe {
        map_page(
            table,
            core::ptr::with_exposed_provenance_mut(crate::virtio::RNG_DEVICE_ADDRESS),
            PhysicalAddress(crate::virtio::RNG_DEVICE_ADDRESS),
            PageTableFlags::READABLE.bit_or(PageTableFlags::WRITABLE),
        )
    }?;
    Ok(())
}

/// Allocate new memory to back `data` and map it with the given flags.
///
/// # Safety
/// This writes to the given page table, which must not interfere with rust's understanding of
/// memory.
pub unsafe fn alloc_and_map_slice(
    table: NonNull<PageTable>,
    start_vaddr: PhysicalAddress,
    data: &[u8],
    flags: PageTableFlags,
) -> Result<(), OutOfMemory> {
    let new_pages = crate::alloc::alloc_pages(data.len().div_ceil(PAGE_SIZE))?;
    for (paddr, (vaddr, data)) in (new_pages.addr()..).step_by(PAGE_SIZE).zip(
        (start_vaddr.0..)
            .step_by(PAGE_SIZE)
            .zip(data.chunks(PAGE_SIZE)),
    ) {
        // SAFETY: Outer method preconditions match inner method's.
        unsafe {
            map_page(
                table,
                core::ptr::without_provenance_mut(vaddr),
                PhysicalAddress(paddr),
                flags,
            )
        }?;
        // SAFETY: We just allocated the page, so we can write to it.
        //
        // We write to `paddr` because it's also the address in kernel memory.
        let page = unsafe { &mut *core::ptr::with_exposed_provenance_mut::<[u8; 4096]>(paddr) };
        page[..data.len()].copy_from_slice(data);
    }
    Ok(())
}

/// Get the page table entry for the given virtual address.
fn entry_for_vaddr(vaddr: *const ()) -> Option<PageTableEntry> {
    if let Some(page_table) = crate::csr::current_page_table() {
        let vaddr = vaddr.addr();
        let vpn1 = (vaddr >> 22) & 0x3ff;
        let vpn2 = (vaddr >> 12) & 0x3ff;
        // SAFETY:
        // If `current_page_table` isn't a valid page table, we've already had bigger problems.
        let entry1 = unsafe { page_table.as_ref() }.entries[vpn1];
        if !entry1.flags().valid() {
            // The page wasn't set up.
            return None;
        }
        if !entry1.flags().contains_any(
            PageTableFlags::READABLE
                | PageTableFlags::WRITABLE
                | PageTableFlags::EXECUTABLE
                | PageTableFlags::USER_ACCESSIBLE,
        ) {
            todo!("Handle large pages");
        }
        let table0 = core::ptr::without_provenance::<PageTable>(entry1.physical_addr().0);
        // SAFETY:
        // If `current_page_table` isn't a valid page table, we've already had bigger problems.
        Some(unsafe { &*table0 }.entries[vpn2])
    } else {
        None
    }
}

/// Get the physical address for a given virtual address.
#[inline(never)]
pub fn paddr_for_vaddr<T: ?Sized>(vaddr: *mut T) -> PhysicalAddress {
    if crate::csr::current_page_table().is_some() {
        let Some(page_table_entry) = entry_for_vaddr(vaddr.cast()) else {
            todo!("Handle `vaddr` without a paddr");
        };
        let offset_in_page = vaddr.addr() & 0xfff;
        page_table_entry.physical_addr().byte_add(offset_in_page)
    } else {
        PhysicalAddress(vaddr.addr())
    }
}

/// Check that the given range of virtual addresses has the given flags set for all of its memory.
pub fn check_range_has_flags(vaddr_range: *const [u8], flags: PageTableFlags) -> bool {
    let start_vaddr = vaddr_range.addr() & !0xfff;
    let end_vaddr = vaddr_range.addr() + vaddr_range.len();
    for page_start_vaddr in (start_vaddr..end_vaddr).step_by(PAGE_SIZE) {
        let Some(entry) = entry_for_vaddr(core::ptr::without_provenance(page_start_vaddr)) else {
            return false;
        };
        if !entry.flags().contains(flags) {
            return false;
        }
    }
    true
}

/// A read-only reference to a region of user-space memory.
#[derive(Copy, Clone)]
pub struct UserMemRef<'a>(&'a [u8]);
impl<'a> UserMemRef<'a> {
    /// Construct a value for the given region.
    ///
    /// # Safety
    /// The resulting lifetime must be valid for the memory access.
    pub unsafe fn for_region(
        memory: *const [u8],
        _allow: &'a crate::csr::AllowUserModeMemory,
    ) -> Option<Self> {
        if !check_range_has_flags(
            memory,
            PageTableFlags::VALID | PageTableFlags::USER_ACCESSIBLE | PageTableFlags::READABLE,
        ) {
            return None;
        }
        // SAFETY: By method precondition, this is valid.
        Some(Self(unsafe { &*memory }))
    }
}
impl AsRef<[u8]> for UserMemRef<'_> {
    fn as_ref(&self) -> &[u8] {
        self.0
    }
}
impl core::ops::Deref for UserMemRef<'_> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.0
    }
}

/// A read-write reference to a region of user-space memory.
pub struct UserMemMut<'a>(&'a mut [u8]);
impl<'a> UserMemMut<'a> {
    /// Construct a value for the given region.
    ///
    /// # Safety
    /// The resulting lifetime must be valid for the memory access.
    pub unsafe fn for_region(
        memory: *mut [u8],
        _allow: &'a crate::csr::AllowUserModeMemory,
    ) -> Option<Self> {
        if !check_range_has_flags(
            memory,
            PageTableFlags::VALID
                | PageTableFlags::USER_ACCESSIBLE
                | PageTableFlags::READABLE
                | PageTableFlags::WRITABLE,
        ) {
            return None;
        }
        // SAFETY: By method precondition, this is valid.
        Some(Self(unsafe { &mut *memory }))
    }
}
impl AsRef<[u8]> for UserMemMut<'_> {
    fn as_ref(&self) -> &[u8] {
        self.0
    }
}
impl AsMut<[u8]> for UserMemMut<'_> {
    fn as_mut(&mut self) -> &mut [u8] {
        self.0
    }
}
impl core::ops::Deref for UserMemMut<'_> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.0
    }
}
impl core::ops::DerefMut for UserMemMut<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0
    }
}

/// Map the given page into the given page table at the given virtual address.
///
/// # Safety
/// We must have exclusive access to the given table, which must be initialized as a valid page
/// table structure. Also, the result of performing this mapping must not cause issues with Rust's
/// memory model.
pub unsafe fn map_page(
    mut table: NonNull<PageTable>,
    vaddr: *mut (),
    paddr: PhysicalAddress,
    flags: PageTableFlags,
) -> Result<(), OutOfMemory> {
    #![expect(clippy::panic_in_result_fn, reason = "Checking for bugs")]
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

    // SAFETY: Method precondition ensures valid access.
    let table = unsafe { table.as_mut() };
    if !table.entries[vpn1].flags().valid() {
        let new_page = crate::alloc::alloc_pages(1)?;
        table.entries[vpn1] = PageTableEntry::from_addr_flags(
            PhysicalAddress(new_page.addr()),
            PageTableFlags::VALID,
        );
        // SAFETY: Method precondition ensures valid access.
        unsafe {
            new_page.cast::<PageTable>().write(PageTable {
                entries: [PageTableEntry::EMPTY; PAGE_TABLE_LEGNTH],
            });
        }
    }
    // SAFETY: Method precondition ensures valid access.
    let table0 = unsafe {
        &mut *core::ptr::with_exposed_provenance_mut::<PageTable>(
            table.entries[vpn1].physical_addr().0,
        )
    };

    let vpn0 = (vaddr.addr() >> 12) & 0x3ff;
    assert!(!table0.entries[vpn0].flags().valid());
    table0.entries[vpn0] = PageTableEntry::from_addr_flags(paddr, flags | PageTableFlags::VALID);
    Ok(())
}
