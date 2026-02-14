use crate::models::skill::*;
use anyhow::{anyhow, Result};
use chrono::Utc;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use uuid::Uuid;
use walkdir::WalkDir;

pub struct SkillManager {
    skills: HashMap<String, Skill>,
    skill_paths: HashMap<String, PathBuf>,
    skill_entry_points: HashMap<String, String>,
    skills_dir: PathBuf,
}

#[derive(Debug, serde::Deserialize)]
struct GithubCodeSearchResponse {
    #[serde(default)]
    items: Vec<GithubCodeSearchItem>,
}

#[derive(Debug, serde::Deserialize)]
struct GithubCodeSearchItem {
    path: String,
    repository: GithubRepository,
}

#[derive(Debug, serde::Deserialize)]
struct GithubRepository {
    name: String,
    full_name: String,
    html_url: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    stargazers_count: Option<u64>,
    #[serde(default)]
    updated_at: Option<String>,
    #[serde(default)]
    default_branch: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct RemoteSkillManifest {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Debug, Clone, Copy)]
enum GithubManifestKind {
    SkillMarkdown,
    SkillJson,
}

#[derive(Debug, Clone)]
struct SkillMarkdownManifest {
    name: String,
    description: String,
    author: String,
    version: String,
    body: String,
}

#[derive(Debug, Clone)]
struct LoadedSkill {
    skill: Skill,
    entry_point: Option<String>,
}

fn parse_github_full_name_from_repo_url(repo_url: &str) -> Option<String> {
    let normalized = repo_url.trim().trim_end_matches(".git");
    let marker = "github.com/";
    let index = normalized.find(marker)?;
    let tail = &normalized[index + marker.len()..];
    let mut parts = tail.split('/');
    let owner = parts.next()?.trim();
    let repo = parts.next()?.trim();
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some(format!("{}/{}", owner, repo))
}

fn read_string_at_path<'a>(value: &'a Value, path: &[&str]) -> Option<&'a str> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

fn read_u64_at_path(value: &Value, path: &[&str]) -> Option<u64> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current.as_u64()
}

fn read_string_or_number_at_path(value: &Value, path: &[&str]) -> Option<String> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    if let Some(text) = current.as_str() {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    if let Some(num) = current.as_u64() {
        return Some(num.to_string());
    }
    if let Some(num) = current.as_i64() {
        return Some(num.to_string());
    }
    None
}

fn normalize_optional_skill_path(value: Option<String>) -> Option<String> {
    value
        .map(|text| text.trim().trim_matches('/').trim_matches('\\').to_string())
        .filter(|text| !text.is_empty() && text != ".")
}

