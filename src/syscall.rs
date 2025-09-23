const PUT_CHAR_NUM: u32 = shared::Syscall::PutChar as u32;
const GET_CHAR_NUM: u32 = shared::Syscall::GetChar as u32;

pub fn handle_syscall(frame: &mut crate::trap::TrapFrame) {
    match frame.a0 {
        PUT_CHAR_NUM => {
            if let Some(c) = char::from_u32(frame.a1) {
                _ = crate::sbi::putchar(c);
            }
        }
        GET_CHAR_NUM => {
            loop {
                match crate::sbi::getchar() {
                    Ok(Some(c)) => {
                        frame.a0 = c.get() as u32;
                        break;
                    }
                    Ok(None) => {}
                    Err(_e) => {
                        // TODO log the error
                    }
                }
            }
        }
        number => panic!("Unrecognized syscall {number}"), // TODO don't panic here
    }
}
