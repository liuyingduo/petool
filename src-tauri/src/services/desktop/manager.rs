use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::models::config::DesktopConfig;
use crate::utils::get_app_log_dir;

use super::types::{
    ControlSnapshot, DesktopResponseEnvelope, DesktopResponseMeta, DesktopRiskLevel,
    DesktopToolRequest,
};

#[cfg(target_os = "windows")]
use super::win;

#[derive(Debug, Default)]
pub(super) struct DesktopSessionState {
    pub selected_window_hwnd: Option<i64>,
    pub controls_cache: Vec<ControlSnapshot>,
    pub controls_cached_at: Option<Instant>,
}

#[derive(Debug, Default)]
pub struct DesktopManager {
    sessions: HashMap<String, DesktopSessionState>,
}

static DESKTOP_MANAGER: OnceLock<tokio::sync::Mutex<DesktopManager>> = OnceLock::new();

fn desktop_manager() -> &'static tokio::sync::Mutex<DesktopManager> {
    DESKTOP_MANAGER.get_or_init(|| tokio::sync::Mutex::new(DesktopManager::default()))
}

fn default_screenshot_dir() -> Result<PathBuf, String> {
    let log_dir = get_app_log_dir().map_err(|e| e.to_string())?;
    Ok(log_dir.join("desktop-shots"))
}

fn resolve_screenshot_dir(config: &DesktopConfig) -> Result<PathBuf, String> {
    if let Some(raw) = config
        .screenshot_dir
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let candidate = PathBuf::from(raw);
        if candidate.is_absolute() {
            return Ok(candidate);
        }
        let base = get_app_log_dir().map_err(|e| e.to_string())?;
        return Ok(base.join(candidate));
    }
    default_screenshot_dir()
}

impl DesktopManager {
    fn session_mut(&mut self, conversation_id: &str) -> &mut DesktopSessionState {
        self.sessions
            .entry(conversation_id.to_string())
            .or_default()
    }

    async fn execute_request(
        &mut self,
        conversation_id: &str,
        request: &DesktopToolRequest,
        config: &DesktopConfig,
    ) -> Result<Value, String> {
        let action = request.action.trim().to_string();
        if action.is_empty() {
            return Err("'action' is required".to_string());
        }

        let risk_level = classify_action_risk(&action);
        let started_at = Instant::now();

        if !config.enabled {
            let envelope = DesktopResponseEnvelope {
                ok: false,
                data: Value::Null,
                error: Some(
                    "Desktop control is disabled. Set desktop.enabled=true in settings."
                        .to_string(),
                ),
                meta: DesktopResponseMeta {
                    action,
                    duration_ms: started_at.elapsed().as_millis() as u64,
                    risk_level,
                    conversation_id: conversation_id.to_string(),
                },
            };
            return serde_json::to_value(envelope).map_err(|e| e.to_string());
        }

        let data_or_error: Result<Value, String> = {
            #[cfg(target_os = "windows")]
            {
                let screenshot_dir = resolve_screenshot_dir(config)?;
                let session = self.session_mut(conversation_id);
                let timeout_ms = config.operation_timeout_ms.clamp(500, 300_000);
                match tokio::time::timeout(
                    Duration::from_millis(timeout_ms),
                    win::execute_action(&action, &request.params, session, config, &screenshot_dir),
                )
                .await
                {
                    Ok(result) => result,
                    Err(_) => Err(format!(
                        "Desktop action '{}' timed out after {} ms",
                        action, timeout_ms
                    )),
                }
            }

            #[cfg(not(target_os = "windows"))]
            {
                let _ = conversation_id;
                let _ = request;
                let _ = config;
                Err("Desktop tool is only supported on Windows".to_string())
            }
        };

        let (ok, data, error) = match data_or_error {
            Ok(data) => (true, data, None),
            Err(error) => (false, Value::Null, Some(error)),
        };

        let envelope = DesktopResponseEnvelope {
            ok,
            data,
            error,
            meta: DesktopResponseMeta {
                action,
                duration_ms: started_at.elapsed().as_millis() as u64,
                risk_level,
                conversation_id: conversation_id.to_string(),
            },
        };

        serde_json::to_value(envelope).map_err(|e| e.to_string())
    }
}

pub async fn execute_desktop_request(
    conversation_id: &str,
    request: &DesktopToolRequest,
    config: &DesktopConfig,
) -> Result<Value, String> {
    let mut manager = desktop_manager().lock().await;
    manager
        .execute_request(conversation_id, request, config)
        .await
}

pub fn classify_action_risk(action: &str) -> DesktopRiskLevel {
    let normalized = action.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "status"
        | "list_windows"
        | "select_window"
        | "get_window_info"
        | "get_controls"
        | "get_ui_tree"
        | "capture_desktop_screenshot"
        | "capture_window_screenshot"
        | "get_control_texts"
        | "wait"
        | "word_get_doc_info"
        | "excel_get_workbook_info"
        | "ppt_get_presentation_info" => DesktopRiskLevel::Low,
        _ => DesktopRiskLevel::High,
    }
}

pub fn is_high_risk_action(action: &str) -> bool {
    classify_action_risk(action) == DesktopRiskLevel::High
}

pub fn action_from_arguments(arguments: &Value) -> Option<String> {
    arguments
        .get("action")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
}
