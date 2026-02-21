use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use crate::commands::mcp::McpState;
use crate::models::config::{Config, McpTransport};
use crate::services::mcp_client::{HttpTransport, McpClient, StdioTransport};
use crate::services::llm::{ChatTool, ChatToolFunction};

pub(crate) const WORKSPACE_LIST_TOOL: &str = "workspace_list_directory";
pub(crate) const WORKSPACE_READ_TOOL: &str = "workspace_read_file";
pub(crate) const WORKSPACE_WRITE_TOOL: &str = "workspace_write_file";
pub(crate) const WORKSPACE_EDIT_TOOL: &str = "workspace_edit_file";
pub(crate) const WORKSPACE_GLOB_TOOL: &str = "workspace_search_glob";
pub(crate) const WORKSPACE_GREP_TOOL: &str = "workspace_search_grep";
pub(crate) const WORKSPACE_CODESEARCH_TOOL: &str = "workspace_search_code";
pub(crate) const WORKSPACE_LSP_SYMBOLS_TOOL: &str = "workspace_lsp_symbols";
pub(crate) const WORKSPACE_APPLY_PATCH_TOOL: &str = "workspace_apply_patch";
pub(crate) const WORKSPACE_PROCESS_START_TOOL: &str = "workspace_process_start";
pub(crate) const WORKSPACE_PROCESS_LIST_TOOL: &str = "workspace_process_list";
pub(crate) const WORKSPACE_PROCESS_READ_TOOL: &str = "workspace_process_read";
pub(crate) const WORKSPACE_PROCESS_TERMINATE_TOOL: &str = "workspace_process_terminate";
pub(crate) const WORKSPACE_RUN_TOOL: &str = "bash";
pub(crate) const WORKSPACE_PARSE_PDF_TOOL: &str = "workspace_parse_pdf_markdown";
pub(crate) const SKILL_DISCOVER_TOOL: &str = "skill_discover";
pub(crate) const SKILL_INSTALL_TOOL: &str = "skill_install_from_repo";
pub(crate) const SKILL_LIST_TOOL: &str = "skill_list";
pub(crate) const SKILL_EXECUTE_TOOL: &str = "skill_execute";
pub(crate) const CORE_BATCH_TOOL: &str = "core_batch";
pub(crate) const CORE_TASK_TOOL: &str = "core_task";
pub(crate) const TODO_WRITE_TOOL: &str = "todo_write";
pub(crate) const TODO_READ_TOOL: &str = "todo_read";
pub(crate) const WEB_FETCH_TOOL: &str = "web_fetch";
pub(crate) const WEB_SEARCH_TOOL: &str = "web_search";
pub(crate) const BROWSER_TOOL: &str = "browser";
pub(crate) const BROWSER_NAVIGATE_TOOL: &str = "browser_navigate";
pub(crate) const DESKTOP_TOOL: &str = "desktop";
pub(crate) const IMAGE_PROBE_TOOL: &str = "image_probe";
pub(crate) const IMAGE_UNDERSTAND_TOOL: &str = "image_understand";
pub(crate) const SESSIONS_LIST_TOOL: &str = "sessions_list";
pub(crate) const SESSIONS_HISTORY_TOOL: &str = "sessions_history";
pub(crate) const SESSIONS_SEND_TOOL: &str = "sessions_send";
pub(crate) const SESSIONS_SPAWN_TOOL: &str = "sessions_spawn";
pub(crate) const AGENTS_LIST_TOOL: &str = "agents_list";
pub(crate) const SCHEDULER_JOBS_LIST_TOOL: &str = "scheduler_jobs_list";
pub(crate) const SCHEDULER_JOB_CREATE_TOOL: &str = "scheduler_job_create";
pub(crate) const SCHEDULER_JOB_UPDATE_TOOL: &str = "scheduler_job_update";
pub(crate) const SCHEDULER_JOB_DELETE_TOOL: &str = "scheduler_job_delete";
pub(crate) const SCHEDULER_JOB_RUN_TOOL: &str = "scheduler_job_run";
pub(crate) const SCHEDULER_RUNS_LIST_TOOL: &str = "scheduler_runs_list";

