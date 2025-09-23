#![no_std]

#[repr(u32)]
pub enum Syscall {
    PutChar = 1,
    GetChar = 2,
}
