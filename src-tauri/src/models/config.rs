use serde::{Deserialize, Serialize};
use std::collections::HashMap;

fn default_tool_display_mode() -> String {
    "compact".to_string()
}

fn default_browser_enabled() -> bool {
    true
}

fn default_browser_default_profile() -> String {
    "openclaw".to_string()
}

fn default_clawhub_api_base() -> String {
    "https://clawhub.ai".to_string()
}

fn default_clawhub_api_base_option() -> Option<String> {
    Some(default_clawhub_api_base())
}

fn default_ark_api_base() -> String {
    "https://ark.cn-beijing.volces.com/api/v3".to_string()
}

fn default_ark_api_base_option() -> Option<String> {
    Some(default_ark_api_base())
}

fn default_image_model() -> String {
    "doubao-seedream-4-5-251128".to_string()
}

fn default_image_understand_model() -> String {
    "glm-4.6v".to_string()
}

fn default_video_model() -> String {
    "doubao-seedance-1-0-pro-250528".to_string()
}

fn default_image_size() -> String {
    "2K".to_string()
}

fn default_image_watermark() -> bool {
    true
}

fn default_browser_operation_timeout_ms() -> u64 {
    20_000
}

fn default_browser_performance_preset() -> String {
    "balanced".to_string()
}

fn default_browser_capture_response_bodies() -> bool {
    false
}

