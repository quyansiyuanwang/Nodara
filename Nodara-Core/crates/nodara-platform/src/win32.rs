//! Thin, safe wrappers over the Win32 calls this plugin needs.
//!
//! Every `unsafe` block is confined to this module and documented with the
//! invariant that makes it sound. Nothing above this layer handles raw handles.

#![allow(unsafe_code)]

use windows_sys::Win32::Foundation::{CloseHandle, GlobalFree, HGLOBAL, HWND, LPARAM, POINT, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
    ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, SRCCOPY,
};
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    SetClipboardData,
};
use windows_sys::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows_sys::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    keybd_event, mouse_event, MapVirtualKeyW, KEYEVENTF_KEYUP, MAPVK_VK_TO_VSC,
    MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP,
    MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetCursorPos, GetForegroundWindow, GetSystemMetrics, GetWindowRect,
    GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible, PostMessageW,
    SetCursorPos, SetForegroundWindow, SetWindowTextW, SM_CXSCREEN, SM_CYSCREEN, WM_CHAR,
    WM_KEYDOWN, WM_KEYUP,
};

use crate::error::{PlatformError, PlatformResult};

/// Clipboard format identifier for UTF-16 text (`CF_UNICODETEXT`).
///
/// Spelled out here because `windows-sys` publishes it under a feature this
/// crate does not otherwise need; the value is fixed by the Win32 ABI.
const CF_UNICODETEXT: u32 = 13;

/// A native window handle, kept opaque to callers.
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
    /// Window class name.
    pub class_name: String,
    /// Executable file name that owns the window, when it can be queried.
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

fn last_error(operation: &'static str) -> PlatformError {
    PlatformError::Win32 {
        operation,
        code: std::io::Error::last_os_error().raw_os_error().unwrap_or(0) as u32,
    }
}

/// Press or release a virtual key.
pub fn key(virtual_key: u8, up: bool) {
    // SAFETY: `keybd_event` accepts any virtual-key code; the flags are a
    // documented bit set and the extra-info argument is documented as unused.
    unsafe {
        keybd_event(virtual_key, 0, if up { KEYEVENTF_KEYUP } else { 0 }, 0);
    }
}

/// Move the cursor to absolute screen coordinates.
pub fn set_cursor(x: i32, y: i32) -> PlatformResult<()> {
    // SAFETY: no pointer arguments; the call only moves the cursor.
    let ok = unsafe { SetCursorPos(x, y) };
    if ok == 0 {
        Err(last_error("SetCursorPos"))
    } else {
        Ok(())
    }
}

/// Current cursor position in virtual-screen coordinates.
pub fn cursor_position() -> PlatformResult<(i32, i32)> {
    // SAFETY: `POINT` is a plain value struct that the call fills in.
    unsafe {
        let mut point: POINT = std::mem::zeroed();
        if GetCursorPos(&mut point) == 0 {
            Err(last_error("GetCursorPos"))
        } else {
            Ok((point.x, point.y))
        }
    }
}

/// Post a key transition to a specific window without changing focus.
pub fn post_key(window: WindowId, virtual_key: u8, down: bool) -> PlatformResult<()> {
    // SAFETY: both calls accept plain integer arguments. `PostMessageW` only
    // queues a message to a system-owned window handle.
    unsafe {
        let scan = MapVirtualKeyW(u32::from(virtual_key), MAPVK_VK_TO_VSC);
        let mut lparam = 1isize | ((scan as isize) << 16);
        if !down {
            lparam |= 1isize << 30;
            lparam |= 1isize << 31;
        }
        let message = if down { WM_KEYDOWN } else { WM_KEYUP };
        if PostMessageW(window, message, usize::from(virtual_key), lparam) == 0 {
            Err(last_error("PostMessageW(WM_KEY)"))
        } else {
            Ok(())
        }
    }
}

/// Post one UTF-16 code unit as `WM_CHAR` to a specific window.
pub fn post_text(window: WindowId, code_unit: u16) -> PlatformResult<()> {
    // SAFETY: `PostMessageW` only queues a message to a system-owned handle.
    let ok = unsafe { PostMessageW(window, WM_CHAR, usize::from(code_unit), 0) };
    if ok == 0 {
        Err(last_error("PostMessageW(WM_CHAR)"))
    } else {
        Ok(())
    }
}

/// Replace a window's text directly. Intended as a fallback for controls that
/// do not process posted `WM_CHAR` messages.
pub fn set_window_text(window: WindowId, text: &str) -> PlatformResult<()> {
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: the buffer is null-terminated and remains alive for the call.
    let ok = unsafe { SetWindowTextW(window, wide.as_ptr()) };
    if ok == 0 {
        Err(last_error("SetWindowTextW"))
    } else {
        Ok(())
    }
}

