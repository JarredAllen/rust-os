const PUT_CHAR_NUM: u32 = shared::Syscall::PutChar as u32;
const GET_CHAR_NUM: u32 = shared::Syscall::GetChar as u32;
const GET_PID_NUM: u32 = shared::Syscall::GetPid as u32;
const SCHED_YIELD_NUM: u32 = shared::Syscall::SchedYield as u32;
const EXIT_NUM: u32 = shared::Syscall::Exit as u32;

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
                crate::proc::sched_yield();
            }
        }
        GET_PID_NUM => {
            frame.a0 = crate::proc::current_pid();
        }
        SCHED_YIELD_NUM => {
            crate::proc::sched_yield();
        }
        EXIT_NUM => {
            let _exit_status = frame.a1 as i32; // TODO record this status somewhere.
            let current_proc = unsafe { crate::proc::current_proc() };
            _ = crate::println!("Process {} exited", current_proc.pid);
            current_proc.state = crate::proc::ProcessState::Exited;
            crate::proc::sched_yield();
        }
        number => panic!("Unrecognized syscall {number}"), // TODO don't panic here
    }
}
