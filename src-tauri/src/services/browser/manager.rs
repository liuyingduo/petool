use std::fs;
use std::time::Duration;

use anyhow::Result;
use serde_json::{json, Value};

use crate::models::config::BrowserConfig;

use super::ipc::BrowserIpcClient;
use super::paths::{
    browser_profiles_root, browser_sidecar_stderr_log_path, resolve_sidecar_launch_spec,
    sanitize_profile_name,
};
use super::types::{BrowserSidecarPaths, BrowserSidecarRequestPayload, BrowserToolRequest};

pub struct BrowserManager {
    client: Option<BrowserIpcClient>,
}

impl BrowserManager {
    pub fn new() -> Self {
        Self { client: None }
    }

    async fn ensure_started(&mut self) -> Result<()> {
        if let Some(client) = &self.client {
            if client.is_running().await {
                return Ok(());
            }
        }

        let launch_spec = resolve_sidecar_launch_spec()?;
        let client = BrowserIpcClient::spawn(launch_spec).await?;
        self.client = Some(client);
        Ok(())
    }

    async fn restart(&mut self) -> Result<()> {
        if let Some(client) = &self.client {
            client.kill().await;
        }
        self.client = None;
        self.ensure_started().await
    }

    fn resolve_profile<'a>(
        &self,
        request: &'a BrowserToolRequest,
        browser_config: &'a BrowserConfig,
    ) -> Result<String> {
        let requested = request
            .profile
            .as_deref()
            .unwrap_or(&browser_config.default_profile);
        let normalized = sanitize_profile_name(requested);
        if browser_config.profiles.contains_key(&normalized) {
            return Ok(normalized);
        }
        if browser_config
            .profiles
            .contains_key(&browser_config.default_profile)
        {
            return Ok(browser_config.default_profile.clone());
        }
        browser_config
            .profiles
            .keys()
            .next()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("No browser profiles configured"))
    }

    async fn call_once(
        &mut self,
        request: &BrowserToolRequest,
        browser_config: &BrowserConfig,
    ) -> Result<Value> {
        self.ensure_started().await?;
        let resolved_profile = self.resolve_profile(request, browser_config)?;
        let profiles_root = browser_profiles_root()?;
        let app_log_dir = browser_sidecar_stderr_log_path()?
            .parent()
            .map(|value| value.to_path_buf())
            .unwrap_or_else(|| profiles_root.clone());

        let payload = BrowserSidecarRequestPayload {
            request: BrowserToolRequest {
                action: request.action.clone(),
                profile: Some(resolved_profile.clone()),
                target_id: request.target_id.clone(),
                params: request.params.clone(),
            },
            browser_config: browser_config.clone(),
            paths: BrowserSidecarPaths {
                profiles_root: profiles_root.to_string_lossy().to_string(),
                app_log_dir: app_log_dir.to_string_lossy().to_string(),
            },
        };

        let timeout_ms = request
            .params
            .get("timeout_ms")
            .and_then(Value::as_u64)
            .unwrap_or(browser_config.operation_timeout_ms)
            .clamp(1_000, 120_000);

        let response = self
            .client
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Browser sidecar is not available"))?
            .call(
                "browser.action",
                serde_json::to_value(payload)?,
                Duration::from_millis(timeout_ms + 15_000), // add IPC buffer to prevent premature rust timeout
            )
            .await?;

        if response.ok {
            Ok(json!({
                "ok": true,
                "data": response.data.unwrap_or_else(|| json!({})),
                "error": Value::Null,
                "meta": response.meta.unwrap_or_else(|| json!({
                    "profile": resolved_profile,
                    "duration_ms": timeout_ms
                }))
            }))
        } else {
            Ok(json!({
                "ok": false,
                "data": Value::Null,
                "error": response.error.unwrap_or_else(|| "Unknown browser sidecar error".to_string()),
                "meta": response.meta.unwrap_or_else(|| json!({
                    "profile": resolved_profile
                }))
            }))
        }
    }

    pub async fn execute(
        &mut self,
        request: &BrowserToolRequest,
        browser_config: &BrowserConfig,
    ) -> Result<Value> {
        if !browser_config.enabled {
            return Ok(json!({
                "ok": false,
                "data": Value::Null,
                "error": "Browser control is disabled. Set browser.enabled=true in settings.",
                "meta": {
                    "disabled": true
                }
            }));
        }

        let first = self.call_once(request, browser_config).await;
        match first {
            Ok(value) => Ok(value),
            Err(first_error) => {
                let _ = self.restart().await;
                match self.call_once(request, browser_config).await {
                    Ok(value) => Ok(value),
                    Err(second_error) => {
                        let stderr_excerpt = fs::read_to_string(browser_sidecar_stderr_log_path()?)
                            .unwrap_or_default();
                        let stderr_tail = if stderr_excerpt.chars().count() <= 2_000 {
                            stderr_excerpt
                        } else {
                            stderr_excerpt
                                .chars()
                                .rev()
                                .take(2_000)
                                .collect::<Vec<_>>()
                                .into_iter()
                                .rev()
                                .collect()
                        };

                        let launch_help = if let Some(client) = &self.client {
                            client.launch_help_message(&stderr_tail)
                        } else {
                            format!(
                                "Browser sidecar failed after restart. Stderr tail: {}",
                                stderr_tail
                            )
                        };

                        Err(anyhow::anyhow!(
                            "{}\nFirst error: {}\nSecond error: {}",
                            launch_help,
                            first_error,
                            second_error
                        ))
                    }
                }
            }
        }
    }
}

impl Default for BrowserManager {
    fn default() -> Self {
        Self::new()
    }
}
