use crate::commands::skills::SkillManagerState;
use crate::models::config::Config;
use crate::services::browser::paths::{browser_profile_user_data_dir, sanitize_profile_name};
use crate::utils::{
    ensure_writable_directory, get_app_config_dir, load_config, resolve_effective_downloads_dir,
    resolve_node_download_cache_dir, resolve_node_runtime_root, resolve_skill_download_cache_dir,
    resolve_skills_dir, resolve_default_downloads_dir, save_config,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use uuid::Uuid;
use walkdir::WalkDir;

#[derive(Debug, Clone, Deserialize)]
pub struct FeedbackDraftInput {
    pub category: String,
    pub detail: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub attachments: Vec<String>,
    #[serde(default)]
    pub client_meta: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct FeedbackDraftSaved {
    pub draft_id: String,
    pub saved_json_path: String,
    pub saved_attachments: Vec<String>,
    pub created_at: String,
}

fn copy_directory(source: &Path, destination: &Path) -> Result<(), String> {
    for entry in WalkDir::new(source) {
        let entry = entry.map_err(|e| e.to_string())?;
        let entry_path = entry.path();
        let relative = entry_path.strip_prefix(source).map_err(|e| e.to_string())?;
        let target = destination.join(relative);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&target).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            fs::copy(entry_path, &target).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn migrate_directory(src: &Path, dst: &Path) -> Result<(), String> {
    if !src.exists() || src == dst {
        return Ok(());
    }

    if !dst.exists() {
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        if fs::rename(src, dst).is_ok() {
            return Ok(());
        }
        copy_directory(src, dst)?;
        fs::remove_dir_all(src).map_err(|e| e.to_string())?;
        return Ok(());
    }

    let file_name = src
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("migrating-dir");
    let backup_name = format!("{}-migrated-{}", file_name, Utc::now().format("%Y%m%d%H%M%S"));
    let backup = src.with_file_name(backup_name);
    fs::rename(src, backup).map_err(|e| e.to_string())
}

fn normalize_downloads_directory(config: &mut Config) -> Result<(), String> {
    let explicit = config
        .downloads_directory
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);

    let resolved = if let Some(path) = explicit {
        ensure_writable_directory(&path).map_err(|e| e.to_string())?;
        path
    } else {
        let path = resolve_default_downloads_dir();
        ensure_writable_directory(&path).map_err(|e| e.to_string())?;
        path
    };

    config.downloads_directory = Some(resolved.to_string_lossy().to_string());
    Ok(())
}

fn legacy_skills_dir() -> Option<PathBuf> {
    get_app_config_dir().ok().map(|path| path.join("skills"))
}

fn legacy_node_runtime_dir() -> Option<PathBuf> {
    let base = dirs::data_local_dir().or_else(dirs::config_dir)?;
    Some(base.join("petool").join("runtime").join("node"))
}

#[cfg(target_os = "windows")]
fn apply_autostart(enabled: bool) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let run_key = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
    let value = format!("\"{}\"", exe.to_string_lossy());

    if enabled {
        let status = Command::new("reg")
            .args([
                "add",
                run_key,
                "/v",
                "PETool",
                "/t",
                "REG_SZ",
                "/d",
                &value,
                "/f",
            ])
            .status()
            .map_err(|e| e.to_string())?;
        if !status.success() {
            return Err("Failed to enable autostart on Windows".to_string());
        }
        return Ok(());
    }

    let _ = Command::new("reg")
        .args(["delete", run_key, "/v", "PETool", "/f"])
        .status();
    Ok(())
}

#[cfg(target_os = "macos")]
fn apply_autostart(enabled: bool) -> Result<(), String> {
    let home = dirs::home_dir().ok_or_else(|| "Unable to resolve home directory".to_string())?;
    let launch_agents_dir = home.join("Library").join("LaunchAgents");
    fs::create_dir_all(&launch_agents_dir).map_err(|e| e.to_string())?;

    let label = "com.petool.desktop";
    let plist_path = launch_agents_dir.join(format!("{}.plist", label));

    if enabled {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let plist = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{label}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{exe}</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <false/>
</dict>
</plist>
"#,
            label = label,
            exe = exe.to_string_lossy()
        );
        fs::write(&plist_path, plist).map_err(|e| e.to_string())?;
        let _ = Command::new("launchctl").arg("unload").arg(&plist_path).status();
        let status = Command::new("launchctl")
            .arg("load")
            .arg("-w")
            .arg(&plist_path)
            .status()
            .map_err(|e| e.to_string())?;
        if !status.success() {
            return Err("Failed to enable autostart on macOS".to_string());
        }
        return Ok(());
    }

    let _ = Command::new("launchctl")
        .arg("unload")
        .arg("-w")
        .arg(&plist_path)
        .status();
    if plist_path.exists() {
        fs::remove_file(&plist_path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn apply_autostart(_enabled: bool) -> Result<(), String> {
    Ok(())
}

#[tauri::command]
pub async fn get_config() -> Result<Config, String> {
    let mut config = load_config::<Config>().map_err(|e| e.to_string())?;
    normalize_downloads_directory(&mut config)?;
    Ok(config)
}

#[tauri::command]
pub async fn set_config(
    mut config: Config,
    skill_manager: tauri::State<'_, SkillManagerState>,
) -> Result<(), String> {
    let previous_config = load_config::<Config>().unwrap_or_default();

    normalize_downloads_directory(&mut config)?;

    let old_downloads = resolve_effective_downloads_dir(previous_config.downloads_directory.as_deref());
    let new_downloads = resolve_effective_downloads_dir(config.downloads_directory.as_deref());

    apply_autostart(config.autostart_enabled)?;

    if old_downloads != new_downloads {
        migrate_directory(&resolve_skills_dir(&old_downloads), &resolve_skills_dir(&new_downloads))?;
        migrate_directory(
            &resolve_node_runtime_root(&old_downloads),
            &resolve_node_runtime_root(&new_downloads),
        )?;
        migrate_directory(
            &resolve_skill_download_cache_dir(&old_downloads),
            &resolve_skill_download_cache_dir(&new_downloads),
        )?;
        migrate_directory(
            &resolve_node_download_cache_dir(&old_downloads),
            &resolve_node_download_cache_dir(&new_downloads),
        )?;

        if previous_config
            .downloads_directory
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none()
        {
            if let Some(legacy_skills) = legacy_skills_dir() {
                migrate_directory(&legacy_skills, &resolve_skills_dir(&new_downloads))?;
            }
            if let Some(legacy_runtime) = legacy_node_runtime_dir() {
                migrate_directory(&legacy_runtime, &resolve_node_runtime_root(&new_downloads))?;
            }
        }
    }

    save_config(&config).map_err(|e| e.to_string())?;

    let mut manager = skill_manager.lock().await;
    manager
        .set_skills_dir(resolve_skills_dir(&new_downloads))
        .map_err(|e| e.to_string())?;
    manager.load_skills().await.map_err(|e| e.to_string())?;

    Ok(())
}

fn resolve_profile(config: &Config, profile: Option<String>) -> String {
    let raw = profile.unwrap_or_else(|| config.browser.default_profile.clone());
    let sanitized = sanitize_profile_name(&raw);
    if config.browser.profiles.contains_key(&sanitized) {
        return sanitized;
    }
    if config
        .browser
        .profiles
        .contains_key(&config.browser.default_profile)
    {
        return config.browser.default_profile.clone();
    }
    config
        .browser
        .profiles
        .keys()
        .next()
        .cloned()
        .unwrap_or_else(|| "openclaw".to_string())
}

fn open_directory(path: &PathBuf) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        Command::new("explorer")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn profile_user_data_path(config: &Config, profile: &str) -> Result<PathBuf, String> {
    let profile_cfg = config
        .browser
        .profiles
        .get(profile)
        .ok_or_else(|| format!("Unknown browser profile: {}", profile))?;

    let custom_user_data = profile_cfg
        .user_data_dir
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);

    match custom_user_data {
        Some(path) => Ok(path),
        None => browser_profile_user_data_dir(profile).map_err(|e| e.to_string()),
    }
}

#[tauri::command]
pub async fn open_browser_profile_dir(profile: Option<String>) -> Result<String, String> {
    let config = load_config::<Config>().map_err(|e| e.to_string())?;
    let profile = resolve_profile(&config, profile);
    let path = profile_user_data_path(&config, &profile)?;
    if !Path::new(&path).exists() {
        fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    }
    open_directory(&path)?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn reset_browser_profile(profile: Option<String>) -> Result<(), String> {
    let config = load_config::<Config>().map_err(|e| e.to_string())?;
    let profile = resolve_profile(&config, profile);
    let profile_cfg = config
        .browser
        .profiles
        .get(&profile)
        .ok_or_else(|| format!("Unknown browser profile: {}", profile))?;
    if profile_cfg
        .user_data_dir
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_some()
    {
        return Err(
            "reset_browser_profile is blocked when user_data_dir is explicitly set".to_string(),
        );
    }
    let user_data_dir = browser_profile_user_data_dir(&profile).map_err(|e| e.to_string())?;
    let profile_dir = user_data_dir
        .parent()
        .ok_or_else(|| "Invalid profile path".to_string())?
        .to_path_buf();
    if profile_dir.exists() {
        fs::remove_dir_all(&profile_dir).map_err(|e| e.to_string())?;
    }
    let _ = browser_profile_user_data_dir(&profile).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn validate_api_key(api_key: String, api_base: Option<String>) -> Result<bool, String> {
    use reqwest::Client;

    let client = Client::new();
    let base = api_base.unwrap_or_else(|| "https://open.bigmodel.cn/api/paas/v4".to_string());
    let url = format!("{}/models", base.trim_end_matches('/'));

    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    Ok(response.status().is_success())
}

#[tauri::command]
pub async fn submit_feedback(input: FeedbackDraftInput) -> Result<FeedbackDraftSaved, String> {
    let detail = input.detail.trim();
    if detail.is_empty() {
        return Err("反馈内容不能为空".to_string());
    }

    let draft_id = Uuid::new_v4().to_string();
    let created_at = Utc::now().to_rfc3339();
    let prefix = Utc::now().format("%Y%m%d_%H%M%S").to_string();

    let draft_dir = get_app_config_dir()
        .map_err(|e| e.to_string())?
        .join("feedback")
        .join(format!("{}_{}", prefix, draft_id));
    fs::create_dir_all(&draft_dir).map_err(|e| e.to_string())?;

    let attachment_dir = draft_dir.join("attachments");
    fs::create_dir_all(&attachment_dir).map_err(|e| e.to_string())?;

    let mut saved_attachments = Vec::new();
    for raw_path in &input.attachments {
        let src = PathBuf::from(raw_path);
        if !src.exists() || !src.is_file() {
            continue;
        }
        let file_name = src
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("attachment.bin");
        let target = attachment_dir.join(file_name);
        fs::copy(&src, &target).map_err(|e| e.to_string())?;
        saved_attachments.push(target.to_string_lossy().to_string());
    }

    let payload = json!({
        "draft_id": draft_id,
        "created_at": created_at,
        "category": input.category,
        "detail": detail,
        "email": input.email,
        "attachments": saved_attachments,
        "client_meta": input.client_meta
    });

    let json_path = draft_dir.join("feedback.json");
    let content = serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?;
    fs::write(&json_path, content).map_err(|e| e.to_string())?;

    Ok(FeedbackDraftSaved {
        draft_id,
        saved_json_path: json_path.to_string_lossy().to_string(),
        saved_attachments,
        created_at,
    })
}

#[tauri::command]
pub fn app_exit_now(app: tauri::AppHandle) {
    app.exit(0);
}

