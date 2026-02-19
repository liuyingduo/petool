use crate::models::config::Config;
use crate::services::browser::paths::{browser_profile_user_data_dir, sanitize_profile_name};
use crate::utils::{load_config, save_config};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[tauri::command]
pub async fn get_config() -> Result<Config, String> {
    load_config::<Config>().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_config(config: Config) -> Result<(), String> {
    save_config(&config).map_err(|e| e.to_string())
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
pub fn app_exit_now(app: tauri::AppHandle) {
    app.exit(0);
}