#[derive(Debug, Clone)]
pub(crate) enum RuntimeTool {
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
    WorkspaceParsePdfMarkdown,
    WorkspaceProcessStart,
    WorkspaceProcessList,
    WorkspaceProcessRead,
    WorkspaceProcessTerminate,
    SkillDiscover,
    SkillInstallFromRepo,
    SkillList,
    SkillExecute,
    CoreBatch,
    CoreTask,
    TodoRead,
    TodoWrite,
    WebFetch,
    WebSearch,
    Browser,
    BrowserNavigate,
    Desktop,
    ImageProbe,
    ImageUnderstand,
    SessionsList,
    SessionsHistory,
    SessionsSend,
    SessionsSpawn,
    AgentsList,
    SchedulerJobsList,
    SchedulerJobCreate,
    SchedulerJobUpdate,
    SchedulerJobDelete,
    SchedulerJobRun,
    SchedulerRunsList,
}

#[derive(Debug, Clone)]
pub(crate) struct RuntimeToolCatalog {
    pub available_tools: Vec<ChatTool>,
    pub tool_map: HashMap<String, RuntimeTool>,
}

pub(crate) fn build_tool_alias(server: &str, tool_name: &str) -> String {
    format!("mcp__{}__{}", server, tool_name)
}

pub(crate) fn recover_tool_arguments_candidate(candidate: &str) -> Option<String> {
    let trimmed = candidate.trim();
    if trimmed.is_empty() {
        return Some(String::new());
    }

    if trimmed.starts_with("```") && trimmed.ends_with("```") {
        let mut lines = trimmed.lines().collect::<Vec<_>>();
        if lines.len() >= 2 {
            lines.remove(0);
            if lines.last().map(|line| line.trim()) == Some("```") {
                lines.pop();
                return Some(lines.join("\n").trim().to_string());
            }
        }
    }

    if trimmed.starts_with("{{") && trimmed.ends_with("}}") && trimmed.len() >= 4 {
        return Some(trimmed[1..trimmed.len() - 1].trim().to_string());
    }

    let starts = trimmed
        .match_indices('{')
        .map(|(idx, _)| idx)
        .collect::<Vec<_>>();
    let ends = trimmed
        .match_indices('}')
        .map(|(idx, _)| idx)
        .collect::<Vec<_>>();
    for &start in starts.iter().rev() {
        for &end in ends.iter().rev() {
            if end <= start {
                continue;
            }
            let sliced = trimmed[start..=end].trim();
            if sliced.is_empty() || sliced == trimmed {
                continue;
            }
            if let Ok(Value::Object(_)) = serde_json::from_str::<Value>(sliced) {
                return Some(sliced.to_string());
            }
        }
    }

    None
}

pub(crate) fn parse_tool_arguments(raw: &str) -> Value {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return json!({});
    }

    let mut candidate = trimmed.to_string();
    for _ in 0..6 {
        match serde_json::from_str::<Value>(&candidate) {
            Ok(Value::Object(map)) => return Value::Object(map),
            Ok(Value::String(inner)) => {
                let inner_trimmed = inner.trim();
                if inner_trimmed.is_empty() {
                    return json!({});
                }
                if inner_trimmed == candidate {
                    break;
                }
                candidate = inner_trimmed.to_string();
                continue;
            }
            Ok(_) => {}
            Err(_) => {}
        }

        if let Some(recovered) = recover_tool_arguments_candidate(&candidate) {
            if recovered.is_empty() {
                return json!({});
            }
            if recovered == candidate {
                break;
            }
            candidate = recovered;
            continue;
        }

        break;
    }

    json!({
        "raw_arguments": raw
    })
}

