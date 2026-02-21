use crate::models::config::Config;
use crate::services::llm::LlmService;
use sqlx::SqlitePool;
use regex::Regex;

pub(crate) const DEFAULT_GLM_API_BASE: &str = "https://open.bigmodel.cn/api/paas/v4";
pub(crate) const DEFAULT_ARK_API_BASE: &str = "https://ark.cn-beijing.volces.com/api/v3";
pub(crate) const DEFAULT_MINIMAX_OPENAI_API_BASE: &str = "https://api.minimaxi.com/v1";

pub(crate) async fn resolve_conversation_model(
    pool: &SqlitePool,
    conversation_id: &str,
    fallback_model: &str,
) -> Result<String, String> {
    sqlx::query_scalar::<_, String>("SELECT model FROM conversations WHERE id = ?")
        .bind(conversation_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())
        .map(|model| model.unwrap_or_else(|| fallback_model.to_string()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TextModelProvider {
    Glm,
    Doubao,
    MiniMax,
}

pub(crate) fn detect_text_model_provider(model: &str) -> TextModelProvider {
    let normalized = model.trim().to_ascii_lowercase();
    if normalized.starts_with("minimax-") {
        return TextModelProvider::MiniMax;
    }
    if normalized.starts_with("doubao-") {
        return TextModelProvider::Doubao;
    }
    TextModelProvider::Glm
}

pub(crate) fn env_value(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub(crate) fn first_non_empty(values: Vec<Option<String>>) -> Option<String> {
    values
        .into_iter()
        .flatten()
        .find(|value| !value.trim().is_empty())
}

pub(crate) fn resolve_text_llm_service(config: &Config, model: &str) -> Result<LlmService, String> {
    // ── Petool 中转优先 ───────────────────────────────────────────────
    if let Some(token) = config.petool_token.as_deref().filter(|t| !t.is_empty()) {
        let base = config
            .petool_api_base
            .clone()
            .unwrap_or_else(|| "http://localhost:8000".to_string());
        let proxy_base = format!("{}/v1", base.trim_end_matches('/'));
        return Ok(LlmService::new(token.to_string(), Some(proxy_base)));
    }

    // ── 未登录：回退到用户自配置的 API Key（向后兼容）────────────────
    match detect_text_model_provider(model) {
        TextModelProvider::Glm => {
            let api_key = first_non_empty(vec![
                config.api_key.clone(),
                env_value("GLM_API_KEY"),
                env_value("OPENAI_API_KEY"),
            ])
            .ok_or_else(|| "请先登录账号，或在设置中填写 GLM API Key".to_string())?;
            Ok(LlmService::new(
                api_key,
                Some(DEFAULT_GLM_API_BASE.to_string()),
            ))
        }
        TextModelProvider::Doubao => {
            let api_key = first_non_empty(vec![
                config.ark_api_key.clone(),
                env_value("ARK_API_KEY"),
                env_value("DOUBAO_API_KEY"),
                config.api_key.clone(),
            ])
            .ok_or_else(|| "请先登录账号，或在设置中填写 Doubao API Key".to_string())?;
            Ok(LlmService::new(
                api_key,
                Some(DEFAULT_ARK_API_BASE.to_string()),
            ))
        }
        TextModelProvider::MiniMax => {
            let api_key = first_non_empty(vec![
                config.minimax_api_key.clone(),
                env_value("MINIMAX_API_KEY"),
                env_value("OPENAI_API_KEY"),
                env_value("ANTHROPIC_API_KEY"),
            ])
            .ok_or_else(|| "请先登录账号，或在设置中填写 MiniMax API Key".to_string())?;
            Ok(LlmService::new(
                api_key,
                Some(DEFAULT_MINIMAX_OPENAI_API_BASE.to_string()),
            ))
        }
    }
}

pub(crate) fn resolve_image_generation_llm_service(config: &Config) -> Result<LlmService, String> {
    let api_key = first_non_empty(vec![
        config.ark_api_key.clone(),
        env_value("ARK_API_KEY"),
        env_value("DOUBAO_API_KEY"),
        config.api_key.clone(),
    ])
    .ok_or_else(|| "Image API key not set".to_string())?;

    Ok(LlmService::new(
        api_key,
        Some(DEFAULT_ARK_API_BASE.to_string()),
    ))
}

pub(crate) fn resolve_clawhub_settings_for_discovery() -> (Option<String>, Option<String>) {
    if let Ok(config) = crate::utils::load_config::<Config>() {
        let key = config
            .clawhub_api_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let base = config
            .clawhub_api_base
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        if key.is_some() || base.is_some() {
            return (key, base);
        }
    }

    let Ok(path) = crate::utils::get_config_path() else {
        return (None, None);
    };
    let Ok(raw) = std::fs::read_to_string(path) else {
        return (None, None);
    };
    let key = Regex::new(r#""clawhub_api_key"\s*:\s*"([^"]*)""#)
        .ok()
        .and_then(|regex| regex.captures(&raw))
        .and_then(|caps| {
            caps.get(1)
                .map(|capture| capture.as_str().trim().to_string())
        })
        .filter(|value| !value.is_empty());
    let base = Regex::new(r#""clawhub_api_base"\s*:\s*"([^"]*)""#)
        .ok()
        .and_then(|regex| regex.captures(&raw))
        .and_then(|caps| {
            caps.get(1)
                .map(|capture| capture.as_str().trim().to_string())
        })
        .filter(|value| !value.is_empty());
    (key, base)
}
