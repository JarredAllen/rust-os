//! Code for handling open resource descriptions.

/// The state of an open resource.
pub struct ResourceDescriptor {
    /// The set of methods on this resource.
    vtable: RawResourceDescriptorVTable,
    /// The data associated with this resource.
    data: ResourceDescriptorData,
}

impl ResourceDescriptor {
    /// Create a new "null" descriptor.
    ///
    /// This resource can't be acted upon and is already closed.
    pub const fn null() -> Self {
        Self {
            vtable: RawResourceDescriptorVTable {
                read: |_, _| {
                    panic!("Read from non-present descriptor");
                },
                write: |_, _| {
                    panic!("Write to non-present descriptor");
                },
                close: |_| {
                    panic!("Closing non-present descriptor");
                },
                present: |_| false,
            },
            data: ResourceDescriptorData { null: () },
        }
    }

    /// Create a new descriptor for the given file data.
    pub const fn for_file(file_data: FileResourceDescriptorData) -> Self {
        Self {
            vtable: RawResourceDescriptorVTable::FILE_VTABLE,
            data: ResourceDescriptorData { file: file_data },
        }
    }

    /// Read from the given resource.
    pub fn read(&mut self, buf: &mut [u8]) -> usize {
        unsafe { (self.vtable.read)(&mut self.data, buf) }
    }

    /// Write to the given resource.
    pub fn write(&mut self, buf: &[u8]) -> usize {
        unsafe { (self.vtable.write)(&mut self.data, buf) }
    }

    /// Close the given resource.
    pub fn close(&mut self) {
        unsafe { (self.vtable.close)(&mut self.data) }
    }

    /// Check if this resource is still present.
    pub fn present(&self) -> bool {
        unsafe { (self.vtable.present)(&self.data) }
    }
}

bitset::bitset!(
    pub FileFlags(u32) {
        Present,
        Readable,
        Writable,
    }
);
impl FileFlags {
    /// Flags for making a new read-only file.
    pub const NEW_READ_ONLY: Self = Self::PRESENT.bit_or(Self::READABLE);
}

/// A VTable with methods for a [`ResourceDescriptor`].
///
/// # Safety
/// All of these functions are unsafe. They mut be fully-defined when called with a
/// [`ResourceDescriptorData`] associated with the same resource descriptor as contains this
/// vtable.
struct RawResourceDescriptorVTable {
    read: unsafe fn(&mut ResourceDescriptorData, &mut [u8]) -> usize,
    write: unsafe fn(&mut ResourceDescriptorData, &[u8]) -> usize,
    close: unsafe fn(&mut ResourceDescriptorData),
    present: unsafe fn(&ResourceDescriptorData) -> bool,
}
impl RawResourceDescriptorVTable {
    /// The [`RawResourceDescriptorVTable`] for file operations.
    const FILE_VTABLE: Self = {
        fn file_read(file_data: &mut FileResourceDescriptorData, buf: &mut [u8]) -> usize {
            assert!(file_data.flags.present() && file_data.flags.readable());
            crate::DEVICE_TREE
                .storage
                .lock()
                .as_mut()
                .unwrap()
                .read_file_from_offset(file_data.inode_num, file_data.offset, buf)
                .expect("Read failed")
        }
        fn file_write(file_data: &mut FileResourceDescriptorData, buf: &[u8]) -> usize {
            assert!(file_data.flags.present() && file_data.flags.readable());
            crate::DEVICE_TREE
                .storage
                .lock()
                .as_mut()
                .unwrap()
                .write_file_from_offset(file_data.inode_num, file_data.offset, buf)
                .expect("Read failed")
        }
        fn file_close(file_data: &mut FileResourceDescriptorData) {
            file_data.flags = FileFlags::empty();
            file_data.offset = 0;
            file_data.inode_num = 0;
        }
        fn file_present(file_data: &FileResourceDescriptorData) -> bool {
            file_data.flags.present()
        }
        Self {
            read: |data, buf| {
                let data = unsafe { &mut data.file };
                if !file_present(data) {
                    panic!("Read from closed file");
                }
                file_read(data, buf)
            },
            write: |data, buf| {
                let data = unsafe { &mut data.file };
                if !file_present(data) {
                    panic!("Write to closed file");
                }
                file_write(data, buf)
            },
            close: |data| {
                let data = unsafe { &mut data.file };
                if !file_present(data) {
                    panic!("Closing already closed file");
                }
                file_close(data)
            },
            present: |data| {
                let data = unsafe { &data.file };
                file_present(data)
            },
        }
    };
}

/// The kinds of data that a resource descriptor might keep.
pub(crate) union ResourceDescriptorData {
    /// State information for anything resembling a file.
    file: FileResourceDescriptorData,
    /// Some descriptors don't need anything more.
    null: (),
}

/// The data needed for a file-backed resource.
#[derive(Clone, Copy)]
pub(crate) struct FileResourceDescriptorData {
    /// The flags which were used for the file.
    pub(crate) flags: FileFlags,
    /// The inode number of this file on disk.
    pub(crate) inode_num: u32,
    /// The offset in the file.
    pub(crate) offset: u64,
}
