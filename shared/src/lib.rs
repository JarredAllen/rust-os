#![no_std]

/// The syscall types supported by the kernel.
#[repr(u32)]
pub enum Syscall {
    /// Write a character to the console.
    PutChar = 1,
    /// Read a character from the console.
    GetChar = 2,
    /// Get the PID of the current process.
    GetPid = 3,
    /// Yield to let another process run.
    SchedYield = 4,
    /// Exit the current process.
    Exit = 5,
    /// Fill a buffer with random bytes.
    GetRandom = 6,
    /// Open a file
    Open = 7,
    /// Close a resource descriptor
    Close = 8,
    /// Read data from a resource descriptor.
    Read = 9,
    /// Write data to a resource descriptor.
    Write = 10,
    /// Map a new memory region.
    Mmap = 11,
}

bitset::bitset!(
    pub FileOpenFlags(u32) {
        ReadOnly,
        WriteOnly,
        Append,
    }
);
impl FileOpenFlags {
    pub const READWRITE: Self = Self::READ_ONLY.bit_or(Self::WRITE_ONLY);
}

/// Possible kinds of errors from kernel syscalls.
#[derive(Debug, Clone, Copy)]
#[repr(u32)]
pub enum ErrorKind {
    OutOfMemory = 1,
    Io = 2,
    Unsupported = 3,
    NotFound = 4,
    InvalidFormat = 5,
    LimitReached = 6,
}
impl ErrorKind {
    pub fn from_num(num: u32) -> Option<Self> {
        Some(match num {
            1 => Self::OutOfMemory,
            2 => Self::Io,
            3 => Self::Unsupported,
            4 => Self::NotFound,
            5 => Self::InvalidFormat,
            6 => Self::LimitReached,
            _ => return None,
        })
    }
}
impl core::fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::OutOfMemory => "Out of memory",
            Self::Io => "I/O Error",
            Self::Unsupported => "Unsupported operation",
            Self::NotFound => "Requested entity not found",
            Self::InvalidFormat => "Supplied data did not match expected format",
            Self::LimitReached => "Process reached resource limit",
        })
    }
}
