//! Syscalls to the host OS.
//!
//! The eventual goal is to implement convenient wrappers around these functions in a way that
//! resembles the convenience of the stdlib, sorted by module.

use core::ptr::NonNull;

pub use shared::Syscall;

/// Read a character from the console.
pub fn getchar() -> Result<char, shared::ErrorKind> {
    // NOTE: This disallows most non-ASCII characters from being read.
    let mut buf = 0_u8;
    loop {
        let len = read(0, core::slice::from_mut(&mut buf))?;
        if len > 0 {
            debug_assert_eq!(len, 1);
            break;
        }
    }
    Ok(buf.into())
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
pub fn get_random(buf: &mut [u8]) -> Result<(), shared::ErrorKind> {
    // SAFETY: This matches the definition of this syscall.
    let (ok, err) = unsafe {
        syscall(
            Syscall::GetRandom as u32,
            [core::ptr::from_mut(buf).addr() as u32, buf.len() as u32, 0],
        )
    };
    match (ok, err) {
        (0, _) => Ok(()),
        (0xFFFF_FFFF_u32, Some(err)) => Err(err),
        _ => unreachable!(),
    }
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

/// Request the kernel map more pages for us.
///
/// `size` is the minimum requested size, in bytes. The kernel might give more memory than that,
/// but presently it has no way to signal that it did so.
pub(crate) fn mmap(size: usize) -> Result<NonNull<()>, shared::ErrorKind> {
    // SAFETY: This matches the definition of this syscall.
    let (addr, err) = unsafe { syscall(Syscall::Mmap as u32, [size as u32, 0, 0]) };
    NonNull::new(core::ptr::without_provenance_mut(addr as usize)).ok_or_else(|| err.unwrap())
}

/// Unmap pages that were allocated via [`mmap`].
///
/// # Safety
/// `addr` must exactly match an address returned by `mmap`, and `size` must exactly match the
/// `size` value from that call to `mmap`. Additionally, there must be no remaining references to
/// that memory.
pub(crate) unsafe fn munmap(addr: NonNull<()>, size: usize) -> Result<(), shared::ErrorKind> {
    // SAFETY:
    // Because this memory region was `mmap`ed (see preconditions on this function), and nothing in
    // user memory is still using it, we can safely ask the kernel to unmap it.
    let (ok, err) = unsafe {
        syscall(
            Syscall::Munmap as u32,
            [addr.addr().get() as u32, size as u32, 0],
        )
    };
    match (ok, err) {
        (0, _) => Ok(()),
        (0xFFFF_FFFF_u32, Some(err)) => Err(err),
        _ => unreachable!(),
    }
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
