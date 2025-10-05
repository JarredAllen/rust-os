#![no_std]
#![no_main]

use userlib::{fs::File, prelude::*};

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

                let mut cmd_parts = cmd.split_whitespace(); // TODO Support complex escaping

                let Some(cmd_name) = cmd_parts.next() else {
                    print!("> ");
                    continue;
                };

                match cmd_name {
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
                    "cat" => {
                        let Some(filename) = cmd_parts.next() else {
                            print!("Missing filename for cat command\n> ");
                            continue;
                        };
                        let file = File::open(filename);
                        let read_buf = &mut [0; 2048];
                        let contents =
                            str::from_utf8(file.read(read_buf)).expect("File was invalid utf-8");
                        print!("{contents}");
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