fn sanitize_skill_dir_name(value: &str) -> String {
    let normalized: String = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    normalized
        .split('-')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn build_stable_skill_id(name: &str, path: &Path) -> String {
    let from_name = sanitize_skill_dir_name(name);
    if !from_name.is_empty() {
        return from_name;
    }
    let from_path = path
        .file_name()
        .and_then(|value| value.to_str())
        .map(sanitize_skill_dir_name)
        .unwrap_or_default();
    if !from_path.is_empty() {
        return from_path;
    }
    Uuid::new_v4().to_string()
}

fn split_markdown_frontmatter(content: &str) -> Option<(String, String)> {
    let mut lines = content.lines();
    let first = lines.next()?.trim();
    if first != "---" {
        return None;
    }

    let mut frontmatter_lines: Vec<&str> = Vec::new();
    let mut body_lines: Vec<&str> = Vec::new();
    let mut in_body = false;
    for line in lines {
        if !in_body && line.trim() == "---" {
            in_body = true;
            continue;
        }
        if in_body {
            body_lines.push(line);
        } else {
            frontmatter_lines.push(line);
        }
    }
    if !in_body {
        return None;
    }
    Some((frontmatter_lines.join("\n"), body_lines.join("\n")))
}

fn read_yaml_string(map: &serde_yaml::Mapping, key: &str) -> Option<String> {
    let key_value = serde_yaml::Value::String(key.to_string());
    map.get(&key_value)?
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn summarize_markdown_body(body: &str) -> String {
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with('#') {
            continue;
        }
        return trimmed.to_string();
    }
    String::new()
}

fn parse_skill_markdown_manifest(content: &str) -> Option<SkillMarkdownManifest> {
    let (frontmatter_raw, body_raw) = split_markdown_frontmatter(content)?;
    let yaml: serde_yaml::Value = serde_yaml::from_str(&frontmatter_raw).ok()?;
    let map = yaml.as_mapping()?;

    let name = read_yaml_string(map, "name")?;
    let body = body_raw.trim().to_string();
    let description = read_yaml_string(map, "description")
        .or_else(|| read_yaml_string(map, "summary"))
        .unwrap_or_else(|| summarize_markdown_body(&body));
    let author = read_yaml_string(map, "author").unwrap_or_else(|| "unknown".to_string());
    let version = read_yaml_string(map, "version").unwrap_or_else(|| "1.0.0".to_string());

    Some(SkillMarkdownManifest {
        name,
        description,
        author,
        version,
        body,
    })
}

fn copy_directory_without_git(source: &Path, destination: &Path) -> Result<()> {
    for entry in WalkDir::new(source) {
        let entry = entry?;
        let entry_path = entry.path();
        let relative = entry_path
            .strip_prefix(source)
            .map_err(|e| anyhow!("Failed to resolve relative path: {}", e))?;
        if relative
            .components()
            .any(|component| component.as_os_str() == ".git")
        {
            continue;
        }
        let target = destination.join(relative);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&target)?;
        } else {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(entry_path, &target)?;
        }
    }
    Ok(())
}

impl SkillManager {
    pub fn new(skills_dir: PathBuf) -> Result<Self> {
        // Create skills directory if it doesn't exist
        fs::create_dir_all(&skills_dir)?;

        Ok(Self {
            skills: HashMap::new(),
            skill_paths: HashMap::new(),
            skill_entry_points: HashMap::new(),
            skills_dir,
        })
    }

