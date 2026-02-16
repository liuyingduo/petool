#[cfg(target_os = "windows")]
use core::ffi::c_void;
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, RECT};
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{
    EnumChildWindows, GetClassNameW, GetParent, GetWindowRect, GetWindowTextW, IsWindow,
};

use crate::services::desktop::types::{ControlSnapshot, DesktopRect};

#[cfg(target_os = "windows")]
struct EnumControlsContext {
    parent_hwnd: i64,
    controls: *mut Vec<ControlSnapshot>,
    max_controls: usize,
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
fn rect_to_snapshot(rect: RECT) -> DesktopRect {
    DesktopRect {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    }
}

#[cfg(target_os = "windows")]
fn window_text(hwnd: HWND) -> String {
    let mut buffer = [0u16; 512];
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
unsafe extern "system" fn enum_child_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let context = &mut *(lparam.0 as *mut EnumControlsContext);
    let controls = &mut *context.controls;

    if controls.len() >= context.max_controls {
        return BOOL(0);
    }

    if !IsWindow(hwnd).as_bool() {
        return BOOL(1);
    }

    let mut rect = RECT::default();
    if GetWindowRect(hwnd, &mut rect).is_ok() {
        let hwnd_i64 = hwnd_to_i64(hwnd);
        controls.push(ControlSnapshot {
            id: hwnd_i64.to_string(),
            name: window_text(hwnd),
            class_name: class_name(hwnd),
            rect: rect_to_snapshot(rect),
            parent_window_id: context.parent_hwnd.to_string(),
            hwnd: hwnd_i64,
        });
    }

    BOOL(1)
}

#[cfg(target_os = "windows")]
pub(super) fn get_controls(
    window_hwnd: i64,
    max_controls: usize,
) -> Result<Vec<ControlSnapshot>, String> {
    let hwnd = hwnd_from_i64(window_hwnd);
    if !unsafe { IsWindow(hwnd).as_bool() } {
        return Err(format!("Window not found: {}", window_hwnd));
    }

    let mut controls = Vec::<ControlSnapshot>::new();
    let mut context = EnumControlsContext {
        parent_hwnd: window_hwnd,
        controls: &mut controls,
        max_controls,
    };

    unsafe {
        let _ = EnumChildWindows(
            hwnd,
            Some(enum_child_proc),
            LPARAM((&mut context as *mut EnumControlsContext) as isize),
        );
    }

    Ok(controls)
}

#[cfg(target_os = "windows")]
pub(super) fn get_ui_tree(
    window_hwnd: i64,
    max_controls: usize,
) -> Result<serde_json::Value, String> {
    let controls = get_controls(window_hwnd, max_controls)?;
    let mut nodes = Vec::with_capacity(controls.len());

    for control in controls {
        let parent =
            unsafe { GetParent(hwnd_from_i64(control.hwnd)) }.unwrap_or(HWND(std::ptr::null_mut()));
        let parent_hwnd = hwnd_to_i64(parent);
        nodes.push(serde_json::json!({
            "id": control.id,
            "name": control.name,
            "class_name": control.class_name,
            "rect": {
                "left": control.rect.left,
                "top": control.rect.top,
                "right": control.rect.right,
                "bottom": control.rect.bottom,
                "width": control.rect.width(),
                "height": control.rect.height()
            },
            "parent_id": if parent.0.is_null() { None::<String> } else { Some(parent_hwnd.to_string()) }
        }));
    }

    Ok(serde_json::json!({
        "window_id": window_hwnd.to_string(),
        "nodes": nodes
    }))
}

#[cfg(target_os = "windows")]
pub(super) fn resolve_control_hwnd(
    params: &serde_json::Value,
    cached_controls: &[ControlSnapshot],
) -> Result<i64, String> {
    if let Some(raw_id) = params
        .get("control_id")
        .or_else(|| params.get("id"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if let Ok(parsed) = raw_id.parse::<i64>() {
            return Ok(parsed);
        }

        if let Some(control) = cached_controls.iter().find(|control| control.id == raw_id) {
            return Ok(control.hwnd);
        }

        return Err(format!("Control not found: {}", raw_id));
    }

    if let Some(hwnd) = params.get("hwnd").and_then(|value| value.as_i64()) {
        return Ok(hwnd);
    }

    if let Some(name) = params
        .get("name")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let lowered = name.to_lowercase();
        if let Some(control) = cached_controls
            .iter()
            .find(|control| control.name.to_lowercase() == lowered)
            .or_else(|| {
                cached_controls
                    .iter()
                    .find(|control| control.name.to_lowercase().contains(&lowered))
            })
        {
            return Ok(control.hwnd);
        }
        return Err(format!("Control name not found in cache: {}", name));
    }

    Err("Missing control selector. Provide control_id/id/name/hwnd".to_string())
}
