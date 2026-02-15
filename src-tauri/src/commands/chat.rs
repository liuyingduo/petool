use crate::commands::mcp::McpState;
use crate::commands::skills::SkillManagerState;
use crate::models::chat::*;
use crate::models::config::{Config, McpTransport, ToolPathPermissionRule, ToolPermissionAction};
use crate::services::llm::{
    ChatMessage, ChatTool, ChatToolCall, ChatToolFunction, LlmService, LlmStreamEvent,
    LlmStreamResult,
};
use crate::services::mcp_client::{HttpTransport, McpClient, StdioTransport};
use crate::state::AppState;
use chrono::Utc;
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::SqlitePool;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::net::IpAddr;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tauri::{Emitter, State, Window};
use tokio::process::Command as TokioCommand;
use tokio::sync::oneshot;
use uuid::Uuid;
use walkdir::WalkDir;

mod browser_tools;
mod image_tools;
mod process_tools;
mod tool_catalog;
mod web_tools;

type ToolApprovalSender = oneshot::Sender<ToolApprovalDecision>;

static TOOL_APPROVAL_WAITERS: OnceLock<tokio::sync::Mutex<HashMap<String, ToolApprovalSender>>> =
    OnceLock::new();
static STREAM_STOP_FLAGS: OnceLock<tokio::sync::Mutex<HashMap<String, Arc<AtomicBool>>>> =
    OnceLock::new();

fn tool_approval_waiters() -> &'static tokio::sync::Mutex<HashMap<String, ToolApprovalSender>> {
    TOOL_APPROVAL_WAITERS.get_or_init(|| tokio::sync::Mutex::new(HashMap::new()))
}

fn stream_stop_flags() -> &'static tokio::sync::Mutex<HashMap<String, Arc<AtomicBool>>> {
    STREAM_STOP_FLAGS.get_or_init(|| tokio::sync::Mutex::new(HashMap::new()))
}

async fn register_stream_stop_flag(conversation_id: &str) -> Arc<AtomicBool> {
    let flag = Arc::new(AtomicBool::new(false));
    let mut flags = stream_stop_flags().lock().await;
    flags.insert(conversation_id.to_string(), flag.clone());
    flag
}

async fn clear_stream_stop_flag(conversation_id: &str) {
    let mut flags = stream_stop_flags().lock().await;
    flags.remove(conversation_id);
}

