#[cfg(target_os = "windows")]
use std::thread;
#[cfg(target_os = "windows")]
use std::time::Duration;

#[cfg(target_os = "windows")]
use core::ffi::c_void;
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::{HANDLE, HWND, LPARAM, RECT, WPARAM};
#[cfg(target_os = "windows")]
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
#[cfg(target_os = "windows")]
use windows::Win32::System::Memory::{
    GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE, GMEM_ZEROINIT,
};
#[cfg(target_os = "windows")]
use windows::Win32::System::Ole::CF_UNICODETEXT;
#[cfg(target_os = "windows")]
use windows::Win32::UI::Input::KeyboardAndMouse::{
    keybd_event, mouse_event, VkKeyScanW, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP,
    MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP,
    MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_WHEEL, MOUSE_EVENT_FLAGS, VK_CONTROL,
    VK_DELETE, VK_MENU, VK_RETURN, VK_SHIFT, VK_TAB,
};
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowRect, GetWindowTextW, IsWindow, SendMessageW, SetCursorPos, SetForegroundWindow,
    WM_SETTEXT,
};

use crate::services::desktop::types::{ControlSnapshot, DesktopRect};

#[cfg(target_os = "windows")]
use super::uia;

#[cfg(target_os = "windows")]
fn hwnd_from_i64(hwnd_value: i64) -> HWND {
    HWND(hwnd_value as isize as *mut c_void)
}

#[cfg(target_os = "windows")]
fn ensure_window(hwnd_value: i64) -> Result<HWND, String> {
    let hwnd = hwnd_from_i64(hwnd_value);
    if !unsafe { IsWindow(hwnd).as_bool() } {
        return Err(format!("Window not found: {}", hwnd_value));
    }
    Ok(hwnd)
}

#[cfg(target_os = "windows")]
fn window_rect(hwnd: HWND) -> Result<RECT, String> {
    let mut rect = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut rect) }
        .map_err(|_| "Failed to query window rect".to_string())?;
    Ok(rect)
}

#[cfg(target_os = "windows")]
fn click_flags(button: &str) -> Result<(MOUSE_EVENT_FLAGS, MOUSE_EVENT_FLAGS), String> {
    match button.trim().to_ascii_lowercase().as_str() {
        "left" | "" => Ok((MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP)),
        "right" => Ok((MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP)),
        "middle" => Ok((MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP)),
        other => Err(format!("Unsupported mouse button: {}", other)),
    }
}