fn default_browser_default_act_timeout_ms() -> u64 {
    1_400
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub api_key: Option<String>,
    pub api_base: Option<String>,
    #[serde(default)]
    pub clawhub_api_key: Option<String>,
    #[serde(default = "default_clawhub_api_base_option")]
    pub clawhub_api_base: Option<String>,
    #[serde(default)]
    pub ark_api_key: Option<String>,
    #[serde(default = "default_ark_api_base_option")]
    pub ark_api_base: Option<String>,
    #[serde(default)]
    pub minimax_api_key: Option<String>,
    #[serde(default = "default_image_model")]
    pub image_model: String,
    #[serde(default = "default_image_understand_model")]
    pub image_understand_model: String,
    #[serde(default = "default_video_model")]
    pub video_model: String,
    #[serde(default = "default_image_size")]
    pub image_size: String,
    #[serde(default = "default_image_watermark")]
    pub image_watermark: bool,
    pub model: String,
    pub system_prompt: Option<String>,
    pub work_directory: Option<String>,
    #[serde(default)]
    pub conversation_workspaces: HashMap<String, String>,
    pub theme: String,
    #[serde(default = "default_tool_display_mode")]
    pub tool_display_mode: String,
    pub mcp_servers: Vec<McpServerConfig>,
    #[serde(default)]
    pub tool_permissions: HashMap<String, ToolPermissionAction>,
    #[serde(default)]
    pub tool_path_permissions: Vec<ToolPathPermissionRule>,
    #[serde(default)]
    pub auto_approve_tool_requests: bool,
    #[serde(default)]
    pub browser: BrowserConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolPermissionAction {
    Allow,
    Ask,
    Deny,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolPathPermissionRule {
    pub tool_pattern: String,
    pub path_pattern: String,
    pub action: ToolPermissionAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub name: String,
    pub transport: McpTransport,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum McpTransport {
    Stdio { command: String, args: Vec<String> },
    Http { url: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserEngine {
    Chrome,
    Chromium,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserViewport {
    pub width: u32,
    pub height: u32,
}

impl Default for BrowserViewport {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 800,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserProfileConfig {
    #[serde(default)]
    pub engine: BrowserEngine,
    #[serde(default)]
    pub headless: bool,
    #[serde(default)]
    pub executable_path: Option<String>,
    #[serde(default)]
    pub cdp_url: Option<String>,
    #[serde(default)]
    pub user_data_dir: Option<String>,
    #[serde(default = "default_openclaw_color")]
    pub color: String,
    #[serde(default)]
    pub viewport: BrowserViewport,
}

impl Default for BrowserProfileConfig {
    fn default() -> Self {
        Self {
            engine: BrowserEngine::Chrome,
            headless: false,
            executable_path: None,
            cdp_url: None,
            user_data_dir: None,
            color: default_openclaw_color(),
            viewport: BrowserViewport::default(),
        }
    }
}

fn default_openclaw_color() -> String {
    "#FF6A00".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserConfig {
    #[serde(default = "default_browser_enabled")]
    pub enabled: bool,
    #[serde(default = "default_browser_default_profile")]
    pub default_profile: String,
    #[serde(default)]
    pub evaluate_enabled: bool,
    #[serde(default)]
    pub allow_private_network: bool,
    #[serde(default = "default_browser_performance_preset")]
    pub performance_preset: String,
    #[serde(default = "default_browser_capture_response_bodies")]
    pub capture_response_bodies: bool,
    #[serde(default = "default_browser_default_act_timeout_ms")]
    pub default_act_timeout_ms: u64,
    #[serde(default = "default_browser_operation_timeout_ms")]
    pub operation_timeout_ms: u64,
    #[serde(default = "default_browser_profiles")]
    pub profiles: HashMap<String, BrowserProfileConfig>,
}

fn default_browser_profiles() -> HashMap<String, BrowserProfileConfig> {
    let mut profiles = HashMap::new();
    profiles.insert("openclaw".to_string(), BrowserProfileConfig::default());
    profiles
}

impl Default for BrowserConfig {
    fn default() -> Self {
        Self {
            enabled: default_browser_enabled(),
            default_profile: default_browser_default_profile(),
            evaluate_enabled: false,
            allow_private_network: false,
            performance_preset: default_browser_performance_preset(),
            capture_response_bodies: default_browser_capture_response_bodies(),
            default_act_timeout_ms: default_browser_default_act_timeout_ms(),
            operation_timeout_ms: default_browser_operation_timeout_ms(),
            profiles: default_browser_profiles(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            api_key: None,
            api_base: Some("https://open.bigmodel.cn/api/paas/v4".to_string()),
            clawhub_api_key: None,
            clawhub_api_base: Some(default_clawhub_api_base()),
            ark_api_key: None,
            ark_api_base: Some(default_ark_api_base()),
            minimax_api_key: None,
            image_model: default_image_model(),
            image_understand_model: default_image_understand_model(),
            video_model: default_video_model(),
            image_size: default_image_size(),
            image_watermark: default_image_watermark(),
            model: "glm-5".to_string(),
            system_prompt: None,
            work_directory: None,
            conversation_workspaces: HashMap::new(),
            theme: "light".to_string(),
            tool_display_mode: default_tool_display_mode(),
            mcp_servers: Vec::new(),
            tool_permissions: HashMap::new(),
            tool_path_permissions: Vec::new(),
            auto_approve_tool_requests: false,
            browser: BrowserConfig::default(),
        }
    }
}

impl Default for BrowserEngine {
    fn default() -> Self {
        Self::Chrome
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_includes_browser_defaults() {
        let config = Config::default();
        assert!(!config.auto_approve_tool_requests);
        assert_eq!(
            config.clawhub_api_base,
            Some("https://clawhub.ai".to_string())
        );
        assert!(config.browser.enabled);
        assert_eq!(config.browser.default_profile, "openclaw");
        assert_eq!(config.browser.performance_preset, "balanced");
        assert!(!config.browser.capture_response_bodies);
        assert_eq!(config.browser.default_act_timeout_ms, 1_400);
        assert_eq!(config.browser.operation_timeout_ms, 20_000);
        assert!(config.browser.profiles.contains_key("openclaw"));
        let profile = config.browser.profiles.get("openclaw").unwrap();
        assert_eq!(profile.color, "#FF6A00");
        assert_eq!(profile.viewport.width, 1280);
        assert_eq!(profile.viewport.height, 800);
    }
}
