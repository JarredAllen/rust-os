#![no_std]
#![no_main]

mod alloc;
mod csr;
mod page_table;
mod proc;
mod sbi;
mod trap;

unsafe extern "C" {
    safe static __bss: *mut ();
    safe static __bss_end: *mut ();
    safe static __stack_top: *mut ();
}

/// The main kernel function.
///
/// This function is called by [`boot`] as soon as we can leave assembly and enter pure Rust code.
#[unsafe(no_mangle)]
fn kernel_main() -> ! {
    // Zero-initialize the BSS section.
    //
    // This needs to run before any code that references a zero-initialized static, in case the
    // bootloader in the BIOS doesn't zero-initialize this memory for us.
    let bss = unsafe {
        core::slice::from_raw_parts_mut(
            __bss.cast::<u8>(),
            __bss_end.byte_offset_from_unsigned(__bss),
        )
    };
    bss.fill(0);

    _ = println!("Hello, world!");

    // SAFETY:
    // `kernel_trap_entry` is a good function for writing here.
    unsafe { csr::write_csr!(stvec = kernel_trap_entry) }

    fn proc_a_entry() {
        loop {
            _ = sbi::putchar('A');
            proc::sched_yield();
            delay();
        }
    }
    fn proc_b_entry() {
        loop {
            _ = sbi::putchar('B');
            proc::sched_yield();
            delay();
        }
    }

    let mut proc_a = proc::Process::create_process(proc_a_entry as fn() as usize as u32);
    proc::Process::create_process(proc_b_entry as fn() as usize as u32);

    let mut idle_proc = proc::Process::create_process(0);
    unsafe {
        proc::switch_context(&mut idle_proc, &mut proc_a);
    };

    panic!("Reached end of `kernel_main`");
}

#[inline(never)]
fn delay() {
    for _ in 0..300000000 {
        unsafe { core::arch::asm!("nop") };
    }
}

#[unsafe(no_mangle)]
fn handle_trap(_frame: &trap::TrapFrame) {
    let scause = csr::read_csr!(scause);
    let stval = csr::read_csr!(stval);
    let user_pc = csr::read_csr!(sepc);
    panic!("Unexpected trap scause={scause:X}, stval={stval:X}, user_pc={user_pc:X}, ");
}

/// Entry point for kernel traps.
#[unsafe(naked)]
extern "C" fn kernel_trap_entry() -> ! {
    core::arch::naked_asm!(
        // Retrieve the kernel stack for this process from sscratch
        // and save the old stack there.
        "csrrw sp, sscratch, sp\n",
        "addi sp, sp, -4 * 31\n",
        "sw ra,  4 * 0(sp)\n",
        "sw gp,  4 * 1(sp)\n",
        "sw tp,  4 * 2(sp)\n",
        "sw t0,  4 * 3(sp)\n",
        "sw t1,  4 * 4(sp)\n",
        "sw t2,  4 * 5(sp)\n",
        "sw t3,  4 * 6(sp)\n",
        "sw t4,  4 * 7(sp)\n",
        "sw t5,  4 * 8(sp)\n",
        "sw t6,  4 * 9(sp)\n",
        "sw a0,  4 * 10(sp)\n",
        "sw a1,  4 * 11(sp)\n",
        "sw a2,  4 * 12(sp)\n",
        "sw a3,  4 * 13(sp)\n",
        "sw a4,  4 * 14(sp)\n",
        "sw a5,  4 * 15(sp)\n",
        "sw a6,  4 * 16(sp)\n",
        "sw a7,  4 * 17(sp)\n",
        "sw s0,  4 * 18(sp)\n",
        "sw s1,  4 * 19(sp)\n",
        "sw s2,  4 * 20(sp)\n",
        "sw s3,  4 * 21(sp)\n",
        "sw s4,  4 * 22(sp)\n",
        "sw s5,  4 * 23(sp)\n",
        "sw s6,  4 * 24(sp)\n",
        "sw s7,  4 * 25(sp)\n",
        "sw s8,  4 * 26(sp)\n",
        "sw s9,  4 * 27(sp)\n",
        "sw s10, 4 * 28(sp)\n",
        "sw s11, 4 * 29(sp)\n",
        // Save the stack pointer at time of exception to the stack
        "csrr a0, sscratch\n",
        "sw a0, 4 * 30(sp)\n",
        // Reset the kernel stack into sscratch
        "addi a0, sp, 4 * 31\n",
        "csrw sscratch, a0\n",
        "mv a0, sp\n",
        "call handle_trap\n",
        "lw ra,  4 * 0(sp)\n",
        "lw gp,  4 * 1(sp)\n",
        "lw tp,  4 * 2(sp)\n",
        "lw t0,  4 * 3(sp)\n",
        "lw t1,  4 * 4(sp)\n",
        "lw t2,  4 * 5(sp)\n",
        "lw t3,  4 * 6(sp)\n",
        "lw t4,  4 * 7(sp)\n",
        "lw t5,  4 * 8(sp)\n",
        "lw t6,  4 * 9(sp)\n",
        "lw a0,  4 * 10(sp)\n",
        "lw a1,  4 * 11(sp)\n",
        "lw a2,  4 * 12(sp)\n",
        "lw a3,  4 * 13(sp)\n",
        "lw a4,  4 * 14(sp)\n",
        "lw a5,  4 * 15(sp)\n",
        "lw a6,  4 * 16(sp)\n",
        "lw a7,  4 * 17(sp)\n",
        "lw s0,  4 * 18(sp)\n",
        "lw s1,  4 * 19(sp)\n",
        "lw s2,  4 * 20(sp)\n",
        "lw s3,  4 * 21(sp)\n",
        "lw s4,  4 * 22(sp)\n",
        "lw s5,  4 * 23(sp)\n",
        "lw s6,  4 * 24(sp)\n",
        "lw s7,  4 * 25(sp)\n",
        "lw s8,  4 * 26(sp)\n",
        "lw s9,  4 * 27(sp)\n",
        "lw s10, 4 * 28(sp)\n",
        "lw s11, 4 * 29(sp)\n",
        "lw sp,  4 * 30(sp)\n",
        "sret\n"
    );
}

/// A [`core::fmt::Write`] implementation for the SBI writing interface.
pub struct SbiPutcharWriter;
impl core::fmt::Write for SbiPutcharWriter {
    fn write_char(&mut self, c: char) -> core::fmt::Result {
        sbi::putchar(c).map_err(|_| core::fmt::Error)
    }

    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for c in s.chars() {
            self.write_char(c)?;
        }
        Ok(())
    }
}
macro_rules! println {
    ($($args:tt)*) => {{
        use core::fmt::Write;
        core::writeln!($crate::SbiPutcharWriter, $($args)*)
    }};
}

use println;

/// The entry function.
///
/// This function does some minimal setup in assembly before calling [`kernel_main`].
#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.boot")]
#[unsafe(naked)]
extern "C" fn boot() -> ! {
    core::arch::naked_asm!(
        // Set up the stack pointer
        "lui sp, %hi({stack_top})",
        "addi sp, sp, %lo({stack_top})",
        // Jump to the main function
        "j kernel_main",

        stack_top = sym __stack_top,
    );
}

#[cfg(target_os = "none")]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    _ = println!();
    _ = println!("===== KERNEL PANIC! =====");
    _ = println!("{info}");
    loop {
        core::hint::spin_loop();
    }
}
