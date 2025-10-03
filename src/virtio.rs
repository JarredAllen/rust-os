//! A virtio block device driver for handling storage.
//!
//! Designed according to the spec from
//! <https://docs.oasis-open.org/virtio/virtio/v1.1/csprd01/virtio-v1.1-csprd01.pdf>.

mod reg;

use core::{marker::PhantomData, mem::MaybeUninit, ptr::NonNull};

use crate::error::{ErrorKind, Result};

/// The address for the block device.
pub(crate) const BLOCK_DEVICE_ADDRESS: usize = 0x1000_1000;

/// The address for the block device.
pub(crate) const RNG_DEVICE_ADDRESS: usize = 0x1000_2000;

/// A driver controlling a virtio block device.
pub struct VirtioBlock<'a> {
    /// The underlying virtio implementation.
    virtio: Virtio<'a, 1>,
}
impl<'a> VirtioBlock<'a> {
    /// Initialize at the address the device appears at in kernel memory.
    ///
    /// # Safety
    /// This takes ownership over a device at the given address, so requires nothing else access
    /// this memory.
    pub unsafe fn init_kernel_address() -> Result<Self> {
        log::info!("Initializing virtio block device");
        let mut virtio = unsafe {
            Virtio::init_for_pointers(core::ptr::with_exposed_provenance_mut(BLOCK_DEVICE_ADDRESS))
        };
        assert_eq!(virtio.read_register(reg::DeviceId), 2);
        let queue = unsafe {
            &mut *crate::alloc::alloc_pages_zeroed(
                core::mem::size_of::<VirtQueue>().div_ceil(crate::page_table::PAGE_SIZE),
            )?
            .cast::<MaybeUninit<VirtQueue>>()
        };
        virtio.initialize_queue(0, queue);
        Ok(Self { virtio })
    }

    /// Send the request to the disk and wait for a response.
    fn do_request(&mut self, request: &mut BlockRequest) {
        // Each descriptor can only be read-only or write-only, so we need to split into multiple
        // parts.
        let desc = self.virtio.queues[0]
            .unwrap()
            .as_ptr()
            .wrapping_byte_add(core::mem::offset_of!(VirtQueue, descriptor))
            .cast::<VirtQueueDescriptor>();
        // Descriptor 0: Device-read-only header
        unsafe {
            desc.write_volatile(VirtQueueDescriptor {
                address: core::ptr::from_mut(request).addr() as u64,
                length: core::mem::offset_of!(BlockRequest, data) as u32,
                flags: DescriptorFlags::NEXT,
                next: 1,
            })
        };
        // Descriptor 1: The data (may be read or written)
        unsafe {
            desc.wrapping_add(1).write_volatile(VirtQueueDescriptor {
                address: core::ptr::from_mut(request).addr() as u64
                    + core::mem::offset_of!(BlockRequest, data) as u64,
                length: BLOCK_SECTOR_LEN as u32,
                flags: match request.ty {
                    BlockRequestType::Read => DescriptorFlags::NEXT | DescriptorFlags::WRITE,
                    BlockRequestType::Write => DescriptorFlags::NEXT,
                    _ => {
                        // We (the driver) don't yet support the other types.
                        request.status = BlockRequestStatus::UNSUPPORTED;
                        return;
                    }
                },
                next: 2,
            })
        };
        // Descriptor 2: The status byte (device-written)
        unsafe {
            desc.wrapping_add(2).write_volatile(VirtQueueDescriptor {
                address: core::ptr::from_mut(request).addr() as u64
                    + core::mem::offset_of!(BlockRequest, status) as u64,
                length: 1,
                flags: DescriptorFlags::WRITE,

                next: 0,
            })
        };

        // SAFETY:
        // The descriptors point to non-overlapping sections of `request`, which we have an
        // exclusive reference to.
        unsafe { self.virtio.run_descriptor(0, 0) };
    }

    /// Read a sector from the device into the buffer.
    pub fn read_sector(&mut self, buf: &mut [u8; BLOCK_SECTOR_LEN], sector: u64) -> Result<()> {
        log::info!("Reading sector {sector} from virtio block device");
        let mut request = BlockRequest {
            ty: BlockRequestType::Read,
            reserved: 0,
            sector,
            data: [0; 512],
            status: BlockRequestStatus::empty(),
        };
        self.do_request(&mut request);
        request.status.success()?;
        buf.copy_from_slice(&request.data);
        Ok(())
    }

