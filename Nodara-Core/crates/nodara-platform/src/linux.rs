//! Linux backend: X11 window management and screen capture through `x11rb`,
//! with input and clipboard provided by the shared Unix layer.
//!
//! The implementation targets X11 and XWayland. Native Wayland compositors do
//! not expose a portable window-enumeration or capture protocol, so workflows
//! that need those capabilities should run the target application under XWayland.

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    AtomEnum, ClientMessageEvent, ConnectionExt, EventMask, ImageFormat, MapState,
};
use x11rb::rust_connection::RustConnection;

use crate::error::{PlatformError, PlatformResult};

pub use crate::platform_types::{MouseButton, Rect, WindowId, WindowRecord};
pub use crate::unix_common::{
    clipboard_read, clipboard_write, cursor_position, key, mouse_button, screen_to_client,
    send_key, send_mouse_button, send_mouse_move, send_text, set_cursor, set_window_text,
};

fn backend_error(error: impl std::fmt::Display) -> PlatformError {
    PlatformError::Backend(error.to_string())
}

fn connect() -> PlatformResult<(RustConnection, usize)> {
    x11rb::connect(None).map_err(backend_error)
}

fn atom(connection: &RustConnection, name: &str) -> PlatformResult<u32> {
    connection
        .intern_atom(false, name.as_bytes())
        .map_err(backend_error)?
        .reply()
        .map(|reply| reply.atom)
        .map_err(backend_error)
}

fn root_window(connection: &RustConnection, screen: usize) -> u32 {
    connection.setup().roots[screen].root
}

fn property_u32(
    connection: &RustConnection,
    window: u32,
    property: u32,
) -> PlatformResult<Vec<u32>> {
    let reply = connection
        .get_property(false, window, property, AtomEnum::ANY, 0, 4096)
        .map_err(backend_error)?
        .reply()
        .map_err(backend_error)?;
    Ok(reply.value32().map(Iterator::collect).unwrap_or_default())
}

fn property_text(
    connection: &RustConnection,
    window: u32,
    property: u32,
) -> PlatformResult<Option<String>> {
    let reply = connection
        .get_property(false, window, property, AtomEnum::ANY, 0, 4096)
        .map_err(backend_error)?
        .reply()
        .map_err(backend_error)?;
    if reply.value.is_empty() {
        return Ok(None);
    }
    let text = String::from_utf8_lossy(&reply.value)
        .trim_end_matches('\0')
        .to_string();
    Ok((!text.is_empty()).then_some(text))
}

fn window_title(connection: &RustConnection, window: u32, net_wm_name: u32) -> String {
    property_text(connection, window, net_wm_name)
        .ok()
        .flatten()
        .or_else(|| {
            let wm_name = atom(connection, "WM_NAME").ok()?;
            property_text(connection, window, wm_name).ok().flatten()
        })
        .unwrap_or_default()
}

fn window_class(connection: &RustConnection, window: u32) -> String {
    let Ok(wm_class) = atom(connection, "WM_CLASS") else {
        return String::new();
    };
    let Ok(reply) = connection
        .get_property(false, window, wm_class, AtomEnum::ANY, 0, 1024)
        .map_err(backend_error)
        .and_then(|cookie| cookie.reply().map_err(backend_error))
    else {
        return String::new();
    };
    reply
        .value
        .split(|byte| *byte == 0)
        .rfind(|part| !part.is_empty())
        .map(|part| String::from_utf8_lossy(part).to_string())
        .unwrap_or_default()
}

fn process_name(pid: u32) -> String {
    if pid == 0 {
        return String::new();
    }
    std::fs::read_to_string(format!("/proc/{pid}/comm"))
        .map(|name| name.trim().to_string())
        .unwrap_or_default()
}