async fn request_stream_stop(conversation_id: &str) -> bool {
    let flags = stream_stop_flags().lock().await;
    if let Some(flag) = flags.get(conversation_id) {
        flag.store(true, Ordering::Relaxed);
        return true;
    }
    false
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolApprovalDecision {
    AllowOnce,
    AllowAlways,
    Deny,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ToolApprovalRequestPayload {
    request_id: String,
    conversation_id: String,
    tool_call_id: String,
    tool_name: String,
    arguments: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateImageResponse {
    user_message: Message,
    assistant_message: Message,
    image_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TodoStatus {
    Pending,
    InProgress,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TodoItem {
    id: String,
    text: String,
    status: TodoStatus,
    created_at: String,
    updated_at: String,
}

type TodoStore = tokio::sync::Mutex<HashMap<String, Vec<TodoItem>>>;

static TODO_STORE: OnceLock<TodoStore> = OnceLock::new();

fn todo_store() -> &'static TodoStore {
    TODO_STORE.get_or_init(|| tokio::sync::Mutex::new(HashMap::new()))
}

fn sanitize_tool_fragment(value: &str) -> String {
    let sanitized: String = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();

    sanitized
        .split('_')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

fn build_tool_alias(server: &str, tool_name: &str) -> String {
    let server_part = sanitize_tool_fragment(server);
    let tool_part = sanitize_tool_fragment(tool_name);
    format!("mcp__{}__{}", server_part, tool_part)
}

const WORKSPACE_LIST_TOOL: &str = "workspace_list_directory";
const WORKSPACE_READ_TOOL: &str = "workspace_read_file";
const WORKSPACE_WRITE_TOOL: &str = "workspace_write_file";
const WORKSPACE_EDIT_TOOL: &str = "workspace_edit_file";
const WORKSPACE_GLOB_TOOL: &str = "workspace_glob";
const WORKSPACE_GREP_TOOL: &str = "workspace_grep";
const WORKSPACE_CODESEARCH_TOOL: &str = "workspace_codesearch";
const WORKSPACE_LSP_SYMBOLS_TOOL: &str = "workspace_lsp_symbols";
const WORKSPACE_APPLY_PATCH_TOOL: &str = "workspace_apply_patch";
const WORKSPACE_RUN_TOOL: &str = "workspace_run_command";
const WORKSPACE_PROCESS_START_TOOL: &str = "workspace_process_start";
const WORKSPACE_PROCESS_LIST_TOOL: &str = "workspace_process_list";
const WORKSPACE_PROCESS_READ_TOOL: &str = "workspace_process_read";
const WORKSPACE_PROCESS_TERMINATE_TOOL: &str = "workspace_process_terminate";
const SKILL_INSTALL_TOOL: &str = "skills_install_from_repo";
const SKILL_DISCOVER_TOOL: &str = "skills_discover";
const SKILL_LIST_TOOL: &str = "skills_list";
const SKILL_EXECUTE_TOOL: &str = "skills_execute";
const CORE_BATCH_TOOL: &str = "core_batch";
const CORE_TASK_TOOL: &str = "core_task";
const TODO_WRITE_TOOL: &str = "todo_write";
const TODO_READ_TOOL: &str = "todo_read";
const WEB_FETCH_TOOL: &str = "web_fetch";
const WEB_SEARCH_TOOL: &str = "web_search";
const BROWSER_TOOL: &str = "browser";
const BROWSER_NAVIGATE_TOOL: &str = "browser_navigate";
const IMAGE_PROBE_TOOL: &str = "image_probe";
const IMAGE_UNDERSTAND_TOOL: &str = "image_understand";
const SESSIONS_LIST_TOOL: &str = "sessions_list";
const SESSIONS_HISTORY_TOOL: &str = "sessions_history";
const SESSIONS_SEND_TOOL: &str = "sessions_send";
const SESSIONS_SPAWN_TOOL: &str = "sessions_spawn";
const AGENTS_LIST_TOOL: &str = "agents_list";
const DEFAULT_WEB_USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36";
const DEFAULT_WEB_ACCEPT_LANGUAGE: &str = "zh-CN,zh;q=0.9,en;q=0.8";
const DEFAULT_EXA_MCP_ENDPOINT: &str = "https://mcp.exa.ai/mcp";
const DEFAULT_GLM_API_BASE: &str = "https://open.bigmodel.cn/api/paas/v4";
const DEFAULT_ARK_API_BASE: &str = "https://ark.cn-beijing.volces.com/api/v3";
const DEFAULT_MINIMAX_ANTHROPIC_API_BASE: &str = "https://api.minimaxi.com/anthropic";
const REPEATED_TOOL_GUARD_TEXT: &str = "Detected repeated identical tool calls from the model. Automatic tool loop was stopped. Please provide a more specific target (file/directory) and try again.";
const STREAM_PAUSED_TEXT: &str = "（已暂停）";

#[derive(Debug, Clone)]
enum RuntimeTool {
    Mcp {
        server_name: String,
        tool_name: String,
    },
    WorkspaceListDirectory,
    WorkspaceReadFile,
    WorkspaceWriteFile,
    WorkspaceEditFile,
    WorkspaceGlob,
    WorkspaceGrep,
    WorkspaceCodeSearch,
    WorkspaceLspSymbols,
    WorkspaceApplyPatch,
    WorkspaceRunCommand,
    WorkspaceProcessStart,
    WorkspaceProcessList,
    WorkspaceProcessRead,
    WorkspaceProcessTerminate,
    SkillInstallFromRepo,
    SkillDiscover,
    SkillList,
    SkillExecute,
    CoreBatch,
    CoreTask,
    TodoWrite,
    TodoRead,
    WebFetch,
    WebSearch,
    Browser,
    BrowserNavigate,
    ImageProbe,
    ImageUnderstand,
    SessionsList,
    SessionsHistory,
    SessionsSend,
    SessionsSpawn,
    AgentsList,
}

#[derive(Debug)]
struct RuntimeToolCatalog {
    available_tools: Vec<ChatTool>,
    tool_map: HashMap<String, RuntimeTool>,
}

async fn insert_message(
    pool: &SqlitePool,
    conversation_id: &str,
    role: &str,
    content: &str,
    tool_calls: Option<String>,
    reasoning: Option<String>,
) -> Result<(), String> {
    let message_id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO messages (id, conversation_id, role, content, created_at, tool_calls, reasoning) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&message_id)
    .bind(conversation_id)
    .bind(role)
    .bind(content)
    .bind(&now)
    .bind(tool_calls)
    .bind(reasoning)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    sqlx::query("UPDATE conversations SET updated_at = ? WHERE id = ?")
        .bind(&now)
        .bind(conversation_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

async fn load_conversation_context(
    pool: &SqlitePool,
    conversation_id: &str,
) -> Result<Vec<ChatMessage>, String> {
    let rows = sqlx::query_as::<_, (String, String, Option<String>, Option<String>)>(
        "SELECT role, content, tool_calls, reasoning FROM messages WHERE conversation_id = ? ORDER BY created_at ASC",
    )
    .bind(conversation_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    let mut messages = Vec::new();

    for (role, content, tool_calls_raw, reasoning_raw) in rows {
        match role.as_str() {
            "assistant" => {
                let tool_calls = tool_calls_raw
                    .as_deref()
                    .and_then(|value| serde_json::from_str::<Vec<ChatToolCall>>(value).ok());

                messages.push(ChatMessage {
                    role,
                    content: if content.is_empty() {
                        None
                    } else {
                        Some(content)
                    },
                    tool_calls,
                    tool_call_id: None,
                    reasoning: reasoning_raw
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(|value| value.to_string()),
                });
            }
            "tool" => {
                let tool_call_id = tool_calls_raw
                    .as_deref()
                    .and_then(|value| serde_json::from_str::<Value>(value).ok())
                    .and_then(|value| {
                        value
                            .get("tool_call_id")
                            .and_then(|item| item.as_str())
                            .map(|item| item.to_string())
                    });

                messages.push(ChatMessage {
                    role,
                    content: Some(content),
                    tool_calls: None,
                    tool_call_id,
                    reasoning: None,
                });
            }
            _ => {
                messages.push(ChatMessage {
                    role,
                    content: Some(content),
                    tool_calls: None,
                    tool_call_id: None,
                    reasoning: None,
                });
            }
        }
    }

    Ok(messages)
}

async fn resolve_conversation_model(
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
enum TextModelProvider {
    Glm,
    Doubao,
    MiniMax,
}

fn detect_text_model_provider(model: &str) -> TextModelProvider {
    let normalized = model.trim().to_ascii_lowercase();
    if normalized.starts_with("minimax-") {
        return TextModelProvider::MiniMax;
    }
    if normalized.starts_with("doubao-") {
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
    values.into_iter().flatten().find(|value| !value.trim().is_empty())
}

fn resolve_text_llm_service(config: &Config, model: &str) -> Result<LlmService, String> {
    match detect_text_model_provider(model) {
        TextModelProvider::Glm => {
            let api_key = first_non_empty(vec![
                config.api_key.clone(),
                env_value("GLM_API_KEY"),
                env_value("OPENAI_API_KEY"),
            ])
            .ok_or_else(|| "GLM API key not set".to_string())?;
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
            .ok_or_else(|| "Doubao API key not set".to_string())?;
            Ok(LlmService::new(
                api_key,
                Some(DEFAULT_ARK_API_BASE.to_string()),
            ))
        }
        TextModelProvider::MiniMax => {
            let api_key = first_non_empty(vec![
                config.minimax_api_key.clone(),
                env_value("MINIMAX_API_KEY"),
                env_value("ANTHROPIC_API_KEY"),
            ])
            .ok_or_else(|| "MiniMax API key not set".to_string())?;
            Ok(LlmService::new(
                api_key,
                Some(DEFAULT_MINIMAX_ANTHROPIC_API_BASE.to_string()),
            ))
        }
    }
}

fn resolve_image_generation_llm_service(config: &Config) -> Result<LlmService, String> {
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

fn prepend_system_prompt(messages: &mut Vec<ChatMessage>, system_prompt: Option<&str>) {
    let Some(system_prompt) = system_prompt else {
        return;
    };

    let trimmed = system_prompt.trim();
    if trimmed.is_empty() {
        return;
    }

    messages.insert(
        0,
        ChatMessage {
            role: "system".to_string(),
            content: Some(trimmed.to_string()),
            tool_calls: None,
            tool_call_id: None,
            reasoning: None,
        },
    );
}

fn prepend_tool_usage_guidance(messages: &mut Vec<ChatMessage>) {
    let guidance = if cfg!(target_os = "windows") {
        "Tool selection policy (Windows): \
Prefer workspace_run_command for filesystem-heavy tasks such as recursive traversal, counting files, computing folder size, extension/type statistics, sorting/filtering large file lists, and bulk inventory. \
Use concise PowerShell commands for these operations (for example: Get-ChildItem -Recurse -File | Measure-Object, or Get-ChildItem -Recurse | Measure-Object -Property Length -Sum). \
Use workspace_list_directory only for quick non-recursive inspection of one directory level or when the user explicitly asks to browse items manually."
    } else {
        "Tool selection policy: \
Prefer workspace_run_command for filesystem-heavy tasks such as recursive traversal, counting files, computing folder size, extension/type statistics, sorting/filtering large file lists, and bulk inventory. \
Use shell pipelines for these operations. \
Use workspace_list_directory only for quick non-recursive inspection of one directory level or when the user explicitly asks to browse items manually."
    };

    messages.insert(
        0,
        ChatMessage {
            role: "system".to_string(),
            content: Some(guidance.to_string()),
            tool_calls: None,
            tool_call_id: None,
            reasoning: None,
        },
    );
}

fn prepend_skills_usage_guidance(messages: &mut Vec<ChatMessage>, skills_guidance: &str) {
    let trimmed = skills_guidance.trim();
    if trimmed.is_empty() {
        return;
    }

    messages.insert(
        0,
        ChatMessage {
            role: "system".to_string(),
            content: Some(trimmed.to_string()),
            tool_calls: None,
            tool_call_id: None,
            reasoning: None,
        },
    );
}

async fn build_skills_usage_guidance(skill_manager_state: &SkillManagerState) -> String {
    let manager = skill_manager_state.lock().await;
    let mut skills = manager.list_skills();
    skills.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));

    let mut lines = vec![
        "Skills-first policy (mandatory when uncertain):".to_string(),
        "- Before implementing an unfamiliar/specialized workflow, call `skills_list` first.".to_string(),
        "- If exactly one skill clearly matches, call `skills_execute` and follow it.".to_string(),
        "- If multiple skills could match, pick the most specific one and execute it.".to_string(),
        "- If no suitable installed skill exists, call `skills_discover` with a focused query."
            .to_string(),
        "- Install with `skills_install_from_repo` (and optional `skill_path`) when discovery returns a suitable result."
            .to_string(),
        "- Only if no suitable skill can be found/installed, continue with direct implementation."
            .to_string(),
    ];

    if skills.is_empty() {
        lines.push("Installed skills: (none)".to_string());
    } else {
        lines.push("<available_skills>".to_string());
        for skill in skills {
            lines.push(format!("- {}: {}", skill.name, skill.description.trim()));
        }
        lines.push("</available_skills>".to_string());
    }

    lines.join("\n")
}

fn resolve_clawhub_settings_for_discovery() -> (Option<String>, Option<String>) {
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

async fn build_runtime_tool_catalog(
    mcp_state: &McpState,
    config: &Config,
    workspace_root: &Path,
) -> Result<RuntimeToolCatalog, String> {
    ensure_mcp_servers_connected(mcp_state, config).await?;

    let (mcp_tools, mcp_tool_map) = collect_mcp_tools(mcp_state).await;
    let (workspace_tools, workspace_tool_map) = collect_workspace_tools(workspace_root);
    let (skill_tools, skill_tool_map) = collect_skill_tools();
    let (core_tools, core_tool_map) = collect_core_tools();

    let mut available_tools = workspace_tools;
    available_tools.extend(mcp_tools);
    available_tools.extend(skill_tools);
    available_tools.extend(core_tools);

    let mut tool_map = workspace_tool_map;
    tool_map.extend(mcp_tool_map);
    tool_map.extend(skill_tool_map);
    tool_map.extend(core_tool_map);

    Ok(RuntimeToolCatalog {
        available_tools,
        tool_map,
    })
}

async fn run_stream_round(
    llm_service: &LlmService,
    window: &Window,
    conversation_id: &str,
    model: &str,
    context_messages: Vec<ChatMessage>,
    available_tools: &[ChatTool],
    stop_flag: Arc<AtomicBool>,
) -> Result<LlmStreamResult, String> {
    let conversation_id_for_stream = conversation_id.to_string();
    let window_for_stream = window.clone();
    llm_service
        .chat_stream_with_tools(
            model,
            context_messages,
            if available_tools.is_empty() {
                None
            } else {
                Some(available_tools.to_vec())
            },
            move |event| match event {
                LlmStreamEvent::Content(chunk) => {
                    let _ = window_for_stream.emit(
                        "chat-chunk",
                        json!({
                            "conversationId": conversation_id_for_stream.clone(),
                            "chunk": chunk
                        }),
                    );
                }
                LlmStreamEvent::Reasoning(chunk) => {
                    let _ = window_for_stream.emit(
                        "chat-reasoning",
                        json!({
                            "conversationId": conversation_id_for_stream.clone(),
                            "chunk": chunk
                        }),
                    );
                }
                LlmStreamEvent::ToolCallDelta(delta) => {
                    let payload = json!({
                        "conversationId": conversation_id_for_stream.clone(),
                        "index": delta.index,
                        "toolCallId": delta.id,
                        "name": delta.name,
                        "argumentsChunk": delta.arguments_chunk,
                    });
                    let _ = window_for_stream.emit("chat-tool-call", payload);
                }
            },
            move || stop_flag.load(Ordering::Relaxed),
        )
        .await
        .map_err(|e| e.to_string())
}

fn emit_tool_result_event(
    window: &Window,
    conversation_id: &str,
    tool_call: &ChatToolCall,
    result: Option<&str>,
    error: Option<&str>,
) -> Result<(), String> {
    window
        .emit(
            "chat-tool-result",
            json!({
                "conversationId": conversation_id,
                "toolCallId": &tool_call.id,
                "name": &tool_call.function.name,
                "result": result,
                "error": error,
            }),
        )
        .map_err(|e| e.to_string())
}

fn build_tool_call_metadata(tool_call: &ChatToolCall) -> Result<String, String> {
    serde_json::to_string(&json!({
        "tool_call_id": &tool_call.id,
        "tool_name": &tool_call.function.name
    }))
    .map_err(|e| e.to_string())
}

fn format_tool_error_result(error_text: &str) -> Result<String, String> {
    serde_json::to_string_pretty(&json!({
        "error": error_text
    }))
    .map_err(|e| e.to_string())
}

async fn persist_tool_result_message(
    pool: &SqlitePool,
    conversation_id: &str,
    context_messages: &mut Vec<ChatMessage>,
    tool_call: &ChatToolCall,
    result_text: String,
) -> Result<(), String> {
    let metadata = build_tool_call_metadata(tool_call)?;
    insert_message(
        pool,
        conversation_id,
        "tool",
        &result_text,
        Some(metadata),
        None,
    )
    .await?;

    context_messages.push(ChatMessage {
        role: "tool".to_string(),
        content: Some(result_text),
        tool_calls: None,
        tool_call_id: Some(tool_call.id.clone()),
        reasoning: None,
    });

    Ok(())
}

async fn resolve_tool_execution_decision(
    config: &Config,
    window: &Window,
    conversation_id: &str,
    tool_call: &ChatToolCall,
    parsed_arguments: &Value,
    always_allowed_tools: &HashSet<String>,
) -> Result<ToolApprovalDecision, String> {
    let configured_action = resolve_tool_permission_action(
        config,
        &tool_call.function.name,
        extract_tool_path_argument(parsed_arguments).as_deref(),
    );

    if config.auto_approve_tool_requests && configured_action != ToolPermissionAction::Deny {
        return Ok(ToolApprovalDecision::AllowAlways);
    }

    Ok(match configured_action {
        ToolPermissionAction::Deny => ToolApprovalDecision::Deny,
        ToolPermissionAction::Allow => ToolApprovalDecision::AllowAlways,
        ToolPermissionAction::Ask => {
            if always_allowed_tools.contains(&tool_call.function.name) {
                ToolApprovalDecision::AllowAlways
            } else if tool_call.function.name == CORE_BATCH_TOOL
                && batch_call_targets_are_safe(parsed_arguments)
            {
                ToolApprovalDecision::AllowOnce
            } else {
                request_tool_approval(window, conversation_id, tool_call).await?
            }
        }
    })
}

fn build_tool_round_signature(tool_calls: &[ChatToolCall]) -> String {
    serde_json::to_string(
        &tool_calls
            .iter()
            .map(|call| (call.function.name.clone(), call.function.arguments.clone()))
            .collect::<Vec<_>>(),
    )
    .unwrap_or_default()
}

fn update_repeated_signature_rounds(
    last_tool_signature: &mut Option<String>,
    repeated_signature_rounds: &mut usize,
    current_signature: String,
) {
    if last_tool_signature.as_ref() == Some(&current_signature) {
        *repeated_signature_rounds += 1;
    } else {
        *repeated_signature_rounds = 0;
    }

    *last_tool_signature = Some(current_signature);
}

async fn ensure_mcp_servers_connected(mcp_state: &McpState, config: &Config) -> Result<(), String> {
    let mut manager = mcp_state.lock().await;

    for server in config.mcp_servers.iter().filter(|server| server.enabled) {
        if manager.get_client(&server.name).is_some() {
            continue;
        }

        let transport: Box<dyn crate::services::mcp_client::McpTransport> = match &server.transport
        {
            McpTransport::Stdio { command, args } => {
                Box::new(StdioTransport::new(command, args).map_err(|e| e.to_string())?)
            }
            McpTransport::Http { url } => Box::new(HttpTransport::new(url.clone())),
        };

        let client = McpClient::new(server.name.clone(), transport)
            .await
            .map_err(|e| e.to_string())?;

        manager
            .add_client(server.name.clone(), client)
            .await
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

async fn collect_mcp_tools(mcp_state: &McpState) -> (Vec<ChatTool>, HashMap<String, RuntimeTool>) {
    let manager = mcp_state.lock().await;
    let mut tools = Vec::new();
    let mut tool_map = HashMap::new();

    for (server_name, client) in manager.list_clients() {
        for tool in client.list_tools() {
            let mut alias = build_tool_alias(&server_name, &tool.name);
            let mut collision_index = 1usize;

            while tool_map.contains_key(&alias) {
                alias = format!(
                    "{}_{}",
                    build_tool_alias(&server_name, &tool.name),
                    collision_index
                );
                collision_index += 1;
            }

            let parameters = if tool.input_schema.is_object() {
                tool.input_schema.clone()
            } else {
                json!({
                    "type": "object",
                    "properties": {}
                })
            };

            tools.push(ChatTool {
                tool_type: "function".to_string(),
                function: ChatToolFunction {
                    name: alias.clone(),
                    description: if tool.description.is_empty() {
                        format!("MCP tool {} from {}", tool.name, server_name)
                    } else {
                        tool.description.clone()
                    },
                    parameters,
                },
            });

            tool_map.insert(
                alias,
                RuntimeTool::Mcp {
                    server_name: server_name.clone(),
                    tool_name: tool.name.clone(),
                },
            );
        }
    }

    (tools, tool_map)
}

fn resolve_workspace_root(
    config: &Config,
    workspace_override: Option<&str>,
) -> Result<PathBuf, String> {
    let override_dir = workspace_override
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let configured = override_dir.or_else(|| {
        config
            .work_directory
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    });

    let configured_path = configured.ok_or_else(|| {
        "Workspace directory is not set. Please choose one in 新冒险 or set default Work Directory in Settings."
            .to_string()
    })?;

    let root = {
        let candidate = PathBuf::from(configured_path);
        if !candidate.exists() || !candidate.is_dir() {
            return Err(format!(
                "Configured work_directory does not exist or is not a directory: {}",
                configured_path
            ));
        }
        candidate
    };

    root.canonicalize().map_err(|e| e.to_string())
}

fn collect_workspace_tools(workspace_root: &Path) -> (Vec<ChatTool>, HashMap<String, RuntimeTool>) {
    tool_catalog::collect_workspace_tools(workspace_root)
}

fn collect_skill_tools() -> (Vec<ChatTool>, HashMap<String, RuntimeTool>) {
    tool_catalog::collect_skill_tools()
}

fn collect_core_tools() -> (Vec<ChatTool>, HashMap<String, RuntimeTool>) {
    tool_catalog::collect_core_tools()
}

fn parse_tool_arguments(raw: &str) -> Value {
    if raw.trim().is_empty() {
        return json!({});
    }

    serde_json::from_str::<Value>(raw).unwrap_or_else(|_| {
        json!({
            "raw_arguments": raw
        })
    })
}

fn read_string_argument(arguments: &Value, key: &str) -> Result<String, String> {
    arguments
        .get(key)
        .and_then(|item| item.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("'{}' is required", key))
}

fn read_path_argument(arguments: &Value, key: &str) -> Result<String, String> {
    read_string_argument(arguments, key)
}

fn read_optional_string_argument(arguments: &Value, key: &str) -> Option<String> {
    arguments
        .get(key)
        .and_then(|item| item.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn read_bool_argument(arguments: &Value, key: &str, default_value: bool) -> bool {
    arguments
        .get(key)
        .and_then(Value::as_bool)
        .unwrap_or(default_value)
}

fn read_u64_argument(arguments: &Value, key: &str, default_value: u64) -> u64 {
    arguments
        .get(key)
        .and_then(Value::as_u64)
        .unwrap_or(default_value)
}

fn normalize_lexical_path(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => result.push(prefix.as_os_str()),
            Component::RootDir => result.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            Component::Normal(value) => result.push(value),
        }
    }
    result
}

fn resolve_workspace_target(
    workspace_root: &Path,
    raw_path: &str,
    allow_missing: bool,
) -> Result<PathBuf, String> {
    let canonical_root = workspace_root.canonicalize().map_err(|e| e.to_string())?;
    let requested_path = {
        let candidate = PathBuf::from(raw_path);
        if candidate.is_absolute() {
            candidate
        } else {
            canonical_root.join(candidate)
        }
    };

    if requested_path.exists() {
        let canonical = requested_path.canonicalize().map_err(|e| e.to_string())?;
        if !canonical.starts_with(&canonical_root) {
            return Err("Path is outside workspace root".to_string());
        }
        return Ok(canonical);
    }

    if !allow_missing {
        return Err(format!("Path does not exist: {}", requested_path.display()));
    }

    let normalized = normalize_lexical_path(&requested_path);
    if !normalized.starts_with(&canonical_root) {
        return Err("Path is outside workspace root".to_string());
    }

    Ok(normalized)
}

fn compile_glob_set(patterns: &[String]) -> Result<GlobSet, String> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let glob = GlobBuilder::new(pattern)
            .literal_separator(false)
            .build()
            .map_err(|e| format!("Invalid glob '{}': {}", pattern, e))?;
        builder.add(glob);
    }
    builder
        .build()
        .map_err(|e| format!("Failed to build glob matcher: {}", e))
}

fn wildcard_match(pattern: &str, value: &str) -> bool {
    let mut regex_pattern = String::from("^");
    for ch in pattern.chars() {
        match ch {
            '*' => regex_pattern.push_str(".*"),
            '?' => regex_pattern.push('.'),
            _ => regex_pattern.push_str(&regex::escape(&ch.to_string())),
        }
    }
    regex_pattern.push('$');
    Regex::new(&regex_pattern)
        .map(|regex| regex.is_match(value))
        .unwrap_or(false)
}

fn normalize_tool_name_for_permission(tool_name: &str) -> &str {
    if tool_name.starts_with("mcp__") {
        return "mcp__*";
    }
    tool_name
}

fn evaluate_tool_rule_match_score(tool_pattern: &str, tool_name: &str) -> Option<usize> {
    if wildcard_match(tool_pattern, tool_name) {
        return Some(tool_pattern.len());
    }

    if tool_pattern == normalize_tool_name_for_permission(tool_name) {
        return Some(tool_pattern.len());
    }

    None
}

fn resolve_tool_permission_action(
    config: &Config,
    tool_name: &str,
    path_candidate: Option<&str>,
) -> ToolPermissionAction {
    let mut decision = ToolPermissionAction::Ask;
    let mut best_match = 0usize;

    for (pattern, action) in &config.tool_permissions {
        if let Some(score) = evaluate_tool_rule_match_score(pattern, tool_name) {
            if score >= best_match {
                best_match = score;
                decision = action.clone();
            }
        }
    }

    if let Some(path_value) = path_candidate {
        for ToolPathPermissionRule {
            tool_pattern,
            path_pattern,
            action,
        } in &config.tool_path_permissions
        {
            if evaluate_tool_rule_match_score(tool_pattern, tool_name).is_some()
                && wildcard_match(path_pattern, path_value)
            {
                decision = action.clone();
            }
        }
    }

    decision
}

fn extract_tool_path_argument(arguments: &Value) -> Option<String> {
    read_optional_string_argument(arguments, "path")
        .or_else(|| read_optional_string_argument(arguments, "cwd"))
        .or_else(|| read_optional_string_argument(arguments, "directory"))
}

fn should_auto_allow_batch_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        WORKSPACE_LIST_TOOL
            | WORKSPACE_READ_TOOL
            | WORKSPACE_GLOB_TOOL
            | WORKSPACE_GREP_TOOL
            | WORKSPACE_CODESEARCH_TOOL
            | WORKSPACE_LSP_SYMBOLS_TOOL
            | TODO_READ_TOOL
            | TODO_WRITE_TOOL
            | WEB_FETCH_TOOL
            | WEB_SEARCH_TOOL
            | IMAGE_PROBE_TOOL
            | IMAGE_UNDERSTAND_TOOL
            | SESSIONS_LIST_TOOL
            | SESSIONS_HISTORY_TOOL
            | AGENTS_LIST_TOOL
            | SKILL_DISCOVER_TOOL
            | SKILL_LIST_TOOL
    )
}

fn batch_call_targets_are_safe(arguments: &Value) -> bool {
    arguments
        .get("tool_calls")
        .and_then(Value::as_array)
        .map(|calls| {
            !calls.is_empty()
                && calls.iter().all(|call| {
                    let tool_name = call
                        .get("tool")
                        .or_else(|| call.get("name"))
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .unwrap_or_default();
                    !tool_name.is_empty() && should_auto_allow_batch_tool(tool_name)
                })
        })
        .unwrap_or(false)
}

fn is_forbidden_loopback_host(url: &reqwest::Url) -> bool {
    let Some(host) = url.host_str() else {
        return true;
    };

    if host.eq_ignore_ascii_case("localhost") || host.ends_with(".local") {
        return true;
    }

    if let Ok(ip) = host.parse::<IpAddr>() {
        return match ip {
            IpAddr::V4(ipv4) => {
                ipv4.is_private()
                    || ipv4.is_loopback()
                    || ipv4.is_link_local()
                    || ipv4.is_broadcast()
                    || ipv4.is_unspecified()
            }
            IpAddr::V6(ipv6) => {
                ipv6.is_loopback()
                    || ipv6.is_unique_local()
                    || ipv6.is_unspecified()
                    || ipv6.is_multicast()
            }
        };
    }

    false
}

fn execute_workspace_list_directory(
    arguments: &Value,
    workspace_root: &Path,
) -> Result<Value, String> {
    let raw_path = arguments
        .get("path")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(".");

    let directory = resolve_workspace_target(workspace_root, raw_path, false)?;
    if !directory.is_dir() {
        return Err(format!("Not a directory: {}", directory.display()));
    }

    let mut items = Vec::new();
    let entries = std::fs::read_dir(&directory).map_err(|e| e.to_string())?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let metadata = entry.metadata().map_err(|e| e.to_string())?;
        items.push(json!({
            "name": entry.file_name().to_string_lossy().to_string(),
            "path": entry.path().to_string_lossy().to_string(),
            "is_dir": metadata.is_dir(),
            "size": if metadata.is_file() { Some(metadata.len()) } else { None::<u64> }
        }));
    }

    items.sort_by(|left, right| {
        let left_is_dir = left.get("is_dir").and_then(Value::as_bool).unwrap_or(false);
        let right_is_dir = right
            .get("is_dir")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        match (left_is_dir, right_is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => {
                let left_name = left.get("name").and_then(Value::as_str).unwrap_or_default();
                let right_name = right
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                left_name.cmp(right_name)
            }
        }
    });

    Ok(json!({
        "workspace_root": workspace_root.to_string_lossy().to_string(),
        "directory": directory.to_string_lossy().to_string(),
        "entries": items
    }))
}

fn execute_workspace_read_file(arguments: &Value, workspace_root: &Path) -> Result<Value, String> {
    let raw_path = read_path_argument(arguments, "path")?;
    let file_path = resolve_workspace_target(workspace_root, &raw_path, false)?;
    if !file_path.is_file() {
        return Err(format!("Not a file: {}", file_path.display()));
    }

    let max_bytes = read_u64_argument(arguments, "max_bytes", 200_000).clamp(1, 2_000_000) as usize;
    let offset_bytes = read_u64_argument(arguments, "offset_bytes", 0) as usize;
    let max_lines = arguments
        .get("max_lines")
        .and_then(Value::as_u64)
        .map(|value| value.clamp(1, 200_000) as usize);

    let bytes = fs::read(&file_path).map_err(|e| e.to_string())?;
    let total_bytes = bytes.len();
    let start = offset_bytes.min(total_bytes);
    let end = (start + max_bytes).min(total_bytes);
    let mut content = String::from_utf8_lossy(&bytes[start..end]).to_string();
    let mut lines_truncated = false;
    if let Some(max_lines) = max_lines {
        let lines: Vec<&str> = content.lines().collect();
        if lines.len() > max_lines {
            content = lines[..max_lines].join("\n");
            lines_truncated = true;
        }
    }

    Ok(json!({
        "workspace_root": workspace_root.to_string_lossy().to_string(),
        "path": file_path.to_string_lossy().to_string(),
        "content": content,
        "offset_bytes": start,
        "bytes_read": end.saturating_sub(start),
        "total_bytes": total_bytes,
        "truncated": end < total_bytes || lines_truncated
    }))
}

fn write_file_atomic(path: &Path, content: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "Invalid target path".to_string())?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let temp_name = format!(
        ".{}.tmp-{}",
        path.file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("petool"),
        Uuid::new_v4()
    );
    let temp_path = parent.join(temp_name);
    fs::write(&temp_path, content).map_err(|e| e.to_string())?;
    match fs::rename(&temp_path, path) {
        Ok(_) => Ok(()),
        Err(rename_error) => {
            fs::copy(&temp_path, path).map_err(|copy_error| {
                format!(
                    "Failed to atomically write file (rename: {}; copy: {})",
                    rename_error, copy_error
                )
            })?;
            let _ = fs::remove_file(&temp_path);
            Ok(())
        }
    }
}

fn execute_workspace_write_file(arguments: &Value, workspace_root: &Path) -> Result<Value, String> {
    let raw_path = read_path_argument(arguments, "path")?;
    let content = arguments
        .get("content")
        .and_then(Value::as_str)
        .ok_or_else(|| "'content' is required".to_string())?
        .to_string();
    let append = arguments
        .get("append")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let file_path = resolve_workspace_target(workspace_root, &raw_path, true)?;

    if append {
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file_path)
            .map_err(|e| e.to_string())?;
        file.write_all(content.as_bytes())
            .map_err(|e| e.to_string())?;
    } else {
        write_file_atomic(&file_path, content.as_bytes())?;
    }

    let metadata = fs::metadata(&file_path).map_err(|e| e.to_string())?;
    Ok(json!({
        "workspace_root": workspace_root.to_string_lossy().to_string(),
        "path": file_path.to_string_lossy().to_string(),
        "bytes_written": metadata.len(),
        "append": append
    }))
}

fn execute_workspace_edit_file(arguments: &Value, workspace_root: &Path) -> Result<Value, String> {
    let raw_path = read_path_argument(arguments, "path")?;
    let old_string = read_string_argument(arguments, "old_string")?;
    let new_string = arguments
        .get("new_string")
        .and_then(Value::as_str)
        .ok_or_else(|| "'new_string' is required".to_string())?
        .to_string();
    let replace_all = read_bool_argument(arguments, "replace_all", false);

    let file_path = resolve_workspace_target(workspace_root, &raw_path, false)?;
    if !file_path.is_file() {
        return Err(format!("Not a file: {}", file_path.display()));
    }

    let content = fs::read_to_string(&file_path).map_err(|e| e.to_string())?;
    let updated = if replace_all {
        content.replace(&old_string, &new_string)
    } else {
        content.replacen(&old_string, &new_string, 1)
    };

    if updated == content {
        return Err("No matching content found to edit".to_string());
    }

    write_file_atomic(&file_path, updated.as_bytes())?;

    Ok(json!({
        "workspace_root": workspace_root.to_string_lossy().to_string(),
        "path": file_path.to_string_lossy().to_string(),
        "replace_all": replace_all,
        "status": "updated"
    }))
}

async fn execute_workspace_run_command(
    arguments: &Value,
    workspace_root: &Path,
) -> Result<Value, String> {
    let command = arguments
        .get("command")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "'command' is required".to_string())?
        .to_string();

    let timeout_ms = read_u64_argument(arguments, "timeout_ms", 20_000).clamp(1_000, 120_000);

    let mut cmd = if cfg!(target_os = "windows") {
        let mut process = TokioCommand::new("powershell");
        process.args(["-NoProfile", "-Command", &command]);
        process
    } else {
        let mut process = TokioCommand::new("sh");
        process.args(["-lc", &command]);
        process
    };

    cmd.current_dir(workspace_root);
    let output = tokio::time::timeout(Duration::from_millis(timeout_ms), cmd.output())
        .await
        .map_err(|_| format!("Command timed out after {} ms", timeout_ms))?
        .map_err(|e| e.to_string())?;

    Ok(json!({
        "workspace_root": workspace_root.to_string_lossy().to_string(),
        "command": command,
        "timeout_ms": timeout_ms,
        "exit_code": output.status.code(),
        "success": output.status.success(),
        "stdout": String::from_utf8_lossy(&output.stdout).to_string(),
        "stderr": String::from_utf8_lossy(&output.stderr).to_string()
    }))
}

fn workspace_relative_display_path(workspace_root: &Path, path: &Path) -> String {
    path.strip_prefix(workspace_root)
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|_| path.to_string_lossy().to_string())
}

fn is_probably_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8192).any(|byte| *byte == 0)
}

