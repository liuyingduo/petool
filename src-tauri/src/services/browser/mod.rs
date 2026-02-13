pub mod ipc;
pub mod manager;
pub mod paths;
pub mod types;

use std::sync::OnceLock;

use serde_json::Value;

use crate::models::config::BrowserConfig;

use manager::BrowserManager;
use types::BrowserToolRequest;

static GLOBAL_BROWSER_MANAGER: OnceLock<tokio::sync::Mutex<BrowserManager>> = OnceLock::new();

fn browser_manager() -> &'static tokio::sync::Mutex<BrowserManager> {
    GLOBAL_BROWSER_MANAGER.get_or_init(|| tokio::sync::Mutex::new(BrowserManager::new()))
}

pub async fn execute_browser_request(
    request: &BrowserToolRequest,
    browser_config: &BrowserConfig,
) -> Result<Value, String> {
    let mut manager = browser_manager().lock().await;
    manager
        .execute(request, browser_config)
        .await
        .map_err(|err| err.to_string())
}
