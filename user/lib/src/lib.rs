#![no_std]

pub fn putchar(c: char) {
    unsafe { syscall(shared::Syscall::PutChar as u32, [c as u32, 0, 0]) };
}

pub fn putstr(s: &str) {
    for c in s.chars() {
        putchar(c);
    }
}

pub fn getchar() -> char {
    let ret = unsafe { syscall(shared::Syscall::GetChar as u32, [0; 3]) };
    // SAFETY: Kernel promises this will be a char.
    unsafe { char::from_u32_unchecked(ret) }
}

/// Perform an arbitrary syscall.
///
/// # Safety
/// This can be wildly unsafe, depending on the call done and the arguments. Prefer using the safe
/// helper functions where possible.
pub unsafe fn syscall(syscall_number: u32, [arg0, arg1, arg2]: [u32; 3]) -> u32 {
    let ret_val;
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a0")  syscall_number,
            in("a1")  arg0,
            in("a2")  arg1,
            in("a3")  arg2,
            lateout("a0") ret_val,
        )
    }
    ret_val
}
