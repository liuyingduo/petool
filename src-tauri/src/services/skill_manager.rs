use crate::models::{config::Config, skill::*};
use crate::services::node_runtime;
use crate::utils::{
    load_config, resolve_effective_downloads_dir, resolve_skill_download_cache_dir,
};
use anyhow::{anyhow, Result};
use chrono::Utc;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
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

#[derive(Debug, Clone, Copy)]
enum ArchiveKind {
    Zip,
    TarGz,
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

fn summarize_error_text(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return "unknown error".to_string();
    }
    let single_line = trimmed
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(3)
        .collect::<Vec<_>>()
        .join(" | ");
    if single_line.len() > 240 {
        format!("{}...", &single_line[..237])
    } else {
        single_line
    }
}

fn detect_archive_kind_from_url(url: &str) -> Option<ArchiveKind> {
    let path = reqwest::Url::parse(url)
        .ok()
        .map(|parsed| parsed.path().to_string())
        .unwrap_or_else(|| {
            url.split('?')
                .next()
                .map(str::to_string)
                .unwrap_or_default()
        });
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".zip") {
        return Some(ArchiveKind::Zip);
    }
    if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        return Some(ArchiveKind::TarGz);
    }
    None
}

fn detect_archive_kind_from_content_type(content_type: &str) -> Option<ArchiveKind> {
    let normalized = content_type.trim().to_ascii_lowercase();
    if normalized.contains("application/zip") || normalized.contains("application/x-zip") {
        return Some(ArchiveKind::Zip);
    }
    if normalized.contains("application/gzip")
        || normalized.contains("application/x-gzip")
        || normalized.contains("application/x-tar")
    {
        return Some(ArchiveKind::TarGz);
    }
    None
}

fn normalize_clawhub_base(input: Option<&str>) -> String {
    let raw = input
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("https://clawhub.ai");
    let trimmed = raw.trim_end_matches('/');

    if let Ok(parsed) = reqwest::Url::parse(trimmed) {
        let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
        let mut base = format!("{}://{}", parsed.scheme(), host);
        if let Some(port) = parsed.port() {
            base.push(':');
            base.push_str(&port.to_string());
        }
        if let Some(path_base) = parsed.path().strip_suffix("/api/v1") {
            let path = path_base.trim_end_matches('/');
            if !path.is_empty() && path != "/" {
                base.push_str(path);
            }
            return base;
        }
        let path = parsed.path().trim_end_matches('/');
        if !path.is_empty() && path != "/" {
            base.push_str(path);
        }
        return base;
    }

    if let Some(base) = trimmed.strip_suffix("/api/v1") {
        return base.trim_end_matches('/').to_string();
    }
    trimmed.to_string()
}

fn build_clawhub_api_url(base: &str, path: &str, query: &[(&str, &str)]) -> Result<String> {
    let origin = normalize_clawhub_base(Some(base));
    let mut url = reqwest::Url::parse(&format!(
        "{}/api/v1/{}",
        origin,
        path.trim_start_matches('/')
    ))
    .map_err(|error| anyhow!("Invalid ClawHub base URL '{}': {}", base, error))?;
    for (key, value) in query {
        url.query_pairs_mut().append_pair(key, value);
    }
    Ok(url.to_string())
}

