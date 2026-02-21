use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use tokio::sync::Mutex;

use crate::commands::mcp::McpState;
use crate::models::chat::{ChatTool, ChatToolFunction};
use crate::models::config::{Config, McpTransport};
use crate::services::mcp_client::{HttpTransport, McpClient, StdioTransport};

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