pub(crate) async fn ensure_mcp_servers_connected(mcp_state: &McpState, config: &Config) -> Result<(), String> {
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

pub(crate) async fn collect_mcp_tools(mcp_state: &McpState) -> (Vec<ChatTool>, HashMap<String, RuntimeTool>) {
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

pub(crate) fn resolve_workspace_root(
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

pub(crate) async fn build_runtime_tool_catalog(
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

fn register_runtime_tool(
    tools: &mut Vec<ChatTool>,
    tool_map: &mut HashMap<String, RuntimeTool>,
    name: &str,
    description: String,
    parameters: Value,
    runtime_tool: RuntimeTool,
) {
    tools.push(ChatTool {
        tool_type: "function".to_string(),
        function: ChatToolFunction {
            name: name.to_string(),
            description,
            parameters,
        },
    });
    tool_map.insert(name.to_string(), runtime_tool);
}

pub(super) fn collect_workspace_tools(
    workspace_root: &Path,
) -> (Vec<ChatTool>, HashMap<String, RuntimeTool>) {
    let root_hint = workspace_root.to_string_lossy().to_string();
    let mut tools = Vec::new();
    let mut tool_map = HashMap::new();

    register_runtime_tool(
        &mut tools,
        &mut tool_map,
        WORKSPACE_LIST_TOOL,
        format!(
            "List direct children of a directory in the local workspace (non-recursive peek). \
             Avoid using this for recursive traversal, file counting, folder-size calculation, or large inventory tasks. \
             On Windows, prefer bash with PowerShell for those operations. Workspace root: {}",
            root_hint
        ),
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Directory path to list. Relative paths are resolved from the workspace root."
                }
            }
        }),
        RuntimeTool::WorkspaceListDirectory,
    );

    register_runtime_tool(
        &mut tools,
        &mut tool_map,
        WORKSPACE_READ_TOOL,
        format!(
            "Read a UTF-8 text file from the local workspace. Supports offset and line caps. Workspace root: {}",
            root_hint
        ),
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "max_bytes": { "type": "integer", "description": "Default 200000, max 2000000." },
                "offset_bytes": { "type": "integer", "description": "Start byte offset. Default 0." },
                "max_lines": { "type": "integer", "description": "Maximum lines to return after offset." }
            },
            "required": ["path"]
        }),
        RuntimeTool::WorkspaceReadFile,
    );

    register_runtime_tool(
        &mut tools,
        &mut tool_map,
        WORKSPACE_PARSE_PDF_TOOL,
        format!(
            "Parse a PDF file inside workspace into ordered markdown (top-to-bottom, left-to-right), optionally extracting embedded images. Workspace root: {}",
            root_hint
        ),
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "PDF file path. Relative paths are resolved from workspace root." },
                "export_images": { "type": "boolean", "description": "Default true. When true, export images and include markdown image links." },
                "max_pages": { "type": "integer", "description": "Default 0 (all pages). Limit parsed pages for very large PDFs." }
            },
            "required": ["path"]
        }),
        RuntimeTool::WorkspaceParsePdfMarkdown,
    );

    register_runtime_tool(
        &mut tools,
        &mut tool_map,
        WORKSPACE_WRITE_TOOL,
        format!(
            "Write text content to a file in the local workspace (atomic overwrite). Workspace root: {}",
            root_hint
        ),
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "content": { "type": "string" },
                "append": { "type": "boolean", "description": "Append mode disables atomic overwrite." }
            },
            "required": ["path", "content"]
        }),
        RuntimeTool::WorkspaceWriteFile,
    );

    register_runtime_tool(
        &mut tools,
        &mut tool_map,
        WORKSPACE_EDIT_TOOL,
        format!(
            "Edit a file by replacing an exact snippet. Workspace root: {}",
            root_hint
        ),
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "old_string": { "type": "string" },
                "new_string": { "type": "string" },
                "replace_all": { "type": "boolean" }
            },
            "required": ["path", "old_string", "new_string"]
        }),
        RuntimeTool::WorkspaceEditFile,
    );

    register_runtime_tool(
        &mut tools,
        &mut tool_map,
        WORKSPACE_APPLY_PATCH_TOOL,
        format!(
            "Apply a structured patch envelope (*** Begin Patch ... *** End Patch) in the workspace. Workspace root: {}",
            root_hint
        ),
        json!({
            "type": "object",
            "properties": {
                "patch": { "type": "string" },
                "dry_run": { "type": "boolean" }
            },
            "required": ["patch"]
        }),
        RuntimeTool::WorkspaceApplyPatch,
    );

    register_runtime_tool(
        &mut tools,
        &mut tool_map,
        WORKSPACE_GLOB_TOOL,
        format!(
            "Find workspace paths by glob patterns. Workspace root: {}",
            root_hint
        ),
        json!({
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "Glob pattern such as **/*.ts" },
                "path": { "type": "string", "description": "Base directory, default ." },
                "max_results": { "type": "integer", "description": "Default 200, max 5000" },
                "include_directories": { "type": "boolean" }
            },
            "required": ["pattern"]
        }),
        RuntimeTool::WorkspaceGlob,
    );

    register_runtime_tool(
        &mut tools,
        &mut tool_map,
        WORKSPACE_GREP_TOOL,
        format!(
            "Search file contents with regex/string matching in the workspace. Workspace root: {}",
            root_hint
        ),
        json!({
            "type": "object",
            "properties": {
                "pattern": { "type": "string" },
                "path": { "type": "string", "description": "Base directory, default ." },
                "glob": { "type": "string", "description": "Optional file glob filter, e.g. **/*.rs" },
                "case_sensitive": { "type": "boolean" },
                "regex": { "type": "boolean" },
                "max_results": { "type": "integer", "description": "Default 200, max 5000" }
            },
            "required": ["pattern"]
        }),
        RuntimeTool::WorkspaceGrep,
    );

    register_runtime_tool(
        &mut tools,
        &mut tool_map,
        WORKSPACE_CODESEARCH_TOOL,
        format!(
            "Code-aware text search with surrounding context snippets. Workspace root: {}",
            root_hint
        ),
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" },
                "path": { "type": "string" },
                "glob": { "type": "string", "description": "Optional file glob, default **/*" },
                "context_lines": { "type": "integer", "description": "Default 2" },
                "max_results": { "type": "integer", "description": "Default 100" }
            },
            "required": ["query"]
        }),
        RuntimeTool::WorkspaceCodeSearch,
    );

    register_runtime_tool(
        &mut tools,
        &mut tool_map,
        WORKSPACE_LSP_SYMBOLS_TOOL,
        format!(
            "Best-effort symbol scan (functions/classes/types) without a language server. Workspace root: {}",
            root_hint
        ),
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Optional symbol name filter" },
                "path": { "type": "string", "description": "Base directory, default ." },
                "max_results": { "type": "integer", "description": "Default 200" }
            }
        }),
        RuntimeTool::WorkspaceLspSymbols,
    );

    register_runtime_tool(
        &mut tools,
        &mut tool_map,
        WORKSPACE_RUN_TOOL,
        format!(
            "Run a shell command in the local workspace and return stdout/stderr. \
             On Windows this executes in PowerShell (-NoProfile -Command). \
             Prefer this tool for recursive file traversal, file statistics, folder size, bulk listing/sorting/filtering, and other batch filesystem tasks. Workspace root: {}",
            root_hint
        ),
        json!({
            "type": "object",
            "properties": {
                "command": { "type": "string" },
                "timeout_ms": { "type": "integer", "description": "Default 20000, max 120000." }
            },
            "required": ["command"]
        }),
        RuntimeTool::WorkspaceRunCommand,
    );

    register_runtime_tool(
        &mut tools,
        &mut tool_map,
        WORKSPACE_PROCESS_START_TOOL,
        format!(
            "Start a long-running background process in the workspace. Workspace root: {}",
            root_hint
        ),
        json!({
            "type": "object",
            "properties": {
                "command": { "type": "string" }
            },
            "required": ["command"]
        }),
        RuntimeTool::WorkspaceProcessStart,
    );

    register_runtime_tool(
        &mut tools,
        &mut tool_map,
        WORKSPACE_PROCESS_LIST_TOOL,
        "List managed background processes started by workspace_process_start.".to_string(),
        json!({
            "type": "object",
            "properties": {}
        }),
        RuntimeTool::WorkspaceProcessList,
    );

    register_runtime_tool(
        &mut tools,
        &mut tool_map,
        WORKSPACE_PROCESS_READ_TOOL,
        "Read stdout/stderr snapshots for a managed background process.".to_string(),
        json!({
            "type": "object",
            "properties": {
                "process_id": { "type": "string" },
                "max_chars": { "type": "integer", "description": "Default 6000" }
            },
            "required": ["process_id"]
        }),
        RuntimeTool::WorkspaceProcessRead,
    );

    register_runtime_tool(
        &mut tools,
        &mut tool_map,
        WORKSPACE_PROCESS_TERMINATE_TOOL,
        "Terminate a managed background process by process_id.".to_string(),
        json!({
            "type": "object",
            "properties": {
                "process_id": { "type": "string" }
            },
            "required": ["process_id"]
        }),
        RuntimeTool::WorkspaceProcessTerminate,
    );

    (tools, tool_map)
}

