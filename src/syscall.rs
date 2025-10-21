use shared::ErrorKind;

use crate::{
    error::Result,
    page_table::{UserMemMut, UserMemMutOpaque, UserMemRef, PAGE_SIZE},
    proc::ResourceDescriptor,
    resource_desc::{FileFlags, ResourceDescription},
};

const GET_PID_NUM: u32 = shared::Syscall::GetPid as u32;
const SCHED_YIELD_NUM: u32 = shared::Syscall::SchedYield as u32;
const EXIT_NUM: u32 = shared::Syscall::Exit as u32;
const GET_RANDOM_NUM: u32 = shared::Syscall::GetRandom as u32;
const OPEN_NUM: u32 = shared::Syscall::Open as u32;
const CLOSE_NUM: u32 = shared::Syscall::Close as u32;
const READ_NUM: u32 = shared::Syscall::Read as u32;
const WRITE_NUM: u32 = shared::Syscall::Write as u32;
const MMAP_NUM: u32 = shared::Syscall::Mmap as u32;
const MUNMAP_NUM: u32 = shared::Syscall::Munmap as u32;

pub fn handle_syscall(frame: &mut crate::trap::TrapFrame) {
    #![allow(
        clippy::too_many_lines,
        reason = "We need to branch for every syscall here"
    )]
    match frame.a0 {
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
            let user_buf = core::ptr::slice_from_raw_parts_mut(buf_start, buf_len);
            // SAFETY:
            // The buffer is in user-space, so it can't alias anything, we drop it when we return
            // from the syscall, so the lifetime isn't too long.
            let Some(user_buf) = (unsafe { UserMemMutOpaque::for_region(user_buf) }) else {
                frame.a1 = -1_i32 as u32;
                frame.a2 = ErrorKind::NotPermitted as u32;
                return;
            };
            crate::DEVICE_TREE
                .random
                .lock()
                .as_mut()
                .unwrap()
                .read_random(user_buf)
                .unwrap();
            frame.a1 = 0;
        }
        OPEN_NUM => {
            let allow = crate::csr::AllowUserModeMemory::allow();
            let path_buf = core::ptr::slice_from_raw_parts(
                core::ptr::with_exposed_provenance::<u8>(frame.a1 as usize),
                frame.a2 as usize,
            );
            // SAFETY:
            // The buffer is in user-space, so it can't alias anything, and `allow` is
            // dropped when we return from the syscall, so the lifetime isn't too long.
            let Some(path_buf) = (unsafe { UserMemRef::for_region(path_buf, &allow) }) else {
                frame.a1 = -1_i32 as u32;
                frame.a2 = ErrorKind::NotPermitted as u32;
                return;
            };
            let flags = shared::FileOpenFlags::from(frame.a3);
            match syscall_open(&path_buf, flags) {
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
            if desc.take().is_none() {
                frame.a1 = -1_i32 as u32;
                frame.a2 = ErrorKind::NotFound as u32;
            }
        }
        READ_NUM => {
            let desc_num = frame.a1;
            let allow = crate::csr::AllowUserModeMemory::allow();
            let buf_start = core::ptr::with_exposed_provenance_mut(frame.a2 as usize);
            let buf_len = frame.a3 as usize;
            let user_buf = core::ptr::slice_from_raw_parts_mut(buf_start, buf_len);
            // SAFETY:
            // The buffer is in user-space, so it can't alias anything, and `allow` is
            // dropped when we return from the syscall, so the lifetime isn't too long.
            let Some(mut user_buf) = (unsafe { UserMemMut::for_region(user_buf, &allow) }) else {
                frame.a1 = -1_i32 as u32;
                frame.a2 = ErrorKind::NotPermitted as u32;
                return;
            };
            match syscall_read(desc_num, &mut user_buf) {
                Ok(read_len) => frame.a1 = read_len as u32,
                Err(e) => {
                    frame.a1 = -1_i32 as u32;
                    frame.a2 = e.kind as u32;
                }
            }
        }
        WRITE_NUM => {
            let allow = crate::csr::AllowUserModeMemory::allow();
            let desc_num = frame.a1;
            let user_buf = core::ptr::slice_from_raw_parts(
                core::ptr::with_exposed_provenance::<u8>(frame.a2 as usize),
                frame.a3 as usize,
            );
            // SAFETY:
            // The buffer is in user-space, so it can't alias anything, and `allow` is
            // dropped when we return from the syscall, so the lifetime isn't too long.
            let Some(user_buf) = (unsafe { UserMemRef::for_region(user_buf, &allow) }) else {
                frame.a1 = -1_i32 as u32;
                frame.a2 = ErrorKind::NotPermitted as u32;
                return;
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
                    frame.a1 = -1_i32 as u32;
                    frame.a2 = e.kind as u32;
                }
            }
        }
        MUNMAP_NUM => {
            #[expect(unused, reason = "Will be used once TODO is done")]
            let alloc_addr = frame.a1;
            #[expect(unused, reason = "Will be used once TODO is done")]
            let alloc_size = frame.a2;
            // TODO Unmap and free the pages
            //
            // This is technically okay but wasteful because we could reuse these pages but we
            // won't.
            frame.a1 = 0;
        }
        number => panic!("Unrecognized syscall {number}"), // TODO don't panic here
    }
}

fn syscall_open(path_name: &[u8], open_flags: shared::FileOpenFlags) -> Result<usize> {
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
    let mut flags = FileFlags::PRESENT;
    if open_flags.read_only() {
        flags = flags.bit_or(FileFlags::READABLE);
    }
    if open_flags.write_only() {
        flags = flags.bit_or(FileFlags::WRITABLE);
    }
    *slot = Some(ResourceDescriptor::new(ResourceDescription::for_file(
        crate::resource_desc::FileResourceDescriptionData {
            flags,
            offset: if open_flags.append() {
                todo!("Set offset to end of file")
            } else {
                0
            },
            inode_num,
        },
    ))?);
    Ok(desc_num)
}

fn syscall_read(desc_num: u32, user_buf: &mut [u8]) -> Result<usize> {
    // SAFETY: We have exclusive access to this thread's running process.
    let proc = unsafe { crate::proc::current_proc() };
    // SAFETY: We can get exclusive access to the resource descriptor set.
    let desc = unsafe { &mut *proc.resource_descriptors }[desc_num as usize]
        .as_ref()
        .ok_or(ErrorKind::NotFound)?;
    desc.description().read(user_buf)
}

fn syscall_write(desc_num: u32, user_buf: UserMemRef) -> Result<usize> {
    // SAFETY: We have exclusive access to this thread's running process.
    let proc = unsafe { crate::proc::current_proc() };
    // SAFETY: We can get exclusive access to the resource descriptor set.
    let desc = unsafe { &mut *proc.resource_descriptors }[desc_num as usize]
        .as_ref()
        .ok_or(ErrorKind::NotFound)?;
    desc.description().write(&user_buf)
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
