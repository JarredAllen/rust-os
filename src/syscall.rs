use shared::ErrorKind;

use crate::{
    error::Result, page_table::PAGE_SIZE, proc::ResourceDescriptor,
    resource_desc::ResourceDescription,
};

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
    #![allow(
        clippy::too_many_lines,
        reason = "We need to branch for every syscall here"
    )]
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
                        frame.a1 = c.get() as u32;
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
            frame.a1 = crate::proc::current_pid();
        }
        SCHED_YIELD_NUM => {
            crate::proc::sched_yield();
        }
        EXIT_NUM => {
            // TODO record the exit status somewhere.
            // let _exit_status = frame.a1 as i32;

            // SAFETY: We have exclusive access to this thread's running process.
            let current_proc = unsafe { crate::proc::current_proc() };
            log::info!("Process {} exited", current_proc.pid);
            current_proc.state = crate::proc::ProcessState::Exited;
            // SAFETY: The process exited, so we can drop the resource descriptors (possibly
            // running cleanup on the resource descriptions they point at).
            unsafe { current_proc.resource_descriptors.drop_in_place() };
            // SAFETY: The process exited, so we can free these pages.
            unsafe {
                crate::alloc::free_pages(
                    current_proc.resource_descriptors.cast(),
                    (crate::proc::MAX_NUM_RESOURCE_DESCRIPTORS
                        * size_of::<Option<ResourceDescriptor>>())
                    .div_ceil(PAGE_SIZE),
                );
            }
            crate::proc::sched_yield();
        }
        GET_RANDOM_NUM => {
            let buf_start = core::ptr::with_exposed_provenance_mut(frame.a1 as usize);
            let buf_len = frame.a2 as usize;
            // SAFETY: TODO Check that the program is allowed to read from this buffer
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
            // SAFETY: TODO Check that the program is allowed to read from this buffer
            let path_name = unsafe {
                core::slice::from_raw_parts(
                    core::ptr::with_exposed_provenance(frame.a1 as usize),
                    frame.a2 as usize,
                )
            };
            match syscall_open(path_name) {
                Ok(desc) => frame.a1 = desc as u32,
                Err(e) => {
                    frame.a1 = -1_i32 as u32;
                    frame.a2 = e.kind as u32;
                }
            }
        }
        CLOSE_NUM => {
            let desc_num = frame.a1;
            assert!(desc_num < crate::proc::MAX_NUM_RESOURCE_DESCRIPTORS as u32);
            // SAFETY: We have exclusive access to this thread's running process.
            let proc = unsafe { crate::proc::current_proc() };
            // SAFETY: We can get exclusive access to the resource descriptor set.
            let desc = &mut unsafe { &mut *proc.resource_descriptors }[desc_num as usize];
            assert!(desc.is_some());
            *desc = None;
        }
        READ_NUM => {
            let desc_num = frame.a1;
            // SAFETY: TODO Check that the program is allowed to read from this buffer
            let user_buf = unsafe {
                core::slice::from_raw_parts_mut(
                    core::ptr::with_exposed_provenance_mut::<u8>(frame.a2 as usize),
                    frame.a3 as usize,
                )
            };
            match syscall_read(desc_num, user_buf) {
                Ok(read_len) => frame.a1 = read_len as u32,
                Err(e) => {
                    frame.a1 = -1_i32 as u32;
                    frame.a2 = e.kind as u32;
                }
            }
        }
        WRITE_NUM => {
            let desc_num = frame.a1;
            // SAFETY: TODO Check that the program is allowed to read from this buffer
            let user_buf = unsafe {
                core::slice::from_raw_parts(
                    core::ptr::with_exposed_provenance::<u8>(frame.a2 as usize),
                    frame.a3 as usize,
                )
            };
            match syscall_write(desc_num, user_buf) {
                Ok(write_len) => frame.a1 = write_len as u32,
                Err(e) => {
                    frame.a1 = -1_i32 as u32;
                    frame.a2 = e.kind as u32;
                }
            }
        }
        MMAP_NUM => {
            let alloc_size = frame.a1;
            match syscall_mmap(alloc_size) {
                Ok(start_user_vaddr) => frame.a1 = start_user_vaddr as u32,
                Err(e) => {
                    frame.a1 = 0;
                    frame.a2 = e.kind as u32;
                }
            }
        }
        number => panic!("Unrecognized syscall {number}"), // TODO don't panic here
    }
}

