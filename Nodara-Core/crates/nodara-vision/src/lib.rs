//! # nodara-vision
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

use nodara_core::CapabilityRegistry;

/// Node types this plugin provides.
pub const NODE_TYPES: &[&str] = &["vision.TemplateMatch", "vision.Ocr"];

/// Capability identifiers this plugin advertises.
pub const CAPABILITIES: &[&str] = &["Vision.TemplateMatch", "Vision.Ocr"];

/// Permissions this plugin requires from the host.
pub const PERMISSIONS: &[&str] = &["vision.analyze"];

/// Register every vision executor.
///
/// Also installs a CLI OCR backend from `NODARA_OCR_COMMAND` when that
/// variable is set, so `--in-process` matches the plugin binary.
pub fn register_vision(registry: &mut CapabilityRegistry) {
    install_backend_from_env();
    registry
        .register(template::TemplateMatchExecutor)
        .register(ocr::OcrExecutor);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn register_vision_installs_ocr_backend_from_env() {
        let _guard = ENV_LOCK.lock().expect("ocr env lock");
        let previous_backend = ocr_backend();
        let previous_env = std::env::var("NODARA_OCR_COMMAND").ok();

        backend::clear_ocr_backend();
        std::env::set_var("NODARA_OCR_COMMAND", "tesseract");

        let mut registry = CapabilityRegistry::new();
        register_vision(&mut registry);

        let installed = ocr_backend().expect("NODARA_OCR_COMMAND should install a backend");
        assert_eq!(installed.name(), "tesseract");
        assert!(registry.can_execute("vision.Ocr"));

        match previous_env {
            Some(value) => std::env::set_var("NODARA_OCR_COMMAND", value),
            None => std::env::remove_var("NODARA_OCR_COMMAND"),
        }
        match previous_backend {
            Some(backend) => set_ocr_backend(backend),
            None => backend::clear_ocr_backend(),
        }
    }
}