/// Synthesise a mouse button transition at the current cursor position.
pub fn mouse_button(button: MouseButton, down: bool) -> PlatformResult<()> {
    let flags = match (button, down) {
        (MouseButton::Left, true) => MOUSEEVENTF_LEFTDOWN,
        (MouseButton::Left, false) => MOUSEEVENTF_LEFTUP,
        (MouseButton::Right, true) => MOUSEEVENTF_RIGHTDOWN,
        (MouseButton::Right, false) => MOUSEEVENTF_RIGHTUP,
        (MouseButton::Middle, true) => MOUSEEVENTF_MIDDLEDOWN,
        (MouseButton::Middle, false) => MOUSEEVENTF_MIDDLEUP,
    };
    // SAFETY: `mouse_event` synthesises input from the documented flag bits; the
    // data and extra-info arguments are documented as unused for these flags.
    unsafe {
        mouse_event(flags, 0, 0, 0, 0);
    }
    Ok(())
}

/// Primary display dimensions in pixels.
pub fn screen_size() -> (i32, i32) {
    // SAFETY: `GetSystemMetrics` takes an index and returns an integer.
    unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) }
}

/// Handle of the foreground window.
pub fn foreground_window() -> WindowId {
    // SAFETY: returns an opaque handle owned by the system.
    unsafe { GetForegroundWindow() }
}

fn read_wide(buffer: &[u16]) -> String {
    let end = buffer
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..end])
}

fn window_title(hwnd: HWND) -> String {
    // SAFETY: `GetWindowTextLengthW` only inspects the window; the buffer is
    // sized from its result and fully written before being read.
    unsafe {
        let length = GetWindowTextLengthW(hwnd);
        if length <= 0 {
            return String::new();
        }
        let mut buffer = vec![0u16; length as usize + 1];
        let written = GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32);
        if written <= 0 {
            return String::new();
        }
        read_wide(&buffer)
    }
}

fn window_class(hwnd: HWND) -> String {
    // SAFETY: fixed-size buffer that the call fully initialises before reading.
    unsafe {
        let mut buffer = vec![0u16; 256];
        let written = GetClassNameW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32);
        if written <= 0 {
            return String::new();
        }
        read_wide(&buffer)
    }
}

fn window_rect(hwnd: HWND) -> Rect {
    // SAFETY: `RECT` is a plain value struct that the call fills in.
    unsafe {
        let mut rect: RECT = std::mem::zeroed();
        if GetWindowRect(hwnd, &mut rect) == 0 {
            return Rect {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            };
        }
        Rect {
            x: rect.left,
            y: rect.top,
            width: (rect.right - rect.left).max(0) as u32,
            height: (rect.bottom - rect.top).max(0) as u32,
        }
    }
}

fn window_process_name(hwnd: HWND) -> String {
    // SAFETY: `GetWindowThreadProcessId` only writes the process id into the
    // supplied stack value. The process handle returned by `OpenProcess` is
    // closed on every path, and the query buffer is sized before the call.
    unsafe {
        let mut process_id = 0u32;
        GetWindowThreadProcessId(hwnd, &mut process_id);
        if process_id == 0 {
            return String::new();
        }
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id);
        if process == 0 {
            return String::new();
        }
        let mut buffer = vec![0u16; 1024];
        let mut length = buffer.len() as u32;
        let ok = QueryFullProcessImageNameW(process, 0, buffer.as_mut_ptr(), &mut length);
        CloseHandle(process);
        if ok == 0 || length == 0 {
            return String::new();
        }
        let path = read_wide(&buffer[..length as usize]);
        std::path::Path::new(&path)
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .unwrap_or(&path)
            .to_string()
    }
}

/// Enumerate every top-level window.
pub fn windows() -> Vec<WindowRecord> {
    let mut records: Vec<WindowRecord> = Vec::new();
    let context = (&mut records as *mut Vec<WindowRecord>) as LPARAM;

    // SAFETY: the callback matches the `WNDENUMPROC` signature and the pointer
    // handed through `LPARAM` is valid for the whole enumeration, which the call
    // performs synchronously before returning.
    unsafe extern "system" fn callback(hwnd: HWND, lparam: LPARAM) -> i32 {
        let records = &mut *(lparam as *mut Vec<WindowRecord>);
        records.push(WindowRecord {
            id: hwnd,
            title: window_title(hwnd),
            class_name: window_class(hwnd),
            process_name: window_process_name(hwnd),
            rect: window_rect(hwnd),
            // SAFETY: read-only query on a handle supplied by the system.
            visible: IsWindowVisible(hwnd) != 0,
        });
        1
    }

    // SAFETY: see above; the enumeration is synchronous and single-threaded.
    unsafe {
        EnumWindows(Some(callback), context);
    }
    records
}

/// Bring a window to the foreground.
pub fn focus(window: WindowId) -> PlatformResult<()> {
    // SAFETY: the handle came from the system and the call only changes z-order.
    let ok = unsafe { SetForegroundWindow(window) };
    if ok == 0 {
        Err(last_error("SetForegroundWindow"))
    } else {
        Ok(())
    }
}

