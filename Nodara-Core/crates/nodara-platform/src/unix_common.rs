//! Input and clipboard primitives shared by the macOS and Linux backends.
//!
//! Both platforms use `enigo` for synthetic input and `arboard` for the
//! clipboard. Window management and screen capture stay in the platform module
//! because those APIs differ substantially.

use std::cell::RefCell;

use arboard::Clipboard;
use enigo::{Button, Coordinate, Direction, Enigo, Key, Keyboard, Mouse, Settings};

use crate::error::{PlatformError, PlatformResult};
use crate::platform_types::{MouseButton, WindowId};

thread_local! {
    static ENIGO: RefCell<Option<Enigo>> = const { RefCell::new(None) };
    static CLIPBOARD: RefCell<Option<Clipboard>> = const { RefCell::new(None) };
}

fn backend_error(error: impl std::fmt::Display) -> PlatformError {
    PlatformError::Backend(error.to_string())
}

fn with_enigo<T>(
    action: impl FnOnce(&mut Enigo) -> Result<T, enigo::InputError>,
) -> PlatformResult<T> {
    ENIGO.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            let enigo = Enigo::new(&Settings::default()).map_err(backend_error)?;
            *slot = Some(enigo);
        }
        let enigo = slot.as_mut().expect("enigo is initialised");
        action(enigo).map_err(backend_error)
    })
}

fn with_clipboard<T>(
    action: impl FnOnce(&mut Clipboard) -> Result<T, arboard::Error>,
) -> PlatformResult<T> {
    CLIPBOARD.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            let clipboard =
                Clipboard::new().map_err(|error| PlatformError::Clipboard(error.to_string()))?;
            *slot = Some(clipboard);
        }
        let clipboard = slot.as_mut().expect("clipboard is initialised");
        action(clipboard).map_err(|error| PlatformError::Clipboard(error.to_string()))
    })
}

/// macOS has no Insert key; keep the code path total.
#[cfg(target_os = "macos")]
fn insert_key() -> Key {
    Key::Other(0x2D)
}

#[cfg(not(target_os = "macos"))]
fn insert_key() -> Key {
    Key::Insert
}

/// `F21`–`F24` do not exist in enigo's macOS key set.
#[cfg(target_os = "macos")]
fn high_function_key(virtual_key: u8) -> Key {
    Key::Other(u32::from(virtual_key))
}

#[cfg(not(target_os = "macos"))]
fn high_function_key(virtual_key: u8) -> Key {
    match virtual_key {
        0x84 => Key::F21,
        0x85 => Key::F22,
        0x86 => Key::F23,
        _ => Key::F24,
    }
}

fn button_for(button: MouseButton) -> Button {
    match button {
        MouseButton::Left => Button::Left,
        MouseButton::Right => Button::Right,
        MouseButton::Middle => Button::Middle,
    }
}

/// Translate a Windows virtual-key code into an `enigo` key.
///
/// `keys.rs` is intentionally Windows-shaped, so the translation lives here
/// rather than in the node code. Printable keys use `Key::Unicode`, which lets
/// enigo resolve the host keyboard layout; named keys map to their native
/// equivalents.
fn key_for(virtual_key: u8) -> Key {
    match virtual_key {
        0x08 => Key::Backspace,
        0x09 => Key::Tab,
        0x0D => Key::Return,
        0x1B => Key::Escape,
        0x20 => Key::Space,
        0x21 => Key::PageUp,
        0x22 => Key::PageDown,
        0x23 => Key::End,
        0x24 => Key::Home,
        0x25 => Key::LeftArrow,
        0x26 => Key::UpArrow,
        0x27 => Key::RightArrow,
        0x28 => Key::DownArrow,
        0x2D => insert_key(),
        0x2E => Key::Delete,
        0x14 => Key::CapsLock,
        0x10 => Key::Shift,
        0x11 => Key::Control,
        0x12 => Key::Alt,
        0x5B => Key::Meta,
        0x70 => Key::F1,
        0x71 => Key::F2,
        0x72 => Key::F3,
        0x73 => Key::F4,
        0x74 => Key::F5,
        0x75 => Key::F6,
        0x76 => Key::F7,
        0x77 => Key::F8,
        0x78 => Key::F9,
        0x79 => Key::F10,
        0x7A => Key::F11,
        0x7B => Key::F12,
        0x7C => Key::F13,
        0x7D => Key::F14,
        0x7E => Key::F15,
        0x7F => Key::F16,
        0x80 => Key::F17,
        0x81 => Key::F18,
        0x82 => Key::F19,
        0x83 => Key::F20,
        0x84..=0x87 => high_function_key(virtual_key),
        0xBA => Key::Unicode(';'),
        0xBB => Key::Unicode('='),
        0xBC => Key::Unicode(','),
        0xBD => Key::Unicode('-'),
        0xBE => Key::Unicode('.'),
        0xBF => Key::Unicode('/'),
        0xC0 => Key::Unicode('`'),
        0xDB => Key::Unicode('['),
        0xDC => Key::Unicode('\\'),
        0xDD => Key::Unicode(']'),
        0xDE => Key::Unicode('\''),
        b'A'..=b'Z' => Key::Unicode((virtual_key as char).to_ascii_lowercase()),
        b'0'..=b'9' => Key::Unicode(virtual_key as char),
        other => Key::Other(u32::from(other)),
    }
}

