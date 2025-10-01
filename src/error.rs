//! Error types.

use core::{error, fmt};

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

/// Possible kinds of errors.
#[derive(Debug, Clone, Copy)]
pub enum ErrorKind {
    OutOfMemory,
    Io,
    Unsupported,
}
impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::OutOfMemory => "Out of memory",
            Self::Io => "I/O Error",
            Self::Unsupported => "Unsupported operation",
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct OutOfMemory;
impl fmt::Display for OutOfMemory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Out of memory")
    }
}
impl core::error::Error for OutOfMemory {}
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
