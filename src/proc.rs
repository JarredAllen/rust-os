use core::sync::atomic::{AtomicU32, AtomicUsize};

use util::cell::SyncUnsafeCell;

use crate::page_table::{PageTableFlags, PhysicalAddress};

const KERNEL_STACK_SIZE: usize = 4096;
const MAX_PROCS: usize = 8;

const USER_BASE: u32 = 0x0100_0000;

static CURRENT_PROC_SLOT: AtomicUsize = AtomicUsize::new(MAX_PROCS);

pub struct Process {
    buf_idx: usize,
}

static PROCS_BUF: [SyncUnsafeCell<ProcessInner>; MAX_PROCS] = [const {
    SyncUnsafeCell::new(ProcessInner {
        pid: 0,
        state: ProcessState::Unused,
        sp: core::ptr::dangling_mut(),
        page_table: PhysicalAddress::null(),
        kernel_stack: core::ptr::dangling_mut(),
    })
}; MAX_PROCS];

impl Process {
    pub fn create_process(image: &[u8]) -> Self {
        let Some((buf_idx, slot)) = PROCS_BUF.iter().enumerate().find(|(_, slot)| {
            let slot = unsafe { &*slot.get() };
            slot.state == ProcessState::Unused
        }) else {
            panic!("Out of space for processes");
        };
        unsafe { slot.get().write(ProcessInner::create_process(image)) };
        Process { buf_idx }
    }

    fn inner(&self) -> &ProcessInner {
        unsafe { &*PROCS_BUF[self.buf_idx].get() }
    }

    fn inner_mut(&mut self) -> &mut ProcessInner {
        unsafe { &mut *PROCS_BUF[self.buf_idx].get() }
    }
}

struct ProcessInner {
    pub pid: u32,
    pub state: ProcessState,
    pub sp: *mut (),
    pub page_table: PhysicalAddress,
    pub kernel_stack: *mut [u8; KERNEL_STACK_SIZE],
}

impl ProcessInner {
    fn create_process(image: &[u8]) -> Self {
        /// Counter for incrementing process IDs.
        static PID_COUNTER: AtomicU32 = AtomicU32::new(1);

        let kernel_stack = crate::alloc::alloc_pages(KERNEL_STACK_SIZE.div_ceil(4096))
            .cast::<[u8; KERNEL_STACK_SIZE]>();
        let sp = kernel_stack
            .wrapping_byte_add(KERNEL_STACK_SIZE)
            .wrapping_byte_sub(52)
            .cast::<()>();
        {
            let pc_ptr = sp.cast::<usize>();
            assert!(pc_ptr.is_aligned(), "Stack misaligned");
            unsafe { pc_ptr.write(user_entry as usize) };
        }
        let page_table = core::ptr::NonNull::new(crate::alloc::alloc_pages(1)).unwrap();
        unsafe { crate::page_table::map_kernel_memory(page_table.cast()) };
        const USER_PAGE_FLAGS: PageTableFlags = PageTableFlags::VALID
            .bit_or(PageTableFlags::READABLE)
            .bit_or(PageTableFlags::WRITABLE)
            .bit_or(PageTableFlags::EXECUTABLE)
            .bit_or(PageTableFlags::USER_ACCESSIBLE);
        unsafe {
            crate::page_table::alloc_and_map_slice(
                page_table.cast(),
                PhysicalAddress(USER_BASE as usize),
                image,
                USER_PAGE_FLAGS,
            )
        };
        Self {
            // TODO Don't collide with pre-existing processes if it wraps.
            pid: PID_COUNTER.fetch_add(1, core::sync::atomic::Ordering::Relaxed),
            state: ProcessState::Runnable,
            sp,
            // Page table has same physical and virtual address.
            page_table: PhysicalAddress(page_table.addr().into()),
            kernel_stack,
        }
    }
}
unsafe impl Send for ProcessInner {}
unsafe impl Sync for ProcessInner {}

#[derive(PartialEq, Eq, Debug)]
pub enum ProcessState {
    Unused,
    Runnable,
}

