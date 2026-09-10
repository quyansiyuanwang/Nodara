//! Platform error type.

use thiserror::Error;

/// Convenience alias.
pub type PlatformResult<T> = Result<T, PlatformError>;

/// Failures raised by host automation primitives.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum PlatformError {
    /// The operating system is not supported by this build.
    #[error("platform is not supported: {0}")]
    Unsupported(String),

    /// A Win32 call failed.
    #[error("{operation} failed (win32 error {code})")]
    Win32 {
        /// Operation that failed.
        operation: &'static str,
        /// `GetLastError` value.
        code: u32,
    },

    /// A window could not be located.
    #[error("no window matched `{query}`")]
    WindowNotFound {
        /// The title or handle that was searched for.
        query: String,
    },

    /// The clipboard could not be read or written.
    #[error("clipboard error: {0}")]
    Clipboard(String),

    /// A screen capture could not be produced.
    #[error("capture error: {0}")]
    Capture(String),

    /// An image could not be encoded.
    #[error("image error: {0}")]
    Image(String),

    /// A key name was not recognised.
    #[error("unknown key `{0}`")]
    UnknownKey(String),

    /// Underlying I/O failure.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
