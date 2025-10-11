//! Error types.

use core::{error, fmt};

pub use shared::ErrorKind;

pub type Result<T, E = Error> = core::result::Result<T, E>;

/// A generic error that can be produced.
#[derive(Debug)]
pub struct Error {
    /// The kind of the error.
    pub kind: ErrorKind,
}
impl From<ErrorKind> for Error {
    fn from(kind: ErrorKind) -> Self {
        Self { kind }
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.kind)
    }
}
impl error::Error for Error {}

#[derive(Debug, Clone, Copy)]
pub struct OutOfMemory;
impl fmt::Display for OutOfMemory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Out of memory")
    }
}
impl error::Error for OutOfMemory {}
impl From<OutOfMemory> for ErrorKind {
    fn from(OutOfMemory: OutOfMemory) -> Self {
        Self::OutOfMemory
    }
}
impl From<OutOfMemory> for Error {
    fn from(OutOfMemory: OutOfMemory) -> Self {
        Self {
            kind: ErrorKind::OutOfMemory,
        }
    }
}
