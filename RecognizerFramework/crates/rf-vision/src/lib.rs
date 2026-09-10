//! # rf-vision
//!
//! The official vision capability set.
//!
//! Two node types are provided:
//!
//! * `vision.TemplateMatch` — locates a template image inside a captured frame;
//! * `vision.Ocr` — extracts text through an injected [`backend::OcrBackend`].
//!
//! OCR deliberately depends on a *backend* rather than bundling an engine. The
//! architecture document requires host-injected transports, and it keeps this
//! crate free of any particular recognition provider.

pub mod backend;
pub mod error;
pub mod ocr;
pub mod template;

pub use backend::{
    backend_name, install_backend_from_env, ocr_backend, set_ocr_backend, CliOcrBackend,
    NullOcrBackend, OcrBackend, OcrResult, OcrWord,
};
pub use error::{VisionError, VisionResult};

use rf_core::CapabilityRegistry;

/// Node types this plugin provides.
pub const NODE_TYPES: &[&str] = &["vision.TemplateMatch", "vision.Ocr"];

/// Capability identifiers this plugin advertises.
pub const CAPABILITIES: &[&str] = &["Vision.TemplateMatch", "Vision.Ocr"];

/// Permissions this plugin requires from the host.
pub const PERMISSIONS: &[&str] = &["vision.analyze"];

/// Register every vision executor.
pub fn register_vision(registry: &mut CapabilityRegistry) {
    registry
        .register(template::TemplateMatchExecutor)
        .register(ocr::OcrExecutor);
}
