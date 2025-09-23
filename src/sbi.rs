//! A library for interfacing with the SBI.

/// Call to the SBI.
///
/// # Safety
/// This can cause a variety of different behaviors, depending on the call. The caller is required
/// to ensure that this call doesn't break the memory model.
pub unsafe fn call(args: [u32; 6], fid: u32, eid: u32) -> Result<u32> {
    let value: i32;
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a0") args[0],
            in("a1") args[1],
            in("a2") args[2],
            in("a3") args[3],
            in("a4") args[4],
            in("a5") args[5],
            in("a6") fid,
            in("a7") eid,
            lateout("a0") value,
        )
    };
    // TODO Legacy functions always return only one value in a0.
    if value < 0 {
        Err(Error::for_reg_value(value).unwrap())
    } else {
        Ok(value as u32)
    }
}

pub fn putchar(c: char) -> Result<()> {
    unsafe { call([c as u32, 0, 0, 0, 0, 0], 0, 1)? };
    Ok(())
}

pub fn getchar() -> Result<Option<core::num::NonZero<char>>> {
    let c = unsafe { call([0; 6], 0, 2) }?;
    Ok(char::from_u32(c).and_then(core::num::NonZero::new))
}

/// A type alias for returning errors more easily.
pub type Result<T> = core::result::Result<T, Error>;

/// Errors from SBI calls.
///
/// This enum is non-exhaustive
#[repr(i32)]
#[non_exhaustive]
pub enum Error {
    Failed = -1,
    NotSupported = -2,
    InvalidParameter = -3,
    Denied = -4,
    InvalidAddress = -5,
    AlreadyAvailable = -6,
    AlreadyStarted = -7,
    AlreadyStopped = -8,
    NoSharedMemory = -9,
    InvalidState = -10,
    BadRange = -11,
    Timeout = -12,
    Io = -13,
    LockedOut = -14,
    /// Some other, unknown error happened.
    ///
    /// You shouldn't match on this variant.
    Other = 1,
}
impl Error {
    fn for_reg_value(reg: i32) -> Option<Self> {
        Some(match reg {
            // Only 0 indicates success
            0 => return None,
            -1 => Self::Failed,
            -2 => Self::NotSupported,
            -3 => Self::InvalidParameter,
            -4 => Self::Denied,
            -5 => Self::InvalidAddress,
            -6 => Self::AlreadyAvailable,
            -7 => Self::AlreadyStarted,
            -8 => Self::AlreadyStopped,
            -9 => Self::NoSharedMemory,
            -10 => Self::InvalidState,
            -11 => Self::BadRange,
            -12 => Self::Timeout,
            -13 => Self::Io,
            -14 => Self::LockedOut,
            _ => Self::Other,
        })
    }
}
