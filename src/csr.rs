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
pub(crate) use read_csr;

/// Write a value to a CSR
macro_rules! write_csr {
    ($csr:ident = $value:expr) => {
        core::arch::asm!(
            concat!("csrw ", stringify!($csr), ", {}"),
            in(reg) $value,
        )
    };
}
pub(crate) use write_csr;

/// Write the satp csr to set the page table.
pub unsafe fn set_page_table(page_table_addr: PhysicalAddress) {
    assert!(page_table_addr.is_aligned(crate::page_table::PAGE_SIZE));
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
