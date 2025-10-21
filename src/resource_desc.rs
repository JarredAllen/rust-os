//! Code for handling open resource descriptions.

use crate::error::Result;

/// The state of an open resource.
pub struct ResourceDescription {
    /// The set of methods on this resource.
    vtable: RawResourceDescriptionVTable,
    /// The data associated with this resource.
    data: ResourceDescriptionData,
}

impl ResourceDescription {
    /// Create a new descriptor for the given file data.
    pub const fn for_file(file_data: FileResourceDescriptionData) -> Self {
        Self {
            vtable: RawResourceDescriptionVTable::FILE_VTABLE,
            data: ResourceDescriptionData { file: file_data },
        }
    }

    pub const fn for_console_in() -> Self {
        Self {
            vtable: RawResourceDescriptionVTable::CONSOLE_IN_VTABLE,
            data: ResourceDescriptionData { null: () },
        }
    }

    pub const fn for_console_out() -> Self {
        Self {
            vtable: RawResourceDescriptionVTable::CONSOLE_OUT_VTABLE,
            data: ResourceDescriptionData { null: () },
        }
    }

    /// Read from the given resource.
    pub fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        // SAFETY: We keep the vtable and the value together to meet the precondition.
        unsafe { (self.vtable.read)(&mut self.data, buf) }
    }

    /// Write to the given resource.
    pub fn write(&mut self, buf: &[u8]) -> Result<usize> {
        // SAFETY: We keep the vtable and the value together to meet the precondition.
        unsafe { (self.vtable.write)(&mut self.data, buf) }
    }

    /// Close the given resource.
    pub fn close(&mut self) {
        // SAFETY: We keep the vtable and the value together to meet the precondition.
        unsafe { (self.vtable.close)(&mut self.data) }
    }
}
impl Drop for ResourceDescription {
    fn drop(&mut self) {
        self.close();
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

/// A `VTable` with methods for a [`ResourceDescription`].
///
/// # Safety
/// All of these functions are unsafe. They mut be fully-defined when called with a
/// [`ResourceDescriptionData`] associated with the same resource descriptor as contains this
/// vtable.
struct RawResourceDescriptionVTable {
    read: unsafe fn(&mut ResourceDescriptionData, &mut [u8]) -> Result<usize>,
    write: unsafe fn(&mut ResourceDescriptionData, &[u8]) -> Result<usize>,
    close: unsafe fn(&mut ResourceDescriptionData),
}
impl RawResourceDescriptionVTable {
    /// The [`RawResourceDescriptionVTable`] for file operations.
    const FILE_VTABLE: Self = {
        fn file_read(file_data: &mut FileResourceDescriptionData, buf: &mut [u8]) -> Result<usize> {
            assert!(file_data.flags.present() && file_data.flags.readable());
            crate::DEVICE_TREE
                .storage
                .lock()
                .as_mut()
                .unwrap()
                .read_file_from_offset(file_data.inode_num, file_data.offset, buf)
        }
        fn file_write(file_data: &mut FileResourceDescriptionData, buf: &[u8]) -> Result<usize> {
            assert!(file_data.flags.present() && file_data.flags.writable());
            let len = crate::DEVICE_TREE
                .storage
                .lock()
                .as_mut()
                .unwrap()
                .write_file_from_offset(file_data.inode_num, file_data.offset, buf)?;
            file_data.offset += len as u64;
            Ok(len)
        }
        fn file_close(file_data: &mut FileResourceDescriptionData) {
            file_data.flags = FileFlags::empty();
            file_data.offset = 0;
            file_data.inode_num = 0;
        }
        Self {
            read: |data, buf| {
                // SAFETY: This can only be called if the data is a file.
                let data = unsafe { &mut data.file };
                file_read(data, buf)
            },
            write: |data, buf| {
                // SAFETY: This can only be called if the data is a file.
                let data = unsafe { &mut data.file };
                file_write(data, buf)
            },
            close: |data| {
                // SAFETY: This can only be called if the data is a file.
                let data = unsafe { &mut data.file };
                file_close(data);
            },
        }
    };

    const CONSOLE_IN_VTABLE: Self = {
        Self {
            read: |_, buf| {
                let c = loop {
                    if let Ok(Some(c)) = crate::sbi::getchar() {
                        // TODO log the error
                        break c;
                    }
                };
                let c_ser = c.get().encode_utf8(buf);
                Ok(c_ser.len())
            },
            write: |_, _| {
                panic!("Write to console in not permitted");
            },
            close: |_| {},
        }
    };

    const CONSOLE_OUT_VTABLE: Self = {
        Self {
            read: |_, _| {
                panic!("Read from console out not permitted");
            },
            write: |_, buf| {
                use core::fmt::Write as _;
                let s = str::from_utf8(buf).expect("TODO Write non-utf8");
                crate::sbi::SbiPutcharWriter
                    .write_str(s)
                    .map_err(|core::fmt::Error| shared::ErrorKind::Io)?;
                Ok(s.len())
            },
            close: |_| {},
        }
    };
}

/// The kinds of data that a resource descriptor might keep.
pub(crate) union ResourceDescriptionData {
    /// State information for anything resembling a file.
    file: FileResourceDescriptionData,
    /// Some descriptors don't need anything more.
    null: (),
}

/// The data needed for a file-backed resource.
#[derive(Clone, Copy)]
pub(crate) struct FileResourceDescriptionData {
    /// The flags which were used for the file.
    pub(crate) flags: FileFlags,
    /// The inode number of this file on disk.
    pub(crate) inode_num: u32,
    /// The offset in the file.
    pub(crate) offset: u64,
}
