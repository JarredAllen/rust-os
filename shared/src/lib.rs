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
}