pub(super) fn collect_skill_tools() -> (Vec<ChatTool>, HashMap<String, RuntimeTool>) {
    let mut tools = Vec::new();
    let mut tool_map = HashMap::new();

    register_runtime_tool(
        &mut tools,
        &mut tool_map,
        SKILL_INSTALL_TOOL,
        "Install a skill package from ClawHub download URL (zip/tar.gz), \
         a ClawHub skill page URL, or a skill slug. Git repository clone is disabled. \
         Use only when user intent requires adding a capability and source is provided/approved."
            .to_string(),
        json!({
            "type": "object",
            "properties": {
                "repo_url": { "type": "string" },
                "skill_path": { "type": "string", "description": "Optional relative path to skill directory inside the downloaded package." }
            },
            "required": ["repo_url"]
        }),
        RuntimeTool::SkillInstallFromRepo,
    );

    register_runtime_tool(
        &mut tools,
        &mut tool_map,
        SKILL_DISCOVER_TOOL,
        "Discover installable skills from ClawHub registry only (downloadable packages). \
         Uses ClawHub API base configured in Settings. \
         Use this when no installed skill clearly matches the task."
            .to_string(),
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Search query, e.g. 'word docx export'." },
                "limit": { "type": "integer", "description": "Number of results, default 8, max 20." }
            }
        }),
        RuntimeTool::SkillDiscover,
    );

    register_runtime_tool(
        &mut tools,
        &mut tool_map,
        SKILL_LIST_TOOL,
        "List installed skills and their enabled status. \
         Always use this first when task requirements are unfamiliar, specialized, or uncertain."
            .to_string(),
        json!({
            "type": "object",
            "properties": {}
        }),
        RuntimeTool::SkillList,
    );

    register_runtime_tool(
        &mut tools,
        &mut tool_map,
        SKILL_EXECUTE_TOOL,
        "Execute an installed skill by id or name after selecting candidate(s) from skills_list."
            .to_string(),
        json!({
            "type": "object",
            "properties": {
                "skill_id": { "type": "string" },
                "skill_name": { "type": "string" },
                "params": { "type": "object" }
            }
        }),
        RuntimeTool::SkillExecute,
    );

    (tools, tool_map)
}

