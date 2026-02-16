#[cfg(target_os = "windows")]
use std::path::Path;
#[cfg(target_os = "windows")]
use std::time::Duration;

#[cfg(target_os = "windows")]
use serde_json::{json, Value};

#[cfg(target_os = "windows")]
use crate::models::config::DesktopConfig;
#[cfg(target_os = "windows")]
use crate::services::desktop::manager::DesktopSessionState;
#[cfg(target_os = "windows")]
use crate::services::desktop::types::{ControlSnapshot, WindowSnapshot};

#[cfg(target_os = "windows")]
mod capture;
#[cfg(target_os = "windows")]
mod com_automation;
#[cfg(target_os = "windows")]
mod com_excel;
#[cfg(target_os = "windows")]
mod com_ppt;
#[cfg(target_os = "windows")]
mod com_word;
#[cfg(target_os = "windows")]
mod input;
#[cfg(target_os = "windows")]
mod uia;
#[cfg(target_os = "windows")]
mod window;

#[cfg(target_os = "windows")]
fn params_object(params: &Value) -> Result<&serde_json::Map<String, Value>, String> {
    params
        .as_object()
        .ok_or_else(|| "'params' must be an object".to_string())
}

#[cfg(target_os = "windows")]
pub(super) fn read_string(params: &Value, key: &str) -> Result<String, String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
        .ok_or_else(|| format!("'{}' is required", key))
}

#[cfg(target_os = "windows")]
pub(super) fn read_optional_string(params: &Value, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
}

#[cfg(target_os = "windows")]
fn read_identifier(params: &Value, key: &str) -> Option<String> {
    params.get(key).and_then(|value| match value {
        Value::String(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        Value::Number(number) => number
            .as_i64()
            .filter(|num| *num > 0)
            .map(|num| num.to_string()),
        _ => None,
    })
}

#[cfg(target_os = "windows")]
fn read_keyboard_sequence(params: &Value) -> (String, Option<String>) {
    if let Some(keys) = params.get("keys").and_then(Value::as_str) {
        return (keys.to_string(), None);
    }

    if let Some(text_alias) = params.get("text").and_then(Value::as_str) {
        return (
            text_alias.to_string(),
            Some("Compat: params.text was used as keyboard_input.keys.".to_string()),
        );
    }

    (
        String::new(),
        Some("Warning: keyboard_input expected params.keys; missing value treated as empty key sequence."
            .to_string()),
    )
}

#[cfg(target_os = "windows")]
fn looks_like_browser_target(raw: &str) -> bool {
    let normalized = raw
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_ascii_lowercase();
    if normalized.is_empty() {
        return false;
    }

    let leaf = normalized
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(normalized.as_str())
        .trim_matches('"')
        .trim_matches('\'');
    let leaf_no_ext = leaf.strip_suffix(".exe").unwrap_or(leaf);
    if matches!(
        leaf_no_ext,
        "chrome"
            | "msedge"
            | "firefox"
            | "brave"
            | "opera"
            | "iexplore"
            | "microsoftedge"
            | "microsoft-edge"
            | "edge"
            | "browser"
    ) {
        return true;
    }

    [
        "chrome.exe",
        "msedge.exe",
        "firefox.exe",
        "brave.exe",
        "opera.exe",
        "iexplore.exe",
        "microsoft-edge:",
        "start chrome",
        "start msedge",
        "start firefox",
        "start brave",
        "start opera",
    ]
    .iter()
    .any(|keyword| normalized.contains(keyword))
}

#[cfg(target_os = "windows")]
fn is_browser_launch_request(params: &Value) -> bool {
    for key in [
        "command",
        "application_path",
        "app_path",
        "executable",
        "app_name",
        "bash_command",
    ] {
        if let Some(value) = read_optional_string(params, key) {
            if looks_like_browser_target(&value) {
                return true;
            }
        }
    }

    params
        .get("args")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .any(looks_like_browser_target)
        })
        .unwrap_or(false)
}

#[cfg(target_os = "windows")]
pub(super) fn read_bool(params: &Value, key: &str, default_value: bool) -> bool {
    params
        .get(key)
        .and_then(Value::as_bool)
        .unwrap_or(default_value)
}

#[cfg(target_os = "windows")]
pub(super) fn read_i64(params: &Value, key: &str, default_value: i64) -> i64 {
    params
        .get(key)
        .and_then(Value::as_i64)
        .unwrap_or(default_value)
}

