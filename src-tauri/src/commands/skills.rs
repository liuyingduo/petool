use crate::models::{config::Config, skill::*};
use crate::services::skill_manager::SkillManager;
use crate::utils::{get_config_path, load_config};
use regex::Regex;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex as AsyncMutex;

pub type SkillManagerState = Arc<AsyncMutex<SkillManager>>;

fn resolve_skillsmp_settings() -> (Option<String>, Option<String>) {
    if let Ok(config) = load_config::<Config>() {
        let key = config
            .skillsmp_api_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let base = config
            .skillsmp_api_base
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        if key.is_some() || base.is_some() {
            return (key, base);
        }
    }

    let Ok(path) = get_config_path() else {
        return (None, None);
    };
    let Ok(raw) = std::fs::read_to_string(path) else {
        return (None, None);
    };
    let key = Regex::new(r#""skillsmp_api_key"\s*:\s*"([^"]*)""#)
        .ok()
        .and_then(|regex| regex.captures(&raw))
        .and_then(|caps| {
            caps.get(1)
                .map(|capture| capture.as_str().trim().to_string())
        })
        .filter(|value| !value.is_empty());
    let base = Regex::new(r#""skillsmp_api_base"\s*:\s*"([^"]*)""#)
        .ok()
        .and_then(|regex| regex.captures(&raw))
        .and_then(|caps| {
            caps.get(1)
                .map(|capture| capture.as_str().trim().to_string())
        })
        .filter(|value| !value.is_empty());
    (key, base)
}

#[tauri::command]
pub async fn list_skills(
    skill_manager: tauri::State<'_, SkillManagerState>,
) -> Result<Vec<Skill>, String> {
    let manager = skill_manager.lock().await;
    Ok(manager.list_skills())
}

#[tauri::command]
pub async fn install_skill(
    skill_manager: tauri::State<'_, SkillManagerState>,
    repo_url: String,
    skill_path: Option<String>,
) -> Result<Skill, String> {
    let mut manager = skill_manager.lock().await;
    let result = if let Some(path) = skill_path.as_deref() {
        manager.install_skill_with_path(&repo_url, Some(path)).await
    } else {
        manager.install_skill(&repo_url).await
    };
    result.map_err(|e: anyhow::Error| e.to_string())
}

#[tauri::command]
pub async fn discover_skills(
    skill_manager: tauri::State<'_, SkillManagerState>,
    query: Option<String>,
    limit: Option<u32>,
) -> Result<Vec<SkillDiscoveryItem>, String> {
    let (skillsmp_api_key, skillsmp_api_base) = resolve_skillsmp_settings();
    let manager = skill_manager.lock().await;
    manager
        .discover_skills(
            query.as_deref(),
            limit.unwrap_or(8) as usize,
            skillsmp_api_key.as_deref(),
            skillsmp_api_base.as_deref(),
        )
        .await
        .map_err(|e: anyhow::Error| e.to_string())
}

#[tauri::command]
pub async fn uninstall_skill(
    skill_manager: tauri::State<'_, SkillManagerState>,
    skill_id: String,
) -> Result<(), String> {
    let mut manager = skill_manager.lock().await;
    manager
        .uninstall_skill(&skill_id)
        .await
        .map_err(|e: anyhow::Error| e.to_string())
}

#[tauri::command]
pub async fn execute_skill(
    skill_manager: tauri::State<'_, SkillManagerState>,
    skill_id: String,
    params: HashMap<String, Value>,
) -> Result<Value, String> {
    let manager = skill_manager.lock().await;
    manager
        .execute_skill(&skill_id, params)
        .await
        .map_err(|e: anyhow::Error| e.to_string())
}

#[tauri::command]
pub async fn toggle_skill(
    skill_manager: tauri::State<'_, SkillManagerState>,
    skill_id: String,
    enabled: bool,
) -> Result<(), String> {
    let mut manager = skill_manager.lock().await;
    manager
        .set_skill_enabled(&skill_id, enabled)
        .map_err(|e: anyhow::Error| e.to_string())
}

#[tauri::command]
pub async fn update_skill(
    skill_manager: tauri::State<'_, SkillManagerState>,
    skill_id: String,
) -> Result<Skill, String> {
    let mut manager = skill_manager.lock().await;
    manager
        .update_skill(&skill_id)
        .await
        .map_err(|e: anyhow::Error| e.to_string())
}
