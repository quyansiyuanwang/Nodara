//! macOS backend: CoreGraphics window listing and capture via `xcap`, with
//! input and clipboard provided by the shared Unix layer.

use xcap::{Monitor, Window};

use crate::error::{PlatformError, PlatformResult};

pub use crate::platform_types::{MouseButton, Rect, WindowId, WindowRecord};
pub use crate::unix_common::{
    clipboard_read, clipboard_write, cursor_position, key, mouse_button, screen_to_client,
    send_key, send_mouse_button, send_mouse_move, send_text, set_cursor, set_window_text,
};

fn backend_error(error: impl std::fmt::Display) -> PlatformError {
    PlatformError::Backend(error.to_string())
}

fn record(window: &Window) -> WindowRecord {
    let id = window.id().map_or(0, |id| id as WindowId);
    let title = window.title().unwrap_or_default();
    let app_name = window.app_name().unwrap_or_default();
    let x = window.x().unwrap_or(0);
    let y = window.y().unwrap_or(0);
    let width = window.width().unwrap_or(0);
    let height = window.height().unwrap_or(0);
    let visible = !window.is_minimized().unwrap_or(false);
    WindowRecord {
        id,
        title,
        class_name: app_name.clone(),
        process_name: app_name,
        rect: Rect {
            x,
            y,
            width,
            height,
        },
        visible,
    }
}

/// Every top-level window reported by CoreGraphics.
pub fn windows() -> Vec<WindowRecord> {
    Window::all()
        .map(|windows| windows.iter().map(record).collect())
        .unwrap_or_default()
}

/// The focused window, or `0` when macOS does not report one.
pub fn foreground_window() -> WindowId {
    Window::all()
        .ok()
        .and_then(|windows| {
            windows
                .into_iter()
                .find(|window| window.is_focused().unwrap_or(false))
                .map(|window| window.id().unwrap_or(0) as WindowId)
        })
        .unwrap_or(0)
}

/// Bring a window's application to the foreground.
///
/// macOS activates applications, not individual windows. System Events is used
/// because it also raises the application's frontmost window; the process needs
/// Accessibility permission for this to take effect.
pub fn focus(window: WindowId) -> PlatformResult<()> {
    let windows = Window::all().map_err(backend_error)?;
    let target = windows
        .into_iter()
        .find(|candidate| matches!(candidate.id(), Ok(id) if id as WindowId == window))
        .ok_or_else(|| PlatformError::WindowNotFound {
            query: window.to_string(),
        })?;
    let pid = target.pid().map_err(backend_error)?;
    let script = format!(
        "tell application \"System Events\" to set frontmost of first process whose unix id is {pid} to true"
    );
    let status = std::process::Command::new("osascript")
        .arg("-e")
        .arg(script)
        .status()
        .map_err(PlatformError::Io)?;
    if status.success() {
        Ok(())
    } else {
        Err(PlatformError::Backend(
            "System Events refused to activate the application (Accessibility permission?)"
                .to_string(),
        ))
    }
}

/// Primary display dimensions in pixels.
pub fn screen_size() -> (i32, i32) {
    let monitors = Monitor::all().unwrap_or_default();
    let monitor = monitors
        .iter()
        .find(|monitor| monitor.is_primary().unwrap_or(false))
        .or_else(|| monitors.first());
    monitor.map_or((0, 0), |monitor| {
        (
            monitor.width().unwrap_or(0) as i32,
            monitor.height().unwrap_or(0) as i32,
        )
    })
}

/// Capture a rectangle of the display as BGRA pixels.
pub fn capture(x: i32, y: i32, width: u32, height: u32) -> PlatformResult<Vec<u8>> {
    if width == 0 || height == 0 {
        return Err(PlatformError::Capture(
            "capture rectangle must not be empty".to_string(),
        ));
    }
    let monitor = match Monitor::from_point(x, y) {
        Ok(monitor) => monitor,
        Err(_) => Monitor::all()
            .map_err(backend_error)?
            .into_iter()
            .next()
            .ok_or_else(|| PlatformError::Capture("no monitors were reported".to_string()))?,
    };
    let monitor_x = monitor.x().map_err(backend_error)?;
    let monitor_y = monitor.y().map_err(backend_error)?;
    let relative_x = (x - monitor_x).max(0) as u32;
    let relative_y = (y - monitor_y).max(0) as u32;
    let image = monitor
        .capture_region(relative_x, relative_y, width, height)
        .map_err(backend_error)?;
    let rgba = image.into_raw();
    let mut bgra = Vec::with_capacity(rgba.len());
    for pixel in rgba.chunks_exact(4) {
        bgra.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 0xFF]);
    }
    Ok(bgra)
}
