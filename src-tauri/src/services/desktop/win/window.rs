#[cfg(target_os = "windows")]
use core::ffi::c_void;
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, RECT, WPARAM};
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetForegroundWindow, GetWindowRect, GetWindowTextW,
    GetWindowThreadProcessId, IsWindow, IsWindowVisible, PostMessageW, SetForegroundWindow,
    ShowWindow, SW_RESTORE, WM_CLOSE,
};

use crate::services::desktop::types::{DesktopRect, WindowSnapshot};

#[cfg(target_os = "windows")]
struct EnumWindowsContext {
    windows: *mut Vec<WindowSnapshot>,
    active_hwnd: HWND,
}

#[cfg(target_os = "windows")]
fn hwnd_from_i64(hwnd_value: i64) -> HWND {
    HWND(hwnd_value as isize as *mut c_void)
}

#[cfg(target_os = "windows")]
fn hwnd_to_i64(hwnd: HWND) -> i64 {
    hwnd.0 as isize as i64
}

#[cfg(target_os = "windows")]
fn window_text(hwnd: HWND) -> String {
    let mut buffer = [0u16; 1024];
    let len = unsafe { GetWindowTextW(hwnd, &mut buffer) };
    if len <= 0 {
        String::new()
    } else {
        String::from_utf16_lossy(&buffer[..len as usize])
            .trim()
            .to_string()
    }
}

#[cfg(target_os = "windows")]
fn class_name(hwnd: HWND) -> String {
    let mut buffer = [0u16; 256];
    let len = unsafe { GetClassNameW(hwnd, &mut buffer) };
    if len <= 0 {
        String::new()
    } else {
        String::from_utf16_lossy(&buffer[..len as usize])
            .trim()
            .to_string()
    }
}

#[cfg(target_os = "windows")]
fn rect_to_snapshot(rect: RECT) -> DesktopRect {
    DesktopRect {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    }
}

#[cfg(target_os = "windows")]
fn snapshot_from_hwnd(hwnd: HWND, active_hwnd: HWND) -> Option<WindowSnapshot> {
    if hwnd.0.is_null() {
        return None;
    }

    if !unsafe { IsWindow(hwnd).as_bool() } {
        return None;
    }

    let is_visible = unsafe { IsWindowVisible(hwnd).as_bool() };
    let title = window_text(hwnd);
    let class_name = class_name(hwnd);

    let mut rect = RECT::default();
    if unsafe { GetWindowRect(hwnd, &mut rect) }.is_err() {
        return None;
    }

    let mut process_id = 0u32;
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut process_id));
    }

    let hwnd_i64 = hwnd_to_i64(hwnd);
    Some(WindowSnapshot {
        id: hwnd_i64.to_string(),
        title,
        class_name,
        process_id,
        rect: rect_to_snapshot(rect),
        is_visible,
        is_active: hwnd.0 == active_hwnd.0,
        hwnd: hwnd_i64,
    })
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn enum_windows_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let context = &mut *(lparam.0 as *mut EnumWindowsContext);
    let windows = &mut *context.windows;

    if let Some(snapshot) = snapshot_from_hwnd(hwnd, context.active_hwnd) {
        if snapshot.is_visible && (!snapshot.title.is_empty() || !snapshot.class_name.is_empty()) {
            windows.push(snapshot);
        }
    }

    BOOL(1)
}

#[cfg(target_os = "windows")]
pub(super) fn list_windows() -> Result<Vec<WindowSnapshot>, String> {
    let active_hwnd = unsafe { GetForegroundWindow() };
    let mut windows = Vec::<WindowSnapshot>::new();
    let mut context = EnumWindowsContext {
        windows: &mut windows,
        active_hwnd,
    };

    unsafe {
        let _ = EnumWindows(
            Some(enum_windows_proc),
            LPARAM((&mut context as *mut EnumWindowsContext) as isize),
        );
    }

    windows.sort_by(|left, right| {
        right
            .is_active
            .cmp(&left.is_active)
            .then_with(|| left.title.to_lowercase().cmp(&right.title.to_lowercase()))
    });

    Ok(windows)
}

#[cfg(target_os = "windows")]
pub(super) fn parse_hwnd_id(value: &str) -> Option<i64> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    let raw = if let Some(rest) = trimmed.strip_prefix("hwnd:") {
        rest.trim()
    } else {
        trimmed
    };

    raw.parse::<i64>().ok()
}

#[cfg(target_os = "windows")]
pub(super) fn get_window_by_hwnd(hwnd_value: i64) -> Result<WindowSnapshot, String> {
    let hwnd = hwnd_from_i64(hwnd_value);
    let active_hwnd = unsafe { GetForegroundWindow() };
    snapshot_from_hwnd(hwnd, active_hwnd).ok_or_else(|| format!("Window not found: {}", hwnd_value))
}

#[cfg(target_os = "windows")]
pub(super) fn resolve_window_hwnd(
    params: &serde_json::Value,
    selected_hwnd: Option<i64>,
) -> Result<i64, String> {
    if let Some(raw_id) = params
        .get("window_id")
        .or_else(|| params.get("id"))
        .and_then(|value| value.as_str())
    {
        if let Some(parsed) = parse_hwnd_id(raw_id) {
            return Ok(parsed);
        }
        return Err(format!("Invalid window id: {}", raw_id));
    }

    if let Some(target_hwnd) = params.get("hwnd").and_then(|value| value.as_i64()) {
        return Ok(target_hwnd);
    }

    if let Some(title_contains) = params
        .get("title_contains")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let needle = title_contains.to_lowercase();
        let windows = list_windows()?;
        if let Some(found) = windows
            .into_iter()
            .find(|window| window.title.to_lowercase().contains(&needle))
        {
            return Ok(found.hwnd);
        }
        return Err(format!(
            "No window matched title_contains='{}'",
            title_contains
        ));
    }

    selected_hwnd.ok_or_else(|| "No selected window. Call action=select_window first".to_string())
}

#[cfg(target_os = "windows")]
pub(super) fn focus_window(hwnd_value: i64) -> Result<(), String> {
    let hwnd = hwnd_from_i64(hwnd_value);
    if !unsafe { IsWindow(hwnd).as_bool() } {
        return Err(format!("Window not found: {}", hwnd_value));
    }
    unsafe {
        let _ = ShowWindow(hwnd, SW_RESTORE);
        let _ = SetForegroundWindow(hwnd);
    }
    Ok(())
}

#[cfg(target_os = "windows")]
pub(super) fn close_window(hwnd_value: i64) -> Result<(), String> {
    let hwnd = hwnd_from_i64(hwnd_value);
    if !unsafe { IsWindow(hwnd).as_bool() } {
        return Err(format!("Window not found: {}", hwnd_value));
    }
    let posted = unsafe { PostMessageW(hwnd, WM_CLOSE, WPARAM(0), LPARAM(0)) };
    if posted.is_ok() {
        Ok(())
    } else {
        Err(format!(
            "Failed to post close message to window {}",
            hwnd_value
        ))
    }
}
