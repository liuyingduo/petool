use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesktopToolRequest {
    pub action: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DesktopRiskLevel {
    Low,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesktopResponseMeta {
    pub action: String,
    pub duration_ms: u64,
    pub risk_level: DesktopRiskLevel,
    pub conversation_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesktopResponseEnvelope {
    pub ok: bool,
    pub data: Value,
    pub error: Option<String>,
    pub meta: DesktopResponseMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesktopRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl DesktopRect {
    pub fn width(&self) -> i32 {
        self.right - self.left
    }

    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowSnapshot {
    pub id: String,
    pub title: String,
    pub class_name: String,
    pub process_id: u32,
    pub rect: DesktopRect,
    pub is_visible: bool,
    pub is_active: bool,
    pub hwnd: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlSnapshot {
    pub id: String,
    pub name: String,
    pub class_name: String,
    pub rect: DesktopRect,
    pub parent_window_id: String,
    pub hwnd: i64,
}