pub fn sched_yield() {
    let mut current_proc = Process {
        buf_idx: CURRENT_PROC_SLOT.load(core::sync::atomic::Ordering::Relaxed),
    };
    let Some((next_proc_slot, _)) = PROCS_BUF.iter().enumerate().find(|&(slot, proc)| {
        if slot == current_proc.buf_idx {
            return false;
        }
        let proc = unsafe { &*proc.get() };
        if proc.state != ProcessState::Runnable {
            return false;
        }
        // TODO Do we need more checks?
        true
    }) else {
        todo!("Nothing runnable right now");
    };
    let mut next_proc = Process {
        buf_idx: next_proc_slot,
    };
    unsafe { switch_context(&mut current_proc, &mut next_proc) }
}

/// Do a context switch.
///
/// # Safety
/// `old_proc` must correspond to the process that was being run before, and `new_proc` must be a
/// valid and runnable process.
pub unsafe fn switch_context(old_proc: &mut Process, new_proc: &mut Process) {
    debug_assert_eq!(
        new_proc.inner().state,
        ProcessState::Runnable,
        "New process should be runnable"
    );
    let next_proc_stack_bottom = new_proc.inner().kernel_stack.wrapping_add(1).cast::<()>();
    unsafe {
        crate::csr::write_csr!(sscratch = next_proc_stack_bottom);
        core::arch::asm!("sfence.vma");
        crate::csr::set_page_table(new_proc.inner().page_table);
        core::arch::asm!("sfence.vma");
    };
    CURRENT_PROC_SLOT.store(new_proc.buf_idx, core::sync::atomic::Ordering::Relaxed);
    let old_sp = &mut old_proc.inner_mut().sp;
    let new_sp = &mut new_proc.inner_mut().sp;
    unsafe { switch_context_inner(old_sp, new_sp) };
}

/// Actually do the inner context switch
///
/// # Safety
/// `old_sp` and `new_sp` must be references to [`Process::sp`] fields which are properly set up.
#[unsafe(naked)]
unsafe extern "C" fn switch_context_inner(old_sp: &mut *mut (), new_sp: &mut *mut ()) {
    core::arch::naked_asm!(
        // Save callee-saved registers onto the current process's stack.
        "addi sp, sp, -13 * 4", // Allocate stack space for 13 4-byte registers
        "sw ra,  0  * 4(sp)",   // Save callee-saved registers only
        "sw s0,  1  * 4(sp)",
        "sw s1,  2  * 4(sp)",
        "sw s2,  3  * 4(sp)",
        "sw s3,  4  * 4(sp)",
        "sw s4,  5  * 4(sp)",
        "sw s5,  6  * 4(sp)",
        "sw s6,  7  * 4(sp)",
        "sw s7,  8  * 4(sp)",
        "sw s8,  9  * 4(sp)",
        "sw s9,  10 * 4(sp)",
        "sw s10, 11 * 4(sp)",
        "sw s11, 12 * 4(sp)",
        // Switch the stack pointer.
        "sw sp, (a0)",
        "lw sp, (a1)",
        // Restore callee-saved registers from the next process's stack.
        "lw ra,  0  * 4(sp)", // Restore callee-saved registers only
        "lw s0,  1  * 4(sp)",
        "lw s1,  2  * 4(sp)",
        "lw s2,  3  * 4(sp)",
        "lw s3,  4  * 4(sp)",
        "lw s4,  5  * 4(sp)",
        "lw s5,  6  * 4(sp)",
        "lw s6,  7  * 4(sp)",
        "lw s7,  8  * 4(sp)",
        "lw s8,  9  * 4(sp)",
        "lw s9,  10 * 4(sp)",
        "lw s10, 11 * 4(sp)",
        "lw s11, 12 * 4(sp)",
        "addi sp, sp, 13 * 4", // We've popped 13 4-byte registers from the stack
        "ret",
    )
}

#[unsafe(naked)]
unsafe extern "C" fn user_entry() {
    core::arch::naked_asm!(
        "lui t0, %hi({sepc})",
        "addi t0, t0, %lo({sepc})",
        "csrw sepc, t0",
        "lui t0, %hi({sstatus})",
        "addi t0, t0, %lo({sstatus})",
        "csrw sstatus, t0",
        "sret",
        sepc = const USER_BASE,
        sstatus =  const 1 << 5,
    );
}
