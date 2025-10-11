use core::ptr::NonNull;

use crate::page_table::PhysicalAddress;

/// Read a CSR and return the value.
macro_rules! read_csr {
    ($csr:ident) => {
        // SAFETY: Reading CSRs is always valid.
        unsafe {
            let csr: u32;
            core::arch::asm!(
                concat!("csrr {}, ", stringify!($csr)),
                lateout(reg) csr,
            );
            csr
        }
    };
}

/// Write a value to a CSR
macro_rules! write_csr {
    ($csr:ident = $value:expr) => {
        core::arch::asm!(
            concat!("csrw ", stringify!($csr), ", {}"),
            in(reg) $value,
        )
    };
}

pub(crate) use {read_csr, write_csr};

/// Write the satp csr to set the page table.
///
/// # Safety
/// It is on the caller to ensure that switching to the newly-active page table will not cause any
/// problems.
pub unsafe fn set_page_table(page_table_addr: PhysicalAddress) {
    assert!(page_table_addr.is_aligned(crate::page_table::PAGE_SIZE));
    // SAFETY:
    // This sets the page table to the user-given address, which must be valid by the method
    // precondition.
    unsafe { write_csr!(satp = (page_table_addr.0 / crate::page_table::PAGE_SIZE) | (1 << 31)) };
}

/// Get whether paging is enabled.
pub fn current_page_table() -> Option<NonNull<crate::page_table::PageTable>> {
    let satp = read_csr!(satp);
    (satp & (1 << 31) != 0).then(|| {
        let paddr = (satp as usize & !(1 << 31)) * crate::page_table::PAGE_SIZE;
        NonNull::new(core::ptr::with_exposed_provenance_mut(paddr)).unwrap()
    })
}

/// An RAII around accessing user-mode memory.
///
/// If you want to interact with user-mode memory, you must hold an instance of this struct while
/// doing so.
pub struct AllowUserModeMemory {
    _marker: (),
}
impl AllowUserModeMemory {
    /// Allow accessing user-mode memory until this value is dropped.
    pub fn allow() -> Self {
        let sstatus = read_csr!(sstatus);
        // SAFETY:
        // Writing the `SUM` bit is valid.
        unsafe { write_csr!(sstatus = sstatus | 1 << 18) };
        Self { _marker: () }
    }
}
impl Drop for AllowUserModeMemory {
    fn drop(&mut self) {
        let sstatus = read_csr!(sstatus);
        // SAFETY:
        // Writing the `SUM` bit is valid.
        unsafe { write_csr!(sstatus = sstatus & !(1 << 18)) };
    }
}
