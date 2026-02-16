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
        "list_windows" => Ok(json!({
            "windows": window::list_windows()?
        })),
        "select_window" => {
            let hwnd = window::resolve_window_hwnd(params, session.selected_window_hwnd)?;
            window::focus_window(hwnd)?;
            let selected = window::get_window_by_hwnd(hwnd)?;
            session.selected_window_hwnd = Some(hwnd);
            session.controls_cache.clear();
            session.controls_cached_at = None;
            Ok(json!({ "selected_window": selected }))
        }
        "get_window_info" => {
            let hwnd = window::resolve_window_hwnd(params, session.selected_window_hwnd)?;
            let selected = window::get_window_by_hwnd(hwnd)?;
            Ok(json!({ "window": selected }))
        }
        "get_controls" => {
            let hwnd = window::resolve_window_hwnd(params, session.selected_window_hwnd)?;
            let refresh = read_bool(params, "refresh", false);
            ensure_control_cache(session, config, hwnd, refresh)?;
            Ok(json!({
                "window_id": hwnd.to_string(),
                "controls": session.controls_cache,
            }))
        }
        "get_ui_tree" => {
            let hwnd = window::resolve_window_hwnd(params, session.selected_window_hwnd)?;
            let max_controls = read_i64(params, "max_controls", config.max_controls as i64)
                .clamp(10, 10_000) as usize;
            let tree = uia::get_ui_tree(hwnd, max_controls)?;
            Ok(tree)
        }
        "launch_application" => {
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
            Ok(json!({ "closed_window": hwnd }))
        }
        "click_input" => {
            let selected = session
                .selected_window_hwnd
                .ok_or_else(|| "No selected window. Call action=select_window first".to_string())?;
            ensure_control_cache(session, config, selected, false)?;
            let control_hwnd = uia::resolve_control_hwnd(params, &session.controls_cache)?;
            let button =
                read_optional_string(params, "button").unwrap_or_else(|| "left".to_string());
            let double_click = read_bool(params, "double", false);
            let message = input::click_input(control_hwnd, &button, double_click)?;
            Ok(json!({ "message": message, "control_hwnd": control_hwnd }))
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
            ensure_control_cache(session, config, selected, false)?;
            let control_hwnd = uia::resolve_control_hwnd(params, &session.controls_cache)?;
            let text = read_string(params, "text")?;
            let message = input::set_edit_text(control_hwnd, &text)?;
            Ok(json!({ "message": message, "control_hwnd": control_hwnd }))
        }
        "keyboard_input" => {
            let selected = session.selected_window_hwnd;
            let control_focus = read_bool(params, "control_focus", true);
            let keys = read_string(params, "keys")?;
            let target = if params.get("id").is_some() || params.get("control_id").is_some() {
                if let Some(hwnd) = selected {
                    ensure_control_cache(session, config, hwnd, false)?;
                    Some(uia::resolve_control_hwnd(params, &session.controls_cache)?)
                } else {
                    None
                }
            } else {
                selected
            };
            let message = input::keyboard_input(target, &keys, control_focus)?;
            Ok(json!({ "message": message }))
        }
        "wheel_mouse_input" => {
            let selected = session
                .selected_window_hwnd
                .ok_or_else(|| "No selected window. Call action=select_window first".to_string())?;
            let wheel_dist = read_i64(params, "wheel_dist", -3).clamp(-200, 200) as i32;
            let target = if params.get("id").is_some() || params.get("control_id").is_some() {
                ensure_control_cache(session, config, selected, false)?;
                uia::resolve_control_hwnd(params, &session.controls_cache)?
            } else {
                selected
            };
            let message = input::wheel_mouse_input(target, wheel_dist)?;
            Ok(json!({ "message": message, "wheel_dist": wheel_dist }))
        }
        "get_control_texts" => {
            let selected = session
                .selected_window_hwnd
                .ok_or_else(|| "No selected window. Call action=select_window first".to_string())?;
            ensure_control_cache(session, config, selected, false)?;
            if params.get("id").is_some() || params.get("control_id").is_some() {
                let control_hwnd = uia::resolve_control_hwnd(params, &session.controls_cache)?;
                let text = input::read_control_text(control_hwnd)?;
                Ok(json!({
                    "control_hwnd": control_hwnd,
                    "text": text
                }))
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
        other => Err(format!("Unsupported desktop action: {}", other)),
    }
}