#[cfg(target_os = "windows")]
fn read_field_list(params: &Value) -> Option<Vec<String>> {
    let fields = params.get("field_list")?.as_array()?;
    let mut result = Vec::new();
    for field in fields {
        if let Some(value) = field
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            result.push(value.to_string());
        }
    }
    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

#[cfg(target_os = "windows")]
fn control_rect_value(control: &ControlSnapshot) -> Value {
    json!({
        "left": control.rect.left,
        "top": control.rect.top,
        "right": control.rect.right,
        "bottom": control.rect.bottom,
        "x": control.rect.left,
        "y": control.rect.top,
        "width": control.rect.width(),
        "height": control.rect.height()
    })
}

#[cfg(target_os = "windows")]
fn control_catalog_entry(control: &ControlSnapshot) -> Value {
    let rect = control_rect_value(control);
    json!({
        "id": control.id,
        "label": control.id,
        "name": control.name,
        "control_text": control.name,
        "class_name": control.class_name,
        "control_type": control.control_type,
        "automation_id": control.automation_id,
        "source": control.source,
        "is_enabled": control.is_enabled,
        "is_visible": !control.is_offscreen,
        "is_offscreen": control.is_offscreen,
        "control_rect": rect,
        "rect": rect,
        "parent_window_id": control.parent_window_id,
        "hwnd": control.hwnd
    })
}

#[cfg(target_os = "windows")]
fn control_field_value(control: &ControlSnapshot, field: &str) -> Option<Value> {
    match field {
        "id" | "label" => Some(json!(control.id)),
        "name" | "control_text" => Some(json!(control.name)),
        "class_name" => Some(json!(control.class_name)),
        "control_type" => Some(json!(control.control_type)),
        "automation_id" => Some(json!(control.automation_id)),
        "source" => Some(json!(control.source)),
        "is_enabled" => Some(json!(control.is_enabled)),
        "is_visible" => Some(json!(!control.is_offscreen)),
        "is_offscreen" => Some(json!(control.is_offscreen)),
        "control_rect" | "rect" => Some(control_rect_value(control)),
        "parent_window_id" => Some(json!(control.parent_window_id)),
        "hwnd" => Some(json!(control.hwnd)),
        _ => None,
    }
}

#[cfg(target_os = "windows")]
fn control_entry_with_fields(control: &ControlSnapshot, field_list: &[String]) -> Value {
    let mut map = serde_json::Map::new();
    for field in field_list {
        if let Some(value) = control_field_value(control, field) {
            map.insert(field.clone(), value);
        }
    }
    Value::Object(map)
}

#[cfg(target_os = "windows")]
fn window_rect_value(window: &WindowSnapshot) -> Value {
    json!({
        "left": window.rect.left,
        "top": window.rect.top,
        "right": window.rect.right,
        "bottom": window.rect.bottom,
        "x": window.rect.left,
        "y": window.rect.top,
        "width": window.rect.width(),
        "height": window.rect.height()
    })
}

#[cfg(target_os = "windows")]
fn window_field_value(window: &WindowSnapshot, field: &str) -> Option<Value> {
    match field {
        "id" => Some(json!(window.id)),
        "name" | "title" | "control_text" => Some(json!(window.title)),
        "control_type" => Some(json!("Window")),
        "class_name" => Some(json!(window.class_name)),
        "process_id" => Some(json!(window.process_id)),
        "is_visible" => Some(json!(window.is_visible)),
        "is_active" => Some(json!(window.is_active)),
        "control_rect" | "rect" => Some(window_rect_value(window)),
        "hwnd" => Some(json!(window.hwnd)),
        _ => None,
    }
}

#[cfg(target_os = "windows")]
fn window_entry_with_fields(window: &WindowSnapshot, field_list: &[String]) -> Value {
    let mut map = serde_json::Map::new();
    for field in field_list {
        if let Some(value) = window_field_value(window, field) {
            map.insert(field.clone(), value);
        }
    }
    Value::Object(map)
}