/// Capture a rectangle of the primary display as BGRA pixels.
pub fn capture(x: i32, y: i32, width: u32, height: u32) -> PlatformResult<Vec<u8>> {
    if width == 0 || height == 0 {
        return Err(PlatformError::Capture(
            "capture rectangle must not be empty".to_string(),
        ));
    }
    // SAFETY: every GDI handle is created and released within this function, and
    // each buffer passed to `GetDIBits` is sized from the same dimensions.
    unsafe {
        let screen_dc = GetDC(0);
        if screen_dc == 0 {
            return Err(last_error("GetDC"));
        }
        let memory_dc = CreateCompatibleDC(screen_dc);
        if memory_dc == 0 {
            ReleaseDC(0, screen_dc);
            return Err(last_error("CreateCompatibleDC"));
        }
        let bitmap = CreateCompatibleBitmap(screen_dc, width as i32, height as i32);
        if bitmap == 0 {
            DeleteDC(memory_dc);
            ReleaseDC(0, screen_dc);
            return Err(last_error("CreateCompatibleBitmap"));
        }
        let previous = SelectObject(memory_dc, bitmap);
        let copied = BitBlt(
            memory_dc,
            0,
            0,
            width as i32,
            height as i32,
            screen_dc,
            x,
            y,
            SRCCOPY,
        );

        let mut header = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                biHeight: -(height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..std::mem::zeroed()
            },
            bmiColors: [std::mem::zeroed()],
        };
        let mut pixels = vec![0u8; (width as usize) * (height as usize) * 4];
        let scanned = GetDIBits(
            memory_dc,
            bitmap,
            0,
            height,
            pixels.as_mut_ptr().cast(),
            &mut header,
            DIB_RGB_COLORS,
        );

        SelectObject(memory_dc, previous);
        DeleteObject(bitmap);
        DeleteDC(memory_dc);
        ReleaseDC(0, screen_dc);

        if copied == 0 {
            return Err(last_error("BitBlt"));
        }
        if scanned == 0 {
            return Err(last_error("GetDIBits"));
        }
        Ok(pixels)
    }
}

/// Read the clipboard as text, if it currently holds any.
pub fn clipboard_read() -> PlatformResult<Option<String>> {
    // SAFETY: clipboard access is serialised by the OS. The handle returned by
    // `GetClipboardData` belongs to the clipboard and is only read while the
    // clipboard is open; it is never freed here.
    unsafe {
        if IsClipboardFormatAvailable(CF_UNICODETEXT) == 0 {
            return Ok(None);
        }
        if OpenClipboard(0) == 0 {
            return Err(PlatformError::Clipboard("OpenClipboard failed".to_string()));
        }
        let handle = GetClipboardData(CF_UNICODETEXT);
        let text = if handle == 0 {
            None
        } else {
            let global: HGLOBAL = handle as HGLOBAL;
            let pointer = GlobalLock(global);
            if pointer.is_null() {
                None
            } else {
                let units = pointer.cast::<u16>();
                let mut length = 0usize;
                while *units.add(length) != 0 && length < 1 << 20 {
                    length += 1;
                }
                let slice = std::slice::from_raw_parts(units, length);
                let text = String::from_utf16_lossy(slice);
                GlobalUnlock(global);
                Some(text)
            }
        };
        CloseClipboard();
        Ok(text)
    }
}

/// Replace the clipboard contents with `text`.
pub fn clipboard_write(text: &str) -> PlatformResult<()> {
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let bytes = wide.len() * std::mem::size_of::<u16>();

    // SAFETY: the global block is allocated, filled and handed to the clipboard,
    // which takes ownership on success. On any failure it is freed here.
    unsafe {
        if OpenClipboard(0) == 0 {
            return Err(PlatformError::Clipboard("OpenClipboard failed".to_string()));
        }
        if EmptyClipboard() == 0 {
            CloseClipboard();
            return Err(PlatformError::Clipboard(
                "EmptyClipboard failed".to_string(),
            ));
        }
        let block = GlobalAlloc(GMEM_MOVEABLE, bytes);
        if block.is_null() {
            CloseClipboard();
            return Err(PlatformError::Clipboard("GlobalAlloc failed".to_string()));
        }
        let destination = GlobalLock(block);
        if destination.is_null() {
            GlobalFree(block);
            CloseClipboard();
            return Err(PlatformError::Clipboard("GlobalLock failed".to_string()));
        }
        std::ptr::copy_nonoverlapping(wide.as_ptr().cast::<u8>(), destination.cast::<u8>(), bytes);
        GlobalUnlock(block);
        if SetClipboardData(CF_UNICODETEXT, block as isize) == 0 {
            GlobalFree(block);
            CloseClipboard();
            return Err(PlatformError::Clipboard(
                "SetClipboardData failed".to_string(),
            ));
        }
        CloseClipboard();
    }
    Ok(())
}
