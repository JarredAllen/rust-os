pub struct File {
    descriptor: i32,
}

impl File {
    pub fn open(path: &str) -> Self {
        let descriptor = crate::sys::open(path);
        Self { descriptor }
    }

    /// Read from this file into a buffer.
    ///
    /// Returns the written memory, which will be at the start of [`buf`].
    pub fn read<'a>(&self, buf: &'a mut [u8]) -> &'a mut [u8] {
        let read_length = crate::sys::read(self.descriptor, buf);
        &mut buf[..read_length]
    }
}
impl Drop for File {
    fn drop(&mut self) {
        crate::sys::close(self.descriptor);
    }
}
