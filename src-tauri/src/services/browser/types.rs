use crate::models::config::BrowserConfig;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BrowserToolRequest {
    pub action: String,
    #[serde(default)]
    pub profile: Option<String>,
    #[serde(default)]
    pub target_id: Option<String>,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserSidecarPaths {
    pub profiles_root: String,
    pub app_log_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserSidecarRequestPayload {
    pub request: BrowserToolRequest,
    pub browser_config: BrowserConfig,
    pub paths: BrowserSidecarPaths,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserRpcRequest {
    pub id: u64,
    pub method: String,
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserRpcResponse {
    pub id: u64,
    pub ok: bool,
    #[serde(default)]
    pub data: Option<Value>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub meta: Option<Value>,
}