#[cfg(target_os = "windows")]
fn ensure_window_cache(
    session: &mut DesktopSessionState,
    force_refresh: bool,
) -> Result<(), String> {
    if force_refresh || session.window_cache.is_empty() {
        session.window_cache = window::list_windows()?;
        session.windows_cached_at = Some(std::time::Instant::now());
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn ensure_control_cache(
    session: &mut DesktopSessionState,
    config: &DesktopConfig,
    selected_window_hwnd: i64,
    force_refresh: bool,
) -> Result<(), String> {
    let ttl_ms = config.control_cache_ttl_ms.max(250);
    let fresh_enough = session
        .controls_cached_at
        .map(|at| at.elapsed() <= Duration::from_millis(ttl_ms))
        .unwrap_or(false);

    if !force_refresh && fresh_enough && !session.controls_cache.is_empty() {
        return Ok(());
    }

    let max_controls = config.max_controls.clamp(10, 10_000);
    let controls = uia::get_controls(selected_window_hwnd, max_controls)?;
    session.controls_cache = controls;
    session.controls_cached_at = Some(std::time::Instant::now());
    Ok(())
}

#[cfg(target_os = "windows")]
fn activate_window(session: &mut DesktopSessionState, hwnd: i64) -> Result<WindowSnapshot, String> {
    window::focus_window(hwnd)?;
    let selected = window::get_window_by_hwnd(hwnd)?;
    session.selected_window_hwnd = Some(hwnd);
    session.window_cache = window::list_windows().unwrap_or_default();
    session.windows_cached_at = Some(std::time::Instant::now());
    session.controls_cache.clear();
    session.controls_cached_at = None;
    Ok(selected)
}

#[cfg(target_os = "windows")]
pub(super) async fn execute_action(
    action: &str,
    params: &Value,
    session: &mut DesktopSessionState,
    config: &DesktopConfig,
    screenshot_dir: &Path,
) -> Result<Value, String> {
    let _ = params_object(params)?;

    if let Some(value) = com_word::execute(action, params).await? {
        return Ok(value);
    }
    if let Some(value) = com_excel::execute(action, params).await? {
        return Ok(value);
    }
    if let Some(value) = com_ppt::execute(action, params).await? {
        return Ok(value);
    }

    match action {
        "status" => {
            let selected = match session.selected_window_hwnd {
                Some(hwnd) => window::get_window_by_hwnd(hwnd).ok(),
                None => None,
            };
            Ok(json!({
                "selected_window": selected,
                "cached_controls": session.controls_cache.len(),
                "cache_ttl_ms": config.control_cache_ttl_ms,
                "max_controls": config.max_controls
            }))
        }
        "list_windows" => {
            let refresh = read_bool(params, "refresh", true);
            ensure_window_cache(session, refresh)?;
            Ok(json!({ "windows": session.window_cache.clone() }))
        }
        "get_desktop_app_info" => {
            let refresh = read_bool(params, "refresh_app_windows", true);
            let remove_empty = read_bool(params, "remove_empty", true);
            ensure_window_cache(session, refresh)?;

            let windows = session
                .window_cache
                .iter()
                .filter(|window| {
                    if !remove_empty {
                        return true;
                    }
                    !window.title.trim().is_empty() || !window.class_name.trim().is_empty()
                })
                .enumerate()
                .map(|(index, window)| {
                    json!({
                        "id": (index + 1).to_string(),
                        "name": window.title,
                        "title": window.title,
                        "type": "Window",
                        "kind": "window",
                        "hwnd": window.hwnd,
                        "class_name": window.class_name,
                        "process_id": window.process_id,
                        "is_visible": window.is_visible,
                        "is_active": window.is_active,
                        "rect": window_rect_value(window)
                    })
                })
                .collect::<Vec<_>>();

            Ok(json!({ "windows": windows }))
        }
        "get_desktop_app_target_info" => {
            let refresh = read_bool(params, "refresh_app_windows", true);
            let remove_empty = read_bool(params, "remove_empty", true);
            ensure_window_cache(session, refresh)?;

            let windows = session
                .window_cache
                .iter()
                .filter(|window| {
                    if !remove_empty {
                        return true;
                    }
                    !window.title.trim().is_empty() || !window.class_name.trim().is_empty()
                })
                .enumerate()
                .map(|(index, window)| {
                    json!({
                        "kind": "window",
                        "id": (index + 1).to_string(),
                        "name": window.title,
                        "type": "Window",
                        "rect": window_rect_value(window),
                        "hwnd": window.hwnd
                    })
                })
                .collect::<Vec<_>>();

            Ok(json!({ "windows": windows }))
        }
        "select_window" => {
            let hwnd = window::resolve_window_hwnd(params, session.selected_window_hwnd)?;
            let selected = activate_window(session, hwnd)?;
            Ok(json!({ "selected_window": selected }))
        }
        "select_application_window" => {
            let refresh = read_bool(params, "refresh_app_windows", false);
            ensure_window_cache(session, refresh)?;
            let requested_name = read_optional_string(params, "name");
            let raw_id = read_identifier(params, "id");
            let raw_window_id = read_identifier(params, "window_id");
            let explicit_hwnd = params.get("hwnd").and_then(Value::as_i64);

            let (selection_id, target_hwnd) = if let Some(hwnd) = explicit_hwnd {
                (hwnd.to_string(), hwnd)
            } else if let Some(raw_window_id) = raw_window_id {
                if let Some(hwnd) = window::parse_hwnd_id(&raw_window_id) {
                    (raw_window_id, hwnd)
                } else {
                    return Err(format!("Invalid window_id: {}", raw_window_id));
                }
            } else if let Some(raw_id) = raw_id {
                if let Ok(label_index) = raw_id.parse::<usize>() {
                    if label_index >= 1 && label_index <= session.window_cache.len() {
                        let hwnd = session.window_cache[label_index - 1].hwnd;
                        (raw_id, hwnd)
                    } else if let Some(hwnd) = window::parse_hwnd_id(&raw_id) {
                        (raw_id, hwnd)
                    } else {
                        return Err(format!(
                            "Window id '{}' not found. Available labels: 1..{}",
                            raw_id,
                            session.window_cache.len()
                        ));
                    }
                } else if let Some(hwnd) = window::parse_hwnd_id(&raw_id) {
                    (raw_id, hwnd)
                } else {
                    return Err(format!("Invalid application window id: {}", raw_id));
                }
            } else {
                return Err("'id', 'window_id', or 'hwnd' is required".to_string());
            };

            let selected_window = activate_window(session, target_hwnd)?;
            let name_verified = requested_name
                .as_ref()
                .map(|name| selected_window.title == *name)
                .unwrap_or(true);

            let mut response = json!({
                "selected_window": selected_window,
                "id": selection_id,
                "name_verified": name_verified
            });
            if let Some(requested) = requested_name {
                if !name_verified {
                    response["warning"] = json!(format!(
                        "Warning: selected window title is '{}', but provided name was '{}'.",
                        response["selected_window"]["title"], requested
                    ));
                }
            }
            if response.get("warning").is_none() && !name_verified {
                response["warning"] = json!(format!(
                    "Warning: selected window title is '{}'.",
                    response["selected_window"]["title"]
                ));
            }
            Ok(response)
        }
        "get_window_info" => {
            let hwnd = window::resolve_window_hwnd(params, session.selected_window_hwnd)?;
            let selected = window::get_window_by_hwnd(hwnd)?;
            Ok(json!({ "window": selected }))
        }
        "get_app_window_info" => {
            let hwnd = window::resolve_window_hwnd(params, session.selected_window_hwnd)?;
            let selected = window::get_window_by_hwnd(hwnd)?;
            let field_list = read_field_list(params).ok_or_else(|| {
                "'field_list' is required and must be a non-empty string array".to_string()
            })?;
            Ok(window_entry_with_fields(&selected, &field_list))
        }
        "get_controls" => {
            let hwnd = window::resolve_window_hwnd(params, session.selected_window_hwnd)?;
            let refresh = read_bool(params, "refresh", false);
            ensure_control_cache(session, config, hwnd, refresh)?;
            let controls = session
                .controls_cache
                .iter()
                .map(control_catalog_entry)
                .collect::<Vec<_>>();
            Ok(json!({
                "window_id": hwnd.to_string(),
                "controls": controls,
            }))
        }
        "get_app_window_controls_info" => {
            let hwnd = window::resolve_window_hwnd(params, session.selected_window_hwnd)?;
            let refresh = read_bool(params, "refresh", true);
            ensure_control_cache(session, config, hwnd, refresh)?;
            let field_list = read_field_list(params).ok_or_else(|| {
                "'field_list' is required and must be a non-empty string array".to_string()
            })?;
            let controls = session
                .controls_cache
                .iter()
                .map(|control| control_entry_with_fields(control, &field_list))
                .collect::<Vec<_>>();
            Ok(json!(controls))
        }
        "get_app_window_controls_target_info" => {
            let hwnd = window::resolve_window_hwnd(params, session.selected_window_hwnd)?;
            let refresh = read_bool(params, "refresh", true);
            ensure_control_cache(session, config, hwnd, refresh)?;
            let controls = session
                .controls_cache
                .iter()
                .map(|control| {
                    json!({
                        "kind": "control",
                        "id": control.id,
                        "name": control.name,
                        "type": control.control_type,
                        "rect": control_rect_value(control),
                        "source": control.source,
                    })
                })
                .collect::<Vec<_>>();
            Ok(json!(controls))
        }
        "get_ui_tree" => {
            let hwnd = window::resolve_window_hwnd(params, session.selected_window_hwnd)?;
            let max_controls = read_i64(params, "max_controls", config.max_controls as i64)
                .clamp(10, 10_000) as usize;
            let tree = uia::get_ui_tree(hwnd, max_controls)?;
            Ok(tree)
        }
        "launch_application" => {
            if is_browser_launch_request(params) {
                return Err(
                    "Browser operations must use tool=browser only. Use browser action=start/open/navigate instead of desktop.launch_application."
                        .to_string(),
                );
            }
            let bash_command = read_optional_string(params, "bash_command");
            let command = read_optional_string(params, "command")
                .or_else(|| read_optional_string(params, "application_path"))
                .or_else(|| read_optional_string(params, "app_path"))
                .or_else(|| read_optional_string(params, "executable"))
                .or_else(|| read_optional_string(params, "app_name"));
            let args = params
                .get("args")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .map(|s| s.to_string())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let cwd = read_optional_string(params, "cwd");

            if let Some(script) = bash_command {
                let mut cmd = std::process::Command::new("powershell");
                cmd.args(["-NoProfile", "-NonInteractive", "-Command", &script]);
                if let Some(cwd) = cwd {
                    cmd.current_dir(cwd);
                }
                let child = cmd.spawn().map_err(|e| e.to_string())?;
                Ok(json!({
                    "bash_command": script,
                    "pid": child.id()
                }))
            } else if let Some(command) = command {
                let mut cmd = std::process::Command::new(&command);
                if !args.is_empty() {
                    cmd.args(&args);
                }
                if let Some(cwd) = cwd {
                    cmd.current_dir(cwd);
                }
                let child = cmd.spawn().map_err(|e| e.to_string())?;
                Ok(json!({
                    "command": command,
                    "args": args,
                    "pid": child.id()
                }))
            } else {
                Err("'command' is required (or use 'application_path' / 'app_path' / 'executable' / 'app_name' / 'bash_command')".to_string())
            }
        }
        "close_application" => {
            let hwnd = window::resolve_window_hwnd(params, session.selected_window_hwnd)?;
            window::close_window(hwnd)?;
            if session.selected_window_hwnd == Some(hwnd) {
                session.selected_window_hwnd = None;
                session.controls_cache.clear();
                session.controls_cached_at = None;
            }
            session.window_cache.clear();
            session.windows_cached_at = None;
            Ok(json!({ "closed_window": hwnd }))
        }
        "click_input" => {
            let selected = session
                .selected_window_hwnd
                .ok_or_else(|| "No selected window. Call action=select_window first".to_string())?;
            let refresh = read_bool(params, "refresh", true);
            ensure_control_cache(session, config, selected, refresh)?;
            let resolution = uia::resolve_control(params, &session.controls_cache, true)?;
            let button =
                read_optional_string(params, "button").unwrap_or_else(|| "left".to_string());
            let double_click = read_bool(params, "double", false);
            let message = input::click_control(&resolution.control, &button, double_click)?;
            let mut response = json!({
                "message": message,
                "control_id": resolution.control.id,
                "control_name": resolution.control.name,
                "control_hwnd": resolution.control.hwnd,
                "name_verified": resolution.name_verified
            });
            if !resolution.name_verified {
                let requested = resolution
                    .requested_name
                    .unwrap_or_else(|| "<missing>".to_string());
                response["warning"] = json!(format!(
                    "Warning: selected control id {} has name '{}', but provided name was '{}'.",
                    response["control_id"], response["control_name"], requested
                ));
            }
            Ok(response)
        }
        "click_on_coordinates" => {
            let hwnd = window::resolve_window_hwnd(params, session.selected_window_hwnd)?;
            let x = params.get("x").and_then(Value::as_f64).unwrap_or(0.5);
            let y = params.get("y").and_then(Value::as_f64).unwrap_or(0.5);
            let button =
                read_optional_string(params, "button").unwrap_or_else(|| "left".to_string());
            let double_click = read_bool(params, "double", false);
            let message = input::click_on_coordinates(hwnd, x, y, &button, double_click)?;
            Ok(json!({ "message": message, "window_hwnd": hwnd, "x": x, "y": y }))
        }
        "drag_on_coordinates" => {
            let hwnd = window::resolve_window_hwnd(params, session.selected_window_hwnd)?;
            let start_x = params.get("start_x").and_then(Value::as_f64).unwrap_or(0.3);
            let start_y = params.get("start_y").and_then(Value::as_f64).unwrap_or(0.3);
            let end_x = params.get("end_x").and_then(Value::as_f64).unwrap_or(0.7);
            let end_y = params.get("end_y").and_then(Value::as_f64).unwrap_or(0.7);
            let button =
                read_optional_string(params, "button").unwrap_or_else(|| "left".to_string());
            let duration = params
                .get("duration")
                .and_then(Value::as_f64)
                .unwrap_or(0.6);
            let message = input::drag_on_coordinates(
                hwnd, start_x, start_y, end_x, end_y, &button, duration,
            )?;
            Ok(json!({ "message": message }))
        }
        "set_edit_text" => {
            let selected = session
                .selected_window_hwnd
                .ok_or_else(|| "No selected window. Call action=select_window first".to_string())?;
            let refresh = read_bool(params, "refresh", true);
            ensure_control_cache(session, config, selected, refresh)?;
            let resolution = uia::resolve_control(params, &session.controls_cache, true)?;
            let text = read_string(params, "text")?;
            let message = input::set_edit_text_on_control(&resolution.control, &text)?;
            let mut response = json!({
                "message": message,
                "control_id": resolution.control.id,
                "control_name": resolution.control.name,
                "control_hwnd": resolution.control.hwnd,
                "name_verified": resolution.name_verified
            });
            if !resolution.name_verified {
                let requested = resolution
                    .requested_name
                    .unwrap_or_else(|| "<missing>".to_string());
                response["warning"] = json!(format!(
                    "Warning: selected control id {} has name '{}', but provided name was '{}'.",
                    response["control_id"], response["control_name"], requested
                ));
            }
            Ok(response)
        }
        "keyboard_input" => {
            let selected = session.selected_window_hwnd;
            let control_focus = read_bool(params, "control_focus", true);
            let (keys, keys_warning) = read_keyboard_sequence(params);
            let mut warning: Option<String> = None;
            let target = if params.get("id").is_some() || params.get("control_id").is_some() {
                if let Some(hwnd) = selected {
                    let refresh = read_bool(params, "refresh", true);
                    ensure_control_cache(session, config, hwnd, refresh)?;
                    let resolution = uia::resolve_control(params, &session.controls_cache, true)?;
                    if !resolution.name_verified {
                        let requested = resolution
                            .requested_name
                            .unwrap_or_else(|| "<missing>".to_string());
                        warning = Some(format!(
                            "Warning: selected control id {} has name '{}', but provided name was '{}'.",
                            resolution.control.id, resolution.control.name, requested
                        ));
                    }
                    Some(resolution.control.hwnd)
                } else {
                    None
                }
            } else {
                selected
            };
            let message = input::keyboard_input(target, keys.as_str(), control_focus)?;
            let mut response = json!({ "message": message });
            let mut warnings: Vec<String> = Vec::new();
            if let Some(value) = warning.take() {
                warnings.push(value);
            }
            if let Some(value) = keys_warning {
                warnings.push(value);
            }
            if !warnings.is_empty() {
                response["warning"] = json!(warnings.join(" "));
            }
            Ok(response)
        }
        "wheel_mouse_input" => {
            let selected = session
                .selected_window_hwnd
                .ok_or_else(|| "No selected window. Call action=select_window first".to_string())?;
            let wheel_dist = read_i64(params, "wheel_dist", -3).clamp(-200, 200) as i32;
            let mut warning: Option<String> = None;
            let message = if params.get("id").is_some() || params.get("control_id").is_some() {
                let refresh = read_bool(params, "refresh", true);
                ensure_control_cache(session, config, selected, refresh)?;
                let resolution = uia::resolve_control(params, &session.controls_cache, true)?;
                if !resolution.name_verified {
                    let requested = resolution
                        .requested_name
                        .unwrap_or_else(|| "<missing>".to_string());
                    warning = Some(format!(
                        "Warning: selected control id {} has name '{}', but provided name was '{}'.",
                        resolution.control.id, resolution.control.name, requested
                    ));
                }
                input::wheel_mouse_on_control(&resolution.control, wheel_dist)?
            } else {
                input::wheel_mouse_input(selected, wheel_dist)?
            };
            let mut response = json!({ "message": message, "wheel_dist": wheel_dist });
            if let Some(value) = warning {
                response["warning"] = json!(value);
            }
            Ok(response)
        }
        "get_control_texts" => {
            let selected = session
                .selected_window_hwnd
                .ok_or_else(|| "No selected window. Call action=select_window first".to_string())?;
            let refresh = read_bool(params, "refresh", false);
            ensure_control_cache(session, config, selected, refresh)?;
            if params.get("id").is_some() || params.get("control_id").is_some() {
                let resolution = uia::resolve_control(params, &session.controls_cache, true)?;
                let text = input::read_control_text(resolution.control.hwnd)?;
                let mut response = json!({
                    "control_id": resolution.control.id,
                    "control_name": resolution.control.name,
                    "control_hwnd": resolution.control.hwnd,
                    "text": text,
                    "name_verified": resolution.name_verified
                });
                if !resolution.name_verified {
                    let requested = resolution
                        .requested_name
                        .unwrap_or_else(|| "<missing>".to_string());
                    response["warning"] = json!(format!(
                        "Warning: selected control id {} has name '{}', but provided name was '{}'.",
                        response["control_id"], response["control_name"], requested
                    ));
                }
                Ok(response)
            } else {
                let mut items = Vec::new();
                for control in &session.controls_cache {
                    let text = input::read_control_text(control.hwnd).unwrap_or_default();
                    if !text.trim().is_empty() {
                        items.push(json!({
                            "id": control.id,
                            "name": control.name,
                            "text": text
                        }));
                    }
                }
                Ok(json!({ "texts": items }))
            }
        }
        "texts" => {
            let selected = session
                .selected_window_hwnd
                .ok_or_else(|| "No selected window. Call action=select_window first".to_string())?;
            let refresh = read_bool(params, "refresh", true);
            ensure_control_cache(session, config, selected, refresh)?;
            let resolution = uia::resolve_control(params, &session.controls_cache, true)?;
            let text = input::read_control_text(resolution.control.hwnd)?;
            let mut response = json!({
                "id": resolution.control.id,
                "name": resolution.control.name,
                "text": text,
                "name_verified": resolution.name_verified
            });
            if !resolution.name_verified {
                let requested = resolution
                    .requested_name
                    .unwrap_or_else(|| "<missing>".to_string());
                response["warning"] = json!(format!(
                    "Warning: selected control id {} has name '{}', but provided name was '{}'.",
                    response["id"], response["name"], requested
                ));
            }
            Ok(response)
        }
        "capture_desktop_screenshot" => {
            let path = capture::capture_to_png(screenshot_dir, None, config.screenshot_keep_count)?;
            Ok(json!({ "path": path.to_string_lossy().to_string() }))
        }
        "capture_window_screenshot" => {
            let hwnd = window::resolve_window_hwnd(params, session.selected_window_hwnd)?;
            let path =
                capture::capture_to_png(screenshot_dir, Some(hwnd), config.screenshot_keep_count)?;
            Ok(json!({ "path": path.to_string_lossy().to_string(), "window_hwnd": hwnd }))
        }
        "wait" => {
            let seconds = params.get("seconds").and_then(Value::as_f64).unwrap_or(1.0);
            let ms = (seconds.max(0.01) * 1000.0).round() as u64;
            tokio::time::sleep(Duration::from_millis(ms)).await;
            Ok(json!({ "waited_ms": ms }))
        }
        "summary" => Ok(json!({ "text": read_string(params, "text")? })),
        other => Err(format!("Unsupported desktop action: {}", other)),
    }
}
