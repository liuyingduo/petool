use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub api_key: Option<String>,
    pub api_base: Option<String>,
    pub model: String,
    pub system_prompt: Option<String>,
    pub work_directory: Option<String>,
    pub theme: String,
    pub mcp_servers: Vec<McpServerConfig>,
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
            model: "glm-4.7".to_string(),
            system_prompt: None,
            work_directory: None,
            theme: "dark".to_string(),
            mcp_servers: Vec::new(),
        }
    }
}
