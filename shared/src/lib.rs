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
