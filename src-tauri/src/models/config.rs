use serde::{Deserialize, Serialize};
use std::collections::HashMap;

fn default_tool_display_mode() -> String {
    "compact".to_string()
}

fn default_automation_enabled() -> bool {
    true
}

fn default_autostart_enabled() -> bool {
    false
}

fn default_notification_sound_enabled() -> bool {
    false
}

fn default_notification_break_reminder_enabled() -> bool {
    true
}

fn default_notification_task_completed_enabled() -> bool {
    true
}

fn default_downloads_directory_option() -> Option<String> {
    Some(
        crate::utils::resolve_default_downloads_dir()
            .to_string_lossy()
            .to_string(),
    )
}

fn default_automation_max_concurrent_runs() -> u32 {
    1
}

fn default_automation_close_behavior() -> AutomationCloseBehavior {
    AutomationCloseBehavior::Ask
}

fn default_heartbeat_enabled() -> bool {
    true
}

fn default_heartbeat_every_minutes() -> u32 {
    30
}

fn default_heartbeat_prompt() -> String {
    "Read HEARTBEAT.md if it exists in workspace and check pending tasks. If nothing needs attention, reply HEARTBEAT_OK."
        .to_string()
}

fn default_heartbeat_tool_whitelist() -> Vec<String> {
    vec![
        "workspace_list_directory".to_string(),
        "workspace_read_file".to_string(),
        "workspace_glob".to_string(),
        "workspace_grep".to_string(),
        "workspace_codesearch".to_string(),
        "workspace_lsp_symbols".to_string(),
        "web_fetch".to_string(),
        "web_search".to_string(),
        "sessions_list".to_string(),
        "sessions_history".to_string(),
        "sessions_send".to_string(),
        "sessions_spawn".to_string(),
        "workspace_write_file".to_string(),
        "workspace_edit_file".to_string(),
        "workspace_apply_patch".to_string(),
    ]
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

fn default_desktop_enabled() -> bool {
    cfg!(target_os = "windows")
}

fn default_desktop_operation_timeout_ms() -> u64 {
    20_000
}

fn default_desktop_control_cache_ttl_ms() -> u64 {
    120_000
}

fn default_desktop_max_controls() -> usize {
    800
}

fn default_desktop_screenshot_keep_count() -> usize {
    200
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
    #[serde(default = "default_autostart_enabled")]
    pub autostart_enabled: bool,
    #[serde(default = "default_downloads_directory_option")]
    pub downloads_directory: Option<String>,
    #[serde(default)]
    pub notifications: NotificationSettingsConfig,
    #[serde(default)]
    pub browser: BrowserConfig,
    #[serde(default)]
    pub desktop: DesktopConfig,
    #[serde(default)]
    pub automation: AutomationConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationSettingsConfig {
    #[serde(default = "default_notification_sound_enabled")]
    pub sound_enabled: bool,
    #[serde(default = "default_notification_break_reminder_enabled")]
    pub break_reminder_enabled: bool,
    #[serde(default = "default_notification_task_completed_enabled")]
    pub task_completed_enabled: bool,
}

impl Default for NotificationSettingsConfig {
    fn default() -> Self {
        Self {
            sound_enabled: default_notification_sound_enabled(),
            break_reminder_enabled: default_notification_break_reminder_enabled(),
            task_completed_enabled: default_notification_task_completed_enabled(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutomationCloseBehavior {
    Ask,
    MinimizeToTray,
    Exit,
}

impl Default for AutomationCloseBehavior {
    fn default() -> Self {
        default_automation_close_behavior()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatAutomationConfig {
    #[serde(default = "default_heartbeat_enabled")]
    pub enabled: bool,
    #[serde(default = "default_heartbeat_every_minutes")]
    pub every_minutes: u32,
    #[serde(default)]
    pub target_conversation_id: Option<String>,
    #[serde(default = "default_heartbeat_prompt")]
    pub prompt: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub workspace_directory: Option<String>,
    #[serde(default = "default_heartbeat_tool_whitelist")]
    pub tool_whitelist: Vec<String>,
}

impl Default for HeartbeatAutomationConfig {
    fn default() -> Self {
        Self {
            enabled: default_heartbeat_enabled(),
            every_minutes: default_heartbeat_every_minutes(),
            target_conversation_id: None,
            prompt: default_heartbeat_prompt(),
            model: None,
            workspace_directory: None,
            tool_whitelist: default_heartbeat_tool_whitelist(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationConfig {
    #[serde(default = "default_automation_enabled")]
    pub enabled: bool,
    #[serde(default = "default_automation_max_concurrent_runs")]
    pub max_concurrent_runs: u32,
    #[serde(default = "default_automation_close_behavior")]
    pub close_behavior: AutomationCloseBehavior,
    #[serde(default)]
    pub heartbeat: HeartbeatAutomationConfig,
}

impl Default for AutomationConfig {
    fn default() -> Self {
        Self {
            enabled: default_automation_enabled(),
            max_concurrent_runs: default_automation_max_concurrent_runs(),
            close_behavior: default_automation_close_behavior(),
            heartbeat: HeartbeatAutomationConfig::default(),
        }
    }
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DesktopApprovalMode {
    HighRiskOnly,
    AlwaysAsk,
    AlwaysAllow,
}

impl Default for DesktopApprovalMode {
    fn default() -> Self {
        Self::HighRiskOnly
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesktopConfig {
    #[serde(default = "default_desktop_enabled")]
    pub enabled: bool,
    #[serde(default = "default_desktop_operation_timeout_ms")]
    pub operation_timeout_ms: u64,
    #[serde(default = "default_desktop_control_cache_ttl_ms")]
    pub control_cache_ttl_ms: u64,
    #[serde(default = "default_desktop_max_controls")]
    pub max_controls: usize,
    #[serde(default)]
    pub screenshot_dir: Option<String>,
    #[serde(default = "default_desktop_screenshot_keep_count")]
    pub screenshot_keep_count: usize,
    #[serde(default)]
    pub approval_mode: DesktopApprovalMode,
}

impl Default for DesktopConfig {
    fn default() -> Self {
        Self {
            enabled: default_desktop_enabled(),
            operation_timeout_ms: default_desktop_operation_timeout_ms(),
            control_cache_ttl_ms: default_desktop_control_cache_ttl_ms(),
            max_controls: default_desktop_max_controls(),
            screenshot_dir: None,
            screenshot_keep_count: default_desktop_screenshot_keep_count(),
            approval_mode: DesktopApprovalMode::default(),
        }
    }
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
            autostart_enabled: default_autostart_enabled(),
            downloads_directory: default_downloads_directory_option(),
            notifications: NotificationSettingsConfig::default(),
            browser: BrowserConfig::default(),
            desktop: DesktopConfig::default(),
            automation: AutomationConfig::default(),
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
        assert!(!config.autostart_enabled);
        assert!(config.downloads_directory.is_some());
        assert!(!config.notifications.sound_enabled);
        assert!(config.notifications.break_reminder_enabled);
        assert!(config.notifications.task_completed_enabled);
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
        assert_eq!(config.desktop.operation_timeout_ms, 20_000);
        assert_eq!(config.desktop.control_cache_ttl_ms, 120_000);
        assert_eq!(config.desktop.max_controls, 800);
        assert_eq!(config.desktop.screenshot_keep_count, 200);
        assert_eq!(
            config.desktop.approval_mode,
            DesktopApprovalMode::HighRiskOnly
        );
        assert!(config.automation.enabled);
        assert_eq!(config.automation.max_concurrent_runs, 1);
        assert_eq!(
            config.automation.close_behavior,
            AutomationCloseBehavior::Ask
        );
        assert!(config.automation.heartbeat.enabled);
        assert_eq!(config.automation.heartbeat.every_minutes, 30);
        assert!(!config.automation.heartbeat.tool_whitelist.is_empty());
    }
}

