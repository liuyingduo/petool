use std::collections::HashMap;
use std::path::Path;

use serde_json::{json, Value};

use super::{
    ChatTool, ChatToolFunction, RuntimeTool, AGENTS_LIST_TOOL, BROWSER_NAVIGATE_TOOL,
    CORE_BATCH_TOOL, CORE_TASK_TOOL, IMAGE_PROBE_TOOL, SESSIONS_HISTORY_TOOL,
    SESSIONS_LIST_TOOL, SESSIONS_SEND_TOOL, SESSIONS_SPAWN_TOOL, SKILL_EXECUTE_TOOL,
    SKILL_INSTALL_TOOL, SKILL_LIST_TOOL, TODO_READ_TOOL, TODO_WRITE_TOOL, WEB_FETCH_TOOL,
    WEB_SEARCH_TOOL, WORKSPACE_APPLY_PATCH_TOOL, WORKSPACE_CODESEARCH_TOOL, WORKSPACE_EDIT_TOOL,
    WORKSPACE_GLOB_TOOL, WORKSPACE_GREP_TOOL, WORKSPACE_LIST_TOOL, WORKSPACE_LSP_SYMBOLS_TOOL,
    WORKSPACE_PROCESS_LIST_TOOL, WORKSPACE_PROCESS_READ_TOOL, WORKSPACE_PROCESS_START_TOOL,
    WORKSPACE_PROCESS_TERMINATE_TOOL, WORKSPACE_READ_TOOL, WORKSPACE_RUN_TOOL, WORKSPACE_WRITE_TOOL,
};

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
            "List files and subdirectories in the local workspace. Workspace root: {}",
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
            "Run a shell command in the local workspace and return stdout/stderr. Workspace root: {}",
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
        "Install a skill from a git repository URL so the assistant can use new capabilities."
            .to_string(),
        json!({
            "type": "object",
            "properties": {
                "repo_url": { "type": "string" }
            },
            "required": ["repo_url"]
        }),
        RuntimeTool::SkillInstallFromRepo,
    );

    register_runtime_tool(
        &mut tools,
        &mut tool_map,
        SKILL_LIST_TOOL,
        "List installed skills and their enabled status.".to_string(),
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
        "Execute an installed skill by id or name.".to_string(),
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
        BROWSER_NAVIGATE_TOOL,
        "Retrieve page title and links from a web URL (lightweight browser surrogate).".to_string(),
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

    (tools, tool_map)
}
