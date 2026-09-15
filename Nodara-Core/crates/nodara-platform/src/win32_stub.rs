//! Fallback used when building on a non-Windows host.
//!
//! Every operation reports [`PlatformError::Unsupported`] so the crate, and
//! therefore the workspace, still builds and its pure logic still tests.

use crate::error::{PlatformError, PlatformResult};

/// Opaque window handle.
pub type WindowId = isize;

/// Screen rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    /// Left edge.
    pub x: i32,
    /// Top edge.
    pub y: i32,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

/// Description of a top-level window.
#[derive(Debug, Clone)]
pub struct WindowRecord {
    /// Opaque handle.
    pub id: WindowId,
    /// Window title.
    pub title: String,
    /// Window class.
    pub class_name: String,
    /// Executable file name that owns the window.
    pub process_name: String,
    /// Screen geometry.
    pub rect: Rect,
    /// Whether the window is visible.
    pub visible: bool,
}

/// Mouse buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    /// Primary button.
    Left,
    /// Secondary button.
    Right,
    /// Middle button.
    Middle,
}

fn unsupported() -> PlatformError {
    PlatformError::Unsupported(std::env::consts::OS.to_string())
}

/// No-op on unsupported hosts.
pub fn key(_virtual_key: u8, _up: bool) {}

/// Unsupported on this host.
pub fn set_cursor(_x: i32, _y: i32) -> PlatformResult<()> {
    Err(unsupported())
}

/// Unsupported on this host.
pub fn cursor_position() -> PlatformResult<(i32, i32)> {
    Err(unsupported())
}

/// Unsupported on this host.
pub fn post_key(_window: WindowId, _virtual_key: u8, _down: bool) -> PlatformResult<()> {
    Err(unsupported())
}

/// Unsupported on this host.
pub fn post_text(_window: WindowId, _code_unit: u16) -> PlatformResult<()> {
    Err(unsupported())
}

/// Unsupported on this host.
pub fn set_window_text(_window: WindowId, _text: &str) -> PlatformResult<()> {
    Err(unsupported())
}

/// Unsupported on this host.
pub fn mouse_button(_button: MouseButton, _down: bool) -> PlatformResult<()> {
    Err(unsupported())
}

/// Unsupported on this host.
pub fn screen_size() -> (i32, i32) {
    (0, 0)
}

/// Unsupported on this host.
pub fn foreground_window() -> WindowId {
    0
}

/// Unsupported on this host.
pub fn windows() -> Vec<WindowRecord> {
    Vec::new()
}

/// Unsupported on this host.
pub fn focus(_window: WindowId) -> PlatformResult<()> {
    Err(unsupported())
}

/// Unsupported on this host.
pub fn capture(_x: i32, _y: i32, _width: u32, _height: u32) -> PlatformResult<Vec<u8>> {
    Err(unsupported())
}

/// Unsupported on this host.
pub fn clipboard_read() -> PlatformResult<Option<String>> {
    Err(unsupported())
}

/// Unsupported on this host.
pub fn clipboard_write(_text: &str) -> PlatformResult<()> {
    Err(unsupported())
}
