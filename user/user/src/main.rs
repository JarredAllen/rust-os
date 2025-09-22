#![no_std]
#![no_main]

unsafe extern "C" {
    safe static __stack_top: *mut ();
}

#[unsafe(link_section = ".text")]
#[unsafe(no_mangle)]
fn main() {
    loop {
        core::hint::spin_loop();
    }
}

#[unsafe(link_section = ".text.start")]
#[unsafe(naked)]
#[unsafe(no_mangle)]
extern "C" fn start() -> ! {
    core::arch::naked_asm!(
        "lui sp, %hi({stack_top})",
        "addi sp, sp, %lo({stack_top})",

        "call main",
        "call exit",

        stack_top = sym __stack_top,
    )
}

#[unsafe(no_mangle)]
fn exit() -> ! {
    todo!("Exit syscall");
}

#[cfg(target_os = "none")]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
