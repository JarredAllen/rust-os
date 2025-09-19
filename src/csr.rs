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
