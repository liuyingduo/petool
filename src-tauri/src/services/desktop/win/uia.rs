#[cfg(target_os = "windows")]
use core::ffi::c_void;
#[cfg(target_os = "windows")]
use windows::core::BSTR;
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::{HWND, RECT, RPC_E_CHANGED_MODE};
#[cfg(target_os = "windows")]
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED,
};
#[cfg(target_os = "windows")]
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationTextPattern,
    IUIAutomationValuePattern, TreeScope_Descendants, UIA_TextPatternId, UIA_ValuePatternId,
};
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::IsWindow;

use crate::services::desktop::types::{ControlSnapshot, DesktopRect};

#[cfg(target_os = "windows")]
#[derive(Debug, Clone)]
pub(super) struct ResolvedControl {
    pub control: ControlSnapshot,
    pub requested_name: Option<String>,
    pub name_verified: bool,
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
struct ComScope {
    should_uninitialize: bool,
}

#[cfg(target_os = "windows")]
impl ComScope {
    fn new() -> Result<Self, String> {
        let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        if hr.is_ok() {
            return Ok(Self {
                should_uninitialize: true,
            });
        }
        if hr == RPC_E_CHANGED_MODE {
            return Ok(Self {
                should_uninitialize: false,
            });
        }
        Err(format!("CoInitializeEx failed: {}", hr))
    }
}

#[cfg(target_os = "windows")]
impl Drop for ComScope {
    fn drop(&mut self) {
        if self.should_uninitialize {
            unsafe {
                CoUninitialize();
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn with_automation<T>(f: impl FnOnce(&IUIAutomation) -> Result<T, String>) -> Result<T, String> {
    let _scope = ComScope::new()?;
    let automation: IUIAutomation =
        unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }
            .map_err(|e| format!("Failed to create UIAutomation: {}", e))?;
    f(&automation)
}

#[cfg(target_os = "windows")]
fn trim_to_non_empty(value: String) -> String {
    value.trim().to_string()
}

#[cfg(target_os = "windows")]
fn read_element_text(element: &IUIAutomationElement) -> Option<String> {
    if let Ok(pattern) =
        unsafe { element.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId) }
    {
        if let Ok(value) = unsafe { pattern.CurrentValue() } {
            let text = trim_to_non_empty(value.to_string());
            if !text.is_empty() {
                return Some(text);
            }
        }
    }

    if let Ok(pattern) =
        unsafe { element.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId) }
    {
        if let Ok(range) = unsafe { pattern.DocumentRange() } {
            if let Ok(value) = unsafe { range.GetText(-1) } {
                let text = trim_to_non_empty(value.to_string());
                if !text.is_empty() {
                    return Some(text);
                }
            }
        }
    }

    if let Ok(name) = unsafe { element.CurrentName() } {
        let text = trim_to_non_empty(name.to_string());
        if !text.is_empty() {
            return Some(text);
        }
    }

    None
}

#[cfg(target_os = "windows")]
fn control_type_label(element: &IUIAutomationElement) -> String {
    unsafe { element.CurrentLocalizedControlType() }
        .map(|value| trim_to_non_empty(value.to_string()))
        .unwrap_or_default()
}

#[cfg(target_os = "windows")]
fn collect_controls_via_uia(
    automation: &IUIAutomation,
    window_hwnd: i64,
    max_controls: usize,
) -> Result<Vec<ControlSnapshot>, String> {
    let hwnd = hwnd_from_i64(window_hwnd);
    if !unsafe { IsWindow(hwnd).as_bool() } {
        return Err(format!("Window not found: {}", window_hwnd));
    }

    let root = unsafe { automation.ElementFromHandle(hwnd) }
        .map_err(|e| format!("ElementFromHandle failed: {}", e))?;
    let true_condition = unsafe { automation.CreateTrueCondition() }
        .map_err(|e| format!("CreateTrueCondition failed: {}", e))?;
    let elements = unsafe { root.FindAll(TreeScope_Descendants, &true_condition) }
        .map_err(|e| format!("FindAll(TreeScope_Descendants) failed: {}", e))?;
    let length = unsafe { elements.Length() }.unwrap_or(0).max(0) as usize;

    let mut controls = Vec::<ControlSnapshot>::new();
    for index in 0..length {
        if controls.len() >= max_controls {
            break;
        }

        let element = match unsafe { elements.GetElement(index as i32) } {
            Ok(value) => value,
            Err(_) => continue,
        };

        let is_control = unsafe { element.CurrentIsControlElement() }
            .map(|value| value.as_bool())
            .unwrap_or(true);
        if !is_control {
            continue;
        }

        let rect = match unsafe { element.CurrentBoundingRectangle() } {
            Ok(value) => value,
            Err(_) => continue,
        };
        if rect.right <= rect.left || rect.bottom <= rect.top {
            continue;
        }

        let name = unsafe { element.CurrentName() }
            .map(|value| trim_to_non_empty(value.to_string()))
            .unwrap_or_default();
        let class_name = unsafe { element.CurrentClassName() }
            .map(|value| trim_to_non_empty(value.to_string()))
            .unwrap_or_default();
        let control_type = control_type_label(&element);
        let automation_id = unsafe { element.CurrentAutomationId() }
            .map(|value| trim_to_non_empty(value.to_string()))
            .unwrap_or_default();
        let native_hwnd = unsafe { element.CurrentNativeWindowHandle() }
            .map(hwnd_to_i64)
            .unwrap_or(window_hwnd);
        let hwnd_for_action = if native_hwnd == 0 {
            window_hwnd
        } else {
            native_hwnd
        };
        let is_enabled = unsafe { element.CurrentIsEnabled() }
            .map(|value| value.as_bool())
            .unwrap_or(true);
        let is_offscreen = unsafe { element.CurrentIsOffscreen() }
            .map(|value| value.as_bool())
            .unwrap_or(false);

        controls.push(ControlSnapshot {
            id: (index + 1).to_string(),
            name,
            class_name,
            control_type,
            automation_id,
            source: "uia".to_string(),
            is_enabled,
            is_offscreen,
            rect: rect_to_snapshot(rect),
            parent_window_id: window_hwnd.to_string(),
            hwnd: hwnd_for_action,
        });
    }

    Ok(controls)
}

#[cfg(target_os = "windows")]
pub(super) fn get_controls(
    window_hwnd: i64,
    max_controls: usize,
) -> Result<Vec<ControlSnapshot>, String> {
    with_automation(|automation| collect_controls_via_uia(automation, window_hwnd, max_controls))
}

#[cfg(target_os = "windows")]
pub(super) fn get_ui_tree(
    window_hwnd: i64,
    max_controls: usize,
) -> Result<serde_json::Value, String> {
    let controls = get_controls(window_hwnd, max_controls)?;
    let nodes = controls
        .into_iter()
        .map(|control| {
            serde_json::json!({
                "id": control.id,
                "name": control.name,
                "class_name": control.class_name,
                "control_type": control.control_type,
                "automation_id": control.automation_id,
                "source": control.source,
                "is_enabled": control.is_enabled,
                "is_offscreen": control.is_offscreen,
                "rect": {
                    "left": control.rect.left,
                    "top": control.rect.top,
                    "right": control.rect.right,
                    "bottom": control.rect.bottom,
                    "width": control.rect.width(),
                    "height": control.rect.height()
                },
                "parent_id": control.parent_window_id,
                "hwnd": control.hwnd,
            })
        })
        .collect::<Vec<_>>();

    Ok(serde_json::json!({
        "window_id": window_hwnd.to_string(),
        "nodes": nodes
    }))
}

#[cfg(target_os = "windows")]
fn read_control_id(params: &serde_json::Value) -> Option<String> {
    let value = params
        .get("control_id")
        .or_else(|| params.get("id"))
        .cloned()?;
    if let Some(text) = value
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(text.to_string());
    }
    value
        .as_i64()
        .filter(|value| *value > 0)
        .map(|value| value.to_string())
}

#[cfg(target_os = "windows")]
fn read_control_name(params: &serde_json::Value) -> Option<String> {
    params
        .get("name")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
}

#[cfg(target_os = "windows")]
fn summarize_control_ids(cached_controls: &[ControlSnapshot]) -> String {
    let mut ids = cached_controls
        .iter()
        .map(|control| control.id.clone())
        .collect::<Vec<_>>();
    ids.sort();
    if ids.is_empty() {
        return "[]".to_string();
    }
    if ids.len() > 20 {
        ids.truncate(20);
        ids.push("...".to_string());
    }
    format!("[{}]", ids.join(", "))
}

#[cfg(target_os = "windows")]
pub(super) fn resolve_control(
    params: &serde_json::Value,
    cached_controls: &[ControlSnapshot],
    require_name: bool,
) -> Result<ResolvedControl, String> {
    let raw_id = read_control_id(params).ok_or_else(|| {
        "Control id is required. Provide params.control_id or params.id.".to_string()
    })?;

    let control = cached_controls
        .iter()
        .find(|item| item.id == raw_id)
        .cloned()
        .ok_or_else(|| {
            format!(
                "Control with id '{}' not found. Available ids: {}",
                raw_id,
                summarize_control_ids(cached_controls)
            )
        })?;

    let requested_name = read_control_name(params);
    if require_name && requested_name.is_none() {
        // UFO style: allow execution with id only, treat missing name as unverified.
    }

    let name_verified = match requested_name.as_deref() {
        Some(name) => control.name == name,
        None => false,
    };

    Ok(ResolvedControl {
        control,
        requested_name,
        name_verified,
    })
}

#[cfg(target_os = "windows")]
pub(super) fn try_set_value_by_hwnd(control_hwnd: i64, text: &str) -> Result<bool, String> {
    with_automation(|automation| {
        let hwnd = hwnd_from_i64(control_hwnd);
        if !unsafe { IsWindow(hwnd).as_bool() } {
            return Ok(false);
        }
        let element = match unsafe { automation.ElementFromHandle(hwnd) } {
            Ok(value) => value,
            Err(_) => return Ok(false),
        };
        let pattern = match unsafe {
            element.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
        } {
            Ok(value) => value,
            Err(_) => return Ok(false),
        };
        unsafe { pattern.SetValue(&BSTR::from(text)) }
            .map(|_| true)
            .map_err(|e| format!("UIA ValuePattern::SetValue failed: {}", e))
    })
}

#[cfg(target_os = "windows")]
pub(super) fn read_text_by_hwnd(control_hwnd: i64) -> Result<Option<String>, String> {
    with_automation(|automation| {
        let hwnd = hwnd_from_i64(control_hwnd);
        if !unsafe { IsWindow(hwnd).as_bool() } {
            return Ok(None);
        }
        let element = match unsafe { automation.ElementFromHandle(hwnd) } {
            Ok(value) => value,
            Err(_) => return Ok(None),
        };
        Ok(read_element_text(&element))
    })
}

#[cfg(target_os = "windows")]
pub(super) fn read_focused_text() -> Result<Option<String>, String> {
    with_automation(|automation| {
        let focused = match unsafe { automation.GetFocusedElement() } {
            Ok(value) => value,
            Err(_) => return Ok(None),
        };
        Ok(read_element_text(&focused))
    })
}