fn collect_workspace_files(
    workspace_root: &Path,
    base_directory: &Path,
    glob_pattern: Option<&str>,
    max_files: usize,
) -> Result<Vec<PathBuf>, String> {
    let patterns = if let Some(pattern) = glob_pattern {
        vec![pattern.to_string()]
    } else {
        vec!["**/*".to_string()]
    };
    let glob_set = compile_glob_set(&patterns)?;
    let mut results = Vec::new();

    for entry in WalkDir::new(base_directory).follow_links(false) {
        let entry = entry.map_err(|e| e.to_string())?;
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path().to_path_buf();
        let relative_to_root = workspace_relative_display_path(workspace_root, &path);
        if glob_set.is_match(relative_to_root.as_str()) {
            results.push(path);
            if results.len() >= max_files {
                break;
            }
        }
    }

    Ok(results)
}

fn execute_workspace_glob(arguments: &Value, workspace_root: &Path) -> Result<Value, String> {
    let pattern = read_string_argument(arguments, "pattern")?;
    let raw_path =
        read_optional_string_argument(arguments, "path").unwrap_or_else(|| ".".to_string());
    let max_results = read_u64_argument(arguments, "max_results", 200).clamp(1, 5_000) as usize;
    let include_directories = read_bool_argument(arguments, "include_directories", false);
    let base_dir = resolve_workspace_target(workspace_root, &raw_path, false)?;
    if !base_dir.is_dir() {
        return Err(format!("Not a directory: {}", base_dir.display()));
    }

    let glob_set = compile_glob_set(&[pattern])?;
    let mut matches = Vec::new();
    for entry in WalkDir::new(&base_dir).follow_links(false) {
        let entry = entry.map_err(|e| e.to_string())?;
        if !include_directories && !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path().to_path_buf();
        let relative = workspace_relative_display_path(workspace_root, &path);
        if glob_set.is_match(relative.as_str()) {
            let metadata = fs::metadata(&path).map_err(|e| e.to_string())?;
            matches.push(json!({
                "path": relative,
                "absolute_path": path.to_string_lossy().to_string(),
                "is_dir": metadata.is_dir(),
                "size": if metadata.is_file() { Some(metadata.len()) } else { None::<u64> }
            }));
            if matches.len() >= max_results {
                break;
            }
        }
    }

    Ok(json!({
        "workspace_root": workspace_root.to_string_lossy().to_string(),
        "base_directory": base_dir.to_string_lossy().to_string(),
        "matches": matches,
        "truncated": matches.len() >= max_results
    }))
}

