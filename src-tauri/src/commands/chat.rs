use crate::commands::mcp::McpState;
use crate::commands::skills::SkillManagerState;
use crate::models::chat::*;
use crate::models::config::{Config, McpTransport};
use crate::services::llm::{
    ChatMessage, ChatTool, ChatToolCall, ChatToolFunction, LlmService, LlmStreamEvent,
};
use crate::services::mcp_client::{HttpTransport, McpClient, StdioTransport};
use crate::state::AppState;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::SqlitePool;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tauri::{Emitter, State, Window};
use tokio::process::Command as TokioCommand;
use tokio::sync::oneshot;
use uuid::Uuid;

type ToolApprovalSender = oneshot::Sender<ToolApprovalDecision>;

static TOOL_APPROVAL_WAITERS: OnceLock<tokio::sync::Mutex<HashMap<String, ToolApprovalSender>>> =
    OnceLock::new();

fn tool_approval_waiters() -> &'static tokio::sync::Mutex<HashMap<String, ToolApprovalSender>> {
    TOOL_APPROVAL_WAITERS.get_or_init(|| tokio::sync::Mutex::new(HashMap::new()))
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
const WORKSPACE_RUN_TOOL: &str = "workspace_run_command";
const SKILL_INSTALL_TOOL: &str = "skills_install_from_repo";

#[derive(Debug, Clone)]
enum RuntimeTool {
    Mcp {
        server_name: String,
        tool_name: String,
    },
    WorkspaceListDirectory,
    WorkspaceReadFile,
    WorkspaceWriteFile,
    WorkspaceRunCommand,
    SkillInstallFromRepo,
}

async fn insert_message(
    pool: &SqlitePool,
    conversation_id: &str,
    role: &str,
    content: &str,
    tool_calls: Option<String>,
) -> Result<(), String> {
    let message_id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO messages (id, conversation_id, role, content, created_at, tool_calls) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&message_id)
    .bind(conversation_id)
    .bind(role)
    .bind(content)
    .bind(&now)
    .bind(tool_calls)
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
    let rows = sqlx::query_as::<_, (String, String, Option<String>)>(
        "SELECT role, content, tool_calls FROM messages WHERE conversation_id = ? ORDER BY created_at ASC",
    )
    .bind(conversation_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    let mut messages = Vec::new();

    for (role, content, tool_calls_raw) in rows {
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
                });
            }
            _ => {
                messages.push(ChatMessage {
                    role,
                    content: Some(content),
                    tool_calls: None,
                    tool_call_id: None,
                });
            }
        }
    }

    Ok(messages)
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

fn resolve_workspace_root(config: &Config, workspace_override: Option<&str>) -> Result<PathBuf, String> {
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

    let root = if let Some(path) = configured {
        let candidate = PathBuf::from(path);
        if !candidate.exists() || !candidate.is_dir() {
            return Err(format!(
                "Configured work_directory does not exist or is not a directory: {}",
                path
            ));
        }
        candidate
    } else {
        std::env::current_dir().map_err(|e| e.to_string())?
    };

    root.canonicalize().map_err(|e| e.to_string())
}