fn parse_clawhub_slug_from_url(raw: &str) -> Option<String> {
    let parsed = reqwest::Url::parse(raw).ok()?;
    let segments: Vec<String> = parsed
        .path_segments()
        .map(|iter| {
            iter.filter(|segment| !segment.trim().is_empty())
                .map(|segment| segment.trim().to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if segments.is_empty() {
        return None;
    }

    if let Some(slug) = parsed.query_pairs().find_map(|(key, value)| {
        if key == "slug" {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        } else {
            None
        }
    }) {
        return Some(slug);
    }

    if segments.first().map(|value| value.as_str()) == Some("skills") && segments.len() >= 2 {
        return segments.get(1).cloned();
    }
    if segments.len() == 1 && segments.first().map(|value| value.as_str()) == Some("skills") {
        return None;
    }
    if segments.first().map(|value| value.as_str()) == Some("api")
        && segments.get(1).map(|value| value.as_str()) == Some("v1")
        && segments.get(2).map(|value| value.as_str()) == Some("skills")
        && segments.len() >= 4
    {
        return segments.get(3).cloned();
    }
    if segments.len() >= 2 {
        return segments.get(1).cloned();
    }
    segments.first().cloned()
}

fn resolve_clawhub_install_source_url(input: &str, api_base: Option<&str>) -> Result<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("Skill source is empty"));
    }

    if let Ok(parsed_url) = reqwest::Url::parse(trimmed) {
        let host = parsed_url
            .host_str()
            .unwrap_or_default()
            .to_ascii_lowercase();
        if parsed_url.path().contains("/api/v1/download") {
            return Ok(parsed_url.to_string());
        }
        if host.contains("clawhub.ai") || host.contains("clawhub.com") {
            if let Some(slug) = parse_clawhub_slug_from_url(trimmed) {
                let mut query_pairs: Vec<(&str, String)> = vec![("slug", slug)];
                if let Some(version) = parsed_url.query_pairs().find_map(|(key, value)| {
                    if key == "version" {
                        let text = value.trim();
                        if text.is_empty() {
                            None
                        } else {
                            Some(text.to_string())
                        }
                    } else {
                        None
                    }
                }) {
                    query_pairs.push(("version", version));
                }
                let owned_query: Vec<(&str, &str)> = query_pairs
                    .iter()
                    .map(|(key, value)| (*key, value.as_str()))
                    .collect();
                return build_clawhub_api_url(
                    parsed_url.origin().ascii_serialization().as_str(),
                    "/download",
                    &owned_query,
                );
            }
            return Err(anyhow!(
                "ClawHub URL does not point to a skill. Please provide a skill slug or download URL."
            ));
        }
        return Ok(trimmed.to_string());
    }

    let slug = trimmed.trim_matches('/').to_string();
    if slug.is_empty() {
        return Err(anyhow!("Skill slug is empty"));
    }
    build_clawhub_api_url(
        &normalize_clawhub_base(api_base),
        "/download",
        &[("slug", &slug)],
    )
}

fn extract_zip_archive(archive_path: &Path, destination: &Path) -> Result<()> {
    let file = fs::File::open(archive_path)?;
    let mut zip = zip::ZipArchive::new(file)?;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index)?;
        let Some(safe_path) = entry.enclosed_name().map(|value| value.to_owned()) else {
            continue;
        };
        let output_path = destination.join(safe_path);
        if entry.is_dir() {
            fs::create_dir_all(&output_path)?;
            continue;
        }
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut out_file = fs::File::create(&output_path)?;
        io::copy(&mut entry, &mut out_file)?;
    }
    Ok(())
}

fn extract_tar_gz_archive(archive_path: &Path, destination: &Path) -> Result<()> {
    let file = fs::File::open(archive_path)?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    archive.unpack(destination)?;
    Ok(())
}

fn resolve_extracted_source_root(extracted_dir: &Path) -> Result<PathBuf> {
    let mut child_dirs: Vec<PathBuf> = Vec::new();
    let mut file_count = 0usize;
    for entry in fs::read_dir(extracted_dir)? {
        let entry = entry?;
        let entry_path = entry.path();
        if entry_path.is_dir() {
            child_dirs.push(entry_path);
        } else {
            file_count += 1;
        }
    }
    if file_count == 0 && child_dirs.len() == 1 {
        return Ok(child_dirs.remove(0));
    }
    Ok(extracted_dir.to_path_buf())
}

async fn download_archive_to_file(
    client: &reqwest::Client,
    url: &str,
    archive_file: &Path,
) -> Result<Option<String>> {
    let response = client.get(url).send().await?;
    if !response.status().is_success() {
        return Err(anyhow!(
            "download failed (status {}): {}",
            response.status(),
            url
        ));
    }
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let bytes = response.bytes().await?;
    fs::write(archive_file, bytes)?;
    Ok(content_type)
}