fn build_search_regex(
    pattern: &str,
    use_regex: bool,
    case_sensitive: bool,
) -> Result<Regex, String> {
    let source = if use_regex {
        pattern.to_string()
    } else {
        regex::escape(pattern)
    };
    RegexBuilder::new(&source)
        .case_insensitive(!case_sensitive)
        .build()
        .map_err(|e| format!("Invalid search pattern: {}", e))
}

fn execute_workspace_grep(arguments: &Value, workspace_root: &Path) -> Result<Value, String> {
    let pattern = read_string_argument(arguments, "pattern")?;
    let use_regex = read_bool_argument(arguments, "regex", false);
    let case_sensitive = read_bool_argument(arguments, "case_sensitive", false);
    let max_results = read_u64_argument(arguments, "max_results", 200).clamp(1, 5_000) as usize;
    let raw_path =
        read_optional_string_argument(arguments, "path").unwrap_or_else(|| ".".to_string());
    let glob = read_optional_string_argument(arguments, "glob");
    let file_max_bytes = read_u64_argument(arguments, "file_max_bytes", 2_000_000) as usize;
    let regex = build_search_regex(&pattern, use_regex, case_sensitive)?;
    let base_dir = resolve_workspace_target(workspace_root, &raw_path, false)?;
    if !base_dir.is_dir() {
        return Err(format!("Not a directory: {}", base_dir.display()));
    }

    let files = collect_workspace_files(workspace_root, &base_dir, glob.as_deref(), 20_000)?;
    let mut matches = Vec::new();

    for file_path in files {
        let bytes = fs::read(&file_path).map_err(|e| e.to_string())?;
        if bytes.is_empty() || is_probably_binary(&bytes) {
            continue;
        }
        if bytes.len() > file_max_bytes {
            continue;
        }

        let text = String::from_utf8_lossy(&bytes);
        for (index, line) in text.lines().enumerate() {
            if regex.is_match(line) {
                matches.push(json!({
                    "path": workspace_relative_display_path(workspace_root, &file_path),
                    "line_number": index + 1,
                    "line": line
                }));
                if matches.len() >= max_results {
                    break;
                }
            }
        }

        if matches.len() >= max_results {
            break;
        }
    }

    Ok(json!({
        "pattern": pattern,
        "regex": use_regex,
        "case_sensitive": case_sensitive,
        "matches": matches,
        "truncated": matches.len() >= max_results
    }))
}

fn execute_workspace_codesearch(arguments: &Value, workspace_root: &Path) -> Result<Value, String> {
    let query = read_string_argument(arguments, "query")?;
    let raw_path =
        read_optional_string_argument(arguments, "path").unwrap_or_else(|| ".".to_string());
    let glob = read_optional_string_argument(arguments, "glob");
    let context_lines = read_u64_argument(arguments, "context_lines", 2).clamp(0, 20) as usize;
    let max_results = read_u64_argument(arguments, "max_results", 100).clamp(1, 2_000) as usize;
    let regex = build_search_regex(&query, false, false)?;
    let base_dir = resolve_workspace_target(workspace_root, &raw_path, false)?;
    if !base_dir.is_dir() {
        return Err(format!("Not a directory: {}", base_dir.display()));
    }

    let files = collect_workspace_files(workspace_root, &base_dir, glob.as_deref(), 20_000)?;
    let mut matches = Vec::new();

    for file_path in files {
        let bytes = fs::read(&file_path).map_err(|e| e.to_string())?;
        if bytes.is_empty() || is_probably_binary(&bytes) {
            continue;
        }

        let text = String::from_utf8_lossy(&bytes);
        let lines: Vec<&str> = text.lines().collect();
        for index in 0..lines.len() {
            if regex.is_match(lines[index]) {
                let start = index.saturating_sub(context_lines);
                let end = (index + context_lines + 1).min(lines.len());
                let snippet = lines[start..end]
                    .iter()
                    .enumerate()
                    .map(|(offset, line)| format!("{}: {}", start + offset + 1, line))
                    .collect::<Vec<_>>()
                    .join("\n");
                matches.push(json!({
                    "path": workspace_relative_display_path(workspace_root, &file_path),
                    "line_number": index + 1,
                    "snippet": snippet
                }));
                if matches.len() >= max_results {
                    break;
                }
            }
        }
        if matches.len() >= max_results {
            break;
        }
    }

    Ok(json!({
        "query": query,
        "results": matches,
        "truncated": matches.len() >= max_results
    }))
}

fn execute_workspace_lsp_symbols(
    arguments: &Value,
    workspace_root: &Path,
) -> Result<Value, String> {
    let raw_path =
        read_optional_string_argument(arguments, "path").unwrap_or_else(|| ".".to_string());
    let query = read_optional_string_argument(arguments, "query")
        .map(|value| value.to_lowercase())
        .unwrap_or_default();
    let max_results = read_u64_argument(arguments, "max_results", 200).clamp(1, 2_000) as usize;
    let base_dir = resolve_workspace_target(workspace_root, &raw_path, false)?;
    if !base_dir.is_dir() {
        return Err(format!("Not a directory: {}", base_dir.display()));
    }

    let symbol_patterns = vec![
        (
            "function",
            Regex::new(r"^\s*(?:pub\s+)?(?:async\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)")
                .map_err(|e| e.to_string())?,
        ),
        (
            "class",
            Regex::new(r"^\s*class\s+([A-Za-z_][A-Za-z0-9_]*)").map_err(|e| e.to_string())?,
        ),
        (
            "type",
            Regex::new(
                r"^\s*(?:export\s+)?(?:interface|type|struct|enum)\s+([A-Za-z_][A-Za-z0-9_]*)",
            )
            .map_err(|e| e.to_string())?,
        ),
    ];

    let files = collect_workspace_files(workspace_root, &base_dir, None, 40_000)?;
    let mut symbols = Vec::new();

    for file_path in files {
        let relative = workspace_relative_display_path(workspace_root, &file_path);
        let extension = file_path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_lowercase();
        if !matches!(
            extension.as_str(),
            "rs" | "ts"
                | "tsx"
                | "js"
                | "jsx"
                | "py"
                | "go"
                | "java"
                | "c"
                | "cpp"
                | "h"
                | "hpp"
                | "vue"
        ) {
            continue;
        }

        let bytes = fs::read(&file_path).map_err(|e| e.to_string())?;
        if bytes.is_empty() || is_probably_binary(&bytes) {
            continue;
        }

        let text = String::from_utf8_lossy(&bytes);
        for (line_idx, line) in text.lines().enumerate() {
            for (kind, regex) in &symbol_patterns {
                if let Some(captures) = regex.captures(line) {
                    let symbol_name = captures
                        .get(1)
                        .map(|value| value.as_str())
                        .unwrap_or_default()
                        .to_string();
                    if !query.is_empty() && !symbol_name.to_lowercase().contains(&query) {
                        continue;
                    }
                    symbols.push(json!({
                        "name": symbol_name,
                        "kind": kind,
                        "path": relative,
                        "line_number": line_idx + 1
                    }));
                    if symbols.len() >= max_results {
                        break;
                    }
                }
            }
            if symbols.len() >= max_results {
                break;
            }
        }
        if symbols.len() >= max_results {
            break;
        }
    }

    Ok(json!({
        "query": query,
        "symbols": symbols,
        "truncated": symbols.len() >= max_results
    }))
}

#[derive(Debug)]
enum PatchOperation {
    Add {
        path: String,
        lines: Vec<String>,
    },
    Delete {
        path: String,
    },
    Update {
        path: String,
        move_to: Option<String>,
        hunks: Vec<Vec<String>>,
    },
}