fn collect_workspace_tools(workspace_root: &Path) -> (Vec<ChatTool>, HashMap<String, RuntimeTool>) {
    let root_hint = workspace_root.to_string_lossy().to_string();
    let mut tools = Vec::new();
    let mut tool_map = HashMap::new();

    tools.push(ChatTool {
        tool_type: "function".to_string(),
        function: ChatToolFunction {
            name: WORKSPACE_LIST_TOOL.to_string(),
            description: format!(
                "List files and subdirectories in the local workspace. Workspace root: {}",
                root_hint
            ),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Directory path to list. Relative paths are resolved from the workspace root."
                    }
                }
            }),
        },
    });
    tool_map.insert(
        WORKSPACE_LIST_TOOL.to_string(),
        RuntimeTool::WorkspaceListDirectory,
    );

    tools.push(ChatTool {
        tool_type: "function".to_string(),
        function: ChatToolFunction {
            name: WORKSPACE_READ_TOOL.to_string(),
            description: format!(
                "Read a UTF-8 text file from the local workspace. Workspace root: {}",
                root_hint
            ),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path to read. Relative paths are resolved from the workspace root."
                    },
                    "max_bytes": {
                        "type": "integer",
                        "description": "Maximum bytes to read. Defaults to 200000."
                    }
                },
                "required": ["path"]
            }),
        },
    });
    tool_map.insert(
        WORKSPACE_READ_TOOL.to_string(),
        RuntimeTool::WorkspaceReadFile,
    );

    tools.push(ChatTool {
        tool_type: "function".to_string(),
        function: ChatToolFunction {
            name: WORKSPACE_WRITE_TOOL.to_string(),
            description: format!(
                "Write text content to a file in the local workspace. Workspace root: {}",
                root_hint
            ),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path to write. Relative paths are resolved from the workspace root."
                    },
                    "content": {
                        "type": "string",
                        "description": "Text content to write."
                    },
                    "append": {
                        "type": "boolean",
                        "description": "If true, append to existing file. Otherwise overwrite."
                    }
                },
                "required": ["path", "content"]
            }),
        },
    });
    tool_map.insert(
        WORKSPACE_WRITE_TOOL.to_string(),
        RuntimeTool::WorkspaceWriteFile,
    );

    tools.push(ChatTool {
        tool_type: "function".to_string(),
        function: ChatToolFunction {
            name: WORKSPACE_RUN_TOOL.to_string(),
            description: format!(
                "Run a shell command in the local workspace and return stdout/stderr. Workspace root: {}",
                root_hint
            ),
            parameters: json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "Shell command to run."
                    },
                    "timeout_ms": {
                        "type": "integer",
                        "description": "Command timeout in milliseconds. Defaults to 20000."
                    }
                },
                "required": ["command"]
            }),
        },
    });
    tool_map.insert(
        WORKSPACE_RUN_TOOL.to_string(),
        RuntimeTool::WorkspaceRunCommand,
    );

    (tools, tool_map)
}