    /// Write a sector to the buffer.
    pub fn write_sector(&mut self, data: &[u8; BLOCK_SECTOR_LEN], sector: u64) -> Result<()> {
        log::info!("Writing sector {sector} to virtio block device");
        let mut request = BlockRequest {
            ty: BlockRequestType::Write,
            reserved: 0,
            sector,
            data: *data,
            status: BlockRequestStatus::empty(),
        };
        self.do_request(&mut request);
        request.status.success()?;
        Ok(())
    }

    /// Get the capacity in number of 512-byte sectors.
    pub fn capacity(&self) -> u64 {
        self.virtio.read_register(reg::Capacity)
    }
}

pub struct VirtioRandom<'a> {
    virtio: Virtio<'a, 1>,
}
impl<'a> VirtioRandom<'a> {
    /// Initialize at the address the device appears at in kernel memory.
    ///
    /// # Safety
    /// This takes ownership over a device at the given address, so requires nothing else access
    /// this memory.
    pub unsafe fn init_kernel_address() -> Result<Self> {
        log::info!("Initializing virtio random device");
        let mut virtio = unsafe {
            Virtio::init_for_pointers(core::ptr::with_exposed_provenance_mut(RNG_DEVICE_ADDRESS))
        };
        assert_eq!(virtio.read_register(reg::DeviceId), 4);
        let queue = unsafe {
            &mut *crate::alloc::alloc_pages_zeroed(
                core::mem::size_of::<VirtQueue>().div_ceil(crate::page_table::PAGE_SIZE),
            )?
            .cast::<MaybeUninit<VirtQueue>>()
        };
        virtio.initialize_queue(0, queue);
        Ok(Self { virtio })
    }

    /// Fill this buffer with random bytes.
    ///
    /// This function assumes the buffer is in kernel memory (i.e. the physical and virtual
    /// addresses are the same).
    pub fn read_random(&mut self, mut buf: &mut [u8]) -> Result<()> {
        const MAX_NUM_ITERS: u8 = 128;
        let mut num_iters = 0;
        loop {
            num_iters += 1;
            if num_iters > MAX_NUM_ITERS {
                log::error!("Entropy device didn't make random data on time");
                return Err(crate::error::ErrorKind::Io.into());
            }
            // Each descriptor can only be read-only or write-only, so we need to split into multiple
            // parts.
            let desc = self.virtio.queues[0]
                .unwrap()
                .as_ptr()
                .wrapping_byte_add(core::mem::offset_of!(VirtQueue, descriptor))
                .cast::<VirtQueueDescriptor>();
            // Descriptor 0: Device-read-only header
            unsafe {
                desc.write_volatile(VirtQueueDescriptor {
                    address: crate::page_table::paddr_for_vaddr(core::ptr::from_mut(buf)).0 as u64,
                    length: buf.len() as u32,
                    flags: DescriptorFlags::WRITE,
                    next: 0,
                })
            };
            // SAFETY:
            // The descriptors point to non-overlapping sections of `request`, which we have an
            // exclusive reference to.
            let used = unsafe { self.virtio.run_descriptor(0, 0) };
            if used.length as usize == buf.len() {
                return Ok(());
            }
            buf = &mut buf[used.length as usize..];
            // TODO Enable this once it gets called from user syscalls
            // crate::proc::sched_yield();
        }
    }
}

/// A driver controlling a virtio device.
///
/// This type handles the code common to all virtio device types. Device-specific logic should be
/// implemented on the publicly-exported types
struct Virtio<'a, const NUM_QUEUES: usize> {
    /// A pointer to the registers for the device.
    ///
    /// This isn't a reference because the underlying hardware can modify the pointed-to data, so
    /// the aliasing rules for exclusive references are violated.
    regs: *mut (),
    /// A pointer to the queue buffer.
    ///
    /// This isn't a reference because the underlying hardware can modify the pointed-to data, so
    /// the aliasing rules for exclusive references are violated.
    ///
    /// The driver presently only supports having exactly one queue. TODO Add support for
    /// initializing and destroying queues.
    queues: [Option<NonNull<VirtQueue>>; NUM_QUEUES],
    /// Phantom to track the lifetime.
    phantom: PhantomData<&'a mut ()>,
}

impl<'a, const NUM_QUEUES: usize> Virtio<'a, NUM_QUEUES> {
    unsafe fn init_for_pointers(regs: *mut ()) -> Self {
        let mut this = Self {
            regs,
            queues: [None; NUM_QUEUES],
            phantom: PhantomData,
        };
        this.initialize();
        this
    }

