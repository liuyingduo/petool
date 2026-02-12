use serde::{Deserialize, Serialize};
use std::collections::HashMap;

fn default_tool_display_mode() -> String {
    "compact".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub api_key: Option<String>,
    pub api_base: Option<String>,
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

impl Default for Config {
    fn default() -> Self {
        Self {
            api_key: None,
            api_base: Some("https://open.bigmodel.cn/api/paas/v4".to_string()),
            model: "glm-5".to_string(),
            system_prompt: None,
            work_directory: None,
            conversation_workspaces: HashMap::new(),
            theme: "dark".to_string(),
            tool_display_mode: default_tool_display_mode(),
            mcp_servers: Vec::new(),
            tool_permissions: HashMap::new(),
            tool_path_permissions: Vec::new(),
        }
    }
}