fn collect_skill_tools() -> (Vec<ChatTool>, HashMap<String, RuntimeTool>) {
    let tools = vec![ChatTool {
        tool_type: "function".to_string(),
        function: ChatToolFunction {
            name: SKILL_INSTALL_TOOL.to_string(),
            description: "Install a skill from a git repository URL so the assistant can use new capabilities.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "repo_url": {
                        "type": "string",
                        "description": "Git repository URL for the skill, e.g. https://github.com/org/repo.git"
                    }
                },
                "required": ["repo_url"]
            }),
        },
    }];

    let mut tool_map = HashMap::new();
    tool_map.insert(
        SKILL_INSTALL_TOOL.to_string(),
        RuntimeTool::SkillInstallFromRepo,
    );

    (tools, tool_map)
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

    let parent = requested_path
        .parent()
        .ok_or_else(|| "Invalid path".to_string())?;
    let canonical_parent = parent.canonicalize().map_err(|e| e.to_string())?;
    if !canonical_parent.starts_with(&canonical_root) {
        return Err("Path is outside workspace root".to_string());
    }

    let file_name = requested_path
        .file_name()
        .ok_or_else(|| "Invalid path".to_string())?;
    Ok(canonical_parent.join(file_name))
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

    let max_bytes = arguments
        .get("max_bytes")
        .and_then(Value::as_u64)
        .unwrap_or(200_000)
        .clamp(1, 2_000_000) as usize;

    let bytes = std::fs::read(&file_path).map_err(|e| e.to_string())?;
    let total_bytes = bytes.len();
    let limit = total_bytes.min(max_bytes);
    let content = String::from_utf8_lossy(&bytes[..limit]).to_string();

    Ok(json!({
        "workspace_root": workspace_root.to_string_lossy().to_string(),
        "path": file_path.to_string_lossy().to_string(),
        "content": content,
        "bytes_read": limit,
        "total_bytes": total_bytes,
        "truncated": total_bytes > max_bytes
    }))
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

    if let Some(parent) = file_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    if append {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file_path)
            .map_err(|e| e.to_string())?;
        file.write_all(content.as_bytes())
            .map_err(|e| e.to_string())?;
    } else {
        std::fs::write(&file_path, content.as_bytes()).map_err(|e| e.to_string())?;
    }

    let metadata = std::fs::metadata(&file_path).map_err(|e| e.to_string())?;
    Ok(json!({
        "workspace_root": workspace_root.to_string_lossy().to_string(),
        "path": file_path.to_string_lossy().to_string(),
        "bytes_written": metadata.len(),
        "append": append
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

    let timeout_ms = arguments
        .get("timeout_ms")
        .and_then(Value::as_u64)
        .unwrap_or(20_000)
        .clamp(1_000, 120_000);

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
    let output = tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), cmd.output())
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

async fn execute_tool_call(
    mcp_state: &McpState,
    skill_manager_state: &SkillManagerState,
    tool_map: &HashMap<String, RuntimeTool>,
    tool_call: &ChatToolCall,
    workspace_root: &Path,
) -> Result<Value, String> {
    let tool = tool_map
        .get(&tool_call.function.name)
        .ok_or_else(|| format!("Unknown tool: {}", tool_call.function.name))?
        .clone();
    let arguments = parse_tool_arguments(&tool_call.function.arguments);

    match tool {
        RuntimeTool::Mcp {
            server_name,
            tool_name,
        } => {
            let mut manager = mcp_state.lock().await;
            let client = manager
                .get_client_mut(&server_name)
                .ok_or_else(|| format!("MCP server '{}' is not connected", server_name))?;

            client
                .call_tool(&tool_name, arguments)
                .await
                .map_err(|e| e.to_string())
        }
        RuntimeTool::WorkspaceListDirectory => {
            execute_workspace_list_directory(&arguments, workspace_root)
        }
        RuntimeTool::WorkspaceReadFile => execute_workspace_read_file(&arguments, workspace_root),
        RuntimeTool::WorkspaceWriteFile => execute_workspace_write_file(&arguments, workspace_root),
        RuntimeTool::WorkspaceRunCommand => {
            execute_workspace_run_command(&arguments, workspace_root).await
        }
        RuntimeTool::SkillInstallFromRepo => {
            let repo_url = read_string_argument(&arguments, "repo_url")?;
            let mut manager = skill_manager_state.lock().await;
            let installed = manager
                .install_skill(&repo_url)
                .await
                .map_err(|e| e.to_string())?;

            Ok(json!({
                "installed": true,
                "repo_url": repo_url,
                "skill": installed
            }))
        }
    }
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
pub async fn send_message(
    state: State<'_, AppState>,
    conversation_id: String,
    content: String,
) -> Result<String, String> {
    let config = crate::utils::load_config::<Config>().map_err(|e| e.to_string())?;
    let api_key = config
        .api_key
        .clone()
        .ok_or("API key not set".to_string())?;
    let llm_service = LlmService::new(api_key, config.api_base.clone());

    let pool = {
        let guard = state.lock().map_err(|e| e.to_string())?;
        guard.db().pool().clone()
    };
    let model_to_use =
        sqlx::query_scalar::<_, String>("SELECT model FROM conversations WHERE id = ?")
            .bind(&conversation_id)
            .fetch_optional(&pool)
            .await
            .map_err(|e| e.to_string())?
            .unwrap_or_else(|| config.model.clone());

    insert_message(&pool, &conversation_id, "user", &content, None).await?;

    let mut messages = load_conversation_context(&pool, &conversation_id).await?;
    if let Some(system_prompt) = config.system_prompt.clone() {
        if !system_prompt.trim().is_empty() {
            messages.insert(
                0,
                ChatMessage {
                    role: "system".to_string(),
                    content: Some(system_prompt),
                    tool_calls: None,
                    tool_call_id: None,
                },
            );
        }
    }

    let response = llm_service
        .chat(&model_to_use, messages)
        .await
        .map_err(|e| e.to_string())?;

    insert_message(&pool, &conversation_id, "assistant", &response, None).await?;

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
    let config = crate::utils::load_config::<Config>().map_err(|e| e.to_string())?;
    let api_key = config
        .api_key
        .clone()
        .ok_or("API key not set".to_string())?;
    let llm_service = LlmService::new(api_key, config.api_base.clone());
    let mcp_state = mcp_manager.inner();
    let skill_state = skill_manager.inner();
    let workspace_root = resolve_workspace_root(&config, workspace_directory.as_deref())?;

    ensure_mcp_servers_connected(mcp_state, &config).await?;
    let (mcp_tools, mcp_tool_map) = collect_mcp_tools(mcp_state).await;
    let (workspace_tools, workspace_tool_map) = collect_workspace_tools(&workspace_root);
    let (skill_tools, skill_tool_map) = collect_skill_tools();
    let mut available_tools = workspace_tools;
    available_tools.extend(mcp_tools);
    available_tools.extend(skill_tools);
    let mut tool_map = workspace_tool_map;
    tool_map.extend(mcp_tool_map);
    tool_map.extend(skill_tool_map);

    let pool = {
        let guard = state.lock().map_err(|e| e.to_string())?;
        guard.db().pool().clone()
    };
    let model_to_use =
        sqlx::query_scalar::<_, String>("SELECT model FROM conversations WHERE id = ?")
            .bind(&conversation_id)
            .fetch_optional(&pool)
            .await
            .map_err(|e| e.to_string())?
            .unwrap_or_else(|| config.model.clone());

    insert_message(&pool, &conversation_id, "user", &content, None).await?;

    let mut context_messages = load_conversation_context(&pool, &conversation_id).await?;
    if let Some(system_prompt) = config.system_prompt.clone() {
        if !system_prompt.trim().is_empty() {
            context_messages.insert(
                0,
                ChatMessage {
                    role: "system".to_string(),
                    content: Some(system_prompt),
                    tool_calls: None,
                    tool_call_id: None,
                },
            );
        }
    }

    let max_tool_rounds = 8usize;
    let mut always_allowed_tools = HashSet::<String>::new();
    let mut last_tool_signature: Option<String> = None;
    let mut repeated_signature_rounds = 0usize;
    for _round in 0..max_tool_rounds {
        let window_for_stream = window.clone();
        let stream_result = llm_service
            .chat_stream_with_tools(
                &model_to_use,
                context_messages.clone(),
                if available_tools.is_empty() {
                    None
                } else {
                    Some(available_tools.clone())
                },
                move |event| match event {
                    LlmStreamEvent::Content(chunk) => {
                        let _ = window_for_stream.emit("chat-chunk", &chunk);
                    }
                    LlmStreamEvent::Reasoning(chunk) => {
                        let _ = window_for_stream.emit("chat-reasoning", &chunk);
                    }
                    LlmStreamEvent::ToolCallDelta(delta) => {
                        let payload = json!({
                            "index": delta.index,
                            "toolCallId": delta.id,
                            "name": delta.name,
                            "argumentsChunk": delta.arguments_chunk,
                        });
                        let _ = window_for_stream.emit("chat-tool-call", payload);
                    }
                },
            )
            .await
            .map_err(|e| e.to_string())?;

        let assistant_content = stream_result.content.clone();

        if stream_result.tool_calls.is_empty() {
            insert_message(
                &pool,
                &conversation_id,
                "assistant",
                &assistant_content,
                None,
            )
            .await?;
            window.emit("chat-end", &()).map_err(|e| e.to_string())?;
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
        });

        let round_signature = serde_json::to_string(
            &stream_result
                .tool_calls
                .iter()
                .map(|call| (call.function.name.clone(), call.function.arguments.clone()))
                .collect::<Vec<_>>(),
        )
        .unwrap_or_else(|_| String::new());

        if let Some(previous_signature) = &last_tool_signature {
            if previous_signature == &round_signature {
                repeated_signature_rounds += 1;
            } else {
                repeated_signature_rounds = 0;
            }
        }
        last_tool_signature = Some(round_signature);

        if repeated_signature_rounds >= 2 {
            let guard_text = "检测到模型重复调用相同工具，已停止自动循环。请补充更具体目标，或直接指定要读取的文件/目录。";
            window
                .emit("chat-chunk", guard_text)
                .map_err(|e| e.to_string())?;
            insert_message(&pool, &conversation_id, "assistant", guard_text, None).await?;
            window.emit("chat-end", &()).map_err(|e| e.to_string())?;
            return Ok(());
        }

        for tool_call in stream_result.tool_calls {
            let decision = if always_allowed_tools.contains(&tool_call.function.name) {
                ToolApprovalDecision::AllowAlways
            } else {
                request_tool_approval(&window, &conversation_id, &tool_call).await?
            };

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
                    window
                        .emit(
                            "chat-tool-result",
                            json!({
                                "toolCallId": tool_call.id,
                                "name": tool_call.function.name,
                                "result": null,
                                "error": error_text,
                            }),
                        )
                        .map_err(|e| e.to_string())?;

                    let result_text = serde_json::to_string_pretty(&json!({
                        "error": error_text
                    }))
                    .map_err(|e| e.to_string())?;

                    let metadata = serde_json::to_string(&json!({
                        "tool_call_id": tool_call.id,
                        "tool_name": tool_call.function.name
                    }))
                    .map_err(|e| e.to_string())?;

                    insert_message(
                        &pool,
                        &conversation_id,
                        "tool",
                        &result_text,
                        Some(metadata),
                    )
                    .await?;

                    context_messages.push(ChatMessage {
                        role: "tool".to_string(),
                        content: Some(result_text),
                        tool_calls: None,
                        tool_call_id: Some(tool_call.id),
                    });

                    continue;
                }
            }

            let tool_result =
                execute_tool_call(mcp_state, skill_state, &tool_map, &tool_call, &workspace_root)
                    .await;
            match tool_result {
                Ok(value) => {
                    let result_text =
                        serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string());
                    window
                        .emit(
                            "chat-tool-result",
                            json!({
                                "toolCallId": tool_call.id,
                                "name": tool_call.function.name,
                                "result": result_text,
                                "error": null,
                            }),
                        )
                        .map_err(|e| e.to_string())?;

                    let metadata = serde_json::to_string(&json!({
                        "tool_call_id": tool_call.id,
                        "tool_name": tool_call.function.name
                    }))
                    .map_err(|e| e.to_string())?;

                    insert_message(
                        &pool,
                        &conversation_id,
                        "tool",
                        &result_text,
                        Some(metadata),
                    )
                    .await?;

                    context_messages.push(ChatMessage {
                        role: "tool".to_string(),
                        content: Some(result_text),
                        tool_calls: None,
                        tool_call_id: Some(tool_call.id),
                    });
                }
                Err(error_text) => {
                    window
                        .emit(
                            "chat-tool-result",
                            json!({
                                "toolCallId": tool_call.id,
                                "name": tool_call.function.name,
                                "result": null,
                                "error": error_text,
                            }),
                        )
                        .map_err(|e| e.to_string())?;

                    let result_text = serde_json::to_string_pretty(&json!({
                        "error": error_text
                    }))
                    .map_err(|e| e.to_string())?;

                    let metadata = serde_json::to_string(&json!({
                        "tool_call_id": tool_call.id,
                        "tool_name": tool_call.function.name
                    }))
                    .map_err(|e| e.to_string())?;

                    insert_message(
                        &pool,
                        &conversation_id,
                        "tool",
                        &result_text,
                        Some(metadata),
                    )
                    .await?;

                    context_messages.push(ChatMessage {
                        role: "tool".to_string(),
                        content: Some(result_text),
                        tool_calls: None,
                        tool_call_id: Some(tool_call.id),
                    });
                }
            }
        }
    }

    let limit_text =
        "工具调用达到上限，已停止自动调用。请缩小范围后重试，例如：只列出 src 目录，或读取指定文件。";
    window
        .emit("chat-chunk", limit_text)
        .map_err(|e| e.to_string())?;
    insert_message(&pool, &conversation_id, "assistant", limit_text, None).await?;
    window.emit("chat-end", &()).map_err(|e| e.to_string())?;
    Ok(())
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

    let rows = sqlx::query_as::<_, (String, String, String, String, String, Option<String>)>(
        "SELECT id, conversation_id, role, content, created_at, tool_calls FROM messages WHERE conversation_id = ? ORDER BY created_at ASC",
    )
    .bind(&conversation_id)
    .fetch_all(&pool)
    .await
    .map_err(|e| e.to_string())?;

    let messages = rows
        .into_iter()
        .map(
            |(id, conversation_id, role_raw, content, created_at_raw, tool_calls_raw)| {
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
                    });

                Message {
                    id,
                    conversation_id,
                    role,
                    content,
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
