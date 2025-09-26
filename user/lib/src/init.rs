//! Initialization routines for user-space programs.
//!
//! This code exists such that user libraries can just write a `main` function and have it be
//! called automatically.

unsafe extern "C" {
    /// The top of the stack for user-space programs.
    safe static __stack_top: *mut ();
}

/// The entry hook run by the OS.
///
/// This function does the necessary instructions to call a `main` function with a
/// properly-intialized environment.
///
/// This entry point relies on the linked-to user binary having a function named `main`, without
/// any symbol mangling, which it calls. If the `main` function returns, then the process exits
/// with a 0 status code.
#[unsafe(link_section = ".text.start")]
#[unsafe(naked)]
#[unsafe(no_mangle)]
extern "C" fn start() -> ! {
    core::arch::naked_asm!(
        "lui sp, %hi({stack_top})",
        "addi sp, sp, %lo({stack_top})",

        "call {main}",
        "call {exit}",

        stack_top = sym __stack_top,
        exit = sym __exit,
        main = sym main,
    )
}

unsafe extern "Rust" {
    /// The `main` function provided by the user binary.
    safe fn main();
}

/// The panic handler for user-space code.
///
/// This handler just displays the panic information and exits with a non-zero status.
#[cfg_attr(target_os = "none", panic_handler)]
fn panic(info: &core::panic::PanicInfo) -> ! {
    use core::fmt::Write;
    // SAFETY:
    // This panic handler will never return to outside code, so it is safe to take ownership over
    // the stdout stream.
    let mut stdout = unsafe { crate::io::Stdout::force_lock() };
    _ = writeln!(stdout, "\n{info}");
    crate::sys::exit(1);
}

/// Exit the process.
///
/// This function exists for [`start`] to exit without returning, which would be problematic.
fn __exit() -> ! {
    crate::sys::exit(0)
}
