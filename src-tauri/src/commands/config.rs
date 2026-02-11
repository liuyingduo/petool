use crate::models::config::Config;
use crate::utils::{load_config, save_config};

#[tauri::command]
pub async fn get_config() -> Result<Config, String> {
    load_config::<Config>().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_config(config: Config) -> Result<(), String> {
    save_config(&config).map_err(|e| e.to_string())
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
