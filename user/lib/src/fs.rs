//! Filesystem access.

/// Owned access to a file.
pub struct File {
    /// The underlying resource descriptor.
    descriptor: i32,
}

impl File {
    /// Open an existing file for reading.
    pub fn open(path: &str) -> Result<Self, shared::ErrorKind> {
        let descriptor = crate::sys::open(path, shared::FileOpenFlags::READ_ONLY)?;
        Ok(Self { descriptor })
    }

    /// Open an existing file for appending.
    pub fn append(path: &str) -> Result<Self, shared::ErrorKind> {
        let descriptor = crate::sys::open(
            path,
            shared::FileOpenFlags::WRITE_ONLY | shared::FileOpenFlags::APPEND,
        )?;
        Ok(Self { descriptor })
    }

    /// Read from this file into a buffer.
    ///
    /// Returns the written memory, which will be at the start of [`buf`].
    pub fn read<'a>(&self, buf: &'a mut [u8]) -> Result<&'a mut [u8], shared::ErrorKind> {
        let read_length = crate::sys::read(self.descriptor, buf)?;
        Ok(&mut buf[..read_length])
    }

    /// Write from a buffer into this file.
    ///
    /// Returns the number of bytes writen, which will be at the start of [`buf`].
    pub fn write(&self, buf: &[u8]) -> Result<usize, shared::ErrorKind> {
        crate::sys::write(self.descriptor, buf)
    }

    /// Write the entire buffer into this file.
    pub fn write_all(&self, mut buf: &[u8]) -> Result<(), shared::ErrorKind> {
        loop {
            let len = self.write(buf)?;
            if len == buf.len() {
                return Ok(());
            }
            buf = &buf[len..];
        }
    }
}
impl Drop for File {
    fn drop(&mut self) {
        crate::sys::close(self.descriptor);
    }
}