pub(super) fn collect_core_tools() -> (Vec<ChatTool>, HashMap<String, RuntimeTool>) {
    let mut tools = Vec::new();
    let mut tool_map = HashMap::new();

    register_runtime_tool(
        &mut tools,
        &mut tool_map,
        CORE_BATCH_TOOL,
        "Run multiple independent safe tool calls in one request.".to_string(),
        json!({
            "type": "object",
            "properties": {
                "tool_calls": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "tool": { "type": "string" },
                            "arguments": { "type": "object" }
                        },
                        "required": ["tool"]
                    }
                }
            },
            "required": ["tool_calls"]
        }),
        RuntimeTool::CoreBatch,
    );

    register_runtime_tool(
        &mut tools,
        &mut tool_map,
        CORE_TASK_TOOL,
        "Run a sub-task prompt with text-only response.".to_string(),
        json!({
            "type": "object",
            "properties": {
                "prompt": { "type": "string" },
                "model": { "type": "string" }
            },
            "required": ["prompt"]
        }),
        RuntimeTool::CoreTask,
    );

    register_runtime_tool(
        &mut tools,
        &mut tool_map,
        TODO_WRITE_TOOL,
        "Create/update/remove TODO items for the current conversation.".to_string(),
        json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "description": "add|set|update|remove|clear" },
                "id": { "type": "string" },
                "text": { "type": "string" },
                "status": { "type": "string", "description": "pending|in_progress|completed" },
                "items": { "type": "array", "items": { "type": "object" } }
            },
            "required": ["action"]
        }),
        RuntimeTool::TodoWrite,
    );

    register_runtime_tool(
        &mut tools,
        &mut tool_map,
        TODO_READ_TOOL,
        "Read TODO items for the current conversation.".to_string(),
        json!({
            "type": "object",
            "properties": {}
        }),
        RuntimeTool::TodoRead,
    );

    register_runtime_tool(
        &mut tools,
        &mut tool_map,
        WEB_FETCH_TOOL,
        "Fetch URL content with retry, redirects and HTML extraction controls.".to_string(),
        json!({
            "type": "object",
            "properties": {
                "url": { "type": "string" },
                "max_chars": { "type": "integer" },
                "timeout_ms": { "type": "integer" },
                "retries": { "type": "integer", "description": "0-3 retry attempts for transient failures" },
                "max_redirects": { "type": "integer", "description": "0-10 redirects" },
                "format": { "type": "string", "description": "auto|text|markdown|html" },
                "user_agent": { "type": "string" },
                "accept_language": { "type": "string" }
            },
            "required": ["url"]
        }),
        RuntimeTool::WebFetch,
    );

    register_runtime_tool(
        &mut tools,
        &mut tool_map,
        WEB_SEARCH_TOOL,
        "Search the web (Exa MCP first, then Bing/DuckDuckGo fallback).".to_string(),
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" },
                "max_results": { "type": "integer" },
                "timeout_ms": { "type": "integer" },
                "provider": { "type": "string", "description": "auto|exa|bing|duckduckgo" },
                "exa_url": { "type": "string", "description": "MCP endpoint URL, defaults to https://mcp.exa.ai/mcp" },
                "search_lang": { "type": "string" },
                "ui_lang": { "type": "string" },
                "country": { "type": "string" }
            },
            "required": ["query"]
        }),
        RuntimeTool::WebSearch,
    );

    register_runtime_tool(
        &mut tools,
        &mut tool_map,
        BROWSER_TOOL,
        "Control managed browser sessions (status/start/stop/profiles/tabs/open/focus/close/navigate/snapshot/screenshot/act/act_batch/console/errors/requests/response_body/pdf/cookies/storage/evaluate/trace). Use this tool exclusively for browser launch/navigation/page interactions. For fast and stable interactions: snapshot after navigation or after an act failure, and use act_batch for consecutive actions. Important contract: for action=act or action=act_batch, use params.kind in each action item (never actions[].action)."
            .to_string(),
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "description": "status|start|stop|profiles|tabs|open|focus|close|navigate|snapshot|screenshot|act|act_batch|console|errors|requests|response_body|pdf|cookies_get|cookies_set|cookies_clear|storage_get|storage_set|storage_clear|set_offline|set_headers|set_credentials|set_geolocation|set_media|set_timezone|set_locale|set_device|trace_start|trace_stop|evaluate|reset_profile"
                },
                "profile": { "type": "string" },
                "target_id": { "type": "string" },
                "params": {
                    "type": "object",
                    "description": "Action-specific parameters. For action=act, use {kind, ref|selector, ...}. For action=act_batch, use {actions:[{kind, ref|selector, ...}, ...], stop_on_error?}. Valid act kinds include: click|type|press|hover|scroll|select|wait|drag. Example: {\"action\":\"act_batch\",\"params\":{\"actions\":[{\"kind\":\"type\",\"ref\":\"e7\",\"text\":\"李佳琦\"},{\"kind\":\"click\",\"ref\":\"e8\"}]}}. Do not use actions[].action."
                }
            },
            "required": ["action"]
        }),
        RuntimeTool::Browser,
    );

    register_runtime_tool(
        &mut tools,
        &mut tool_map,
        BROWSER_NAVIGATE_TOOL,
        "Legacy browser navigate alias. Internally forwards to browser action=navigate and returns title+links payload.".to_string(),
        json!({
            "type": "object",
            "properties": {
                "url": { "type": "string" },
                "max_links": { "type": "integer" }
            },
            "required": ["url"]
        }),
        RuntimeTool::BrowserNavigate,
    );

    #[cfg(target_os = "windows")]
    register_runtime_tool(
        &mut tools,
        &mut tool_map,
        DESKTOP_TOOL,
        "Control Windows desktop GUI and Office apps with UFO-style workflow. \
         Required sequence for reliable UI actions: \
         1) get_desktop_app_info or list_windows, \
         2) select_application_window or select_window, \
         3) get_app_window_controls_info or get_controls (refresh=true), \
         4) control actions (click_input/set_edit_text/keyboard_input/wheel_mouse_input/texts) using exact control id + exact name. \
         Browser operations are excluded and must use the browser tool. \
         Use click_on_coordinates only when target control is missing from control list.".to_string(),
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "description": "status|list_windows|get_desktop_app_info|get_desktop_app_target_info|select_window|select_application_window|get_window_info|get_app_window_info|get_controls|get_app_window_controls_info|get_app_window_controls_target_info|get_ui_tree|capture_desktop_screenshot|capture_window_screenshot|get_control_texts|texts|wait|summary|launch_application|close_application|click_input|click_on_coordinates|drag_on_coordinates|set_edit_text|keyboard_input|wheel_mouse_input|word_get_doc_info|word_insert_text|word_insert_table|word_save_as|excel_get_workbook_info|excel_set_cell|excel_set_range|excel_save_as|ppt_get_presentation_info|ppt_add_slide|ppt_set_text|ppt_save_as"
                },
                "params": {
                    "type": "object",
                    "description": "Action-specific parameters. UFO-style canonical args: select_application_window(id,name), set_edit_text(id,name,text), keyboard_input(id,name,keys,control_focus), wheel_mouse_input(id,name,wheel_dist), click_input(id,name,button,double), texts(id,name). For control actions use exact id + exact name. For control/window collectors pass field_list (string[]). For launch_application use one of: command | application_path | app_path | executable | app_name | bash_command, but only for non-browser apps. Browser launch/navigation is forbidden here and must use tool=browser. Optional: args (string[]), cwd. Compatibility: select_application_window also accepts window_id/hwnd; keyboard_input also accepts text as alias of keys."
                }
            },
            "required": ["action"]
        }),
        RuntimeTool::Desktop,
    );

    register_runtime_tool(
        &mut tools,
        &mut tool_map,
        IMAGE_PROBE_TOOL,
        "Probe image metadata from local path or URL (format, dimensions when detectable)."
            .to_string(),
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "url": { "type": "string" }
            }
        }),
        RuntimeTool::ImageProbe,
    );

    register_runtime_tool(
        &mut tools,
        &mut tool_map,
        IMAGE_UNDERSTAND_TOOL,
        "Analyze an image with a vision model using a prompt. Supports workspace path or public URL."
            .to_string(),
        json!({
            "type": "object",
            "properties": {
                "prompt": { "type": "string", "description": "Question or instruction for the image." },
                "path": { "type": "string", "description": "Workspace-relative or absolute path inside workspace root." },
                "url": { "type": "string", "description": "Public http/https URL or data:image/... URL." },
                "model": { "type": "string", "description": "Vision model name. Default glm-4.6v." },
                "thinking": { "type": "boolean", "description": "Enable model thinking mode when supported." },
                "max_bytes": { "type": "integer", "description": "Max local image bytes. Default 4MB, max 8MB." }
            },
            "required": ["prompt"]
        }),
        RuntimeTool::ImageUnderstand,
    );

    register_runtime_tool(
        &mut tools,
        &mut tool_map,
        SESSIONS_LIST_TOOL,
        "List conversations/sessions.".to_string(),
        json!({
            "type": "object",
            "properties": {
                "limit": { "type": "integer" }
            }
        }),
        RuntimeTool::SessionsList,
    );

    register_runtime_tool(
        &mut tools,
        &mut tool_map,
        SESSIONS_HISTORY_TOOL,
        "Read message history for a conversation.".to_string(),
        json!({
            "type": "object",
            "properties": {
                "conversation_id": { "type": "string" },
                "limit": { "type": "integer" }
            },
            "required": ["conversation_id"]
        }),
        RuntimeTool::SessionsHistory,
    );

    register_runtime_tool(
        &mut tools,
        &mut tool_map,
        SESSIONS_SEND_TOOL,
        "Send a message into an existing conversation and optionally run one assistant step."
            .to_string(),
        json!({
            "type": "object",
            "properties": {
                "conversation_id": { "type": "string" },
                "content": { "type": "string" },
                "run_assistant": { "type": "boolean" },
                "model": { "type": "string" }
            },
            "required": ["conversation_id", "content"]
        }),
        RuntimeTool::SessionsSend,
    );

    register_runtime_tool(
        &mut tools,
        &mut tool_map,
        SESSIONS_SPAWN_TOOL,
        "Create a new conversation/session and optionally run one assistant step.".to_string(),
        json!({
            "type": "object",
            "properties": {
                "title": { "type": "string" },
                "content": { "type": "string" },
                "model": { "type": "string" },
                "run_assistant": { "type": "boolean" }
            },
            "required": ["title"]
        }),
        RuntimeTool::SessionsSpawn,
    );

    register_runtime_tool(
        &mut tools,
        &mut tool_map,
        AGENTS_LIST_TOOL,
        "List available built-in agent profiles.".to_string(),
        json!({
            "type": "object",
            "properties": {}
        }),
        RuntimeTool::AgentsList,
    );

    register_runtime_tool(
        &mut tools,
        &mut tool_map,
        SCHEDULER_JOBS_LIST_TOOL,
        "List scheduler jobs.".to_string(),
        json!({
            "type": "object",
            "properties": {
                "include_disabled": { "type": "boolean", "description": "Whether to include disabled jobs." }
            }
        }),
        RuntimeTool::SchedulerJobsList,
    );

    register_runtime_tool(
        &mut tools,
        &mut tool_map,
        SCHEDULER_JOB_CREATE_TOOL,
        "Create a scheduler job (at/every/cron).".to_string(),
        json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "description": { "type": "string" },
                "enabled": { "type": "boolean" },
                "schedule_kind": { "type": "string", "description": "at|every|cron" },
                "schedule_at": { "type": "string", "description": "RFC3339 time for at schedule." },
                "every_ms": { "type": "integer" },
                "cron_expr": { "type": "string" },
                "timezone": { "type": "string", "description": "IANA timezone for cron, e.g. Asia/Shanghai." },
                "session_target": { "type": "string", "description": "main|isolated" },
                "target_conversation_id": { "type": "string" },
                "message": { "type": "string" },
                "model_override": { "type": "string" },
                "workspace_directory": { "type": "string" },
                "tool_whitelist": { "type": "array", "items": { "type": "string" } },
                "run_timeout_seconds": { "type": "integer" },
                "delete_after_run": { "type": "boolean" }
            },
            "required": ["name", "schedule_kind", "session_target", "target_conversation_id", "message"]
        }),
        RuntimeTool::SchedulerJobCreate,
    );

    register_runtime_tool(
        &mut tools,
        &mut tool_map,
        SCHEDULER_JOB_UPDATE_TOOL,
        "Update an existing scheduler job.".to_string(),
        json!({
            "type": "object",
            "properties": {
                "job_id": { "type": "string" },
                "patch": { "type": "object" }
            },
            "required": ["job_id", "patch"]
        }),
        RuntimeTool::SchedulerJobUpdate,
    );

    register_runtime_tool(
        &mut tools,
        &mut tool_map,
        SCHEDULER_JOB_DELETE_TOOL,
        "Delete a scheduler job by id.".to_string(),
        json!({
            "type": "object",
            "properties": {
                "job_id": { "type": "string" }
            },
            "required": ["job_id"]
        }),
        RuntimeTool::SchedulerJobDelete,
    );

    register_runtime_tool(
        &mut tools,
        &mut tool_map,
        SCHEDULER_JOB_RUN_TOOL,
        "Run a scheduler job immediately.".to_string(),
        json!({
            "type": "object",
            "properties": {
                "job_id": { "type": "string" }
            },
            "required": ["job_id"]
        }),
        RuntimeTool::SchedulerJobRun,
    );

    register_runtime_tool(
        &mut tools,
        &mut tool_map,
        SCHEDULER_RUNS_LIST_TOOL,
        "List scheduler run history.".to_string(),
        json!({
            "type": "object",
            "properties": {
                "job_id": { "type": "string" },
                "limit": { "type": "integer" }
            }
        }),
        RuntimeTool::SchedulerRunsList,
    );

    (tools, tool_map)
}