fn parse_patch_envelope(patch: &str) -> Result<Vec<PatchOperation>, String> {
    let lines: Vec<&str> = patch.lines().collect();
    if lines.is_empty() || lines.first().copied() != Some("*** Begin Patch") {
        return Err("Patch must start with '*** Begin Patch'".to_string());
    }

    let mut index = 1usize;
    let mut operations = Vec::new();

    while index < lines.len() {
        let line = lines[index];
        if line == "*** End Patch" {
            return Ok(operations);
        }

        if let Some(path) = line.strip_prefix("*** Add File: ") {
            index += 1;
            let mut add_lines = Vec::new();
            while index < lines.len() && !lines[index].starts_with("*** ") {
                if !lines[index].starts_with('+') {
                    return Err(format!(
                        "Add file operation expects '+' lines, found: {}",
                        lines[index]
                    ));
                }
                add_lines.push(lines[index][1..].to_string());
                index += 1;
            }
            operations.push(PatchOperation::Add {
                path: path.trim().to_string(),
                lines: add_lines,
            });
            continue;
        }

        if let Some(path) = line.strip_prefix("*** Delete File: ") {
            operations.push(PatchOperation::Delete {
                path: path.trim().to_string(),
            });
            index += 1;
            continue;
        }

        if let Some(path) = line.strip_prefix("*** Update File: ") {
            index += 1;
            let mut move_to = None;
            if index < lines.len() {
                if let Some(target) = lines[index].strip_prefix("*** Move to: ") {
                    move_to = Some(target.trim().to_string());
                    index += 1;
                }
            }

            let mut hunks = Vec::new();
            while index < lines.len() && !lines[index].starts_with("*** ") {
                if !lines[index].starts_with("@@") {
                    return Err(format!("Expected hunk header '@@', got: {}", lines[index]));
                }
                index += 1;
                let mut hunk_lines = Vec::new();
                while index < lines.len()
                    && !lines[index].starts_with("@@")
                    && !lines[index].starts_with("*** ")
                {
                    if lines[index] == "*** End of File" {
                        index += 1;
                        break;
                    }
                    let prefix = lines[index].chars().next().unwrap_or(' ');
                    if prefix != ' ' && prefix != '+' && prefix != '-' {
                        return Err(format!("Invalid hunk line prefix: {}", lines[index]));
                    }
                    hunk_lines.push(lines[index].to_string());
                    index += 1;
                }
                hunks.push(hunk_lines);
            }

            operations.push(PatchOperation::Update {
                path: path.trim().to_string(),
                move_to,
                hunks,
            });
            continue;
        }

        return Err(format!("Unknown patch operation: {}", line));
    }

    Err("Patch is missing '*** End Patch'".to_string())
}

fn find_subsequence(haystack: &[String], needle: &[String], start_index: usize) -> Option<usize> {
    if needle.is_empty() {
        return Some(start_index.min(haystack.len()));
    }
    if haystack.len() < needle.len() {
        return None;
    }
    (start_index..=haystack.len().saturating_sub(needle.len()))
        .find(|index| haystack[*index..*index + needle.len()] == *needle)
}

fn apply_hunks_to_content(content: &str, hunks: &[Vec<String>]) -> Result<String, String> {
    let had_trailing_newline = content.ends_with('\n');
    let mut lines: Vec<String> = content.lines().map(|line| line.to_string()).collect();
    let mut cursor = 0usize;

    for hunk in hunks {
        let old_lines: Vec<String> = hunk
            .iter()
            .filter_map(|line| match line.chars().next().unwrap_or(' ') {
                ' ' | '-' => Some(line[1..].to_string()),
                _ => None,
            })
            .collect();
        let new_lines: Vec<String> = hunk
            .iter()
            .filter_map(|line| match line.chars().next().unwrap_or(' ') {
                ' ' | '+' => Some(line[1..].to_string()),
                _ => None,
            })
            .collect();

        let Some(position) = find_subsequence(&lines, &old_lines, cursor)
            .or_else(|| find_subsequence(&lines, &old_lines, 0))
        else {
            return Err("Failed to locate hunk context in file".to_string());
        };

        let replace_end = position + old_lines.len();
        lines.splice(position..replace_end, new_lines.clone());
        cursor = position + new_lines.len();
    }

    let mut result = lines.join("\n");
    if had_trailing_newline {
        result.push('\n');
    }
    Ok(result)
}

fn execute_workspace_apply_patch(
    arguments: &Value,
    workspace_root: &Path,
) -> Result<Value, String> {
    let patch_text = read_string_argument(arguments, "patch")?;
    let dry_run = read_bool_argument(arguments, "dry_run", false);
    let operations = parse_patch_envelope(&patch_text)?;
    let mut applied = Vec::new();

    for operation in operations {
        match operation {
            PatchOperation::Add { path, lines } => {
                let target_path = resolve_workspace_target(workspace_root, &path, true)?;
                if target_path.exists() {
                    return Err(format!("Cannot add file that already exists: {}", path));
                }
                let content = if lines.is_empty() {
                    String::new()
                } else {
                    format!("{}\n", lines.join("\n"))
                };
                if !dry_run {
                    if let Some(parent) = target_path.parent() {
                        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                    }
                    write_file_atomic(&target_path, content.as_bytes())?;
                }
                applied.push(json!({
                    "operation": "add",
                    "path": workspace_relative_display_path(workspace_root, &target_path)
                }));
            }
            PatchOperation::Delete { path } => {
                let target_path = resolve_workspace_target(workspace_root, &path, false)?;
                if !target_path.exists() {
                    return Err(format!("Cannot delete missing file: {}", path));
                }
                if !dry_run {
                    fs::remove_file(&target_path).map_err(|e| e.to_string())?;
                }
                applied.push(json!({
                    "operation": "delete",
                    "path": workspace_relative_display_path(workspace_root, &target_path)
                }));
            }
            PatchOperation::Update {
                path,
                move_to,
                hunks,
            } => {
                let source_path = resolve_workspace_target(workspace_root, &path, false)?;
                let content = fs::read_to_string(&source_path).map_err(|e| e.to_string())?;
                let updated = apply_hunks_to_content(&content, &hunks)?;
                if !dry_run {
                    write_file_atomic(&source_path, updated.as_bytes())?;
                }

                let final_path = if let Some(move_target_raw) = move_to {
                    let target_path =
                        resolve_workspace_target(workspace_root, &move_target_raw, true)?;
                    if !dry_run {
                        if let Some(parent) = target_path.parent() {
                            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                        }
                        fs::rename(&source_path, &target_path).map_err(|e| e.to_string())?;
                    }
                    target_path
                } else {
                    source_path
                };

                applied.push(json!({
                    "operation": "update",
                    "path": workspace_relative_display_path(workspace_root, &final_path)
                }));
            }
        }
    }

    Ok(json!({
        "dry_run": dry_run,
        "operations": applied
    }))
}

fn parse_todo_status(status: Option<&str>) -> Result<TodoStatus, String> {
    let Some(value) = status else {
        return Ok(TodoStatus::Pending);
    };

    match value.trim().to_ascii_lowercase().as_str() {
        "pending" => Ok(TodoStatus::Pending),
        "in_progress" | "inprogress" | "in-progress" => Ok(TodoStatus::InProgress),
        "completed" | "done" => Ok(TodoStatus::Completed),
        _ => Err(format!("Invalid TODO status: {}", value)),
    }
}

async fn execute_todo_write(arguments: &Value, conversation_id: &str) -> Result<Value, String> {
    let action = read_string_argument(arguments, "action")?;
    let mut store = todo_store().lock().await;
    let items = store.entry(conversation_id.to_string()).or_default();
    let now = Utc::now().to_rfc3339();

    match action.as_str() {
        "add" => {
            let text = read_string_argument(arguments, "text")?;
            let status = parse_todo_status(
                arguments
                    .get("status")
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty()),
            )?;
            let id = read_optional_string_argument(arguments, "id")
                .unwrap_or_else(|| Uuid::new_v4().to_string());
            items.push(TodoItem {
                id,
                text,
                status,
                created_at: now.clone(),
                updated_at: now.clone(),
            });
        }
        "set" => {
            let raw_items = arguments
                .get("items")
                .and_then(Value::as_array)
                .ok_or_else(|| "'items' is required for action 'set'".to_string())?;
            items.clear();
            for raw_item in raw_items {
                let text = raw_item
                    .get("text")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| "Each TODO item requires non-empty 'text'".to_string())?
                    .to_string();
                let id = raw_item
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
                    .unwrap_or_else(|| Uuid::new_v4().to_string());
                let status = parse_todo_status(raw_item.get("status").and_then(Value::as_str))?;
                items.push(TodoItem {
                    id,
                    text,
                    status,
                    created_at: now.clone(),
                    updated_at: now.clone(),
                });
            }
        }
        "update" => {
            let id = read_string_argument(arguments, "id")?;
            let item = items
                .iter_mut()
                .find(|candidate| candidate.id == id)
                .ok_or_else(|| format!("TODO item not found: {}", id))?;
            if let Some(text) = read_optional_string_argument(arguments, "text") {
                item.text = text;
            }
            if let Some(status_raw) = read_optional_string_argument(arguments, "status") {
                item.status = parse_todo_status(Some(status_raw.as_str()))?;
            }
            item.updated_at = now.clone();
        }
        "remove" => {
            let id = read_string_argument(arguments, "id")?;
            let before = items.len();
            items.retain(|candidate| candidate.id != id);
            if items.len() == before {
                return Err(format!("TODO item not found: {}", id));
            }
        }
        "clear" => {
            items.clear();
        }
        _ => {
            return Err("Unsupported action. Use add|set|update|remove|clear".to_string());
        }
    }

    Ok(json!({
        "conversation_id": conversation_id,
        "action": action,
        "items": items
    }))
}

async fn execute_todo_read(arguments: &Value, conversation_id: &str) -> Result<Value, String> {
    let include_done = read_bool_argument(arguments, "include_completed", true);
    let store = todo_store().lock().await;
    let mut items = store.get(conversation_id).cloned().unwrap_or_default();
    if !include_done {
        items.retain(|item| !matches!(item.status, TodoStatus::Completed));
    }
    Ok(json!({
        "conversation_id": conversation_id,
        "items": items
    }))
}

async fn execute_web_fetch(arguments: &Value) -> Result<Value, String> {
    web_tools::execute_web_fetch(arguments).await
}

async fn execute_web_search(arguments: &Value) -> Result<Value, String> {
    web_tools::execute_web_search(arguments).await
}

async fn execute_browser(arguments: &Value) -> Result<Value, String> {
    browser_tools::execute_browser(arguments).await
}

async fn execute_browser_navigate(arguments: &Value) -> Result<Value, String> {
    browser_tools::execute_browser_navigate_compat(arguments).await
}
async fn execute_image_probe(arguments: &Value, workspace_root: &Path) -> Result<Value, String> {
    image_tools::execute_image_probe(arguments, workspace_root).await
}

async fn execute_image_understand(
    arguments: &Value,
    workspace_root: &Path,
    llm_service: &LlmService,
    default_model: &str,
) -> Result<Value, String> {
    image_tools::execute_image_understand(arguments, workspace_root, llm_service, default_model)
        .await
}
async fn execute_workspace_process_start(
    arguments: &Value,
    workspace_root: &Path,
) -> Result<Value, String> {
    process_tools::execute_workspace_process_start(arguments, workspace_root).await
}

async fn execute_workspace_process_list(arguments: &Value) -> Result<Value, String> {
    process_tools::execute_workspace_process_list(arguments).await
}

async fn execute_workspace_process_read(arguments: &Value) -> Result<Value, String> {
    process_tools::execute_workspace_process_read(arguments).await
}

async fn execute_workspace_process_terminate(arguments: &Value) -> Result<Value, String> {
    process_tools::execute_workspace_process_terminate(arguments).await
}
async fn execute_skill_list(skill_manager_state: &SkillManagerState) -> Result<Value, String> {
    let manager = skill_manager_state.lock().await;
    Ok(json!({
        "skills": manager.list_skills()
    }))
}

async fn execute_skill_discover(
    arguments: &Value,
    skill_manager_state: &SkillManagerState,
) -> Result<Value, String> {
    let query = read_optional_string_argument(arguments, "query");
    let limit = read_u64_argument(arguments, "limit", 8).clamp(1, 20) as usize;
    let (clawhub_api_key, clawhub_api_base) = resolve_clawhub_settings_for_discovery();
    let manager = skill_manager_state.lock().await;
    let results = manager
        .discover_skills(
            query.as_deref(),
            limit,
            clawhub_api_key.as_deref(),
            clawhub_api_base.as_deref(),
        )
        .await
        .map_err(|e| e.to_string())?;
    Ok(json!({
        "query": query.unwrap_or_default(),
        "limit": limit,
        "results": results
    }))
}

