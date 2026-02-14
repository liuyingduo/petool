use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub enabled: bool,
    pub installed_at: DateTime<Utc>,
    pub script_type: SkillType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SkillType {
    Rust,
    JavaScript,
    Markdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillManifest {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(default = "default_entry_point")]
    pub entry_point: String,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub dependencies: Option<Vec<String>>,
}

fn default_entry_point() -> String {
    "index.js".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDiscoveryItem {
    pub id: String,
    pub name: String,
    pub description: String,
    pub repo_url: String,
    pub repo_full_name: String,
    pub repo_html_url: String,
    pub source: String,
    pub skill_path: Option<String>,
    pub stars: u64,
    pub updated_at: Option<String>,
    pub installed: bool,
}
