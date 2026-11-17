//! Fallback used when building on a host without a native backend.
//!
//! Every operation reports [`PlatformError::Unsupported`] so the crate, and
//! therefore the workspace, still builds and its pure logic still tests.

use crate::error::{PlatformError, PlatformResult};

pub use crate::platform_types::{MouseButton, Rect, WindowId, WindowRecord};

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
pub fn send_key(_window: WindowId, _virtual_key: u8, _down: bool) -> PlatformResult<()> {
    Err(unsupported())
}

/// Unsupported on this host.
pub fn send_text(_window: WindowId, _character: char) -> PlatformResult<()> {
    Err(unsupported())
}

/// Unsupported on this host.
pub fn set_window_text(_window: WindowId, _text: &str) -> PlatformResult<()> {
    Err(unsupported())
}

/// Unsupported on this host.
pub fn screen_to_client(_window: WindowId, _x: i32, _y: i32) -> PlatformResult<(i32, i32)> {
    Err(unsupported())
}

/// Unsupported on this host.
pub fn send_mouse_move(
    _window: WindowId,
    _x: i32,
    _y: i32,
    _held: Option<MouseButton>,
) -> PlatformResult<()> {
    Err(unsupported())
}

/// Unsupported on this host.
pub fn send_mouse_button(
    _window: WindowId,
    _x: i32,
    _y: i32,
    _button: MouseButton,
    _down: bool,
) -> PlatformResult<()> {
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