async fn try_extract_source_from_archive(source_url: &str, temp_dir: &Path) -> Result<PathBuf> {
    let client = reqwest::Client::builder()
        .user_agent("petool/0.1")
        .timeout(std::time::Duration::from_secs(30))
        .build()?;
    let extracted_dir = temp_dir.join("source");
    fs::create_dir_all(&extracted_dir)?;

    let archive_file = temp_dir.join("skill-archive");
    let content_type = download_archive_to_file(&client, source_url, &archive_file).await?;
    let archive_kind = detect_archive_kind_from_url(source_url)
        .or_else(|| {
            content_type
                .as_deref()
                .and_then(detect_archive_kind_from_content_type)
        })
        .ok_or_else(|| {
            anyhow!(
                "unsupported skill package type (expected zip/tar.gz), url: {}",
                source_url
            )
        })?;
    match archive_kind {
        ArchiveKind::Zip => extract_zip_archive(&archive_file, &extracted_dir)?,
        ArchiveKind::TarGz => extract_tar_gz_archive(&archive_file, &extracted_dir)?,
    }
    resolve_extracted_source_root(&extracted_dir)
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

    pub fn set_skills_dir(&mut self, skills_dir: PathBuf) -> Result<()> {
        fs::create_dir_all(&skills_dir)?;
        self.skills_dir = skills_dir;
        Ok(())
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
        let source_url = resolve_clawhub_install_source_url(repo_url, None)?;
        let config = load_config::<Config>().unwrap_or_default();
        let downloads_dir = resolve_effective_downloads_dir(config.downloads_directory.as_deref());
        let cache_root = resolve_skill_download_cache_dir(&downloads_dir);
        fs::create_dir_all(&cache_root)?;
        let temp_dir = cache_root.join(format!("petool-skill-install-{}", Uuid::new_v4()));
        let repo_root = match try_extract_source_from_archive(&source_url, &temp_dir).await {
            Ok(path) => path,
            Err(download_error) => {
                let _ = fs::remove_dir_all(&temp_dir);
                return Err(anyhow!(
                    "Failed to install skill package: {}",
                    summarize_error_text(&download_error.to_string())
                ));
            }
        };

        let selected_dir = if let Some(raw_path) = skill_path {
            let trimmed = raw_path.trim().trim_matches('/').trim_matches('\\');
            if trimmed.is_empty() || trimmed == "." {
                repo_root.clone()
            } else {
                repo_root.join(trimmed)
            }
        } else {
            repo_root.clone()
        };

        if !selected_dir.exists() || !selected_dir.is_dir() {
            let _ = fs::remove_dir_all(&temp_dir);
            return Err(anyhow!(
                "Skill path '{}' does not exist in downloaded package",
                skill_path.unwrap_or(".")
            ));
        }
        let canonical_temp = repo_root.canonicalize()?;
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
        clawhub_api_key: Option<&str>,
        clawhub_api_base: Option<&str>,
    ) -> Result<Vec<SkillDiscoveryItem>> {
        let search_limit = limit.clamp(1, 20);
        let query_text = query.map(str::trim).filter(|value| !value.is_empty());
        let installed_names: HashSet<String> = self
            .skills
            .values()
            .map(|skill| skill.name.to_lowercase())
            .collect();
        let client = reqwest::Client::builder()
            .user_agent("petool/0.1")
            .timeout(std::time::Duration::from_secs(12))
            .build()?;

        let api_key = clawhub_api_key
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let api_base = normalize_clawhub_base(clawhub_api_base);

        self.discover_skills_from_clawhub(
            &client,
            query_text,
            search_limit,
            api_key,
            &api_base,
            &installed_names,
        )
        .await
    }

    async fn discover_skills_from_clawhub(
        &self,
        client: &reqwest::Client,
        query_text: Option<&str>,
        limit: usize,
        api_key: Option<&str>,
        api_base: &str,
        installed_names: &HashSet<String>,
    ) -> Result<Vec<SkillDiscoveryItem>> {
        let endpoint = if query_text.is_some() {
            build_clawhub_api_url(
                api_base,
                "/search",
                &[
                    ("q", query_text.unwrap_or_default()),
                    ("limit", &limit.to_string()),
                ],
            )?
        } else {
            build_clawhub_api_url(
                api_base,
                "/skills",
                &[("limit", &limit.to_string()), ("sort", "downloads")],
            )?
        };

        let mut request = client.get(&endpoint);
        if let Some(token) = api_key {
            request = request.bearer_auth(token);
        }
        let response = request.send().await?;
        if !response.status().is_success() {
            return Err(anyhow!(
                "ClawHub discovery failed (status {}): {}",
                response.status(),
                endpoint
            ));
        }

        let payload = response.json::<Value>().await?;
        let candidate_items = if query_text.is_some() {
            payload
                .get("results")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
        } else {
            payload
                .get("items")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
        };

        let mut seen = HashSet::<String>::new();
        let mut results = Vec::<SkillDiscoveryItem>::new();
        for item in candidate_items {
            let slug = read_string_at_path(&item, &["slug"])
                .or_else(|| read_string_at_path(&item, &["name"]))
                .or_else(|| read_string_at_path(&item, &["id"]))
                .unwrap_or("")
                .trim()
                .to_string();
            if slug.is_empty() {
                continue;
            }
            let name = read_string_at_path(&item, &["displayName"])
                .or_else(|| read_string_at_path(&item, &["display_name"]))
                .or_else(|| read_string_at_path(&item, &["name"]))
                .unwrap_or(slug.as_str())
                .to_string();
            let description = read_string_at_path(&item, &["summary"])
                .or_else(|| read_string_at_path(&item, &["description"]))
                .unwrap_or("")
                .to_string();

            let version = read_string_at_path(&item, &["version"])
                .or_else(|| read_string_at_path(&item, &["latestVersion", "version"]))
                .or_else(|| read_string_at_path(&item, &["latest_version", "version"]))
                .map(str::to_string);
            let mut query_pairs: Vec<(&str, String)> = vec![("slug", slug.clone())];
            if let Some(version) = version.as_ref().filter(|value| !value.trim().is_empty()) {
                query_pairs.push(("version", version.clone()));
            }
            let owned_query: Vec<(&str, &str)> = query_pairs
                .iter()
                .map(|(key, value)| (*key, value.as_str()))
                .collect();
            let install_source_url = build_clawhub_api_url(api_base, "/download", &owned_query)?;
            let html_url = format!(
                "{}/skills?q={}",
                normalize_clawhub_base(Some(api_base)),
                slug
            );
            let id = slug.clone();
            if !seen.insert(id.clone()) {
                continue;
            }
            let updated_at = read_string_or_number_at_path(&item, &["updatedAt"])
                .or_else(|| read_string_or_number_at_path(&item, &["updated_at"]))
                .or_else(|| read_string_or_number_at_path(&item, &["latestVersion", "createdAt"]));
            let stars = read_u64_at_path(&item, &["stats", "stars"])
                .or_else(|| read_u64_at_path(&item, &["stars"]))
                .unwrap_or(0);

            results.push(SkillDiscoveryItem {
                id,
                name: name.clone(),
                description,
                repo_url: install_source_url,
                repo_full_name: slug.clone(),
                repo_html_url: html_url,
                source: "clawhub_api".to_string(),
                skill_path: None,
                stars,
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

        let runtime = node_runtime::ensure_node_runtime()
            .await
            .map_err(|error| anyhow!("Node.js runtime unavailable: {}", error))?;

        let mut command = Command::new(&runtime.node_command);
        runtime.apply_to_command(&mut command);
        let output = command.arg(&index_path).arg(&params_json).output()?;

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
        assert!(result
            .get("instructions")
            .and_then(Value::as_str)
            .unwrap_or("")
            .contains("weather APIs"));

        let _ = fs::remove_dir_all(&root);
    }
}
