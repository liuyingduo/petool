use crate::models::config::Config;
use chrono::Utc;
use mem0_rust::config::{OpenAIEmbedderConfig, OpenAILLMConfig};
use mem0_rust::{
    AddOptions, EmbedderConfig, GetAllOptions, LLMConfig, Memory, MemoryConfig, Message,
    MockEmbedderConfig, SearchOptions,
};
use serde_json::{json, Value};
use sqlx::SqlitePool;
use std::collections::{HashMap, HashSet};

const DEFAULT_MEMORY_API_BASE: &str = "http://localhost:8000";
const DEFAULT_GLM_API_BASE: &str = "https://open.bigmodel.cn/api/paas/v4";
const DEFAULT_ARK_API_BASE: &str = "https://ark.cn-beijing.volces.com/api/v3";
const DEFAULT_MINIMAX_API_BASE: &str = "https://api.minimaxi.com/v1";
const DEFAULT_OPENAI_API_BASE: &str = "https://api.openai.com/v1";
const MEMORY_COLLECTION_NAME: &str = "petool_memory";
const MEMORY_SEARCH_LIMIT: usize = 8;
const MEMORY_SEARCH_THRESHOLD: f32 = 0.38;
const MEMORY_MAX_RECORDS: usize = 600;
const MEMORY_EMBEDDING_MODEL_ENV: &str = "MEM0_EMBEDDING_MODEL";

#[derive(Debug, Clone)]
struct MemoryHit {
    content: String,
    score: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TextModelProvider {
    Glm,
    Doubao,
    MiniMax,
    OpenAi,
}

pub async fn prepare_memory_prompt_and_remember_turn(
    pool: &SqlitePool,
    config: &Config,
    model: &str,
    conversation_id: &str,
    user_content: &str,
) -> Result<Option<String>, String> {
    let trimmed_content = user_content.trim();
    if trimmed_content.is_empty() {
        return Ok(None);
    }

    let memory_user_id = resolve_memory_user_id(config);
    let memory = build_memory(config, model).await?;

    hydrate_memory_snapshot(pool, &memory, &memory_user_id).await?;
    let memory_prompt = search_memory_prompt(&memory, &memory_user_id, trimmed_content).await?;
    remember_user_turn(
        pool,
        &memory,
        &memory_user_id,
        conversation_id,
        trimmed_content,
    )
    .await?;

    Ok(memory_prompt)
}

fn search_options_for_user(user_id: &str) -> SearchOptions {
    SearchOptions::for_user(user_id)
        .with_limit(MEMORY_SEARCH_LIMIT)
        .with_threshold(MEMORY_SEARCH_THRESHOLD)
}

async fn search_memory_prompt(
    memory: &Memory,
    memory_user_id: &str,
    query: &str,
) -> Result<Option<String>, String> {
    let result = memory
        .search(query, search_options_for_user(memory_user_id))
        .await
        .map_err(|e| e.to_string())?;
    let hits = collect_memory_hits(query, result.results);
    if hits.is_empty() {
        return Ok(None);
    }

    let mut lines = Vec::with_capacity(hits.len() + 1);
    lines.push(
        "以下是跨会话用户记忆（偏好/意图/事实），仅在与当前请求相关时参考；与当前用户最新表达冲突时，以当前表达为准。"
            .to_string(),
    );
    for hit in hits {
        lines.push(format!("- ({:.2}) {}", hit.score, hit.content));
    }

    Ok(Some(lines.join("\n")))
}

fn collect_memory_hits(
    query: &str,
    candidates: Vec<mem0_rust::models::ScoredMemory>,
) -> Vec<MemoryHit> {
    let mut dedupe = HashSet::<String>::new();
    let query_terms = tokenize_terms(query);
    let mut hits = Vec::<MemoryHit>::new();

    for item in candidates {
        let normalized = normalize_memory_text(&item.record.content);
        if normalized.is_empty() {
            continue;
        }
        let overlap = has_term_overlap(&query_terms, &tokenize_terms(&normalized));
        if item.score < MEMORY_SEARCH_THRESHOLD && !overlap {
            continue;
        }
        if !dedupe.insert(normalized.clone()) {
            continue;
        }

        hits.push(MemoryHit {
            content: normalized,
            score: item.score,
        });
        if hits.len() >= MEMORY_SEARCH_LIMIT {
            break;
        }
    }

    hits
}

fn tokenize_terms(text: &str) -> HashSet<String> {
    text.to_ascii_lowercase()
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter_map(|part| {
            let token = part.trim();
            if token.len() >= 2 {
                Some(token.to_string())
            } else {
                None
            }
        })
        .collect()
}

fn has_term_overlap(left: &HashSet<String>, right: &HashSet<String>) -> bool {
    left.iter().any(|token| right.contains(token))
}

fn normalize_memory_text(text: &str) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = collapsed.trim();
    if trimmed.chars().count() <= 220 {
        trimmed.to_string()
    } else {
        trimmed.chars().take(220).collect()
    }
}