/// Press or release a virtual key globally.
pub fn key(virtual_key: u8, up: bool) {
    let direction = if up {
        Direction::Release
    } else {
        Direction::Press
    };
    let _ = with_enigo(|enigo| enigo.key(key_for(virtual_key), direction));
}

/// Press or release a virtual key.
///
/// The native backends have no portable way to post a key to a specific window,
/// so the target is ignored and the event goes to the focused window.
pub fn send_key(_window: WindowId, virtual_key: u8, down: bool) -> PlatformResult<()> {
    with_enigo(|enigo| {
        enigo.key(
            key_for(virtual_key),
            if down {
                Direction::Press
            } else {
                Direction::Release
            },
        )
    })
}

/// Type one character into the focused window.
pub fn send_text(_window: WindowId, character: char) -> PlatformResult<()> {
    let mut buffer = [0u8; 4];
    with_enigo(|enigo| enigo.text(character.encode_utf8(&mut buffer)))
}

/// Replacing a window's text is a Win32 concept.
pub fn set_window_text(_window: WindowId, _text: &str) -> PlatformResult<()> {
    Err(PlatformError::Unsupported(
        "set_text is only available on Windows".to_string(),
    ))
}

/// Move the cursor to absolute screen coordinates.
pub fn set_cursor(x: i32, y: i32) -> PlatformResult<()> {
    with_enigo(|enigo| enigo.move_mouse(x, y, Coordinate::Abs))
}

/// Current cursor position in screen coordinates.
pub fn cursor_position() -> PlatformResult<(i32, i32)> {
    ENIGO.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            let enigo = Enigo::new(&Settings::default()).map_err(backend_error)?;
            *slot = Some(enigo);
        }
        slot.as_ref()
            .expect("enigo is initialised")
            .location()
            .map_err(backend_error)
    })
}

/// Move the cursor to a window-relative point.
///
/// Non-Windows backends work in screen coordinates; callers pass the same point
/// they received from [`screen_to_client`], which is the identity mapping here.
pub fn send_mouse_move(
    _window: WindowId,
    x: i32,
    y: i32,
    _held: Option<MouseButton>,
) -> PlatformResult<()> {
    set_cursor(x, y)
}

/// Move to a point and press or release a mouse button.
pub fn send_mouse_button(
    _window: WindowId,
    x: i32,
    y: i32,
    button: MouseButton,
    down: bool,
) -> PlatformResult<()> {
    set_cursor(x, y)?;
    mouse_button(button, down)
}

/// Press or release a mouse button at the current cursor position.
pub fn mouse_button(button: MouseButton, down: bool) -> PlatformResult<()> {
    with_enigo(|enigo| {
        enigo.button(
            button_for(button),
            if down {
                Direction::Press
            } else {
                Direction::Release
            },
        )
    })
}

/// Screen coordinates are already client coordinates for global input.
pub fn screen_to_client(_window: WindowId, x: i32, y: i32) -> PlatformResult<(i32, i32)> {
    Ok((x, y))
}

/// Read the clipboard as text, if it currently holds any.
pub fn clipboard_read() -> PlatformResult<Option<String>> {
    with_clipboard(|clipboard| match clipboard.get_text() {
        Ok(text) => Ok(Some(text)),
        Err(arboard::Error::ContentNotAvailable) => Ok(None),
        Err(error) => Err(error),
    })
}

/// Replace the clipboard contents with `text`.
pub fn clipboard_write(text: &str) -> PlatformResult<()> {
    with_clipboard(|clipboard| clipboard.set_text(text.to_string()))
}
