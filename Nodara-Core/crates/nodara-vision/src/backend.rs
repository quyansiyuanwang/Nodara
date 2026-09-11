//! OCR backends.
//!
//! A backend turns image bytes into text. Engines are injected rather than
//! compiled in, so the plugin stays provider-neutral and can be deployed with
//! Windows OCR, Tesseract or a remote service without being rebuilt.

use std::path::Path;
use std::sync::Arc;

use parking_lot::Mutex;

use crate::error::{VisionError, VisionResult};

/// Recognised text plus optional layout information.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OcrResult {
    /// Full recognised text.
    pub text: String,
    /// Mean confidence in `0.0..=1.0`, when the engine reports one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    /// Per-line or per-word boxes, when the engine reports them.
    #[serde(default)]
    pub words: Vec<OcrWord>,
}

impl OcrResult {
    /// A plain text result with no layout information.
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            confidence: None,
            words: Vec::new(),
        }
    }
}

/// One recognised word with its bounding box.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OcrWord {
    /// Recognised text.
    pub text: String,
    /// Confidence in `0.0..=1.0`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    /// Left edge in pixels.
    pub x: i32,
    /// Top edge in pixels.
    pub y: i32,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

/// Turns an encoded image into text.
pub trait OcrBackend: Send + Sync {
    /// Human-readable backend name, used in logs and errors.
    fn name(&self) -> &str;

    /// Recognise text in `image` (an encoded PNG/JPEG/BMP byte stream).
    fn recognize(&self, image: &[u8], language: Option<&str>) -> VisionResult<OcrResult>;
}

/// A backend that always fails with [`VisionError::NoOcrBackend`].
#[derive(Debug, Default)]
pub struct NullOcrBackend;

impl OcrBackend for NullOcrBackend {
    fn name(&self) -> &'static str {
        "none"
    }

    fn recognize(&self, _image: &[u8], _language: Option<&str>) -> VisionResult<OcrResult> {
        Err(VisionError::NoOcrBackend)
    }
}

/// A backend that shells out to an external OCR program.
///
/// The program receives the path to a temporary image file as its first
/// argument and is expected to print the recognised text on stdout. This is the
/// simplest way to reuse an existing engine (Tesseract, a vendor CLI, a script).
#[derive(Debug, Clone)]
pub struct CliOcrBackend {
    program: String,
    extra_args: Vec<String>,
}

impl CliOcrBackend {
    /// Build a backend around `program`.
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            extra_args: Vec::new(),
        }
    }

    /// Append extra arguments passed before the image path.
    #[must_use]
    pub fn with_args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.extra_args = args.into_iter().map(Into::into).collect();
        self
    }

    /// The configured program.
    pub fn program(&self) -> &str {
        &self.program
    }
}

impl OcrBackend for CliOcrBackend {
    fn name(&self) -> &str {
        &self.program
    }

    fn recognize(&self, image: &[u8], _language: Option<&str>) -> VisionResult<OcrResult> {
        let directory = std::env::temp_dir().join(format!(
            "nodara-vision-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&directory)?;
        let path = directory.join("input.png");
        std::fs::write(&path, image)?;

        let output = std::process::Command::new(&self.program)
            .args(&self.extra_args)
            .arg(&path)
            .output();
        let _ = std::fs::remove_dir_all(&directory);

        let output = output.map_err(|error| {
            VisionError::Ocr(format!("could not run `{}`: {error}", self.program))
        })?;
        if !output.status.success() {
            return Err(VisionError::Ocr(format!(
                "`{}` exited with {}",
                self.program,
                output.status.code().unwrap_or(-1)
            )));
        }
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(OcrResult::text(text))
    }
}

static BACKEND: Mutex<Option<Arc<dyn OcrBackend>>> = Mutex::new(None);

/// Install the process-wide OCR backend.
pub fn set_ocr_backend(backend: Arc<dyn OcrBackend>) {
    *BACKEND.lock() = Some(backend);
}

/// The installed backend, if any.
pub fn ocr_backend() -> Option<Arc<dyn OcrBackend>> {
    BACKEND.lock().clone()
}

/// Install a [`CliOcrBackend`] from `NODARA_OCR_COMMAND`, when set.
pub fn install_backend_from_env() {
    if let Ok(program) = std::env::var("NODARA_OCR_COMMAND") {
        if !program.trim().is_empty() {
            set_ocr_backend(Arc::new(CliOcrBackend::new(program)));
        }
    }
}

/// Convenience: a short description of the active backend.
pub fn backend_name() -> String {
    ocr_backend().map_or_else(|| "none".to_string(), |backend| backend.name().to_string())
}

/// True when `path` exists, used by callers validating configured programs.
pub fn program_exists(path: &str) -> bool {
    Path::new(path).exists()
}