async fn remember_user_turn(
    pool: &SqlitePool,
    memory: &Memory,
    memory_user_id: &str,
    conversation_id: &str,
    user_content: &str,
) -> Result<(), String> {
    let mut metadata = HashMap::<String, Value>::new();
    metadata.insert("conversation_id".to_string(), json!(conversation_id));
    metadata.insert("source".to_string(), json!("chat_user_turn"));
    metadata.insert("recorded_at".to_string(), json!(Utc::now().to_rfc3339()));

    let mut add_options = AddOptions::for_user(memory_user_id.to_string());
    add_options.metadata = Some(metadata);

    memory
        .add(vec![Message::user(user_content.to_string())], add_options)
        .await
        .map_err(|e| e.to_string())?;

    let mut records = memory
        .get_all(GetAllOptions {
            user_id: Some(memory_user_id.to_string()),
            limit: Some(MEMORY_MAX_RECORDS),
            ..Default::default()
        })
        .await
        .map_err(|e| e.to_string())?;

    records.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    records.truncate(MEMORY_MAX_RECORDS);

    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM memory_snapshots WHERE user_id = ?")
        .bind(memory_user_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;

    for record in records {
        let metadata_text =
            serde_json::to_string(&record.metadata).unwrap_or_else(|_| "{}".to_string());
        sqlx::query(
            "INSERT INTO memory_snapshots (id, user_id, content, metadata, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(record.id.to_string())
        .bind(memory_user_id)
        .bind(&record.content)
        .bind(metadata_text)
        .bind(record.created_at.to_rfc3339())
        .bind(record.updated_at.to_rfc3339())
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    }

    tx.commit().await.map_err(|e| e.to_string())
}

async fn hydrate_memory_snapshot(
    pool: &SqlitePool,
    memory: &Memory,
    memory_user_id: &str,
) -> Result<(), String> {
    let rows = sqlx::query_as::<_, (String, String)>(
        "SELECT content, metadata FROM memory_snapshots WHERE user_id = ? ORDER BY updated_at ASC",
    )
    .bind(memory_user_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    for (content, metadata_raw) in rows {
        let metadata = serde_json::from_str::<HashMap<String, Value>>(&metadata_raw)
            .unwrap_or_else(|_| HashMap::new());
        let add_options = AddOptions {
            user_id: Some(memory_user_id.to_string()),
            metadata: if metadata.is_empty() {
                None
            } else {
                Some(metadata)
            },
            infer: false,
            ..Default::default()
        };
        memory
            .add(content, add_options)
            .await
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

async fn build_memory(config: &Config, model: &str) -> Result<Memory, String> {
    let credentials = resolve_llm_credentials(config, model);
    let llm_config = credentials.as_ref().map(|credential| {
        LLMConfig::OpenAI(OpenAILLMConfig {
            api_key: Some(credential.api_key.clone()),
            model: model.to_string(),
            temperature: 0.0,
            max_tokens: Some(1024),
            base_url: Some(credential.api_base.clone()),
        })
    });

    let embedder_config = resolve_embedder_config(model, credentials.as_ref());

    let memory_config = MemoryConfig {
        embedder: embedder_config,
        llm: llm_config,
        collection_name: MEMORY_COLLECTION_NAME.to_string(),
        ..Default::default()
    };

    Memory::new(memory_config).await.map_err(|e| e.to_string())
}

fn resolve_embedder_config(model: &str, credentials: Option<&LlmCredentials>) -> EmbedderConfig {
    let embedding_model = env_value(MEMORY_EMBEDDING_MODEL_ENV);
    if let (Some(embed_model), Some(credential)) = (embedding_model, credentials) {
        return EmbedderConfig::OpenAI(OpenAIEmbedderConfig {
            api_key: Some(credential.api_key.clone()),
            model: embed_model,
            dimensions: None,
            base_url: Some(credential.api_base.clone()),
        });
    }

    if detect_text_model_provider(model) == TextModelProvider::OpenAi {
        if let Some(credential) = credentials {
            return EmbedderConfig::OpenAI(OpenAIEmbedderConfig {
                api_key: Some(credential.api_key.clone()),
                model: "text-embedding-3-small".to_string(),
                dimensions: Some(1536),
                base_url: Some(credential.api_base.clone()),
            });
        }
    }

    EmbedderConfig::Mock(MockEmbedderConfig { dimensions: 384 })
}

#[derive(Debug, Clone)]
struct LlmCredentials {
    api_key: String,
    api_base: String,
}

fn resolve_llm_credentials(config: &Config, model: &str) -> Option<LlmCredentials> {
    if let Some(token) = config
        .petool_token
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        let base = config
            .petool_api_base
            .clone()
            .unwrap_or_else(|| DEFAULT_MEMORY_API_BASE.to_string())
            .trim_end_matches('/')
            .to_string();
        return Some(LlmCredentials {
            api_key: token.to_string(),
            api_base: format!("{}/v1", base),
        });
    }

    match detect_text_model_provider(model) {
        TextModelProvider::Glm => first_non_empty(vec![
            config.api_key.clone(),
            env_value("GLM_API_KEY"),
            env_value("OPENAI_API_KEY"),
        ])
        .map(|api_key| LlmCredentials {
            api_key,
            api_base: config
                .api_base
                .clone()
                .unwrap_or_else(|| DEFAULT_GLM_API_BASE.to_string()),
        }),
        TextModelProvider::Doubao => first_non_empty(vec![
            config.ark_api_key.clone(),
            env_value("ARK_API_KEY"),
            env_value("DOUBAO_API_KEY"),
            config.api_key.clone(),
        ])
        .map(|api_key| LlmCredentials {
            api_key,
            api_base: config
                .ark_api_base
                .clone()
                .unwrap_or_else(|| DEFAULT_ARK_API_BASE.to_string()),
        }),
        TextModelProvider::MiniMax => first_non_empty(vec![
            config.minimax_api_key.clone(),
            env_value("MINIMAX_API_KEY"),
            env_value("OPENAI_API_KEY"),
            env_value("ANTHROPIC_API_KEY"),
        ])
        .map(|api_key| LlmCredentials {
            api_key,
            api_base: DEFAULT_MINIMAX_API_BASE.to_string(),
        }),
        TextModelProvider::OpenAi => {
            first_non_empty(vec![env_value("OPENAI_API_KEY"), config.api_key.clone()]).map(
                |api_key| LlmCredentials {
                    api_key,
                    api_base: env_value("OPENAI_API_BASE")
                        .unwrap_or_else(|| DEFAULT_OPENAI_API_BASE.to_string()),
                },
            )
        }
    }
}

fn detect_text_model_provider(model: &str) -> TextModelProvider {
    let normalized = model.trim().to_ascii_lowercase();
    if normalized.starts_with("gpt-") {
        return TextModelProvider::OpenAi;
    }
    if normalized.starts_with("minimax-") || normalized.starts_with("abab") {
        return TextModelProvider::MiniMax;
    }
    if normalized.starts_with("doubao-") || normalized.starts_with("ep-") {
        return TextModelProvider::Doubao;
    }
    TextModelProvider::Glm
}

fn env_value(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn first_non_empty(values: Vec<Option<String>>) -> Option<String> {
    values
        .into_iter()
        .flatten()
        .find(|value| !value.trim().is_empty())
}

fn resolve_memory_user_id(config: &Config) -> String {
    if let Some(token) = config.petool_token.as_deref() {
        if let Some(sub) = decode_jwt_sub(token) {
            return format!("petool:{}", sub);
        }
    }

    let local_user = first_non_empty(vec![
        env_value("USERNAME"),
        env_value("USER"),
        env_value("LOGNAME"),
    ])
    .unwrap_or_else(|| "default".to_string());

    format!("local:{}", sanitize_identifier(&local_user))
}

fn sanitize_identifier(value: &str) -> String {
    let normalized: String = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();

    let compact = normalized
        .split('_')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("_");

    if compact.is_empty() {
        "default".to_string()
    } else {
        compact
    }
}

fn decode_jwt_sub(token: &str) -> Option<String> {
    use base64::engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD};
    use base64::Engine as _;

    let payload = token.split('.').nth(1)?;
    let decoded = URL_SAFE_NO_PAD
        .decode(payload)
        .or_else(|_| URL_SAFE.decode(payload))
        .ok()?;
    let claims: Value = serde_json::from_slice(&decoded).ok()?;
    claims
        .get("sub")
        .and_then(Value::as_str)
        .map(|value| value.to_string())
}
