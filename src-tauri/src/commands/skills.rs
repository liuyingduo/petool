use crate::models::skill::*;
use crate::services::skill_manager::SkillManager;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex as AsyncMutex;

pub type SkillManagerState = Arc<AsyncMutex<SkillManager>>;

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
) -> Result<Skill, String> {
    let mut manager = skill_manager.lock().await;
    manager.install_skill(&repo_url).await.map_err(|e: anyhow::Error| e.to_string())
}

#[tauri::command]
pub async fn uninstall_skill(
    skill_manager: tauri::State<'_, SkillManagerState>,
    skill_id: String,
) -> Result<(), String> {
    let mut manager = skill_manager.lock().await;
    manager.uninstall_skill(&skill_id).await.map_err(|e: anyhow::Error| e.to_string())
}

#[tauri::command]
pub async fn execute_skill(
    skill_manager: tauri::State<'_, SkillManagerState>,
    skill_id: String,
    params: HashMap<String, Value>,
) -> Result<Value, String> {
    let manager = skill_manager.lock().await;
    manager.execute_skill(&skill_id, params).await.map_err(|e: anyhow::Error| e.to_string())
}

#[tauri::command]
pub async fn toggle_skill(
    _skill_manager: tauri::State<'_, SkillManagerState>,
    _skill_id: String,
    _enabled: bool,
) -> Result<(), String> {
    // This would require adding a toggle method to SkillManager
    // For now, we just return success
    Ok(())
}