#[cfg(target_os = "windows")]
fn click_at_point(x: i32, y: i32, button: &str, double_click: bool) -> Result<(), String> {
    let (down, up) = click_flags(button)?;
    unsafe {
        SetCursorPos(x, y).map_err(|_| "Failed to move cursor".to_string())?;
        let count = if double_click { 2 } else { 1 };
        for _ in 0..count {
            mouse_event(down, 0, 0, 0, 0);
            mouse_event(up, 0, 0, 0, 0);
            thread::sleep(Duration::from_millis(60));
        }
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn rect_center(rect: &DesktopRect) -> (i32, i32) {
    let width = (rect.right - rect.left).max(1);
    let height = (rect.bottom - rect.top).max(1);
    let x = rect.left + (width / 2);
    let y = rect.top + (height / 2);
    (x, y)
}

#[cfg(target_os = "windows")]
fn read_window_text(hwnd: HWND) -> String {
    let mut buffer = [0u16; 2048];
    let len = unsafe { GetWindowTextW(hwnd, &mut buffer) };
    if len <= 0 {
        String::new()
    } else {
        String::from_utf16_lossy(&buffer[..len as usize])
    }
}

#[cfg(target_os = "windows")]
fn tap_vk(vk: u8) {
    unsafe {
        keybd_event(vk, 0, Default::default(), 0);
        keybd_event(vk, 0, KEYEVENTF_KEYUP, 0);
    }
}

#[cfg(target_os = "windows")]
fn with_modifier(modifier: u8, f: impl FnOnce()) {
    unsafe {
        keybd_event(modifier, 0, KEYEVENTF_EXTENDEDKEY, 0);
    }
    f();
    unsafe {
        keybd_event(modifier, 0, KEYEVENTF_EXTENDEDKEY | KEYEVENTF_KEYUP, 0);
    }
}

#[cfg(target_os = "windows")]
fn type_text(text: &str) {
    for ch in text.chars() {
        let codepoint = ch as u32;
        if codepoint > u16::MAX as u32 {
            continue;
        }
        let mapped = unsafe { VkKeyScanW(codepoint as u16) };
        if mapped == -1 {
            continue;
        }
        let vk = (mapped & 0xff) as u8;
        let shift_state = ((mapped >> 8) & 0xff) as u8;
        match shift_state {
            0 => tap_vk(vk),
            1 => with_modifier(VK_SHIFT.0 as u8, || tap_vk(vk)),
            2 => with_modifier(VK_CONTROL.0 as u8, || tap_vk(vk)),
            4 => with_modifier(VK_MENU.0 as u8, || tap_vk(vk)),
            _ => tap_vk(vk),
        }
        thread::sleep(Duration::from_millis(6));
    }
}

#[cfg(target_os = "windows")]
fn send_ctrl_shortcut(ch: char) {
    let codepoint = ch as u32;
    if codepoint > u16::MAX as u32 {
        return;
    }
    let mapped = unsafe { VkKeyScanW(codepoint as u16) };
    if mapped == -1 {
        return;
    }
    let vk = (mapped & 0xff) as u8;
    with_modifier(VK_CONTROL.0 as u8, || tap_vk(vk));
}

#[cfg(target_os = "windows")]
struct ClipboardGuard;

#[cfg(target_os = "windows")]
impl Drop for ClipboardGuard {
    fn drop(&mut self) {
        let _ = unsafe { CloseClipboard() };
    }
}

#[cfg(target_os = "windows")]
fn set_clipboard_text(text: &str) -> Result<(), String> {
    let mut wide: Vec<u16> = text.encode_utf16().collect();
    wide.push(0);
    let bytes_len = wide.len() * std::mem::size_of::<u16>();

    unsafe { OpenClipboard(HWND(std::ptr::null_mut())) }
        .map_err(|e| format!("OpenClipboard failed: {}", e))?;
    let _clipboard_guard = ClipboardGuard;
    unsafe { EmptyClipboard() }.map_err(|e| format!("EmptyClipboard failed: {}", e))?;

    let memory = unsafe { GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, bytes_len) }
        .map_err(|e| format!("GlobalAlloc failed: {}", e))?;
    let lock_ptr = unsafe { GlobalLock(memory) } as *mut u16;
    if lock_ptr.is_null() {
        return Err("GlobalLock failed".to_string());
    }

    unsafe {
        std::ptr::copy_nonoverlapping(wide.as_ptr(), lock_ptr, wide.len());
        let _ = GlobalUnlock(memory);
    }

    let handle = HANDLE(memory.0);
    unsafe { SetClipboardData(CF_UNICODETEXT.0 as u32, handle) }
        .map_err(|e| format!("SetClipboardData failed: {}", e))?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn capture_input_text_snapshot(target_hwnd: Option<i64>) -> Option<String> {
    if let Some(hwnd) = target_hwnd {
        if let Ok(Some(text)) = uia::read_text_by_hwnd(hwnd) {
            let trimmed = text.trim().to_string();
            if !trimmed.is_empty() && trimmed.len() <= 4096 {
                return Some(trimmed);
            }
        }
        if let Ok(window) = ensure_window(hwnd) {
            let text = read_window_text(window).trim().to_string();
            if !text.is_empty() && text.len() <= 4096 {
                return Some(text);
            }
        }
    }

    if let Ok(Some(text)) = uia::read_focused_text() {
        let trimmed = text.trim().to_string();
        if !trimmed.is_empty() && trimmed.len() <= 4096 {
            return Some(trimmed);
        }
    }

    None
}

#[cfg(target_os = "windows")]
fn verify_text_written(control_hwnd: i64, expected: &str) -> bool {
    if expected.trim().is_empty() {
        return true;
    }

    if let Ok(Some(text)) = uia::read_text_by_hwnd(control_hwnd) {
        if text.contains(expected) {
            return true;
        }
    }
    if let Ok(window) = ensure_window(control_hwnd) {
        let text = read_window_text(window);
        if text.contains(expected) {
            return true;
        }
    }
    if let Ok(Some(text)) = uia::read_focused_text() {
        if text.contains(expected) {
            return true;
        }
    }
    false
}

#[cfg(target_os = "windows")]
fn paste_text_via_clipboard(
    hwnd: HWND,
    text: &str,
    focus_point: Option<(i32, i32)>,
) -> Result<(), String> {
    set_clipboard_text(text)?;
    unsafe {
        let _ = SetForegroundWindow(hwnd);
    }
    if let Some((x, y)) = focus_point {
        let _ = click_at_point(x, y, "left", false);
    } else if let Ok(rect) = window_rect(hwnd) {
        let x = rect.left + ((rect.right - rect.left) / 2);
        let y = rect.top + ((rect.bottom - rect.top) / 2);
        let _ = click_at_point(x, y, "left", false);
    }
    send_ctrl_shortcut('a');
    thread::sleep(Duration::from_millis(30));
    tap_vk(VK_DELETE.0 as u8);
    thread::sleep(Duration::from_millis(30));
    send_ctrl_shortcut('v');
    thread::sleep(Duration::from_millis(120));
    Ok(())
}

#[cfg(target_os = "windows")]
pub(super) fn click_on_coordinates(
    window_hwnd: i64,
    x: f64,
    y: f64,
    button: &str,
    double_click: bool,
) -> Result<String, String> {
    let hwnd = ensure_window(window_hwnd)?;
    let rect = window_rect(hwnd)?;
    let width = (rect.right - rect.left).max(1);
    let height = (rect.bottom - rect.top).max(1);
    let abs_x = rect.left + (width as f64 * x.clamp(0.0, 1.0)).round() as i32;
    let abs_y = rect.top + (height as f64 * y.clamp(0.0, 1.0)).round() as i32;

    click_at_point(abs_x, abs_y, button, double_click)?;
    Ok(format!("Clicked at ({}, {})", abs_x, abs_y))
}

#[cfg(target_os = "windows")]
pub(super) fn click_control(
    control: &ControlSnapshot,
    button: &str,
    double_click: bool,
) -> Result<String, String> {
    if control.hwnd != 0 {
        if let Ok(hwnd) = ensure_window(control.hwnd) {
            unsafe {
                let _ = SetForegroundWindow(hwnd);
            }
        }
    }

    let (x, y) = rect_center(&control.rect);
    click_at_point(x, y, button, double_click)?;
    Ok(format!("Clicked control {} at ({}, {})", control.id, x, y))
}

#[cfg(target_os = "windows")]
pub(super) fn drag_on_coordinates(
    window_hwnd: i64,
    start_x: f64,
    start_y: f64,
    end_x: f64,
    end_y: f64,
    button: &str,
    duration_sec: f64,
) -> Result<String, String> {
    let hwnd = ensure_window(window_hwnd)?;
    let rect = window_rect(hwnd)?;
    let width = (rect.right - rect.left).max(1);
    let height = (rect.bottom - rect.top).max(1);

    let sx = rect.left + (width as f64 * start_x.clamp(0.0, 1.0)).round() as i32;
    let sy = rect.top + (height as f64 * start_y.clamp(0.0, 1.0)).round() as i32;
    let ex = rect.left + (width as f64 * end_x.clamp(0.0, 1.0)).round() as i32;
    let ey = rect.top + (height as f64 * end_y.clamp(0.0, 1.0)).round() as i32;

    let (down, up) = click_flags(button)?;
    let steps = 16i32;
    let sleep_ms = ((duration_sec.max(0.1) * 1000.0) / f64::from(steps)).max(5.0) as u64;

    unsafe {
        SetCursorPos(sx, sy).map_err(|_| "Failed to move cursor".to_string())?;
        mouse_event(down, 0, 0, 0, 0);
        for step in 1..=steps {
            let nx = sx + ((ex - sx) * step / steps);
            let ny = sy + ((ey - sy) * step / steps);
            SetCursorPos(nx, ny).map_err(|_| "Failed to move cursor".to_string())?;
            thread::sleep(Duration::from_millis(sleep_ms));
        }
        mouse_event(up, 0, 0, 0, 0);
    }

    Ok(format!("Dragged from ({}, {}) to ({}, {})", sx, sy, ex, ey))
}

#[cfg(target_os = "windows")]
pub(super) fn set_edit_text_on_control(
    control: &ControlSnapshot,
    text: &str,
) -> Result<String, String> {
    let hwnd = ensure_window(control.hwnd)?;
    unsafe {
        let _ = SetForegroundWindow(hwnd);
    }

    if uia::try_set_value_by_hwnd(control.hwnd, text).unwrap_or(false) {
        thread::sleep(Duration::from_millis(60));
        if verify_text_written(control.hwnd, text) {
            return Ok("Text updated via UIA ValuePattern (verified)".to_string());
        }
    }

    let mut wide: Vec<u16> = text.encode_utf16().collect();
    wide.push(0);
    unsafe {
        SendMessageW(hwnd, WM_SETTEXT, WPARAM(0), LPARAM(wide.as_ptr() as isize));
    }
    thread::sleep(Duration::from_millis(60));
    if verify_text_written(control.hwnd, text) {
        return Ok("Text updated via WM_SETTEXT (verified)".to_string());
    }

    paste_text_via_clipboard(hwnd, text, Some(rect_center(&control.rect)))?;
    if verify_text_written(control.hwnd, text) {
        return Ok("Text updated via clipboard paste (verified)".to_string());
    }

    Err(
        "Text input was attempted (UIA + WM_SETTEXT + clipboard), but verification failed"
            .to_string(),
    )
}

#[cfg(target_os = "windows")]
pub(super) fn keyboard_input(
    target_hwnd: Option<i64>,
    keys: &str,
    control_focus: bool,
) -> Result<String, String> {
    if control_focus {
        if let Some(hwnd_value) = target_hwnd {
            let hwnd = ensure_window(hwnd_value)?;
            unsafe {
                let _ = SetForegroundWindow(hwnd);
            }
        }
    }

    let trimmed = keys.trim();
    if trimmed.eq_ignore_ascii_case("{ENTER}") {
        let before = capture_input_text_snapshot(target_hwnd);
        tap_vk(VK_RETURN.0 as u8);
        thread::sleep(Duration::from_millis(260));
        let after = capture_input_text_snapshot(target_hwnd);

        if let Some(before_text) = before {
            let before_trimmed = before_text.trim();
            if !before_trimmed.is_empty() {
                match after {
                    Some(after_text) => {
                        let after_trimmed = after_text.trim();
                        if after_trimmed.is_empty() {
                            return Ok(
                                "Sent Enter (verified: input cleared after send)".to_string()
                            );
                        }
                        if after_trimmed == before_trimmed {
                            return Err("Sent Enter, but input text did not change; send verification failed"
                                .to_string());
                        }
                        return Ok("Sent Enter (partial verification: input changed)".to_string());
                    }
                    None => {
                        return Ok("Sent Enter (partial verification: input became unreadable)"
                            .to_string())
                    }
                }
            }
        }
        return Ok("Sent Enter".to_string());
    }

    if trimmed.eq_ignore_ascii_case("{TAB}") {
        tap_vk(VK_TAB.0 as u8);
        return Ok("Sent Tab".to_string());
    }

    if let Some(rest) = trimmed.strip_prefix("{TAB ") {
        if let Some(count_text) = rest.strip_suffix('}') {
            if let Ok(count) = count_text.trim().parse::<usize>() {
                for _ in 0..count.max(1) {
                    tap_vk(VK_TAB.0 as u8);
                }
                return Ok(format!("Sent Tab x{}", count.max(1)));
            }
        }
    }

    if let Some(rest) = trimmed.strip_prefix("{VK_CONTROL}") {
        with_modifier(VK_CONTROL.0 as u8, || type_text(rest));
        return Ok("Sent Ctrl+sequence".to_string());
    }
    if let Some(rest) = trimmed.strip_prefix("{VK_MENU}") {
        with_modifier(VK_MENU.0 as u8, || type_text(rest));
        return Ok("Sent Alt+sequence".to_string());
    }
    if let Some(rest) = trimmed.strip_prefix("{VK_SHIFT}") {
        with_modifier(VK_SHIFT.0 as u8, || type_text(rest));
        return Ok("Sent Shift+sequence".to_string());
    }

    type_text(trimmed);
    Ok("Typed key sequence".to_string())
}

#[cfg(target_os = "windows")]
pub(super) fn wheel_mouse_input(target_hwnd: i64, wheel_dist: i32) -> Result<String, String> {
    let hwnd = ensure_window(target_hwnd)?;
    let rect = window_rect(hwnd)?;
    let x = rect.left + ((rect.right - rect.left) / 2);
    let y = rect.top + ((rect.bottom - rect.top) / 2);

    unsafe {
        SetCursorPos(x, y).map_err(|_| "Failed to move cursor".to_string())?;
        mouse_event(MOUSEEVENTF_WHEEL, 0, 0, wheel_dist.saturating_mul(120), 0);
    }

    Ok(format!("Mouse wheel scrolled: {}", wheel_dist))
}

#[cfg(target_os = "windows")]
pub(super) fn wheel_mouse_on_control(
    control: &ControlSnapshot,
    wheel_dist: i32,
) -> Result<String, String> {
    if control.hwnd != 0 {
        if let Ok(hwnd) = ensure_window(control.hwnd) {
            unsafe {
                let _ = SetForegroundWindow(hwnd);
            }
        }
    }

    let (x, y) = rect_center(&control.rect);
    unsafe {
        SetCursorPos(x, y).map_err(|_| "Failed to move cursor".to_string())?;
        mouse_event(MOUSEEVENTF_WHEEL, 0, 0, wheel_dist.saturating_mul(120), 0);
    }

    Ok(format!(
        "Mouse wheel scrolled on control {}: {}",
        control.id, wheel_dist
    ))
}

#[cfg(target_os = "windows")]
pub(super) fn read_control_text(control_hwnd: i64) -> Result<String, String> {
    if let Ok(Some(text)) = uia::read_text_by_hwnd(control_hwnd) {
        return Ok(text);
    }

    let hwnd = ensure_window(control_hwnd)?;
    let text = read_window_text(hwnd);
    if !text.trim().is_empty() {
        return Ok(text);
    }

    if let Ok(Some(text)) = uia::read_focused_text() {
        return Ok(text);
    }

    Ok(String::new())
}
