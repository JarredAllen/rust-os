//! Details shared between kernel-space and user-space.

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
    /// Flags for opening a new file.
    pub FileOpenFlags(u32) {
        /// Flags for opening a file with read-only permissions.
        ReadOnly,
        /// Flags for opening a file with write-only permissions.
        WriteOnly,
        /// If writing a file, append to the end.
        Append,
    }
);
impl FileOpenFlags {
    /// Flags for opening a file with read and write permissions.
    pub const READWRITE: Self = Self::READ_ONLY.bit_or(Self::WRITE_ONLY);
}

/// Possible kinds of errors from kernel syscalls.
#[derive(Debug, Clone, Copy)]
#[repr(u32)]
pub enum ErrorKind {
    /// The system is out of memory.
    OutOfMemory = 1,
    /// Generic I/O error.
    Io = 2,
    /// The operation wasn't supported.
    Unsupported = 3,
    /// The operation referenced something that wasn't found.
    NotFound = 4,
    /// The operation had data that wasn't in the required format.
    ///
    /// For example, an operation wanted utf-8 data, but the input wasn't valid utf-8.
    InvalidFormat = 5,
    /// The operation hit some resource limit.
    LimitReached = 6,
}
impl ErrorKind {
    /// Get the error kind from a number.
    #[must_use]
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