    fn initialize_queue(&mut self, queue_num: u32, queue: &'a mut MaybeUninit<VirtQueue>) {
        self.write_register(reg::QueueSelect, queue_num);

        // Check that the selected queue isn't active.
        assert_eq!(self.read_register(reg::QueueReady), 0);

        // Initialize the queue
        self.write_register(
            reg::QueueSize,
            const {
                assert!(QUEUE_SIZE <= u32::MAX as usize);
                QUEUE_SIZE as u32
            },
        );
        let queue = queue.write(VirtQueue::default());
        self.queues[queue_num as usize] = NonNull::new(queue);

        self.write_register(reg::QueuePfn, core::ptr::from_mut(queue).addr() as u32);

        // Mark the queue as ready for operation.
        self.write_register(reg::QueueReady, 1);
    }

    fn read_register<Register: VirtioBlockRegister>(&self, _register: Register) -> Register::RegTy {
        const { assert!(Register::READABLE) };
        let reg_ptr = self
            .regs
            .wrapping_byte_add(Register::OFFSET)
            .cast::<Register::RegTy>();
        unsafe { reg_ptr.read_volatile() }
    }

    fn write_register<Register: VirtioBlockRegister>(
        &self,
        _register: Register,
        value: Register::RegTy,
    ) {
        const { assert!(Register::WRITABLE) };
        let reg_ptr = self
            .regs
            .wrapping_byte_add(Register::OFFSET)
            .cast::<Register::RegTy>();
        unsafe { reg_ptr.write_volatile(value) }
    }

    /// Initialize the device.
    fn initialize(&mut self) {
        log::info!("Initializing virtio device");
        // Initialize device per section 3.1
        // 1. Reset the device.
        self.write_register(reg::DeviceStatus, reg::DeviceStatusFlags::empty());
        // 2. Set the acknowledge bit to tell the device we know about it.
        self.write_register(reg::DeviceStatus, reg::DeviceStatusFlags::ACKNOWLEDGE);
        // 3. Set the driver status bit to tell the device we know how to drive it.
        self.write_register(
            reg::DeviceStatus,
            reg::DeviceStatusFlags::ACKNOWLEDGE | reg::DeviceStatusFlags::DRIVER,
        );

        // 4. Read the device feature bits, and write the subset that we understand.

        // First check that the device is what we expect.
        assert_eq!(self.read_register(reg::Magic), 0x74726976);
        assert_eq!(self.read_register(reg::Version), 1);

        // Then read the features, check that we support them, and write them back.
        let features = self.read_register(reg::DeviceFeatures);
        log::info!("virtio device advertizes features {features}");
        assert!(!features.read_only());
        // NOTE We currently don't use any features

        // 5. Set the status bit to indicate we've accepted the features.
        self.write_register(
            reg::DeviceStatus,
            reg::DeviceStatusFlags::ACKNOWLEDGE
                | reg::DeviceStatusFlags::DRIVER
                | reg::DeviceStatusFlags::FEATURES_OK,
        );
        // 6. Re-read device status to make sure the device is okay with the features we request.
        assert!(self.read_register(reg::DeviceStatus).features_ok());

        // 7. Do device specific initialization.

        // 8. Set the DRIVER_OK bit to make the device live.
        self.write_register(
            reg::DeviceStatus,
            reg::DeviceStatusFlags::ACKNOWLEDGE
                | reg::DeviceStatusFlags::DRIVER
                | reg::DeviceStatusFlags::FEATURES_OK
                | reg::DeviceStatusFlags::DRIVER_OK,
        );

        // Check for errors from the device
        let status = self.read_register(reg::DeviceStatus);
        assert!(!status.failed());
        assert!(!status.device_needs_reset());

        log::info!("virtio device initialized!");
    }

