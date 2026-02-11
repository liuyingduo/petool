use crate::models::skill::*;
use anyhow::{Result, anyhow};
use chrono::Utc;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

pub struct SkillManager {
    skills: HashMap<String, Skill>,
    skills_dir: PathBuf,
}

impl SkillManager {
    pub fn new(skills_dir: PathBuf) -> Result<Self> {
        // Create skills directory if it doesn't exist
        fs::create_dir_all(&skills_dir)?;

        Ok(Self {
            skills: HashMap::new(),
            skills_dir,
        })
    }

    pub async fn load_skills(&mut self) -> Result<()> {
        self.skills.clear();

        let entries = fs::read_dir(&self.skills_dir)?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                if let Ok(skill) = self.load_skill_from_dir(&path).await {
                    self.skills.insert(skill.id.clone(), skill);
                }
            }
        }

        Ok(())
    }

    async fn load_skill_from_dir(&self, path: &PathBuf) -> Result<Skill> {
        let skill_json_path = path.join("skill.json");

        if !skill_json_path.exists() {
            return Err(anyhow!("skill.json not found in {}", path.display()));
        }

        let skill_json_content = fs::read_to_string(&skill_json_path)?;
        let skill_meta: serde_json::Value = serde_json::from_str(&skill_json_content)?;

        let default_id = uuid::Uuid::new_v4().to_string();
        let id = skill_meta.get("id")
            .and_then(|v| v.as_str())
            .unwrap_or(&default_id)
            .to_string();

        let name = skill_meta.get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Skill name not found"))?
            .to_string();

        let version = skill_meta.get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("1.0.0")
            .to_string();

        let description = skill_meta.get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let author = skill_meta.get("author")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown")
            .to_string();

        // Determine skill type
        let script_type = if path.join("main.rs").exists() {
            SkillType::Rust
        } else if path.join("index.js").exists() || path.join("index.ts").exists() {
            SkillType::JavaScript
        } else {
            SkillType::JavaScript
        };

        Ok(Skill {
            id,
            name,
            version,
            description,
            author,
            enabled: true,
            installed_at: Utc::now(),
            script_type,
        })
    }

    pub fn list_skills(&self) -> Vec<Skill> {
        self.skills.values().cloned().collect()
    }

    pub fn get_skill(&self, id: &str) -> Option<&Skill> {
        self.skills.get(id)
    }

    pub async fn install_skill(&mut self, repo_url: &str) -> Result<Skill> {
        // Parse repo URL to get skill name
        let skill_name = repo_url
            .rsplit('/')
            .next()
            .ok_or_else(|| anyhow!("Invalid repo URL"))?
            .trim_end_matches(".git");

        let skill_path = self.skills_dir.join(skill_name);

        // Clone the repository
        let status = Command::new("git")
            .args(["clone", "--depth", "1", repo_url, skill_path.to_str().unwrap()])
            .status()?;

        if !status.success() {
            return Err(anyhow!("Failed to clone repository"));
        }

        // Load the skill
        let skill = self.load_skill_from_dir(&skill_path).await?;
        self.skills.insert(skill.id.clone(), skill.clone());

        Ok(skill)
    }

    pub async fn uninstall_skill(&mut self, id: &str) -> Result<()> {
        let _skill = self.get_skill(id)
            .ok_or_else(|| anyhow!("Skill not found: {}", id))?;

        // Find and remove the skill directory
        let entries = fs::read_dir(&self.skills_dir)?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                let skill_json_path = path.join("skill.json");
                if skill_json_path.exists() {
                    if let Ok(content) = fs::read_to_string(&skill_json_path) {
                        if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&content) {
                            if meta.get("id")
                                .and_then(|v| v.as_str())
                                .map(|s| s == id)
                                .unwrap_or(false)
                            {
                                // Remove directory
                                fs::remove_dir_all(&path)?;
                                break;
                            }
                        }
                    }
                }
            }
        }

        self.skills.remove(id);
        Ok(())
    }

    pub async fn execute_skill(
        &self,
        id: &str,
        params: HashMap<String, serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let skill = self.get_skill(id)
            .ok_or_else(|| anyhow!("Skill not found: {}", id))?;

        if !skill.enabled {
            return Err(anyhow!("Skill is disabled: {}", id));
        }

        // Find skill directory
        let entries = fs::read_dir(&self.skills_dir)?;
        let mut skill_path: Option<PathBuf> = None;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                let skill_json_path = path.join("skill.json");
                if skill_json_path.exists() {
                    if let Ok(content) = fs::read_to_string(&skill_json_path) {
                        if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&content) {
                            if meta.get("id")
                                .and_then(|v| v.as_str())
                                .map(|s| s == id)
                                .unwrap_or(false)
                            {
                                skill_path = Some(path);
                                break;
                            }
                        }
                    }
                }
            }
        }

        let skill_path = skill_path.ok_or_else(|| anyhow!("Skill path not found"))?;

        match skill.script_type {
            SkillType::JavaScript => {
                self.execute_javascript_skill(&skill_path, params).await
            }
            SkillType::Rust => {
                self.execute_rust_skill(&skill_path, params).await
            }
        }
    }

    async fn execute_javascript_skill(
        &self,
        skill_path: &PathBuf,
        params: HashMap<String, serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let index_path = skill_path.join("index.js");
        let params_json = serde_json::to_string(&params)?;

        // Use Node.js to execute the skill
        let output = Command::new("node")
            .arg(&index_path)
            .arg(&params_json)
            .output()?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("Skill execution failed: {}", error));
        }

        let result = String::from_utf8_lossy(&output.stdout);
        let result_json: serde_json::Value = serde_json::from_str(&result)?;
        Ok(result_json)
    }

    async fn execute_rust_skill(
        &self,
        skill_path: &PathBuf,
        params: HashMap<String, serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let exe_path = if cfg!(windows) {
            skill_path.join("skill.exe")
        } else {
            skill_path.join("skill")
        };

        if !exe_path.exists() {
            // Try to compile the skill
            let main_rs = skill_path.join("main.rs");
            let cargo_toml = skill_path.join("Cargo.toml");

            if cargo_toml.exists() {
                let status = Command::new("cargo")
                    .args(["build", "--release"])
                    .current_dir(skill_path)
                    .status()?;

                if !status.success() {
                    return Err(anyhow!("Failed to compile Rust skill"));
                }
            } else if main_rs.exists() {
                let status = Command::new("rustc")
                    .args(["-o", "skill", "main.rs"])
                    .current_dir(skill_path)
                    .status()?;

                if !status.success() {
                    return Err(anyhow!("Failed to compile Rust skill"));
                }
            }
        }

        let params_json = serde_json::to_string(&params)?;
        let output = Command::new(&exe_path)
            .arg(&params_json)
            .output()?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("Skill execution failed: {}", error));
        }

        let result = String::from_utf8_lossy(&output.stdout);
        let result_json: serde_json::Value = serde_json::from_str(&result)?;
        Ok(result_json)
    }

    pub async fn update_skill(&mut self, _id: &str) -> Result<Skill> {
        // Uninstall and reinstall to update
        // In a real implementation, you'd store the repo URL
        Err(anyhow!("Update not implemented - requires storing repo URL"))
    }
}
