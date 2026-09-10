//! # rf-platform
//!
//! The official host-automation capability set: keyboard, mouse, window
//! management, screen capture and the clipboard.
//!
//! Everything here is an ordinary [`rf_core::NodeExecutor`]. The crate builds two
//! artefacts from the same code:
//!
//! * `rf-platform-plugin` — a standalone process speaking the plugin protocol,
//!   so the runtime can load these capabilities out of process;
//! * the library itself, so an embedded host can register them in process.
//!
//! Nothing in `rf-core` knows this crate exists, which is the point of the
//! architecture.

pub mod capture;
pub mod clipboard;
pub mod error;
pub mod input;
pub mod keys;
pub mod window;

#[cfg(windows)]
mod win32;
#[cfg(not(windows))]
mod win32_stub;

#[cfg(windows)]
use win32 as native;
#[cfg(not(windows))]
use win32_stub as native;

pub use error::{PlatformError, PlatformResult};

use rf_core::CapabilityRegistry;

/// Node types this plugin provides.
pub const NODE_TYPES: &[&str] = &[
    "windows.Input.Keyboard",
    "windows.Input.Mouse",
    "windows.Input.Text",
    "windows.Window.Find",
    "windows.Window.Focus",
    "windows.Window.Capture",
    "windows.Desktop.Capture",
    "system.Clipboard",
];

/// Capability identifiers this plugin advertises.
pub const CAPABILITIES: &[&str] = &[
    "Input.Keyboard",
    "Input.Mouse",
    "Window.Find",
    "Window.Focus",
    "Window.Capture",
    "Desktop.Capture",
    "Clipboard.Read",
    "Clipboard.Write",
];

/// Permissions this plugin requires from the host.
pub const PERMISSIONS: &[&str] = &[
    "input.control",
    "window.control",
    "screen.capture",
    "clipboard",
];

/// Register every platform executor.
pub fn register_platform(registry: &mut CapabilityRegistry) {
    registry
        .register(input::KeyboardExecutor)
        .register(input::MouseExecutor)
        .register(input::TextExecutor)
        .register(window::FindExecutor)
        .register(window::FocusExecutor)
        .register(window::CaptureExecutor)
        .register(capture::DesktopCaptureExecutor)
        .register(clipboard::ClipboardExecutor);
}

/// True when the host operating system is supported by this build.
pub const fn is_supported() -> bool {
    cfg!(windows)
}
