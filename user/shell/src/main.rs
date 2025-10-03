#![no_std]
#![no_main]

use userlib::prelude::*;

#[unsafe(no_mangle)]
fn main() {
    let line_buf = &mut [0; 128];
    let mut line_buf_len = 0;
    print!("> ");
    loop {
        match userlib::sys::getchar() {
            // On newline, run the queued command
            '\r' | '\n' => {
                let cmd = str::from_utf8(&line_buf[..line_buf_len]).expect("Invalid utf-8");
                println!();

                match cmd {
                    "hello" => println!("Hello from user shell!"),
                    "getpid" => {
                        let pid = userlib::sys::get_pid();
                        println!("{pid}");
                    }
                    "exit" => userlib::sys::exit(0),
                    "getrandom" => {
                        let mut buf = [0u8; 16];
                        userlib::sys::get_random(&mut buf);
                        for byte in buf {
                            print!("{byte:02X}");
                        }
                        println!();
                    }
                    _ => {
                        println!("Unrecognized command: {cmd}");
                    }
                }

                line_buf_len = 0;
                print!("> ");
            }
            // Handle backspace to allow command editing.
            '\x7f' => {
                if let Some(new_len) = line_buf_len.checked_sub(1) {
                    line_buf_len = new_len;
                    print!("\x08 \x08");
                }
            }
            // Otherwise, assume normal character
            //
            // TODO Handle other special characters
            c => {
                userlib::sys::putchar(c);
                line_buf[line_buf_len] = c as u8;
                line_buf_len += 1;
            }
        }
    }
}
