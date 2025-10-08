use crate::resource_desc::ResourceDescriptor;

const PUT_CHAR_NUM: u32 = shared::Syscall::PutChar as u32;
const GET_CHAR_NUM: u32 = shared::Syscall::GetChar as u32;
const GET_PID_NUM: u32 = shared::Syscall::GetPid as u32;
const SCHED_YIELD_NUM: u32 = shared::Syscall::SchedYield as u32;
const EXIT_NUM: u32 = shared::Syscall::Exit as u32;
const GET_RANDOM_NUM: u32 = shared::Syscall::GetRandom as u32;
const OPEN_NUM: u32 = shared::Syscall::Open as u32;
const CLOSE_NUM: u32 = shared::Syscall::Close as u32;
const READ_NUM: u32 = shared::Syscall::Read as u32;
const WRITE_NUM: u32 = shared::Syscall::Write as u32;
const MMAP_NUM: u32 = shared::Syscall::Mmap as u32;

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
            log::info!("Process {} exited", current_proc.pid);
            current_proc.state = crate::proc::ProcessState::Exited;
            crate::proc::sched_yield();
        }
        GET_RANDOM_NUM => {
            let buf_start = core::ptr::with_exposed_provenance_mut(frame.a1 as usize);
            let buf_len = frame.a2 as usize;
            // TODO Check that the program is allowed to write to this buffer
            let buf = unsafe { core::slice::from_raw_parts_mut(buf_start, buf_len) };
            crate::DEVICE_TREE
                .random
                .lock()
                .as_mut()
                .unwrap()
                .read_random(buf)
                .unwrap();
        }
        OPEN_NUM => {
            let _allow = crate::csr::AllowUserModeMemory::allow();
            let path_name = str::from_utf8(unsafe {
                core::slice::from_raw_parts(
                    core::ptr::with_exposed_provenance(frame.a1 as usize),
                    frame.a2 as usize,
                )
            })
            .expect("Path wasn't valid utf-8");
            // TODO Support relative paths.
            let path_name = path_name
                .strip_prefix('/')
                .expect("Paths should start with '/'");

            let proc = unsafe { crate::proc::current_proc() };
            let (desc_num, slot) = unsafe { &mut *proc.resource_descriptors }
                .iter_mut()
                .enumerate()
                .find(|(_, slot)| !slot.present())
                .expect("Process out of file descriptor slots");
            // Return the file descriptor number to the process.
            frame.a0 = desc_num as u32;
            // Initialize the slot
            let inode_num = crate::DEVICE_TREE
                .storage
                .lock()
                .as_mut()
                .unwrap()
                .lookup_path(path_name.split('/'))
                .expect("Couldn't find given path");
            *slot =
                ResourceDescriptor::for_file(crate::resource_desc::FileResourceDescriptorData {
                    flags: crate::resource_desc::FileFlags::NEW_READ_ONLY,
                    offset: 0,
                    inode_num,
                });
        }
        CLOSE_NUM => {
            let desc_num = frame.a1;
            assert!(desc_num < crate::proc::MAX_NUM_RESOURCE_DESCRIPTORS as u32);
            let proc = unsafe { crate::proc::current_proc() };
            let desc = unsafe {
                &mut *proc
                    .resource_descriptors
                    .cast::<crate::resource_desc::ResourceDescriptor>()
                    .wrapping_add(desc_num as usize)
            };
            assert!(desc.present());
            desc.close();
        }
        READ_NUM => {
            let _allow = crate::csr::AllowUserModeMemory::allow();
            let user_buf = unsafe {
                core::slice::from_raw_parts_mut(
                    core::ptr::with_exposed_provenance_mut::<u8>(frame.a2 as usize),
                    frame.a3 as usize,
                )
            };
            let proc = unsafe { crate::proc::current_proc() };
            let desc_num = frame.a1;
            let desc = unsafe {
                &mut *proc
                    .resource_descriptors
                    .cast::<crate::resource_desc::ResourceDescriptor>()
                    .wrapping_add(desc_num as usize)
            };
            frame.a0 = desc.read(user_buf) as u32;
        }
        WRITE_NUM => {
            let _allow = crate::csr::AllowUserModeMemory::allow();
            let user_buf = unsafe {
                core::slice::from_raw_parts(
                    core::ptr::with_exposed_provenance::<u8>(frame.a2 as usize),
                    frame.a3 as usize,
                )
            };
            let proc = unsafe { crate::proc::current_proc() };
            let desc_num = frame.a1;
            let desc = unsafe {
                &mut *proc
                    .resource_descriptors
                    .cast::<crate::resource_desc::ResourceDescriptor>()
                    .wrapping_add(desc_num as usize)
            };
            frame.a0 = desc.write(user_buf) as u32;
        }
        MMAP_NUM => {
            let alloc_size = frame.a1;
            let alloc_num_pages = (alloc_size as usize).div_ceil(crate::page_table::PAGE_SIZE);
            let current_table = crate::csr::current_page_table().unwrap();
            let alloc_first_page = crate::alloc::alloc_pages_zeroed(alloc_num_pages).unwrap();
            let proc = unsafe { crate::proc::current_proc() };
            let start_user_vaddr = proc.mmap_head;
            // Leave a 1-page gap to help user programs avoid overruns.
            proc.mmap_head += crate::page_table::PAGE_SIZE * (alloc_num_pages + 1);
            for (paddr, user_vaddr) in (alloc_first_page.addr()..)
                .step_by(crate::page_table::PAGE_SIZE)
                .take(alloc_num_pages)
                .zip((start_user_vaddr..).step_by(crate::page_table::PAGE_SIZE))
            {
                unsafe {
                    crate::page_table::map_page(
                        current_table,
                        core::ptr::without_provenance_mut(user_vaddr),
                        crate::page_table::PhysicalAddress(paddr),
                        crate::page_table::PageTableFlags::READABLE
                            | crate::page_table::PageTableFlags::WRITABLE
                            | crate::page_table::PageTableFlags::EXECUTABLE
                            | crate::page_table::PageTableFlags::USER_ACCESSIBLE,
                    )
                }
                .expect("Failed to allocate page");
            }
            frame.a0 = start_user_vaddr as u32;
        }
        number => panic!("Unrecognized syscall {number}"), // TODO don't panic here
    }
}