    /// Run the request indicated by `descriptor_idx` (and any descriptors chained).
    ///
    /// This method will block until the read succeeds.
    ///
    /// # Safety
    /// The device will read and/or write the contents the descriptors point at. The caller is
    /// responsible for ensuring that these reads and writes do not violate Rust's memory model.
    unsafe fn run_descriptor(
        &mut self,
        queue_num: u32,
        descriptor_idx: u16,
    ) -> VirtQueueUsedElement {
        let queue = self.queues[queue_num as usize].unwrap().as_ptr();
        // Reference the descriptors in the queue.
        let available = unsafe {
            &mut *queue
                .wrapping_byte_add(core::mem::offset_of!(VirtQueue, available))
                .cast::<VirtQueueAvailableRing>()
        };
        available.ring[available.index as usize % QUEUE_SIZE] = descriptor_idx;
        let available_idx = queue
            .wrapping_byte_add(core::mem::offset_of!(VirtQueue, available.index))
            .cast::<u16>();
        let idx = unsafe { available_idx.read_volatile() };
        let available_slot = queue
            .wrapping_byte_add(core::mem::offset_of!(VirtQueue, available.ring))
            .cast::<u16>()
            .wrapping_add(idx as usize % QUEUE_SIZE);
        unsafe { available_slot.write_volatile(0) };
        unsafe { available_idx.write_volatile(idx.wrapping_add(1)) };

        // Use a fence to ensure we set up the queue before sending the notification
        core::sync::atomic::fence(core::sync::atomic::Ordering::AcqRel);
        // Notify the device that a new operation is available.
        self.write_register(reg::QueueNotify, 0);

        // Wait for the device to finish
        log::debug!("Submitted request to device");
        while self.queue_busy(queue_num) {
            core::hint::spin_loop();
        }
        let used_idx = unsafe {
            queue
                .wrapping_byte_add(core::mem::offset_of!(VirtQueue, used.index))
                .cast::<u16>()
                .read_volatile()
        } as usize
            % QUEUE_SIZE;
        let queue_elem = queue
            .wrapping_byte_add(core::mem::offset_of!(VirtQueue, used.ring))
            .cast::<VirtQueueUsedElement>()
            .wrapping_add(used_idx);
        unsafe { queue_elem.read_volatile() }
    }

    /// Returns `true` if the device is processing elements in the queue.
    fn queue_busy(&self, queue_num: u32) -> bool {
        let queue = self.queues[queue_num as usize].unwrap().as_ptr();
        let available_idx = unsafe {
            queue
                .wrapping_byte_add(core::mem::offset_of!(VirtQueue, available.index))
                .cast::<u16>()
                .read_volatile()
        };
        let used_idx = unsafe {
            queue
                .wrapping_byte_add(core::mem::offset_of!(VirtQueue, used.index))
                .cast::<u16>()
                .read_volatile()
        };
        available_idx != used_idx
    }
}

unsafe impl<const NUM_QUEUES: usize> Send for Virtio<'_, NUM_QUEUES> {}
unsafe impl<const NUM_QUEUES: usize> Sync for Virtio<'_, NUM_QUEUES> {}

/// A register for a virtio block device.
///
/// # Safety
/// The fields of this type must accurately represent fields which are available.
unsafe trait VirtioBlockRegister {
    /// The offset of this register from the base in memory.
    const OFFSET: usize;
    /// The data type for this field.
    type RegTy;

    /// Whether this register is readable.
    const READABLE: bool;
    /// Whether this register is writable.
    const WRITABLE: bool;
}

#[derive(Default, Debug)]
#[repr(C, align(4096))]
struct VirtQueue {
    descriptor: [VirtQueueDescriptor; QUEUE_SIZE],
    available: VirtQueueAvailableRing,
    used: VirtQueueUsedRing,
}

#[repr(C, align(16))]
#[derive(Default, Debug)]
struct VirtQueueDescriptor {
    address: u64,
    length: u32,
    flags: DescriptorFlags,
    next: u16,
}

bitset::bitset!(
    DescriptorFlags(u16) {
        Next = 0,
        Write = 1,
        Indirect = 2,
    }
);

#[repr(C)]
#[derive(Default, Debug)]
struct VirtQueueAvailableRing {
    flags: u16,
    index: u16,
    ring: [u16; QUEUE_SIZE],
}

#[repr(C, align(4096))]
#[derive(Default, Debug)]
struct VirtQueueUsedRing {
    flags: u16,
    index: u16,
    ring: [VirtQueueUsedElement; 16],
}

#[repr(C)]
#[derive(Default, Debug)]
struct VirtQueueUsedElement {
    index: u32,
    length: u32,
}

#[derive(Debug)]
#[repr(C)]
struct BlockRequest {
    ty: BlockRequestType,
    reserved: u32,
    sector: u64,
    data: [u8; 512],
    status: BlockRequestStatus,
}

#[derive(Debug)]
#[repr(u32)]
#[expect(unused, reason = "todo")]
enum BlockRequestType {
    Read = 0,
    Write = 1,
    Flush = 4,
    Discard = 11,
    WriteZeros = 13,
}

bitset::bitset!(
    BlockRequestStatus(u8) {
        IoError = 0,
        Unsupported = 1,
    }
);

impl BlockRequestStatus {
    fn success(self) -> Result<()> {
        if self.io_error() {
            Err(ErrorKind::Io.into())
        } else if self.unsupported() {
            Err(ErrorKind::Unsupported.into())
        } else {
            Ok(())
        }
    }
}

const QUEUE_SIZE: usize = 16;

/// The size of one sector on disk.
pub const BLOCK_SECTOR_LEN: usize = 512;
