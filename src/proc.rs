use core::sync::atomic::{AtomicU32, AtomicUsize};

use util::cell::SyncUnsafeCell;

use crate::{
    alloc::KrcBox,
    error::{OutOfMemory, Result},
    page_table::{PageTableFlags, PhysicalAddress, PAGE_SIZE},
    resource_desc::ResourceDescription,
    sync::KSpinLock,
};

pub(crate) const KERNEL_STACK_SIZE: usize = 4096;
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
        resource_descriptors: core::ptr::dangling_mut(),
        mmap_head: 0,
    })
}; MAX_PROCS];

impl Process {
    pub fn create_process(image: &[u8]) -> Result<Self> {
        let Some((buf_idx, slot)) = PROCS_BUF.iter().enumerate().find(|(_, slot)| {
            let slot = unsafe { &*slot.get() };
            slot.state == ProcessState::Unused
        }) else {
            panic!("Out of space for processes");
        };
        unsafe { slot.get().write(ProcessInner::create_process(image)?) };
        Ok(Process { buf_idx })
    }

    /// Mark this process as the idle process, to only be chosen if nothing else is available.
    pub(crate) fn set_idle(&mut self) {
        self.inner_mut().state = ProcessState::Idle;
    }

    fn inner(&self) -> &ProcessInner {
        unsafe { &*PROCS_BUF[self.buf_idx].get() }
    }

    fn inner_mut(&mut self) -> &mut ProcessInner {
        unsafe { &mut *PROCS_BUF[self.buf_idx].get() }
    }
}

pub(crate) struct ProcessInner {
    pub pid: u32,
    pub state: ProcessState,
    pub sp: *mut (),
    pub page_table: PhysicalAddress,
    pub kernel_stack: *mut [u8; KERNEL_STACK_SIZE],
    pub resource_descriptors: *mut [Option<ResourceDescriptor>; MAX_NUM_RESOURCE_DESCRIPTORS],
    pub mmap_head: usize,
}

impl ProcessInner {
    fn create_process(image: &[u8]) -> Result<Self> {
        /// Counter for incrementing process IDs.
        static PID_COUNTER: AtomicU32 = AtomicU32::new(1);

        let kernel_stack = crate::alloc::alloc_pages(KERNEL_STACK_SIZE.div_ceil(4096))?
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
        let page_table = core::ptr::NonNull::new(crate::alloc::alloc_pages(1)?).unwrap();
        unsafe { crate::page_table::map_kernel_memory(page_table.cast()) }?;
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
        }?;
        let resource_descriptors = crate::alloc::alloc_pages(
            (MAX_NUM_RESOURCE_DESCRIPTORS * core::mem::size_of::<Option<ResourceDescriptor>>())
                .div_ceil(PAGE_SIZE),
        )?
        .cast::<[Option<ResourceDescriptor>; MAX_NUM_RESOURCE_DESCRIPTORS]>();
        unsafe {
            resource_descriptors.write([const { None }; MAX_NUM_RESOURCE_DESCRIPTORS]);
        }
        Ok(Self {
            // TODO Don't collide with pre-existing processes if it wraps.
            pid: PID_COUNTER.fetch_add(1, core::sync::atomic::Ordering::Relaxed),
            state: ProcessState::Runnable,
            sp,
            // Page table has same physical and virtual address.
            page_table: PhysicalAddress(page_table.addr().into()),
            kernel_stack,
            resource_descriptors,
            mmap_head: 0x02000000,
        })
    }
}
unsafe impl Send for ProcessInner {}
unsafe impl Sync for ProcessInner {}

pub(crate) const MAX_NUM_RESOURCE_DESCRIPTORS: usize = 1024;

/// A resource descriptor that a process might have.
#[repr(transparent)]
pub struct ResourceDescriptor {
    /// The inner description.
    description: KrcBox<KSpinLock<ResourceDescription>>,
}
impl ResourceDescriptor {
    pub fn new(description: ResourceDescription) -> Result<Self, OutOfMemory> {
        Ok(Self {
            description: KrcBox::new(KSpinLock::new(description))?,
        })
    }

    /// Get access to the underlying resource description this value points at.
    pub fn description(&self) -> impl core::ops::DerefMut<Target = ResourceDescription> + use<'_> {
        self.description.lock()
    }
}

#[derive(PartialEq, Eq, Debug)]
pub enum ProcessState {
    Unused,
    Runnable,
    Idle,
    Exited,
}

/// Select the next process to run.
fn next_proc_to_run(current_proc: &Process) -> usize {
    // Look for a runnable process other than the current one.
    if let Some((next_proc_slot, _)) = PROCS_BUF.iter().enumerate().find(|&(slot, proc)| {
        if slot == current_proc.buf_idx {
            return false;
        }
        let proc = unsafe { &*proc.get() };
        if proc.state != ProcessState::Runnable {
            return false;
        }
        true
    }) {
        return next_proc_slot;
    }
    if current_proc.inner().state == ProcessState::Runnable {
        return current_proc.buf_idx;
    }
    // If no processes are runnable, run the idle process.
    //
    // TODO We should cache this result, since it won't change.
    if let Some((next_proc_slot, _)) = PROCS_BUF.iter().enumerate().find(|&(_, proc)| {
        let proc = unsafe { &*proc.get() };
        proc.state == ProcessState::Idle
    }) {
        return next_proc_slot;
    };
    todo!("Nothing runnable right now");
}

pub fn sched_yield() {
    let mut current_proc = Process {
        buf_idx: CURRENT_PROC_SLOT.load(core::sync::atomic::Ordering::Relaxed),
    };
    let next_slot_idx = next_proc_to_run(&current_proc);
    if next_slot_idx != current_proc.buf_idx {
        let mut next_proc = Process {
            buf_idx: next_slot_idx,
        };
        unsafe { switch_context(&mut current_proc, &mut next_proc) }
    }
}

/// Get the PID of the currently-active process.
pub fn current_pid() -> u32 {
    unsafe { current_proc() }.pid
}

/// Get a reference to the current process.
///
/// # Safety
/// This reference is only valid until some other function references this slot.
///
/// TODO Support thread-safety in multithreading.
pub(crate) unsafe fn current_proc<'a>() -> &'a mut ProcessInner {
    unsafe { &mut *PROCS_BUF[CURRENT_PROC_SLOT.load(core::sync::atomic::Ordering::Relaxed)].get() }
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
/// `old_sp` and `new_sp` must be references to [`ProcessInner::sp`] fields which are properly set up.
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
