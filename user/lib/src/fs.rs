pub struct File {
    descriptor: i32,
}

impl File {
    pub fn open(path: &str) -> Self {
        let descriptor = crate::sys::open(path, shared::FileOpenFlags::READ_ONLY);
        Self { descriptor }
    }

    pub fn append(path: &str) -> Self {
        let descriptor = crate::sys::open(
            path,
            shared::FileOpenFlags::WRITE_ONLY | shared::FileOpenFlags::APPEND,
        );
        Self { descriptor }
    }

    /// Read from this file into a buffer.
    ///
    /// Returns the written memory, which will be at the start of [`buf`].
    pub fn read<'a>(&self, buf: &'a mut [u8]) -> &'a mut [u8] {
        let read_length = crate::sys::read(self.descriptor, buf);
        &mut buf[..read_length]
    }

    /// Write from a buffer into this file.
    ///
    /// Returns the number of bytes writen, which will be at the start of [`buf`].
    pub fn write(&self, buf: &[u8]) -> usize {
        crate::sys::write(self.descriptor, buf)
    }

    pub fn write_all(&self, mut buf: &[u8]) {
        loop {
            let len = self.write(buf);
            if len == buf.len() {
                return;
            } else {
                buf = &buf[len..];
            }
        }
    }
}
impl Drop for File {
    fn drop(&mut self) {
        crate::sys::close(self.descriptor);
    }
}