fn window_record(
    connection: &RustConnection,
    root: u32,
    window: u32,
    net_wm_name: u32,
) -> Option<WindowRecord> {
    let geometry = connection.get_geometry(window).ok()?.reply().ok()?;
    let translated = connection
        .translate_coordinates(window, root, 0, 0)
        .ok()?
        .reply()
        .ok()?;
    let attributes = connection
        .get_window_attributes(window)
        .ok()?
        .reply()
        .ok()?;
    let pid = atom(connection, "_NET_WM_PID")
        .ok()
        .and_then(|property| property_u32(connection, window, property).ok())
        .and_then(|values| values.first().copied())
        .unwrap_or(0);
    let class_name = window_class(connection, window);
    Some(WindowRecord {
        id: window as WindowId,
        title: window_title(connection, window, net_wm_name),
        process_name: {
            let name = process_name(pid);
            if name.is_empty() {
                class_name.clone()
            } else {
                name
            }
        },
        class_name,
        rect: Rect {
            x: i32::from(translated.dst_x),
            y: i32::from(translated.dst_y),
            width: u32::from(geometry.width),
            height: u32::from(geometry.height),
        },
        visible: attributes.map_state == MapState::VIEWABLE,
    })
}

/// Every top-level window advertised through `_NET_CLIENT_LIST`.
pub fn windows() -> Vec<WindowRecord> {
    let Ok((connection, screen)) = connect() else {
        return Vec::new();
    };
    let root = root_window(&connection, screen);
    let Ok(client_list) = atom(&connection, "_NET_CLIENT_LIST") else {
        return Vec::new();
    };
    let Ok(net_wm_name) = atom(&connection, "_NET_WM_NAME") else {
        return Vec::new();
    };
    property_u32(&connection, root, client_list)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|window| window_record(&connection, root, window, net_wm_name))
        .collect()
}

/// The window referenced by `_NET_ACTIVE_WINDOW`, or `0`.
pub fn foreground_window() -> WindowId {
    let Ok((connection, screen)) = connect() else {
        return 0;
    };
    let root = root_window(&connection, screen);
    atom(&connection, "_NET_ACTIVE_WINDOW")
        .ok()
        .and_then(|property| property_u32(&connection, root, property).ok())
        .and_then(|values| values.first().copied())
        .map_or(0, |window| window as WindowId)
}

/// Ask the window manager to activate a window.
pub fn focus(window: WindowId) -> PlatformResult<()> {
    let (connection, screen) = connect()?;
    let root = root_window(&connection, screen);
    let active = atom(&connection, "_NET_ACTIVE_WINDOW")?;
    let event = ClientMessageEvent::new(32, window as u32, active, [2, 0, 0, 0, 0]);
    connection
        .send_event(
            false,
            root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            event,
        )
        .map_err(backend_error)?;
    connection.flush().map_err(backend_error)?;
    Ok(())
}

/// Primary display dimensions in pixels.
pub fn screen_size() -> (i32, i32) {
    let Ok((connection, screen)) = connect() else {
        return (0, 0);
    };
    let root = &connection.setup().roots[screen];
    (
        i32::from(root.width_in_pixels),
        i32::from(root.height_in_pixels),
    )
}

/// Capture a rectangle of the root window as BGRA pixels.
pub fn capture(x: i32, y: i32, width: u32, height: u32) -> PlatformResult<Vec<u8>> {
    if width == 0 || height == 0 {
        return Err(PlatformError::Capture(
            "capture rectangle must not be empty".to_string(),
        ));
    }
    let (connection, screen) = connect()?;
    let root = root_window(&connection, screen);
    let depth = connection.setup().roots[screen].root_depth;
    if depth != 24 && depth != 32 {
        return Err(PlatformError::Capture(format!(
            "unsupported X11 root depth {depth}; 24 or 32 bpp is required"
        )));
    }
    let reply = connection
        .get_image(
            ImageFormat::Z_PIXMAP,
            root,
            x as i16,
            y as i16,
            width as u16,
            height as u16,
            !0,
        )
        .map_err(backend_error)?
        .reply()
        .map_err(backend_error)?;

    let mut bgra = Vec::with_capacity((width as usize) * (height as usize) * 4);
    for pixel in reply.data.as_chunks::<4>().0 {
        if cfg!(target_endian = "little") {
            bgra.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 0xFF]);
        } else {
            bgra.extend_from_slice(&[pixel[3], pixel[2], pixel[1], 0xFF]);
        }
    }
    Ok(bgra)
}