fn syscall_open(path_name: &[u8]) -> Result<usize> {
    let _allow = crate::csr::AllowUserModeMemory::allow();
    let path_name = str::from_utf8(path_name).map_err(|_| ErrorKind::InvalidFormat)?;
    // TODO Support relative paths.
    let path_name = path_name
        .strip_prefix('/')
        .ok_or(ErrorKind::InvalidFormat)?;

    // SAFETY: We have exclusive access to this thread's running process.
    let proc = unsafe { crate::proc::current_proc() };
    // SAFETY: We can get exclusive access to the resource descriptor set.
    let (desc_num, slot) = unsafe { &mut *proc.resource_descriptors }
        .iter_mut()
        .enumerate()
        .find(|(_, slot)| slot.is_none())
        .ok_or(ErrorKind::LimitReached)?;
    // Initialize the slot
    let inode_num = crate::DEVICE_TREE
        .storage
        .lock()
        .as_mut()
        .unwrap()
        .lookup_path(path_name.split('/'))
        .ok_or(ErrorKind::NotFound)?;
    *slot = Some(ResourceDescriptor::new(ResourceDescription::for_file(
        crate::resource_desc::FileResourceDescriptionData {
            flags: crate::resource_desc::FileFlags::NEW_READ_ONLY,
            offset: 0,
            inode_num,
        },
    ))?);
    Ok(desc_num)
}

fn syscall_read(desc_num: u32, user_buf: &mut [u8]) -> Result<usize> {
    let _allow = crate::csr::AllowUserModeMemory::allow();
    // SAFETY: We have exclusive access to this thread's running process.
    let proc = unsafe { crate::proc::current_proc() };
    // SAFETY: We can get exclusive access to the resource descriptor set.
    let desc = unsafe { &mut *proc.resource_descriptors }[desc_num as usize]
        .as_ref()
        .ok_or(ErrorKind::NotFound)?;
    Ok(desc.description().read(user_buf))
}

fn syscall_write(desc_num: u32, user_buf: &[u8]) -> Result<usize> {
    let _allow = crate::csr::AllowUserModeMemory::allow();
    // SAFETY: We have exclusive access to this thread's running process.
    let proc = unsafe { crate::proc::current_proc() };
    // SAFETY: We can get exclusive access to the resource descriptor set.
    let desc = unsafe { &mut *proc.resource_descriptors }[desc_num as usize]
        .as_ref()
        .ok_or(ErrorKind::NotFound)?;
    Ok(desc.description().write(user_buf))
}

fn syscall_mmap(alloc_size: u32) -> Result<usize> {
    let alloc_num_pages = (alloc_size as usize).div_ceil(PAGE_SIZE);
    let current_table = crate::csr::current_page_table().unwrap();
    let alloc_first_page = crate::alloc::alloc_pages_zeroed(alloc_num_pages).unwrap();
    // SAFETY: We have exclusive access to this thread's running process.
    let proc = unsafe { crate::proc::current_proc() };
    let start_user_vaddr = proc.mmap_head;
    // Leave a 1-page gap to help user programs avoid overruns.
    proc.mmap_head += PAGE_SIZE * (alloc_num_pages + 1);
    for (paddr, user_vaddr) in (alloc_first_page.addr()..)
        .step_by(PAGE_SIZE)
        .take(alloc_num_pages)
        .zip((start_user_vaddr..).step_by(PAGE_SIZE))
    {
        // SAFETY: We're mapping fresh pages into unused memory in userspace.
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
        }?;
        // NOTE: This memory gets leaked, we should track the maps somewhere to clean them up.
    }
    Ok(start_user_vaddr)
}
