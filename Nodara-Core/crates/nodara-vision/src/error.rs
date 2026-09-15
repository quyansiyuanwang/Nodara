//! Vision error type.

use thiserror::Error;

/// Convenience alias.
pub type VisionResult<T> = Result<T, VisionError>;

/// Failures raised by vision primitives.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum VisionError {
    /// Underlying I/O failure.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// An image could not be decoded.
    #[error("image error: {0}")]
    Image(String),

    /// An artefact or file could not be found.
    #[error("image source not found: {0}")]
    NotFound(String),

    /// The requested search region is outside the frame.
    #[error("invalid search region: {0}")]
    InvalidRegion(String),

    /// The template is larger than the frame it is being searched in.
    #[error("template ({tw}x{th}) is larger than the frame ({fw}x{fh})")]
    TemplateTooLarge {
        /// Template width.
        tw: u32,
        /// Template height.
        th: u32,
        /// Frame width.
        fw: u32,
        /// Frame height.
        fh: u32,
    },

    /// No OCR backend has been installed.
    #[error(
        "no OCR backend is configured; install one with `set_ocr_backend` or NODARA_OCR_COMMAND"
    )]
    NoOcrBackend,

    /// The OCR backend failed.
    #[error("ocr backend failed: {0}")]
    Ocr(String),
}
