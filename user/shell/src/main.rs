#![no_std]
#![no_main]

unsafe extern "C" {
    safe static __stack_top: *mut ();
}

#[unsafe(no_mangle)]
fn main() {
    let line_buf = &mut [0; 128];
    let mut line_buf_len = 0;
    userlib::putstr("> ");
    loop {
        match userlib::getchar() {
            '\r' | '\n' => {
                let cmd = str::from_utf8(&line_buf[..line_buf_len]).expect("Invalid utf-8");
                userlib::putchar('\n');

                match cmd {
                    "hello" => userlib::putstr("Hello from user shell!\n"),
                    _ => {
                        userlib::putstr("Unrecognized command: ");
                        userlib::putstr(cmd);
                        userlib::putstr("\n");
                    }
                }

                line_buf_len = 0;
                userlib::putstr("> ");
            }
            c => {
                userlib::putchar(c);
                line_buf[line_buf_len] = c as u8;
                line_buf_len += 1;
            }
        }
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
