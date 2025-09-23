#![no_std]

#[repr(u32)]
pub enum Syscall {
    PutChar = 1,
    GetChar = 2,
    GetPid = 3,
    SchedYield = 4,
}
