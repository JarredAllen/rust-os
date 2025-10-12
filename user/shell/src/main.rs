//! A shell implementation for this userspace.

#![no_std]
#![no_main]

extern crate alloc;

use userlib::{fs::File, prelude::*};

#[unsafe(no_mangle)]
extern "Rust" fn main() {
    let mut line_buf = alloc::vec::Vec::<u8>::new();
    print!("> ");
    loop {
        match userlib::sys::getchar() {
            // On newline, run the queued command
            '\r' | '\n' => {
                let cmd = str::from_utf8(&line_buf).expect("Invalid utf-8");
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
                        let len = cmd_parts
                            .next()
                            .map_or(16, |s| s.parse().expect("Invalid number"));
                        let mut buf = alloc::vec![0_u8; len];
                        userlib::sys::get_random(&mut buf);
                        for byte in buf {
                            print!("{byte:02X}");
                        }
                        println!();
                    }
                    "cat" => {
                        let Some(filename) = cmd_parts.next() else {
                            print!("Missing filename for cat command\n> ");
                            line_buf.clear();
                            continue;
                        };
                        let file = File::open(filename).expect("Failed to open file");
                        let read_buf = &mut [0; 2048];
                        let contents =
                            str::from_utf8(file.read(read_buf).expect("Failed to read file"))
                                .expect("File was invalid utf-8");
                        print!("{contents}");
                    }
                    "prepend" => {
                        let Some(filename) = cmd_parts.next() else {
                            print!("Missing filename for prepend command\n> ");
                            line_buf.clear();
                            continue;
                        };
                        let file = File::open(filename).expect("Failed to open file");
                        let read_buf = &mut [0; 2048];
                        let contents =
                            str::from_utf8(file.read(read_buf).expect("Failed to read file"))
                                .expect("File was invalid utf-8");
                        let file = File::overwrite(filename).expect("Failed to open file");
                        let prepend_buf = &cmd.as_bytes()[9 + filename.len()..];
                        file.write_all(prepend_buf)
                            .expect("Error writing to buffer");
                        file.write_all(contents.as_bytes())
                            .expect("Error writing to buffer");
                    }
                    _ => {
                        println!("Unrecognized command: {cmd}");
                    }
                }
                line_buf.clear();
                print!("> ");
            }
            // Handle backspace to allow command editing.
            '\x7f' => {
                if line_buf.pop().is_some() {
                    print!("\x08 \x08");
                }
            }
            // Otherwise, assume normal character
            //
            // TODO Handle other special characters
            c => {
                userlib::sys::putchar(c);
                line_buf.push(u8::try_from(c).expect("Non-u8 character"));
            }
        }
    }
}
