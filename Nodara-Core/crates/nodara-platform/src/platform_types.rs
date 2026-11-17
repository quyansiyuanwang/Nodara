//! Types shared by every native backend.
//!
//! The backend modules re-export these, so callers keep using
//! `native::WindowRecord` and friends regardless of the host operating system.

/// A native window handle, kept opaque to callers.
///
/// Windows stores an `HWND`, macOS a `CGWindowID` and X11 an `XID`; all three fit
/// in an `isize`, and the JSON representation stays a plain number.
pub type WindowId = isize;

/// Geometry of a window in screen coordinates.
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

/// Description of one top-level window.
#[derive(Debug, Clone)]
pub struct WindowRecord {
    /// Opaque handle.
    pub id: WindowId,
    /// Window title.
    pub title: String,
    /// Window class name, or the closest platform equivalent.
    pub class_name: String,
    /// Executable or application name that owns the window.
    pub process_name: String,
    /// Screen geometry.
    pub rect: Rect,
    /// Whether the window is visible.
    pub visible: bool,
}

/// Mouse buttons supported by the plugin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    /// Primary button.
    Left,
    /// Secondary button.
    Right,
    /// Middle button.
    Middle,
}