    pub async fn load_skills(&mut self) -> Result<()> {
        self.skills.clear();
        self.skill_paths.clear();
        self.skill_entry_points.clear();

        let entries = fs::read_dir(&self.skills_dir)?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                if let Ok(loaded) = self.load_skill_from_dir(&path).await {
                    self.skill_paths
                        .insert(loaded.skill.id.clone(), path.clone());
                    if let Some(entry_point) = loaded.entry_point.as_ref() {
                        self.skill_entry_points
                            .insert(loaded.skill.id.clone(), entry_point.clone());
                    }
                    self.skills.insert(loaded.skill.id.clone(), loaded.skill);
                }
            }
        }

        Ok(())
    }

    async fn load_skill_from_dir(&self, path: &PathBuf) -> Result<LoadedSkill> {
        let skill_json_path = path.join("skill.json");
        if skill_json_path.exists() {
            let skill_json_content = fs::read_to_string(&skill_json_path)?;
            let skill_meta: SkillManifest = serde_json::from_str(&skill_json_content)?;

            let id = skill_meta
                .id
                .unwrap_or_else(|| build_stable_skill_id(&skill_meta.name, path.as_path()));
            let script_type = if path.join("main.rs").exists() {
                SkillType::Rust
            } else if path.join("index.js").exists() || path.join("index.ts").exists() {
                SkillType::JavaScript
            } else {
                SkillType::JavaScript
            };

            let entry_point = if matches!(script_type, SkillType::JavaScript) {
                let raw = skill_meta.entry_point.trim();
                if raw.is_empty() {
                    Some("index.js".to_string())
                } else {
                    Some(raw.to_string())
                }
            } else {
                None
            };

            return Ok(LoadedSkill {
                skill: Skill {
                    id,
                    name: skill_meta.name,
                    version: skill_meta.version,
                    description: skill_meta.description,
                    author: skill_meta.author,
                    enabled: true,
                    installed_at: Utc::now(),
                    script_type,
                },
                entry_point,
            });
        }

        let skill_md_path = path.join("SKILL.md");
        if skill_md_path.exists() {
            let skill_md_content = fs::read_to_string(&skill_md_path)?;
            let manifest = parse_skill_markdown_manifest(&skill_md_content).ok_or_else(|| {
                anyhow!(
                    "Invalid SKILL.md frontmatter in {} (expected OpenClaw style metadata block)",
                    path.display()
                )
            })?;
            let id = build_stable_skill_id(&manifest.name, path.as_path());
            return Ok(LoadedSkill {
                skill: Skill {
                    id,
                    name: manifest.name,
                    version: manifest.version,
                    description: manifest.description,
                    author: manifest.author,
                    enabled: true,
                    installed_at: Utc::now(),
                    script_type: SkillType::Markdown,
                },
                entry_point: None,
            });
        }

        Err(anyhow!(
            "skill.json or SKILL.md not found in {}",
            path.display()
        ))
    }

    pub fn list_skills(&self) -> Vec<Skill> {
        self.skills.values().cloned().collect()
    }

    pub fn get_skill(&self, id: &str) -> Option<&Skill> {
        self.skills.get(id)
    }

    pub async fn install_skill(&mut self, repo_url: &str) -> Result<Skill> {
        self.install_skill_with_path(repo_url, None).await
    }

    pub async fn install_skill_with_path(
        &mut self,
        repo_url: &str,
        skill_path: Option<&str>,
    ) -> Result<Skill> {
        let temp_dir =
            std::env::temp_dir().join(format!("petool-skill-install-{}", Uuid::new_v4()));
        let temp_dir_str = temp_dir
            .to_str()
            .ok_or_else(|| anyhow!("Invalid temp directory path"))?;

        let status = Command::new("git")
            .args(["clone", "--depth", "1", repo_url, temp_dir_str])
            .status()?;
        if !status.success() {
            let _ = fs::remove_dir_all(&temp_dir);
            return Err(anyhow!("Failed to clone repository"));
        }

        let selected_dir = if let Some(raw_path) = skill_path {
            let trimmed = raw_path.trim().trim_matches('/').trim_matches('\\');
            if trimmed.is_empty() || trimmed == "." {
                temp_dir.clone()
            } else {
                temp_dir.join(trimmed)
            }
        } else {
            temp_dir.clone()
        };

        if !selected_dir.exists() || !selected_dir.is_dir() {
            let _ = fs::remove_dir_all(&temp_dir);
            return Err(anyhow!(
                "Skill path '{}' does not exist in repository",
                skill_path.unwrap_or(".")
            ));
        }
        let canonical_temp = temp_dir.canonicalize()?;
        let canonical_selected = selected_dir.canonicalize()?;
        if !canonical_selected.starts_with(&canonical_temp) {
            let _ = fs::remove_dir_all(&temp_dir);
            return Err(anyhow!("Skill path is outside repository root"));
        }

        let loaded = match self.load_skill_from_dir(&selected_dir).await {
            Ok(skill) => skill,
            Err(error) => {
                let _ = fs::remove_dir_all(&temp_dir);
                return Err(error);
            }
        };
        let dir_name = sanitize_skill_dir_name(&loaded.skill.name);
        let install_dir_name = if dir_name.is_empty() {
            format!("skill-{}", Uuid::new_v4())
        } else {
            dir_name
        };
        let destination = self.skills_dir.join(&install_dir_name);
        if destination.exists() {
            let _ = fs::remove_dir_all(&temp_dir);
            return Err(anyhow!(
                "Skill directory already exists: {}",
                destination.display()
            ));
        }

        fs::create_dir_all(&destination)?;
        if let Err(error) = copy_directory_without_git(&selected_dir, &destination) {
            let _ = fs::remove_dir_all(&destination);
            let _ = fs::remove_dir_all(&temp_dir);
            return Err(error);
        }

        let _ = fs::remove_dir_all(&temp_dir);

        let installed_loaded = match self.load_skill_from_dir(&destination).await {
            Ok(skill) => skill,
            Err(error) => {
                let _ = fs::remove_dir_all(&destination);
                return Err(error);
            }
        };
        self.skill_paths
            .insert(installed_loaded.skill.id.clone(), destination.clone());
        if let Some(entry_point) = installed_loaded.entry_point.as_ref() {
            self.skill_entry_points
                .insert(installed_loaded.skill.id.clone(), entry_point.clone());
        }
        self.skills.insert(
            installed_loaded.skill.id.clone(),
            installed_loaded.skill.clone(),
        );
        Ok(installed_loaded.skill)
    }

    pub async fn discover_skills(
        &self,
        query: Option<&str>,
        limit: usize,
        skillsmp_api_key: Option<&str>,
        skillsmp_api_base: Option<&str>,
    ) -> Result<Vec<SkillDiscoveryItem>> {
        let search_limit = limit.clamp(1, 20);
        let query_text = query
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("automation workflow");
        let installed_names: HashSet<String> = self
            .skills
            .values()
            .map(|skill| skill.name.to_lowercase())
            .collect();
        let client = reqwest::Client::builder()
            .user_agent("petool/0.1")
            .timeout(std::time::Duration::from_secs(12))
            .build()?;

        if let Some(api_key) = skillsmp_api_key
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            let api_base = skillsmp_api_base
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("https://skillsmp.com/api/v1");
            if let Ok(results) = self
                .discover_skills_from_skillsmp(
                    &client,
                    query_text,
                    search_limit,
                    api_key,
                    api_base,
                    &installed_names,
                )
                .await
            {
                if !results.is_empty() {
                    return Ok(results);
                }
            }
        }

        self.discover_skills_from_github(&client, query_text, search_limit, &installed_names)
            .await
    }

    async fn discover_skills_from_skillsmp(
        &self,
        client: &reqwest::Client,
        query_text: &str,
        limit: usize,
        api_key: &str,
        api_base: &str,
        installed_names: &HashSet<String>,
    ) -> Result<Vec<SkillDiscoveryItem>> {
        let base = api_base.trim_end_matches('/');
        let endpoints = [
            format!("{}/skills/search", base),
            format!("{}/skills", base),
        ];
        let mut last_error = String::new();

        for endpoint in endpoints {
            let response = client
                .get(&endpoint)
                .query(&[
                    ("q", query_text),
                    ("query", query_text),
                    ("limit", &limit.to_string()),
                ])
                .bearer_auth(api_key)
                .send()
                .await;
            let response = match response {
                Ok(value) => value,
                Err(error) => {
                    last_error = error.to_string();
                    continue;
                }
            };
            if !response.status().is_success() {
                last_error = format!("status {}", response.status());
                continue;
            }

            let payload = response.json::<Value>().await?;
            let candidate_items = payload
                .as_array()
                .cloned()
                .or_else(|| payload.get("items").and_then(Value::as_array).cloned())
                .or_else(|| payload.get("results").and_then(Value::as_array).cloned())
                .or_else(|| payload.get("skills").and_then(Value::as_array).cloned())
                .or_else(|| payload.get("data").and_then(Value::as_array).cloned())
                .or_else(|| {
                    payload
                        .get("data")
                        .and_then(|data| data.get("skills"))
                        .and_then(Value::as_array)
                        .cloned()
                })
                .or_else(|| {
                    payload
                        .get("data")
                        .and_then(|data| data.get("items"))
                        .and_then(Value::as_array)
                        .cloned()
                })
                .unwrap_or_default();

            if candidate_items.is_empty() {
                continue;
            }

            let mut seen = HashSet::<String>::new();
            let mut results = Vec::<SkillDiscoveryItem>::new();
            for item in candidate_items {
                let name = read_string_at_path(&item, &["name"])
                    .or_else(|| read_string_at_path(&item, &["title"]))
                    .or_else(|| read_string_at_path(&item, &["skill_name"]))
                    .or_else(|| read_string_at_path(&item, &["slug"]))
                    .unwrap_or("unknown-skill")
                    .to_string();
                let description = read_string_at_path(&item, &["description"])
                    .or_else(|| read_string_at_path(&item, &["summary"]))
                    .or_else(|| read_string_at_path(&item, &["short_description"]))
                    .unwrap_or("")
                    .to_string();
                let repo_url = read_string_at_path(&item, &["repo_url"])
                    .or_else(|| read_string_at_path(&item, &["repository_url"]))
                    .or_else(|| read_string_at_path(&item, &["github_url"]))
                    .or_else(|| read_string_at_path(&item, &["githubUrl"]))
                    .or_else(|| read_string_at_path(&item, &["git_url"]))
                    .or_else(|| read_string_at_path(&item, &["gitUrl"]))
                    .or_else(|| read_string_at_path(&item, &["repository", "clone_url"]))
                    .or_else(|| read_string_at_path(&item, &["repository", "cloneUrl"]))
                    .unwrap_or("")
                    .to_string();
                if repo_url.trim().is_empty() {
                    continue;
                }
                let skill_path = normalize_optional_skill_path(
                    read_string_at_path(&item, &["skill_path"])
                        .or_else(|| read_string_at_path(&item, &["skillPath"]))
                        .or_else(|| read_string_at_path(&item, &["path"]))
                        .or_else(|| read_string_at_path(&item, &["subpath"]))
                        .map(str::to_string),
                );
                let repo_full_name = read_string_at_path(&item, &["repo_full_name"])
                    .or_else(|| read_string_at_path(&item, &["repository", "full_name"]))
                    .map(str::to_string)
                    .or_else(|| parse_github_full_name_from_repo_url(&repo_url))
                    .unwrap_or_else(|| repo_url.trim().to_string());
                let repo_html_url = read_string_at_path(&item, &["repo_html_url"])
                    .or_else(|| read_string_at_path(&item, &["html_url"]))
                    .or_else(|| read_string_at_path(&item, &["url"]))
                    .or_else(|| read_string_at_path(&item, &["githubUrl"]))
                    .or_else(|| read_string_at_path(&item, &["skillUrl"]))
                    .or_else(|| read_string_at_path(&item, &["repository", "html_url"]))
                    .or_else(|| read_string_at_path(&item, &["repository", "htmlUrl"]))
                    .map(str::to_string)
                    .unwrap_or_else(|| repo_url.trim_end_matches(".git").to_string());
                let id = read_string_or_number_at_path(&item, &["id"])
                    .or_else(|| read_string_at_path(&item, &["slug"]).map(str::to_string))
                    .unwrap_or_else(|| {
                        format!(
                            "{}:{}",
                            repo_full_name,
                            skill_path.clone().unwrap_or_else(|| ".".to_string())
                        )
                    });
                if !seen.insert(id.clone()) {
                    continue;
                }
                let updated_at = read_string_at_path(&item, &["updated_at"])
                    .map(str::to_string)
                    .or_else(|| read_string_or_number_at_path(&item, &["updatedAt"]))
                    .or_else(|| {
                        read_string_at_path(&item, &["repository", "updated_at"])
                            .map(str::to_string)
                    })
                    .or_else(|| read_string_or_number_at_path(&item, &["repository", "updatedAt"]));

                results.push(SkillDiscoveryItem {
                    id,
                    name: name.clone(),
                    description,
                    repo_url,
                    repo_full_name,
                    repo_html_url,
                    source: "skillsmp_api".to_string(),
                    skill_path,
                    stars: read_u64_at_path(&item, &["stars"])
                        .or_else(|| read_u64_at_path(&item, &["stargazers_count"]))
                        .or_else(|| read_u64_at_path(&item, &["repository", "stargazers_count"]))
                        .unwrap_or(0),
                    updated_at,
                    installed: installed_names.contains(&name.to_lowercase()),
                });
            }

            results.sort_by(|left, right| {
                right
                    .stars
                    .cmp(&left.stars)
                    .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
            });
            if results.len() > limit {
                results.truncate(limit);
            }
            return Ok(results);
        }

        if last_error.is_empty() {
            return Err(anyhow!("SkillsMP discovery failed"));
        }
        Err(anyhow!("SkillsMP discovery failed: {}", last_error))
    }

    async fn discover_skills_from_github(
        &self,
        client: &reqwest::Client,
        query_text: &str,
        limit: usize,
        installed_names: &HashSet<String>,
    ) -> Result<Vec<SkillDiscoveryItem>> {
        let mut combined: Vec<SkillDiscoveryItem> = Vec::new();
        let mut seen = HashSet::<String>::new();
        let mut errors: Vec<String> = Vec::new();

        for kind in [
            GithubManifestKind::SkillMarkdown,
            GithubManifestKind::SkillJson,
        ] {
            match self
                .discover_skills_from_github_manifest(
                    client,
                    query_text,
                    limit,
                    installed_names,
                    kind,
                )
                .await
            {
                Ok(items) => {
                    for item in items {
                        if seen.insert(item.id.clone()) {
                            combined.push(item);
                        }
                    }
                }
                Err(error) => errors.push(error.to_string()),
            }
        }

        combined.sort_by(|left, right| {
            right
                .stars
                .cmp(&left.stars)
                .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
        });
        if combined.len() > limit {
            combined.truncate(limit);
        }
        if !combined.is_empty() {
            return Ok(combined);
        }
        if errors.is_empty() {
            return Ok(Vec::new());
        }
        Err(anyhow!(
            "Failed to discover skills from GitHub: {}",
            errors.join(" | ")
        ))
    }

    async fn discover_skills_from_github_manifest(
        &self,
        client: &reqwest::Client,
        query_text: &str,
        limit: usize,
        installed_names: &HashSet<String>,
        kind: GithubManifestKind,
    ) -> Result<Vec<SkillDiscoveryItem>> {
        let manifest_name = match kind {
            GithubManifestKind::SkillMarkdown => "SKILL.md",
            GithubManifestKind::SkillJson => "skill.json",
        };
        let source = match kind {
            GithubManifestKind::SkillMarkdown => "github_code_search_skillmd",
            GithubManifestKind::SkillJson => "github_code_search_skilljson",
        };
        let search_query = format!("filename:{} {}", manifest_name, query_text);
        let response = client
            .get("https://api.github.com/search/code")
            .query(&[
                ("q", search_query.as_str()),
                ("per_page", &limit.to_string()),
            ])
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!(
                "Failed to discover {} from GitHub (status {}): {}",
                manifest_name,
                status,
                body.chars().take(240).collect::<String>()
            ));
        }

        let payload = response.json::<GithubCodeSearchResponse>().await?;
        let mut seen = HashSet::<String>::new();
        let mut results = Vec::<SkillDiscoveryItem>::new();

        for item in payload.items {
            let skill_dir = Path::new(&item.path)
                .parent()
                .map(|value| value.to_string_lossy().to_string())
                .unwrap_or_else(|| ".".to_string());
            let skill_path = normalize_optional_skill_path(Some(skill_dir));
            let repo_full_name = item.repository.full_name.clone();
            let result_id = format!(
                "{}:{}",
                repo_full_name,
                skill_path.clone().unwrap_or_else(|| ".".to_string())
            );
            if !seen.insert(result_id.clone()) {
                continue;
            }

            let mut name = Path::new(
                skill_path
                    .as_deref()
                    .unwrap_or(item.repository.name.as_str()),
            )
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or(item.repository.name.as_str())
            .to_string();
            let mut description = item.repository.description.clone().unwrap_or_default();

            let branch = item
                .repository
                .default_branch
                .clone()
                .unwrap_or_else(|| "main".to_string());
            let raw_manifest_url = format!(
                "https://raw.githubusercontent.com/{}/{}/{}",
                repo_full_name, branch, item.path
            );
            if let Ok(manifest_response) = client.get(&raw_manifest_url).send().await {
                if manifest_response.status().is_success() {
                    match kind {
                        GithubManifestKind::SkillJson => {
                            if let Ok(manifest) =
                                manifest_response.json::<RemoteSkillManifest>().await
                            {
                                if let Some(remote_name) = manifest.name {
                                    if !remote_name.trim().is_empty() {
                                        name = remote_name.trim().to_string();
                                    }
                                }
                                if let Some(remote_description) = manifest.description {
                                    if !remote_description.trim().is_empty() {
                                        description = remote_description.trim().to_string();
                                    }
                                }
                            }
                        }
                        GithubManifestKind::SkillMarkdown => {
                            if let Ok(raw_markdown) = manifest_response.text().await {
                                if let Some(manifest) = parse_skill_markdown_manifest(&raw_markdown)
                                {
                                    name = manifest.name;
                                    if !manifest.description.trim().is_empty() {
                                        description = manifest.description;
                                    }
                                } else {
                                    continue;
                                }
                            }
                        }
                    }
                }
            }

            results.push(SkillDiscoveryItem {
                id: result_id,
                name: name.clone(),
                description,
                repo_url: format!("https://github.com/{}.git", repo_full_name),
                repo_full_name: repo_full_name.clone(),
                repo_html_url: item.repository.html_url.clone(),
                source: source.to_string(),
                skill_path,
                stars: item.repository.stargazers_count.unwrap_or(0),
                updated_at: item.repository.updated_at.clone(),
                installed: installed_names.contains(&name.to_lowercase()),
            });
        }

        Ok(results)
    }

    pub async fn uninstall_skill(&mut self, id: &str) -> Result<()> {
        let _skill = self
            .get_skill(id)
            .ok_or_else(|| anyhow!("Skill not found: {}", id))?;
        let path = self
            .skill_paths
            .get(id)
            .cloned()
            .ok_or_else(|| anyhow!("Skill path not found: {}", id))?;

        if path.exists() {
            fs::remove_dir_all(&path)?;
        }

        self.skill_paths.remove(id);
        self.skill_entry_points.remove(id);
        self.skills.remove(id);
        Ok(())
    }

    pub async fn execute_skill(&self, id: &str, params: HashMap<String, Value>) -> Result<Value> {
        let skill = self
            .get_skill(id)
            .ok_or_else(|| anyhow!("Skill not found: {}", id))?;

        if !skill.enabled {
            return Err(anyhow!("Skill is disabled: {}", id));
        }

        let skill_path = self
            .skill_paths
            .get(id)
            .cloned()
            .ok_or_else(|| anyhow!("Skill path not found: {}", id))?;

        match skill.script_type {
            SkillType::JavaScript => {
                let entry_point = self
                    .skill_entry_points
                    .get(id)
                    .cloned()
                    .unwrap_or_else(|| "index.js".to_string());
                self.execute_javascript_skill(&skill_path, &entry_point, params)
                    .await
            }
            SkillType::Rust => self.execute_rust_skill(&skill_path, params).await,
            SkillType::Markdown => self.execute_markdown_skill(&skill_path, params).await,
        }
    }

    async fn execute_javascript_skill(
        &self,
        skill_path: &PathBuf,
        entry_point: &str,
        params: HashMap<String, Value>,
    ) -> Result<Value> {
        let index_path = skill_path.join(entry_point);
        if !index_path.exists() {
            return Err(anyhow!(
                "Skill entry point not found: {}",
                index_path.display()
            ));
        }
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
        let result_json: Value = serde_json::from_str(&result)?;
        Ok(result_json)
    }

    async fn execute_rust_skill(
        &self,
        skill_path: &PathBuf,
        params: HashMap<String, Value>,
    ) -> Result<Value> {
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
        let output = Command::new(&exe_path).arg(&params_json).output()?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("Skill execution failed: {}", error));
        }

        let result = String::from_utf8_lossy(&output.stdout);
        let result_json: Value = serde_json::from_str(&result)?;
        Ok(result_json)
    }

    async fn execute_markdown_skill(
        &self,
        skill_path: &PathBuf,
        params: HashMap<String, Value>,
    ) -> Result<Value> {
        let skill_md_path = skill_path.join("SKILL.md");
        if !skill_md_path.exists() {
            return Err(anyhow!("SKILL.md not found: {}", skill_md_path.display()));
        }

        let content = fs::read_to_string(&skill_md_path)?;
        let manifest = parse_skill_markdown_manifest(&content).ok_or_else(|| {
            anyhow!(
                "Invalid SKILL.md frontmatter in {}",
                skill_md_path.display()
            )
        })?;
        let instructions = manifest.body.trim();
        if instructions.is_empty() {
            return Err(anyhow!(
                "SKILL.md has no instruction body: {}",
                skill_md_path.display()
            ));
        }

        Ok(json!({
            "mode": "markdown_skill",
            "skill_name": manifest.name,
            "description": manifest.description,
            "base_dir": skill_path.to_string_lossy().to_string(),
            "skill_file": skill_md_path.to_string_lossy().to_string(),
            "instructions": instructions,
            "params": params
        }))
    }

    pub async fn update_skill(&mut self, _id: &str) -> Result<Skill> {
        // Uninstall and reinstall to update
        // In a real implementation, you'd store the repo URL
        Err(anyhow!(
            "Update not implemented - requires storing repo URL"
        ))
    }

    pub fn set_skill_enabled(&mut self, id: &str, enabled: bool) -> Result<()> {
        let skill = self
            .skills
            .get_mut(id)
            .ok_or_else(|| anyhow!("Skill not found: {}", id))?;
        skill.enabled = enabled;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::parse_skill_markdown_manifest;
    use super::{SkillManager, SkillType};
    use serde_json::Value;
    use std::collections::HashMap;
    use std::fs;
    use uuid::Uuid;

    #[test]
    fn parses_openclaw_style_skill_markdown() {
        let content = r#"---
name: local-places
description: Find local restaurants and cafes
author: OpenClaw
---

# Local Places

Use this skill to find nearby places.
"#;
        let parsed = parse_skill_markdown_manifest(content).expect("should parse SKILL.md");
        assert_eq!(parsed.name, "local-places");
        assert_eq!(parsed.description, "Find local restaurants and cafes");
        assert_eq!(parsed.author, "OpenClaw");
        assert!(parsed.body.contains("Use this skill"));
    }

    #[test]
    fn rejects_missing_frontmatter() {
        let content = "# Not a skill file\njust text";
        assert!(parse_skill_markdown_manifest(content).is_none());
    }

    #[tokio::test]
    async fn loads_and_executes_markdown_skill() {
        let root = std::env::temp_dir().join(format!("petool-skill-md-test-{}", Uuid::new_v4()));
        let skill_dir = root.join("weather");
        fs::create_dir_all(&skill_dir).expect("create temp skill dir");
        fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: weather
description: Get weather info
---

# Weather

Use weather APIs and summarize results.
"#,
        )
        .expect("write SKILL.md");

        let mut manager = SkillManager::new(root.clone()).expect("create manager");
        manager.load_skills().await.expect("load skills");
        let skills = manager.list_skills();
        assert_eq!(skills.len(), 1);
        assert!(matches!(skills[0].script_type, SkillType::Markdown));

        let result = manager
            .execute_skill(&skills[0].id, HashMap::<String, Value>::new())
            .await
            .expect("execute markdown skill");
        assert_eq!(
            result.get("mode").and_then(Value::as_str),
            Some("markdown_skill")
        );
        assert!(
            result
                .get("instructions")
                .and_then(Value::as_str)
                .unwrap_or("")
                .contains("weather APIs")
        );

        let _ = fs::remove_dir_all(&root);
    }
}
