//! Syscalls to the host OS.
//!
//! The eventual goal is to implement convenient wrappers around these functions in a way that
//! resembles the convenience of the stdlib, sorted by module.

pub use shared::Syscall;

/// Write a char to the console.
pub fn putchar(c: char) {
    unsafe { syscall(Syscall::PutChar as u32, [c as u32, 0, 0]) };
}

/// Write a whole string to the console.
pub fn putstr(s: &str) {
    for c in s.chars() {
        putchar(c);
    }
}

/// Read a character from the console.
pub fn getchar() -> char {
    let ret = unsafe { syscall(Syscall::GetChar as u32, [0; 3]) };
    // SAFETY: Kernel promises this will be a char.
    unsafe { char::from_u32_unchecked(ret) }
}

/// Get the PID of the currently-active process.
pub fn get_pid() -> u32 {
    unsafe { syscall(Syscall::GetPid as u32, [0; 3]) }
}

/// Yield the current time slice.
pub fn sched_yield() {
    unsafe { syscall(Syscall::SchedYield as u32, [0; 3]) };
}

/// Exit the current process.
pub fn exit(status: i32) -> ! {
    unsafe { syscall(Syscall::Exit as u32, [status as u32, 0, 0]) };
    unreachable!("exit syscall should never return")
}

/// Fill a buffer with random bytes.
pub fn get_random(buf: &mut [u8]) {
    unsafe {
        syscall(
            Syscall::GetRandom as u32,
            [core::ptr::from_mut(buf).addr() as u32, buf.len() as u32, 0],
        )
    };
}

/// Perform an arbitrary syscall.
///
/// See [`Syscall`] for documentation on the supported syscall types and what their numbers are.
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
