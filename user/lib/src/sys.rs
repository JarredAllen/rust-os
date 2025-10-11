//! Syscalls to the host OS.
//!
//! The eventual goal is to implement convenient wrappers around these functions in a way that
//! resembles the convenience of the stdlib, sorted by module.

pub use shared::Syscall;

/// Write a char to the console.
pub fn putchar(c: char) {
    // SAFETY: This matches the definition of this syscall.
    _ = unsafe { syscall(Syscall::PutChar as u32, [c as u32, 0, 0]) };
}

/// Write a whole string to the console.
pub fn putstr(s: &str) {
    for c in s.chars() {
        putchar(c);
    }
}

/// Read a character from the console.
#[must_use]
pub fn getchar() -> char {
    // SAFETY: This matches the definition of this syscall.
    let (ret, ret_err) = unsafe { syscall(Syscall::GetChar as u32, [0; 3]) };
    if ret == 0 {
        panic!("Hit error: {}", ret_err.unwrap());
    } else {
        // SAFETY: Kernel promises this will be a char.
        unsafe { char::from_u32_unchecked(ret) }
    }
}

/// Get the PID of the currently-active process.
#[must_use]
pub fn get_pid() -> u32 {
    // SAFETY: This matches the definition of this syscall.
    unsafe { syscall(Syscall::GetPid as u32, [0; 3]) }.0
}

/// Yield the current time slice.
pub fn sched_yield() {
    // SAFETY: This matches the definition of this syscall.
    _ = unsafe { syscall(Syscall::SchedYield as u32, [0; 3]) };
}

/// Exit the current process.
pub fn exit(status: i32) -> ! {
    // SAFETY: This matches the definition of this syscall.
    _ = unsafe { syscall(Syscall::Exit as u32, [status as u32, 0, 0]) };
    unreachable!("exit syscall should never return")
}

/// Fill a buffer with random bytes.
pub fn get_random(buf: &mut [u8]) {
    // SAFETY: This matches the definition of this syscall.
    _ = unsafe {
        syscall(
            Syscall::GetRandom as u32,
            [core::ptr::from_mut(buf).addr() as u32, buf.len() as u32, 0],
        )
    };
}

pub(crate) fn open(path: &str, flags: shared::FileOpenFlags) -> Result<i32, shared::ErrorKind> {
    // SAFETY: This matches the definition of this syscall.
    let (ret, ret_err) = unsafe {
        syscall(
            Syscall::Open as u32,
            [
                core::ptr::from_ref(path).addr() as u32,
                path.len() as u32,
                flags.into(),
            ],
        )
    };
    let ret = ret as i32;
    if ret == -1 {
        return Err(ret_err.unwrap());
    }
    Ok(ret)
}

pub(crate) fn close(descriptor_num: i32) {
    // SAFETY: This matches the definition of this syscall.
    _ = unsafe { syscall(Syscall::Close as u32, [descriptor_num as u32, 0, 0]) };
}

pub(crate) fn read(descriptor_num: i32, buf: &mut [u8]) -> Result<usize, shared::ErrorKind> {
    // SAFETY: This matches the definition of this syscall.
    let (read_len, err) = unsafe {
        syscall(
            Syscall::Read as u32,
            [
                descriptor_num as u32,
                core::ptr::from_ref(buf).addr() as u32,
                buf.len() as u32,
            ],
        )
    };
    if read_len == -1_i32 as u32 {
        return Err(err.unwrap());
    }
    Ok(read_len as usize)
}

pub(crate) fn write(descriptor_num: i32, buf: &[u8]) -> Result<usize, shared::ErrorKind> {
    // SAFETY: This matches the definition of this syscall.
    let (write_len, err) = unsafe {
        syscall(
            Syscall::Write as u32,
            [
                descriptor_num as u32,
                core::ptr::from_ref(buf).addr() as u32,
                buf.len() as u32,
            ],
        )
    };
    if write_len == -1_i32 as u32 {
        return Err(err.unwrap());
    }
    Ok(write_len as usize)
}

pub(crate) fn mmap(size: usize) -> Result<core::ptr::NonNull<()>, shared::ErrorKind> {
    // SAFETY: This matches the definition of this syscall.
    let (addr, err) = unsafe { syscall(Syscall::Mmap as u32, [size as u32, 0, 0]) };
    core::ptr::NonNull::new(core::ptr::without_provenance_mut(addr as usize))
        .ok_or_else(|| err.unwrap())
}

/// Perform an arbitrary syscall.
///
/// See [`Syscall`] for documentation on the supported syscall types and what their numbers are.
///
/// # Safety
/// This can be wildly unsafe, depending on the call done and the arguments. Prefer using the safe
/// helper functions where possible.
#[must_use]
pub unsafe fn syscall(
    syscall_number: u32,
    [arg0, arg1, arg2]: [u32; 3],
) -> (u32, Option<shared::ErrorKind>) {
    let ret_val;
    let ret_err;
    // SAFETY:
    // This makes the given syscall. The caller of this method is responsible for ensuring that the
    // results are sound.
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a0")  syscall_number,
            in("a1")  arg0,
            in("a2")  arg1,
            in("a3")  arg2,
            lateout("a1") ret_val,
            lateout("a2") ret_err,
        );
    }
    (ret_val, shared::ErrorKind::from_num(ret_err))
}