async fn execute_skill_execute(
    arguments: &Value,
    skill_manager_state: &SkillManagerState,
) -> Result<Value, String> {
    let preferred_skill_id = read_optional_string_argument(arguments, "skill_id");
    let preferred_skill_name = read_optional_string_argument(arguments, "skill_name");
    let params = arguments
        .get("params")
        .and_then(Value::as_object)
        .map(|object| {
            object
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect::<HashMap<String, Value>>()
        })
        .unwrap_or_default();

    let manager = skill_manager_state.lock().await;
    let resolved_skill_id = if let Some(skill_id) = preferred_skill_id {
        skill_id
    } else if let Some(skill_name) = preferred_skill_name {
        manager
            .list_skills()
            .into_iter()
            .find(|item| item.name.eq_ignore_ascii_case(&skill_name))
            .map(|item| item.id)
            .ok_or_else(|| format!("Skill not found by name: {}", skill_name))?
    } else {
        return Err("Either 'skill_id' or 'skill_name' is required".to_string());
    };

    let result = manager
        .execute_skill(&resolved_skill_id, params)
        .await
        .map_err(|e| e.to_string())?;
    Ok(json!({
        "skill_id": resolved_skill_id,
        "result": result
    }))
}

async fn execute_sessions_list(arguments: &Value, pool: &SqlitePool) -> Result<Value, String> {
    let limit = read_u64_argument(arguments, "limit", 30).clamp(1, 200) as i64;
    let rows = sqlx::query_as::<_, (String, String, String, String, String)>(
        "SELECT id, title, model, created_at, updated_at FROM conversations ORDER BY updated_at DESC LIMIT ?",
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    let conversations = rows
        .into_iter()
        .map(|(id, title, model, created_at, updated_at)| {
            json!({
                "id": id,
                "title": title,
                "model": model,
                "created_at": created_at,
                "updated_at": updated_at
            })
        })
        .collect::<Vec<_>>();

    Ok(json!({
        "limit": limit,
        "conversations": conversations
    }))
}

async fn execute_sessions_history(arguments: &Value, pool: &SqlitePool) -> Result<Value, String> {
    let conversation_id = read_string_argument(arguments, "conversation_id")?;
    let limit = read_u64_argument(arguments, "limit", 100).clamp(1, 500) as i64;

    let rows = sqlx::query_as::<_, (String, String, String, String, Option<String>)>(
        "SELECT role, content, created_at, id, tool_calls FROM messages WHERE conversation_id = ? ORDER BY created_at DESC LIMIT ?",
    )
    .bind(&conversation_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    let mut messages = rows
        .into_iter()
        .map(|(role, content, created_at, id, tool_calls)| {
            json!({
                "id": id,
                "role": role,
                "content": content,
                "created_at": created_at,
                "tool_calls": tool_calls
                    .as_deref()
                    .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
            })
        })
        .collect::<Vec<_>>();
    messages.reverse();

    Ok(json!({
        "conversation_id": conversation_id,
        "limit": limit,
        "messages": messages
    }))
}

async fn execute_sessions_send(
    arguments: &Value,
    pool: &SqlitePool,
    llm_service: &LlmService,
    default_model: &str,
) -> Result<Value, String> {
    let conversation_id = read_string_argument(arguments, "conversation_id")?;
    let content = read_string_argument(arguments, "content")?;
    let run_assistant = read_bool_argument(arguments, "run_assistant", false);
    let conversation_model =
        sqlx::query_scalar::<_, String>("SELECT model FROM conversations WHERE id = ?")
            .bind(&conversation_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?;
    if conversation_model.is_none() {
        return Err(format!("Conversation not found: {}", conversation_id));
    }
    let selected_model = read_optional_string_argument(arguments, "model")
        .or(conversation_model)
        .unwrap_or_else(|| default_model.to_string());

    insert_message(pool, &conversation_id, "user", &content, None, None).await?;

    let assistant_response = if run_assistant {
        let messages = load_conversation_context(pool, &conversation_id).await?;
        let response = llm_service
            .chat(&selected_model, messages)
            .await
            .map_err(|e| e.to_string())?;
        insert_message(pool, &conversation_id, "assistant", &response, None, None).await?;
        Some(response)
    } else {
        None
    };

    Ok(json!({
        "conversation_id": conversation_id,
        "model": selected_model,
        "run_assistant": run_assistant,
        "assistant_response": assistant_response
    }))
}

async fn execute_sessions_spawn(
    arguments: &Value,
    pool: &SqlitePool,
    llm_service: &LlmService,
    default_model: &str,
) -> Result<Value, String> {
    let title = read_string_argument(arguments, "title")?;
    let model = read_optional_string_argument(arguments, "model")
        .unwrap_or_else(|| default_model.to_string());
    let content = read_optional_string_argument(arguments, "content");
    let run_assistant = read_bool_argument(arguments, "run_assistant", false);
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO conversations (id, title, model, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&title)
    .bind(&model)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    if let Some(user_text) = content.clone() {
        insert_message(pool, &id, "user", &user_text, None, None).await?;
    }

    let assistant_response = if run_assistant {
        if content.is_none() {
            return Err("'content' is required when 'run_assistant' is true".to_string());
        }
        let messages = load_conversation_context(pool, &id).await?;
        let response = llm_service
            .chat(&model, messages)
            .await
            .map_err(|e| e.to_string())?;
        insert_message(pool, &id, "assistant", &response, None, None).await?;
        Some(response)
    } else {
        None
    };

    Ok(json!({
        "conversation_id": id,
        "title": title,
        "model": model,
        "assistant_response": assistant_response
    }))
}

fn execute_agents_list() -> Result<Value, String> {
    Ok(json!({
        "agents": [
            {
                "id": "default",
                "name": "Default Agent",
                "description": "General-purpose coding and assistant workflow."
            },
            {
                "id": "planner",
                "name": "Planner Agent",
                "description": "Task decomposition and structured planning."
            },
            {
                "id": "analyst",
                "name": "Analyst Agent",
                "description": "Investigation and comparative reasoning."
            }
        ]
    }))
}

async fn execute_core_task(
    arguments: &Value,
    llm_service: &LlmService,
    default_model: &str,
) -> Result<Value, String> {
    let prompt = read_string_argument(arguments, "prompt")?;
    let model = read_optional_string_argument(arguments, "model")
        .unwrap_or_else(|| default_model.to_string());
    let response = llm_service
        .chat(
            &model,
            vec![ChatMessage {
                role: "user".to_string(),
                content: Some(prompt.clone()),
                tool_calls: None,
                tool_call_id: None,
                reasoning: None,
            }],
        )
        .await
        .map_err(|e| e.to_string())?;

    Ok(json!({
        "model": model,
        "prompt": prompt,
        "response": response
    }))
}

async fn execute_core_batch_safe_tool(
    tool_name: &str,
    arguments: &Value,
    workspace_root: &Path,
    conversation_id: &str,
    skill_manager_state: &SkillManagerState,
    pool: &SqlitePool,
    llm_service: &LlmService,
) -> Result<Value, String> {
    match tool_name {
        WORKSPACE_LIST_TOOL => execute_workspace_list_directory(arguments, workspace_root),
        WORKSPACE_READ_TOOL => execute_workspace_read_file(arguments, workspace_root),
        WORKSPACE_GLOB_TOOL => execute_workspace_glob(arguments, workspace_root),
        WORKSPACE_GREP_TOOL => execute_workspace_grep(arguments, workspace_root),
        WORKSPACE_CODESEARCH_TOOL => execute_workspace_codesearch(arguments, workspace_root),
        WORKSPACE_LSP_SYMBOLS_TOOL => execute_workspace_lsp_symbols(arguments, workspace_root),
        TODO_READ_TOOL => execute_todo_read(arguments, conversation_id).await,
        TODO_WRITE_TOOL => execute_todo_write(arguments, conversation_id).await,
        WEB_FETCH_TOOL => execute_web_fetch(arguments).await,
        WEB_SEARCH_TOOL => execute_web_search(arguments).await,
        BROWSER_TOOL => execute_browser(arguments).await,
        BROWSER_NAVIGATE_TOOL => execute_browser_navigate(arguments).await,
        IMAGE_PROBE_TOOL => execute_image_probe(arguments, workspace_root).await,
        IMAGE_UNDERSTAND_TOOL => {
            execute_image_understand(arguments, workspace_root, llm_service, "glm-4.6v").await
        }
        SESSIONS_LIST_TOOL => execute_sessions_list(arguments, pool).await,
        SESSIONS_HISTORY_TOOL => execute_sessions_history(arguments, pool).await,
        AGENTS_LIST_TOOL => execute_agents_list(),
        SKILL_DISCOVER_TOOL => execute_skill_discover(arguments, skill_manager_state).await,
        SKILL_LIST_TOOL => execute_skill_list(skill_manager_state).await,
        _ => Err(format!("Unsupported batch tool: {}", tool_name)),
    }
}

async fn execute_core_batch(
    arguments: &Value,
    workspace_root: &Path,
    conversation_id: &str,
    skill_manager_state: &SkillManagerState,
    pool: &SqlitePool,
    llm_service: &LlmService,
) -> Result<Value, String> {
    let calls = arguments
        .get("tool_calls")
        .and_then(Value::as_array)
        .ok_or_else(|| "'tool_calls' must be an array".to_string())?;

    let mut results = Vec::<Value>::new();
    for (index, call) in calls.iter().enumerate() {
        let tool_name = call
            .get("tool")
            .or_else(|| call.get("name"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| format!("tool_calls[{}].tool is required", index))?;
        let tool_arguments = call.get("arguments").cloned().unwrap_or_else(|| json!({}));
        if !tool_arguments.is_object() {
            return Err(format!("tool_calls[{}].arguments must be an object", index));
        }
        if !should_auto_allow_batch_tool(tool_name) {
            results.push(json!({
                "index": index,
                "tool": tool_name,
                "result": Value::Null,
                "error": "Tool is not allowed in core_batch"
            }));
            continue;
        }

        match execute_core_batch_safe_tool(
            tool_name,
            &tool_arguments,
            workspace_root,
            conversation_id,
            skill_manager_state,
            pool,
            llm_service,
        )
        .await
        {
            Ok(value) => results.push(json!({
                "index": index,
                "tool": tool_name,
                "result": value,
                "error": Value::Null
            })),
            Err(error) => results.push(json!({
                "index": index,
                "tool": tool_name,
                "result": Value::Null,
                "error": error
            })),
        }
    }

    Ok(json!({
        "results": results
    }))
}

async fn execute_runtime_tool(
    mcp_state: &McpState,
    skill_manager_state: &SkillManagerState,
    config: &Config,
    runtime_tool: RuntimeTool,
    arguments: &Value,
    workspace_root: &Path,
    conversation_id: &str,
    pool: &SqlitePool,
    llm_service: &LlmService,
    default_model: &str,
) -> Result<Value, String> {
    match runtime_tool {
        RuntimeTool::Mcp {
            server_name,
            tool_name,
        } => {
            let mut manager = mcp_state.lock().await;
            let client = manager
                .get_client_mut(&server_name)
                .ok_or_else(|| format!("MCP server '{}' is not connected", server_name))?;

            client
                .call_tool(&tool_name, arguments.clone())
                .await
                .map_err(|e| e.to_string())
        }
        RuntimeTool::WorkspaceListDirectory => {
            execute_workspace_list_directory(arguments, workspace_root)
        }
        RuntimeTool::WorkspaceReadFile => execute_workspace_read_file(arguments, workspace_root),
        RuntimeTool::WorkspaceWriteFile => execute_workspace_write_file(arguments, workspace_root),
        RuntimeTool::WorkspaceEditFile => execute_workspace_edit_file(arguments, workspace_root),
        RuntimeTool::WorkspaceGlob => execute_workspace_glob(arguments, workspace_root),
        RuntimeTool::WorkspaceGrep => execute_workspace_grep(arguments, workspace_root),
        RuntimeTool::WorkspaceCodeSearch => execute_workspace_codesearch(arguments, workspace_root),
        RuntimeTool::WorkspaceLspSymbols => {
            execute_workspace_lsp_symbols(arguments, workspace_root)
        }
        RuntimeTool::WorkspaceApplyPatch => {
            execute_workspace_apply_patch(arguments, workspace_root)
        }
        RuntimeTool::WorkspaceRunCommand => {
            execute_workspace_run_command(arguments, workspace_root).await
        }
        RuntimeTool::WorkspaceProcessStart => {
            execute_workspace_process_start(arguments, workspace_root).await
        }
        RuntimeTool::WorkspaceProcessList => execute_workspace_process_list(arguments).await,
        RuntimeTool::WorkspaceProcessRead => execute_workspace_process_read(arguments).await,
        RuntimeTool::WorkspaceProcessTerminate => {
            execute_workspace_process_terminate(arguments).await
        }
        RuntimeTool::SkillInstallFromRepo => {
            let repo_url = read_string_argument(arguments, "repo_url")?;
            let skill_path = read_optional_string_argument(arguments, "skill_path");
            let mut manager = skill_manager_state.lock().await;
            let installed = if let Some(path) = skill_path.as_deref() {
                manager
                    .install_skill_with_path(&repo_url, Some(path))
                    .await
                    .map_err(|e| e.to_string())?
            } else {
                manager
                    .install_skill(&repo_url)
                    .await
                    .map_err(|e| e.to_string())?
            };

            Ok(json!({
                "installed": true,
                "repo_url": repo_url,
                "skill_path": skill_path,
                "skill": installed
            }))
        }
        RuntimeTool::SkillDiscover => execute_skill_discover(arguments, skill_manager_state).await,
        RuntimeTool::SkillList => execute_skill_list(skill_manager_state).await,
        RuntimeTool::SkillExecute => execute_skill_execute(arguments, skill_manager_state).await,
        RuntimeTool::CoreBatch => {
            execute_core_batch(
                arguments,
                workspace_root,
                conversation_id,
                skill_manager_state,
                pool,
                llm_service,
            )
            .await
        }
        RuntimeTool::CoreTask => execute_core_task(arguments, llm_service, default_model).await,
        RuntimeTool::TodoWrite => execute_todo_write(arguments, conversation_id).await,
        RuntimeTool::TodoRead => execute_todo_read(arguments, conversation_id).await,
        RuntimeTool::WebFetch => execute_web_fetch(arguments).await,
        RuntimeTool::WebSearch => execute_web_search(arguments).await,
        RuntimeTool::Browser => execute_browser(arguments).await,
        RuntimeTool::BrowserNavigate => execute_browser_navigate(arguments).await,
        RuntimeTool::ImageProbe => execute_image_probe(arguments, workspace_root).await,
        RuntimeTool::ImageUnderstand => {
            let image_understand_model = config.image_understand_model.trim().to_string();
            let default_model = if image_understand_model.is_empty() {
                "glm-4.6v".to_string()
            } else {
                image_understand_model
            };
            let image_understand_service = resolve_text_llm_service(config, &default_model)?;
            execute_image_understand(
                arguments,
                workspace_root,
                &image_understand_service,
                &default_model,
            )
            .await
        }
        RuntimeTool::SessionsList => execute_sessions_list(arguments, pool).await,
        RuntimeTool::SessionsHistory => execute_sessions_history(arguments, pool).await,
        RuntimeTool::SessionsSend => {
            execute_sessions_send(arguments, pool, llm_service, default_model).await
        }
        RuntimeTool::SessionsSpawn => {
            execute_sessions_spawn(arguments, pool, llm_service, default_model).await
        }
        RuntimeTool::AgentsList => execute_agents_list(),
    }
}

async fn execute_tool_call(
    mcp_state: &McpState,
    skill_manager_state: &SkillManagerState,
    config: &Config,
    tool_map: &HashMap<String, RuntimeTool>,
    tool_call: &ChatToolCall,
    workspace_root: &Path,
    conversation_id: &str,
    pool: &SqlitePool,
    llm_service: &LlmService,
    default_model: &str,
) -> Result<Value, String> {
    let runtime_tool = tool_map
        .get(&tool_call.function.name)
        .ok_or_else(|| format!("Unknown tool: {}", tool_call.function.name))?
        .clone();
    let arguments = parse_tool_arguments(&tool_call.function.arguments);

    execute_runtime_tool(
        mcp_state,
        skill_manager_state,
        config,
        runtime_tool,
        &arguments,
        workspace_root,
        conversation_id,
        pool,
        llm_service,
        default_model,
    )
    .await
}

async fn request_tool_approval(
    window: &Window,
    conversation_id: &str,
    tool_call: &ChatToolCall,
) -> Result<ToolApprovalDecision, String> {
    let request_id = Uuid::new_v4().to_string();
    let (tx, rx) = oneshot::channel::<ToolApprovalDecision>();

    {
        let mut waiters = tool_approval_waiters().lock().await;
        waiters.insert(request_id.clone(), tx);
    }

    let payload = ToolApprovalRequestPayload {
        request_id: request_id.clone(),
        conversation_id: conversation_id.to_string(),
        tool_call_id: tool_call.id.clone(),
        tool_name: tool_call.function.name.clone(),
        arguments: tool_call.function.arguments.clone(),
    };

    if let Err(error) = window.emit("chat-tool-approval-request", payload) {
        let mut waiters = tool_approval_waiters().lock().await;
        waiters.remove(&request_id);
        return Err(error.to_string());
    }

    let decision = tokio::time::timeout(std::time::Duration::from_secs(180), rx).await;

    {
        let mut waiters = tool_approval_waiters().lock().await;
        waiters.remove(&request_id);
    }

    Ok(match decision {
        Ok(Ok(value)) => value,
        Ok(Err(_)) => ToolApprovalDecision::Deny,
        Err(_) => ToolApprovalDecision::Deny,
    })
}

#[tauri::command]
pub async fn resolve_tool_approval(
    request_id: String,
    decision: ToolApprovalDecision,
) -> Result<(), String> {
    let sender = {
        let mut waiters = tool_approval_waiters().lock().await;
        waiters.remove(&request_id)
    };

    let sender = sender.ok_or_else(|| "Tool approval request not found".to_string())?;
    sender
        .send(decision)
        .map_err(|_| "Failed to deliver tool approval response".to_string())
}

#[tauri::command]
pub async fn stop_stream(conversation_id: String) -> Result<bool, String> {
    Ok(request_stream_stop(&conversation_id).await)
}

#[tauri::command]
pub async fn generate_image(
    state: State<'_, AppState>,
    conversation_id: String,
    prompt: String,
    model: Option<String>,
    size: Option<String>,
    watermark: Option<bool>,
) -> Result<GenerateImageResponse, String> {
    let trimmed_prompt = prompt.trim();
    if trimmed_prompt.is_empty() {
        return Err("Prompt cannot be empty".to_string());
    }

    let config = crate::utils::load_config::<Config>().map_err(|e| e.to_string())?;
    let image_model = model.unwrap_or_else(|| config.image_model.clone());
    let image_size = size.unwrap_or_else(|| config.image_size.clone());
    let image_watermark = watermark.unwrap_or(config.image_watermark);

    let llm_service = resolve_image_generation_llm_service(&config)?;
    let image_url = llm_service
        .generate_image(&image_model, trimmed_prompt, &image_size, image_watermark)
        .await
        .map_err(|e| e.to_string())?;

    let pool = {
        let guard = state.lock().map_err(|e| e.to_string())?;
        guard.db().pool().clone()
    };

    let user_message_id = Uuid::new_v4().to_string();
    let user_created_at = Utc::now();
    let user_content = format!("[文生图] {}", trimmed_prompt);
    sqlx::query(
        "INSERT INTO messages (id, conversation_id, role, content, created_at, tool_calls, reasoning) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&user_message_id)
    .bind(&conversation_id)
    .bind("user")
    .bind(&user_content)
    .bind(user_created_at.to_rfc3339())
    .bind(Option::<String>::None)
    .bind(Option::<String>::None)
    .execute(&pool)
    .await
    .map_err(|e| e.to_string())?;

    let assistant_message_id = Uuid::new_v4().to_string();
    let assistant_created_at = Utc::now();
    let assistant_content = format!("![{}]({})\n\n{}", trimmed_prompt, image_url, image_url);
    sqlx::query(
        "INSERT INTO messages (id, conversation_id, role, content, created_at, tool_calls, reasoning) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&assistant_message_id)
    .bind(&conversation_id)
    .bind("assistant")
    .bind(&assistant_content)
    .bind(assistant_created_at.to_rfc3339())
    .bind(Option::<String>::None)
    .bind(Option::<String>::None)
    .execute(&pool)
    .await
    .map_err(|e| e.to_string())?;

    sqlx::query("UPDATE conversations SET updated_at = ? WHERE id = ?")
        .bind(assistant_created_at.to_rfc3339())
        .bind(&conversation_id)
        .execute(&pool)
        .await
        .map_err(|e| e.to_string())?;

    let user_message = Message {
        id: user_message_id,
        conversation_id: conversation_id.clone(),
        role: MessageRole::User,
        content: user_content,
        reasoning: None,
        created_at: user_created_at,
        tool_calls: None,
    };

    let assistant_message = Message {
        id: assistant_message_id,
        conversation_id,
        role: MessageRole::Assistant,
        content: assistant_content,
        reasoning: None,
        created_at: assistant_created_at,
        tool_calls: None,
    };

    Ok(GenerateImageResponse {
        user_message,
        assistant_message,
        image_url,
    })
}

#[tauri::command]
pub async fn send_message(
    state: State<'_, AppState>,
    conversation_id: String,
    content: String,
) -> Result<String, String> {
    let config = crate::utils::load_config::<Config>().map_err(|e| e.to_string())?;

    let pool = {
        let guard = state.lock().map_err(|e| e.to_string())?;
        guard.db().pool().clone()
    };
    let model_to_use = resolve_conversation_model(&pool, &conversation_id, &config.model).await?;
    let llm_service = resolve_text_llm_service(&config, &model_to_use)?;

    insert_message(&pool, &conversation_id, "user", &content, None, None).await?;

    let mut messages = load_conversation_context(&pool, &conversation_id).await?;
    prepend_system_prompt(&mut messages, config.system_prompt.as_deref());

    let response = llm_service
        .chat(&model_to_use, messages)
        .await
        .map_err(|e| e.to_string())?;

    insert_message(&pool, &conversation_id, "assistant", &response, None, None).await?;

    Ok(response)
}

#[tauri::command]
pub async fn stream_message(
    state: State<'_, AppState>,
    mcp_manager: State<'_, McpState>,
    skill_manager: State<'_, SkillManagerState>,
    window: Window,
    conversation_id: String,
    content: String,
    workspace_directory: Option<String>,
) -> Result<(), String> {
    let stop_flag = register_stream_stop_flag(&conversation_id).await;

    let result = async {
        let config = crate::utils::load_config::<Config>().map_err(|e| e.to_string())?;
        let mcp_state = mcp_manager.inner();
        let skill_state = skill_manager.inner();
        let skills_guidance = build_skills_usage_guidance(skill_state).await;
        let workspace_root = resolve_workspace_root(&config, workspace_directory.as_deref())?;
        let RuntimeToolCatalog {
            available_tools,
            tool_map,
        } = build_runtime_tool_catalog(mcp_state, &config, &workspace_root).await?;

        let pool = {
            let guard = state.lock().map_err(|e| e.to_string())?;
            guard.db().pool().clone()
        };
        let model_to_use =
            resolve_conversation_model(&pool, &conversation_id, &config.model).await?;
        let llm_service = resolve_text_llm_service(&config, &model_to_use)?;

        insert_message(&pool, &conversation_id, "user", &content, None, None).await?;

        let mut context_messages = load_conversation_context(&pool, &conversation_id).await?;
        prepend_system_prompt(&mut context_messages, config.system_prompt.as_deref());
        prepend_tool_usage_guidance(&mut context_messages);
        prepend_skills_usage_guidance(&mut context_messages, &skills_guidance);

        let mut always_allowed_tools = HashSet::<String>::new();
        let mut last_tool_signature: Option<String> = None;
        let mut repeated_signature_rounds = 0usize;

        loop {
            if stop_flag.load(Ordering::Relaxed) {
                window
                    .emit("chat-end", json!({ "conversationId": conversation_id }))
                    .map_err(|e| e.to_string())?;
                return Ok(());
            }

            let stream_result = run_stream_round(
                &llm_service,
                &window,
                &conversation_id,
                &model_to_use,
                context_messages.clone(),
                &available_tools,
                stop_flag.clone(),
            )
            .await?;

            let assistant_content = stream_result.content.clone();
            let assistant_reasoning = if stream_result.reasoning.trim().is_empty() {
                None
            } else {
                Some(stream_result.reasoning.clone())
            };

            if stream_result.cancelled {
                for tool_call in &stream_result.tool_calls {
                    let _ = emit_tool_result_event(
                        &window,
                        &conversation_id,
                        tool_call,
                        None,
                        Some("Paused by user"),
                    );
                }

                let paused_content = if assistant_content.trim().is_empty() {
                    window
                        .emit(
                            "chat-chunk",
                            json!({
                                "conversationId": conversation_id,
                                "chunk": STREAM_PAUSED_TEXT
                            }),
                        )
                        .map_err(|e| e.to_string())?;
                    STREAM_PAUSED_TEXT.to_string()
                } else {
                    assistant_content
                };

                insert_message(
                    &pool,
                    &conversation_id,
                    "assistant",
                    &paused_content,
                    None,
                    assistant_reasoning,
                )
                .await?;
                window
                    .emit("chat-end", json!({ "conversationId": conversation_id }))
                    .map_err(|e| e.to_string())?;
                return Ok(());
            }

            if stream_result.tool_calls.is_empty() {
                insert_message(
                    &pool,
                    &conversation_id,
                    "assistant",
                    &assistant_content,
                    None,
                    assistant_reasoning.clone(),
                )
                .await?;
                window
                    .emit("chat-end", json!({ "conversationId": conversation_id }))
                    .map_err(|e| e.to_string())?;
                return Ok(());
            }

            let assistant_tool_calls_json =
                serde_json::to_string(&stream_result.tool_calls).map_err(|e| e.to_string())?;
            insert_message(
                &pool,
                &conversation_id,
                "assistant",
                &assistant_content,
                Some(assistant_tool_calls_json.clone()),
                assistant_reasoning,
            )
            .await?;

            context_messages.push(ChatMessage {
                role: "assistant".to_string(),
                content: if assistant_content.is_empty() {
                    None
                } else {
                    Some(assistant_content)
                },
                tool_calls: Some(stream_result.tool_calls.clone()),
                tool_call_id: None,
                reasoning: if stream_result.reasoning.trim().is_empty() {
                    None
                } else {
                    Some(stream_result.reasoning.clone())
                },
            });

            update_repeated_signature_rounds(
                &mut last_tool_signature,
                &mut repeated_signature_rounds,
                build_tool_round_signature(&stream_result.tool_calls),
            );

            if repeated_signature_rounds >= 2 {
                let guard_text = REPEATED_TOOL_GUARD_TEXT;
                window
                    .emit(
                        "chat-chunk",
                        json!({
                            "conversationId": conversation_id,
                            "chunk": guard_text
                        }),
                    )
                    .map_err(|e| e.to_string())?;
                insert_message(&pool, &conversation_id, "assistant", guard_text, None, None)
                    .await?;
                window
                    .emit("chat-end", json!({ "conversationId": conversation_id }))
                    .map_err(|e| e.to_string())?;
                return Ok(());
            }

            let mut cancelled_during_tools = false;
            for tool_call in stream_result.tool_calls {
                if stop_flag.load(Ordering::Relaxed) {
                    cancelled_during_tools = true;
                    emit_tool_result_event(
                        &window,
                        &conversation_id,
                        &tool_call,
                        None,
                        Some("Paused by user"),
                    )?;
                    let result_text = format_tool_error_result("Paused by user")?;
                    persist_tool_result_message(
                        &pool,
                        &conversation_id,
                        &mut context_messages,
                        &tool_call,
                        result_text,
                    )
                    .await?;
                    break;
                }

                let parsed_arguments = parse_tool_arguments(&tool_call.function.arguments);
                let decision = resolve_tool_execution_decision(
                    &config,
                    &window,
                    &conversation_id,
                    &tool_call,
                    &parsed_arguments,
                    &always_allowed_tools,
                )
                .await?;

                if stop_flag.load(Ordering::Relaxed) {
                    cancelled_during_tools = true;
                    emit_tool_result_event(
                        &window,
                        &conversation_id,
                        &tool_call,
                        None,
                        Some("Paused by user"),
                    )?;
                    let result_text = format_tool_error_result("Paused by user")?;
                    persist_tool_result_message(
                        &pool,
                        &conversation_id,
                        &mut context_messages,
                        &tool_call,
                        result_text,
                    )
                    .await?;
                    break;
                }

                match decision {
                    ToolApprovalDecision::AllowAlways => {
                        always_allowed_tools.insert(tool_call.function.name.clone());
                    }
                    ToolApprovalDecision::AllowOnce => {}
                    ToolApprovalDecision::Deny => {
                        let error_text = format!(
                            "User denied execution of tool '{}'",
                            tool_call.function.name
                        );
                        emit_tool_result_event(
                            &window,
                            &conversation_id,
                            &tool_call,
                            None,
                            Some(&error_text),
                        )?;
                        let result_text = format_tool_error_result(&error_text)?;
                        persist_tool_result_message(
                            &pool,
                            &conversation_id,
                            &mut context_messages,
                            &tool_call,
                            result_text,
                        )
                        .await?;
                        continue;
                    }
                }

                let tool_result = execute_tool_call(
                    mcp_state,
                    skill_state,
                    &config,
                    &tool_map,
                    &tool_call,
                    &workspace_root,
                    &conversation_id,
                    &pool,
                    &llm_service,
                    &model_to_use,
                )
                .await;
                match tool_result {
                    Ok(value) => {
                        let result_text = serde_json::to_string_pretty(&value)
                            .unwrap_or_else(|_| value.to_string());
                        emit_tool_result_event(
                            &window,
                            &conversation_id,
                            &tool_call,
                            Some(&result_text),
                            None,
                        )?;
                        persist_tool_result_message(
                            &pool,
                            &conversation_id,
                            &mut context_messages,
                            &tool_call,
                            result_text,
                        )
                        .await?;
                    }
                    Err(error_text) => {
                        emit_tool_result_event(
                            &window,
                            &conversation_id,
                            &tool_call,
                            None,
                            Some(&error_text),
                        )?;
                        let result_text = format_tool_error_result(&error_text)?;
                        persist_tool_result_message(
                            &pool,
                            &conversation_id,
                            &mut context_messages,
                            &tool_call,
                            result_text,
                        )
                        .await?;
                    }
                }
            }

            if cancelled_during_tools {
                window
                    .emit("chat-end", json!({ "conversationId": conversation_id }))
                    .map_err(|e| e.to_string())?;
                return Ok(());
            }
        }
    }
    .await;

    clear_stream_stop_flag(&conversation_id).await;
    if result.is_err() {
        let _ = window.emit("chat-end", json!({ "conversationId": conversation_id }));
    }
    result
}

#[tauri::command]
pub async fn get_conversations(state: State<'_, AppState>) -> Result<Vec<Conversation>, String> {
    let pool = {
        let guard = state.lock().map_err(|e| e.to_string())?;
        guard.db().pool().clone()
    };

    let rows = sqlx::query_as::<_, (String, String, String, String, String)>(
        "SELECT id, title, model, created_at, updated_at FROM conversations ORDER BY updated_at DESC",
    )
    .fetch_all(&pool)
    .await
    .map_err(|e| e.to_string())?;

    let conversations = rows
        .into_iter()
        .map(
            |(id, title, model, created_at_raw, updated_at_raw)| Conversation {
                id,
                title,
                model,
                created_at: created_at_raw.parse().unwrap_or_else(|_| Utc::now()),
                updated_at: updated_at_raw.parse().unwrap_or_else(|_| Utc::now()),
            },
        )
        .collect();

    Ok(conversations)
}

#[tauri::command]
pub async fn get_messages(
    state: State<'_, AppState>,
    conversation_id: String,
) -> Result<Vec<Message>, String> {
    let pool = {
        let guard = state.lock().map_err(|e| e.to_string())?;
        guard.db().pool().clone()
    };

    let rows = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            String,
            String,
            Option<String>,
            Option<String>,
        ),
    >(
        "SELECT id, conversation_id, role, content, created_at, tool_calls, reasoning FROM messages WHERE conversation_id = ? ORDER BY created_at ASC",
    )
    .bind(&conversation_id)
    .fetch_all(&pool)
    .await
    .map_err(|e| e.to_string())?;

    let messages = rows
        .into_iter()
        .map(
            |(
                id,
                conversation_id,
                role_raw,
                content,
                created_at_raw,
                tool_calls_raw,
                reasoning_raw,
            )| {
                let role = match role_raw.as_str() {
                    "user" => MessageRole::User,
                    "assistant" => MessageRole::Assistant,
                    "system" => MessageRole::System,
                    "tool" => MessageRole::Tool,
                    _ => MessageRole::User,
                };

                let tool_calls: Option<Vec<ToolCall>> =
                    tool_calls_raw.as_deref().and_then(|value| {
                        serde_json::from_str::<Vec<ToolCall>>(value)
                            .ok()
                            .or_else(|| {
                                serde_json::from_str::<Vec<ChatToolCall>>(value)
                                    .ok()
                                    .map(|calls| {
                                        calls
                                            .into_iter()
                                            .map(|call| ToolCall {
                                                id: call.id,
                                                tool_name: call.function.name,
                                                arguments: serde_json::from_str(
                                                    &call.function.arguments,
                                                )
                                                .unwrap_or_else(|_| {
                                                    json!({
                                                        "raw_arguments": call.function.arguments
                                                    })
                                                }),
                                            })
                                            .collect()
                                    })
                            })
                            .or_else(|| {
                                serde_json::from_str::<Value>(value).ok().and_then(|meta| {
                                    let tool_call_id = meta
                                        .get("tool_call_id")
                                        .and_then(Value::as_str)
                                        .map(str::to_string)?;
                                    let tool_name = meta
                                        .get("tool_name")
                                        .and_then(Value::as_str)
                                        .unwrap_or("tool")
                                        .to_string();
                                    Some(vec![ToolCall {
                                        id: tool_call_id,
                                        tool_name,
                                        arguments: json!({}),
                                    }])
                                })
                            })
                    });

                Message {
                    id,
                    conversation_id,
                    role,
                    content,
                    reasoning: reasoning_raw.and_then(|value| {
                        let trimmed = value.trim().to_string();
                        if trimmed.is_empty() {
                            None
                        } else {
                            Some(trimmed)
                        }
                    }),
                    created_at: created_at_raw.parse().unwrap_or_else(|_| Utc::now()),
                    tool_calls,
                }
            },
        )
        .collect();

    Ok(messages)
}

#[tauri::command]
pub async fn create_conversation(
    state: State<'_, AppState>,
    title: String,
    model: String,
) -> Result<Conversation, String> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    let pool = {
        let guard = state.lock().map_err(|e| e.to_string())?;
        guard.db().pool().clone()
    };

    sqlx::query(
        "INSERT INTO conversations (id, title, model, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&title)
    .bind(&model)
    .bind(&now)
    .bind(&now)
    .execute(&pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(Conversation {
        id,
        title,
        model,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    })
}

#[tauri::command]
pub async fn delete_conversation(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let pool = {
        let guard = state.lock().map_err(|e| e.to_string())?;
        guard.db().pool().clone()
    };

    sqlx::query("DELETE FROM conversations WHERE id = ?")
        .bind(&id)
        .execute(&pool)
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn rename_conversation(
    state: State<'_, AppState>,
    id: String,
    title: String,
) -> Result<(), String> {
    let trimmed_title = title.trim();
    if trimmed_title.is_empty() {
        return Err("Conversation title cannot be empty".to_string());
    }

    let now = Utc::now().to_rfc3339();
    let pool = {
        let guard = state.lock().map_err(|e| e.to_string())?;
        guard.db().pool().clone()
    };

    let result = sqlx::query("UPDATE conversations SET title = ?, updated_at = ? WHERE id = ?")
        .bind(trimmed_title)
        .bind(&now)
        .bind(&id)
        .execute(&pool)
        .await
        .map_err(|e| e.to_string())?;

    if result.rows_affected() == 0 {
        return Err(format!("Conversation not found: {}", id));
    }

    Ok(())
}

#[tauri::command]
pub async fn update_conversation_model(
    state: State<'_, AppState>,
    id: String,
    model: String,
) -> Result<(), String> {
    let trimmed_model = model.trim();
    if trimmed_model.is_empty() {
        return Err("Conversation model cannot be empty".to_string());
    }

    let now = Utc::now().to_rfc3339();
    let pool = {
        let guard = state.lock().map_err(|e| e.to_string())?;
        guard.db().pool().clone()
    };

    let result = sqlx::query("UPDATE conversations SET model = ?, updated_at = ? WHERE id = ?")
        .bind(trimmed_model)
        .bind(&now)
        .bind(&id)
        .execute(&pool)
        .await
        .map_err(|e| e.to_string())?;

    if result.rows_affected() == 0 {
        return Err(format!("Conversation not found: {}", id));
    }

    Ok(())
}
