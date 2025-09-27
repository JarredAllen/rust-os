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
#[derive(Debug)]
pub enum ErrorKind {
    Io,
    Unsupported,
}
impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Io => "I/O Error",
            Self::Unsupported => "Unsupported operation",
        })
    }
}
