#[cfg(target_os = "windows")]
use std::thread;
#[cfg(target_os = "windows")]
use std::time::Duration;

#[cfg(target_os = "windows")]
use core::ffi::c_void;
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
#[cfg(target_os = "windows")]
use windows::Win32::UI::Input::KeyboardAndMouse::{
    keybd_event, mouse_event, VkKeyScanW, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP,
    MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP,
    MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_WHEEL, MOUSE_EVENT_FLAGS, VK_CONTROL,
    VK_MENU, VK_RETURN, VK_SHIFT, VK_TAB,
};
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowRect, IsWindow, SendMessageW, SetCursorPos, SetForegroundWindow, WM_SETTEXT,
};

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
pub(super) fn click_input(
    control_hwnd: i64,
    button: &str,
    double_click: bool,
) -> Result<String, String> {
    let hwnd = ensure_window(control_hwnd)?;
    unsafe {
        let _ = SetForegroundWindow(hwnd);
    }
    let rect = window_rect(hwnd)?;
    let x = rect.left + ((rect.right - rect.left) / 2);
    let y = rect.top + ((rect.bottom - rect.top) / 2);
    click_at_point(x, y, button, double_click)?;
    Ok(format!("Clicked control {}", control_hwnd))
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
pub(super) fn set_edit_text(control_hwnd: i64, text: &str) -> Result<String, String> {
    let hwnd = ensure_window(control_hwnd)?;
    let mut wide: Vec<u16> = text.encode_utf16().collect();
    wide.push(0);
    unsafe {
        let _ = SetForegroundWindow(hwnd);
        SendMessageW(hwnd, WM_SETTEXT, WPARAM(0), LPARAM(wide.as_ptr() as isize));
    }
    Ok("Text updated".to_string())
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
        tap_vk(VK_RETURN.0 as u8);
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
pub(super) fn read_control_text(control_hwnd: i64) -> Result<String, String> {
    let hwnd = ensure_window(control_hwnd)?;
    let mut buffer = [0u16; 1024];
    let len = unsafe { windows::Win32::UI::WindowsAndMessaging::GetWindowTextW(hwnd, &mut buffer) };
    if len <= 0 {
        return Ok(String::new());
    }
    Ok(String::from_utf16_lossy(&buffer[..len as usize]))
}
