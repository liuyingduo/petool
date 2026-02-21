use crate::commands::mcp::McpState;
use crate::commands::skills::SkillManagerState;
use crate::models::chat::*;
use crate::models::config::{
    Config, DesktopApprovalMode, ToolPathPermissionRule, ToolPermissionAction,
};
use crate::services::desktop;
use crate::services::llm::{
    reasoning_details_from_text, ChatMessage, ChatTool, ChatToolCall, LlmService,
    LlmStreamEvent, LlmStreamResult,
};
use crate::services::memory::prepare_memory_prompt_and_remember_turn;
use chrono::Utc;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::SqlitePool;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{Emitter, Window};
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::AppState;
use super::*;
use super::storage::*;
use super::llm_provider::*;
use super::tool_executor::*;
use super::commands::*;

pub(crate) type ToolApprovalSender = oneshot::Sender<ToolApprovalDecision>;


#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolApprovalDecision {
    AllowOnce,
    AllowAlways,
    Deny,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ToolApprovalRequestPayload {
    request_id: String,
    conversation_id: String,
    tool_call_id: String,
    tool_name: String,
    arguments: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateImageResponse {
    pub user_message: Message,
    pub assistant_message: Message,
    pub image_url: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadedAttachmentInput {
    pub path: String,
    pub name: Option<String>,
    pub size: Option<u64>,
    pub extension: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct UploadedAttachment {
    path: PathBuf,
    name: String,
    size: u64,
    extension: String,
}



pub(crate) const DEFAULT_WEB_ACCEPT_LANGUAGE: &str = "en-US,en;q=0.9,zh-CN;q=0.8,zh;q=0.7";
pub(crate) const DEFAULT_WEB_USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
pub(crate) const DEFAULT_EXA_MCP_ENDPOINT: &str = "https://mcp.exa.ai/mcp";
pub(crate) const REPEATED_TOOL_GUARD_TEXT: &str = "Detected repeated identical tool calls from the model. Automatic tool loop was stopped. Please provide a more specific target (file/directory) and try again.";
pub(crate) const STREAM_PAUSED_TEXT: &str = "（已暂停）";















pub(crate) fn prepend_system_prompt(messages: &mut Vec<ChatMessage>, system_prompt: Option<&str>) {
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
            reasoning_details: None,
            reasoning: None,
        },
    );
}

pub(crate) async fn maybe_prepare_memory_prompt(
    pool: &SqlitePool,
    config: &Config,
    model: &str,
    conversation_id: &str,
    user_content: &str,
) -> Option<String> {
    match prepare_memory_prompt_and_remember_turn(
        pool,
        config,
        model,
        conversation_id,
        user_content,
    )
    .await
    {
        Ok(prompt) => prompt,
        Err(error) => {
            eprintln!(
                "[memory] prepare prompt failed for conversation {}: {}",
                conversation_id, error
            );
            None
        }
    }
}

pub(crate) fn prepend_tool_usage_guidance(messages: &mut Vec<ChatMessage>) {
    let guidance = if cfg!(target_os = "windows") {
        "Tool selection policy (Windows): \
Prefer bash for filesystem-heavy tasks such as recursive traversal, counting files, computing folder size, extension/type statistics, sorting/filtering large file lists, and bulk inventory. \
Use concise PowerShell commands for these operations (for example: Get-ChildItem -Recurse -File | Measure-Object, or Get-ChildItem -Recurse | Measure-Object -Property Length -Sum). \
Use workspace_list_directory only for quick non-recursive inspection of one directory level or when the user explicitly asks to browse items manually. \
Desktop automation policy (UFO-style): \
Always discover controls before acting: get_desktop_app_info/list_windows -> select_application_window/select_window -> get_app_window_controls_info/get_controls(refresh=true) -> control action by exact id + exact name. \
Use canonical action args: set_edit_text(text), keyboard_input(keys, control_focus), wheel_mouse_input(wheel_dist), select_application_window(id,name). \
Browser policy: all browser lifecycle/navigation/page actions must use tool=browser only; never use desktop.launch_application or desktop.close_application for browsers. \
For browser action=act or action=act_batch, action items must use field `kind` (not `action`). \
Use click_on_coordinates only as fallback when the target control is missing from control list."
    } else {
        "Tool selection policy: \
Prefer bash for filesystem-heavy tasks such as recursive traversal, counting files, computing folder size, extension/type statistics, sorting/filtering large file lists, and bulk inventory. \
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
            reasoning_details: None,
            reasoning: None,
        },
    );
}

pub(crate) fn prepend_skills_usage_guidance(messages: &mut Vec<ChatMessage>, skills_guidance: &str) {
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
            reasoning_details: None,
            reasoning: None,
        },
    );
}

pub(crate) fn normalize_uploaded_attachments(
    attachments: Option<Vec<UploadedAttachmentInput>>,
    workspace_root: &Path,
) -> Result<Vec<UploadedAttachment>, String> {
    let Some(items) = attachments else {
        return Ok(Vec::new());
    };

    let mut normalized = Vec::new();
    for item in items {
        let raw_path = item.path.trim();
        if raw_path.is_empty() {
            continue;
        }

        let resolved = resolve_workspace_target(workspace_root, raw_path, false)?;
        if !resolved.is_file() {
            return Err(format!("Attachment is not a file: {}", resolved.display()));
        }

        let metadata = fs::metadata(&resolved).map_err(|e| e.to_string())?;
        let file_name = item
            .name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string())
            .or_else(|| {
                resolved
                    .file_name()
                    .and_then(|value| value.to_str())
                    .map(|value| value.to_string())
            })
            .unwrap_or_else(|| resolved.to_string_lossy().to_string());
        let extension = item
            .extension
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_ascii_lowercase())
            .or_else(|| {
                resolved
                    .extension()
                    .and_then(|value| value.to_str())
                    .map(|value| value.to_ascii_lowercase())
            })
            .unwrap_or_default();
        let size = item.size.unwrap_or(metadata.len());

        normalized.push(UploadedAttachment {
            path: resolved,
            name: file_name,
            size,
            extension,
        });
    }

    Ok(normalized)
}

pub(crate) fn build_uploaded_attachments_guidance(
    attachments: &[UploadedAttachment],
    workspace_root: &Path,
) -> Option<String> {
    if attachments.is_empty() {
        return None;
    }

    let mut lines = Vec::new();
    lines.push("Uploaded files for this turn (hidden context, do not echo verbatim):".to_string());
    lines.push(format!(
        "- Workspace root: {}",
        workspace_root.to_string_lossy()
    ));
    lines.push("- Use uploaded files as primary context for the current user request.".to_string());

    let mut has_pdf = false;
    for (index, item) in attachments.iter().enumerate() {
        if item.extension == "pdf" {
            has_pdf = true;
        }
        let relative_path = workspace_relative_display_path(workspace_root, &item.path);
        lines.push(format!(
            "{}. {} | path: {} | size: {} bytes | ext: {}",
            index + 1,
            item.name,
            relative_path,
            item.size,
            if item.extension.is_empty() {
                "(none)"
            } else {
                item.extension.as_str()
            }
        ));
    }

    if has_pdf {
        lines.push("For PDF files, call `workspace_parse_pdf_markdown` first (export_images=true by default), then use returned markdown/image paths for analysis.".to_string());
    }

    Some(lines.join("\n"))
}

pub(crate) fn prepend_uploaded_attachments_guidance(
    messages: &mut Vec<ChatMessage>,
    attachments: &[UploadedAttachment],
    workspace_root: &Path,
) {
    let Some(guidance) = build_uploaded_attachments_guidance(attachments, workspace_root) else {
        return;
    };

    messages.insert(
        0,
        ChatMessage {
            role: "system".to_string(),
            content: Some(guidance),
            tool_calls: None,
            tool_call_id: None,
            reasoning_details: None,
            reasoning: None,
        },
    );
}

pub(crate) async fn build_skills_usage_guidance(skill_manager_state: &SkillManagerState) -> String {
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



pub(crate) struct StreamRoundOutput {
    pub(crate) stream_result: LlmStreamResult,
    pub(crate) timeline_events: Vec<PendingTimelineEvent>,
}

pub(crate) async fn run_stream_round(
    llm_service: &LlmService,
    window: &Window,
    conversation_id: &str,
    turn_id: &str,
    seq_counter: &mut i64,
    model: &str,
    context_messages: Vec<ChatMessage>,
    available_tools: &[ChatTool],
    stop_flag: Arc<AtomicBool>,
) -> Result<StreamRoundOutput, String> {
    let conversation_id_for_stream = conversation_id.to_string();
    let turn_id_for_stream = turn_id.to_string();
    let window_for_stream = window.clone();
    let mut timeline_events: Vec<PendingTimelineEvent> = Vec::new();
    let mut stream_tool_call_ids_by_index: HashMap<usize, String> = HashMap::new();
    let mut stream_tool_call_names_by_index: HashMap<usize, String> = HashMap::new();
    let stream_result = llm_service
        .chat_stream_with_tools(
            model,
            context_messages,
            if available_tools.is_empty() {
                None
            } else {
                Some(available_tools.to_vec())
            },
            |event| match event {
                LlmStreamEvent::Content(chunk) => {
                    if !chunk.is_empty() {
                        *seq_counter += 1;
                        let seq = *seq_counter;
                        let created_at = Utc::now().to_rfc3339();
                        timeline_events.push(PendingTimelineEvent {
                            turn_id: turn_id_for_stream.clone(),
                            seq,
                            event_type: TimelineEventType::AssistantText,
                            tool_call_id: None,
                            payload: json!({ "text": chunk.clone() }),
                            created_at: created_at.clone(),
                        });
                        let _ = window_for_stream.emit(
                            "chat-chunk",
                            json!({
                                "conversationId": conversation_id_for_stream.clone(),
                                "turnId": turn_id_for_stream.clone(),
                                "seq": seq,
                                "eventType": "assistant_text",
                                "createdAt": created_at,
                                "chunk": chunk
                            }),
                        );
                    }
                }
                LlmStreamEvent::Reasoning(chunk) => {
                    if !chunk.is_empty() {
                        *seq_counter += 1;
                        let seq = *seq_counter;
                        let created_at = Utc::now().to_rfc3339();
                        timeline_events.push(PendingTimelineEvent {
                            turn_id: turn_id_for_stream.clone(),
                            seq,
                            event_type: TimelineEventType::AssistantReasoning,
                            tool_call_id: None,
                            payload: json!({ "text": chunk.clone() }),
                            created_at: created_at.clone(),
                        });
                        let _ = window_for_stream.emit(
                            "chat-reasoning",
                            json!({
                                "conversationId": conversation_id_for_stream.clone(),
                                "turnId": turn_id_for_stream.clone(),
                                "seq": seq,
                                "eventType": "assistant_reasoning",
                                "createdAt": created_at,
                                "chunk": chunk
                            }),
                        );
                    }
                }
                LlmStreamEvent::ToolCallDelta(delta) => {
                    if let Some(id) = delta
                        .id
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                    {
                        stream_tool_call_ids_by_index.insert(delta.index, id.to_string());
                    }
                    if let Some(name) = delta
                        .name
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                    {
                        stream_tool_call_names_by_index.insert(delta.index, name.to_string());
                    }

                    let resolved_tool_call_id = stream_tool_call_ids_by_index
                        .entry(delta.index)
                        .or_insert_with(|| {
                            format!("{}_tool_call_{}", turn_id_for_stream, delta.index)
                        })
                        .clone();
                    let resolved_name = stream_tool_call_names_by_index.get(&delta.index).cloned();

                    *seq_counter += 1;
                    let seq = *seq_counter;
                    let created_at = Utc::now().to_rfc3339();
                    timeline_events.push(PendingTimelineEvent {
                        turn_id: turn_id_for_stream.clone(),
                        seq,
                        event_type: TimelineEventType::AssistantToolCall,
                        tool_call_id: Some(resolved_tool_call_id.clone()),
                        payload: json!({
                            "index": delta.index,
                            "name": resolved_name,
                            "argumentsChunk": delta.arguments_chunk
                        }),
                        created_at: created_at.clone(),
                    });
                    let payload = json!({
                        "conversationId": conversation_id_for_stream.clone(),
                        "turnId": turn_id_for_stream.clone(),
                        "seq": seq,
                        "eventType": "assistant_tool_call",
                        "createdAt": created_at,
                        "index": delta.index,
                        "toolCallId": resolved_tool_call_id,
                        "name": resolved_name,
                        "argumentsChunk": delta.arguments_chunk,
                    });
                    let _ = window_for_stream.emit("chat-tool-call", payload);
                }
            },
            move || stop_flag.load(Ordering::Relaxed),
        )
        .await
        .map_err(|e| e.to_string())?;

    Ok(StreamRoundOutput {
        stream_result,
        timeline_events,
    })
}






pub(crate) async fn resolve_tool_execution_decision(
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
    let configured_action = resolve_desktop_permission_action(
        config,
        &tool_call.function.name,
        parsed_arguments,
        configured_action,
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
            } else if tool_call.function.name == WORKSPACE_PARSE_PDF_TOOL {
                ToolApprovalDecision::AllowOnce
            } else {
                request_tool_approval(window, conversation_id, tool_call).await?
            }
        }
    })
}

#[derive(Debug, Clone)]
pub(crate) struct BackgroundAgentRunRequest {
    pub target_conversation_id: String,
    pub content: String,
    pub workspace_directory: Option<String>,
    pub model_override: Option<String>,
    pub persist_main_context: bool,
    pub tool_whitelist: Option<HashSet<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BackgroundAgentRunResult {
    pub content: String,
    pub reasoning: Option<String>,
    pub rounds: usize,
    pub tool_calls: usize,
    pub blocked_tools: usize,
    pub guard_stopped: bool,
}

fn is_tool_allowed_by_scheduler_whitelist(
    whitelist: &HashSet<String>,
    tool_name: &str,
    parsed_arguments: &Value,
) -> bool {
    if whitelist.is_empty() {
        return false;
    }
    if whitelist.contains(tool_name) {
        return true;
    }
    if whitelist
        .iter()
        .any(|pattern| wildcard_match(pattern, tool_name))
    {
        return true;
    }
    let path_candidate = extract_tool_path_argument(parsed_arguments).unwrap_or_default();
    if !path_candidate.is_empty()
        && whitelist
            .iter()
            .any(|pattern| wildcard_match(pattern, &path_candidate))
    {
        return true;
    }
    false
}

fn resolve_background_tool_execution_decision(
    config: &Config,
    tool_call: &ChatToolCall,
    parsed_arguments: &Value,
    always_allowed_tools: &HashSet<String>,
    whitelist: Option<&HashSet<String>>,
) -> ToolApprovalDecision {
    let configured_action = resolve_tool_permission_action(
        config,
        &tool_call.function.name,
        extract_tool_path_argument(parsed_arguments).as_deref(),
    );
    let configured_action = resolve_desktop_permission_action(
        config,
        &tool_call.function.name,
        parsed_arguments,
        configured_action,
    );

    if configured_action == ToolPermissionAction::Deny {
        return ToolApprovalDecision::Deny;
    }

    if let Some(whitelist) = whitelist {
        if !is_tool_allowed_by_scheduler_whitelist(
            whitelist,
            &tool_call.function.name,
            parsed_arguments,
        ) {
            return ToolApprovalDecision::Deny;
        }
        return ToolApprovalDecision::AllowAlways;
    }

    match configured_action {
        ToolPermissionAction::Allow => ToolApprovalDecision::AllowAlways,
        ToolPermissionAction::Ask => {
            if config.auto_approve_tool_requests
                || always_allowed_tools.contains(&tool_call.function.name)
            {
                ToolApprovalDecision::AllowAlways
            } else {
                ToolApprovalDecision::Deny
            }
        }
        ToolPermissionAction::Deny => ToolApprovalDecision::Deny,
    }
}


pub(crate) async fn run_agent_turn_background(
    state: AppState,
    mcp_state: McpState,
    skill_state: SkillManagerState,
    request: BackgroundAgentRunRequest,
) -> Result<BackgroundAgentRunResult, String> {
    let content = request.content.trim().to_string();
    if content.is_empty() {
        return Err("Background run content cannot be empty".to_string());
    }

    let config = crate::utils::load_config::<Config>().map_err(|e| e.to_string())?;
    let workspace_root = resolve_workspace_root(&config, request.workspace_directory.as_deref())?;
    let RuntimeToolCatalog {
        available_tools,
        tool_map,
    } = build_runtime_tool_catalog(&mcp_state, &config, &workspace_root).await?;

    let pool = state.lock().await.db().pool().clone();

    let model_to_use = if let Some(override_model) = request
        .model_override
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        override_model.to_string()
    } else if request.persist_main_context {
        resolve_conversation_model(&pool, &request.target_conversation_id, &config.model).await?
    } else {
        config.model.clone()
    };

    let llm_service = resolve_text_llm_service(&config, &model_to_use)?;

    let mut context_messages = if request.persist_main_context {
        load_conversation_context(&pool, &request.target_conversation_id).await?
    } else {
        Vec::new()
    };
    prepend_system_prompt(&mut context_messages, config.system_prompt.as_deref());
    prepend_tool_usage_guidance(&mut context_messages);
    let skills_guidance = build_skills_usage_guidance(&skill_state).await;
    prepend_skills_usage_guidance(&mut context_messages, &skills_guidance);

    if request.persist_main_context {
        insert_message(
            &pool,
            &request.target_conversation_id,
            "user",
            &content,
            None,
            None,
        )
        .await?;
    }

    context_messages.push(ChatMessage {
        role: "user".to_string(),
        content: Some(content),
        tool_calls: None,
        tool_call_id: None,
        reasoning_details: None,
        reasoning: None,
    });

    let mut always_allowed_tools = HashSet::<String>::new();
    let mut last_tool_signature: Option<String> = None;
    let mut repeated_signature_rounds = 0usize;
    let mut rounds = 0usize;
    let mut total_tool_calls = 0usize;
    let mut blocked_tools = 0usize;
    let mut guard_stopped = false;

    loop {
        rounds += 1;
        let stream_result = llm_service
            .chat_stream_with_tools(
                &model_to_use,
                context_messages.clone(),
                if available_tools.is_empty() {
                    None
                } else {
                    Some(available_tools.clone())
                },
                |_| {},
                || false,
            )
            .await
            .map_err(|e| e.to_string())?;

        let assistant_content = stream_result.content.clone();
        let assistant_reasoning = if stream_result.reasoning.trim().is_empty() {
            None
        } else {
            Some(stream_result.reasoning.clone())
        };
        let assistant_reasoning_details = stream_result.reasoning_details.clone().or_else(|| {
            assistant_reasoning
                .as_deref()
                .and_then(reasoning_details_from_text)
        });

        if stream_result.tool_calls.is_empty() {
            if request.persist_main_context {
                insert_message(
                    &pool,
                    &request.target_conversation_id,
                    "assistant",
                    &assistant_content,
                    None,
                    assistant_reasoning.clone(),
                )
                .await?;
            }

            return Ok(BackgroundAgentRunResult {
                content: assistant_content,
                reasoning: assistant_reasoning,
                rounds,
                tool_calls: total_tool_calls,
                blocked_tools,
                guard_stopped,
            });
        }

        total_tool_calls += stream_result.tool_calls.len();
        let assistant_tool_calls_json =
            serde_json::to_string(&stream_result.tool_calls).map_err(|e| e.to_string())?;
        if request.persist_main_context {
            insert_message(
                &pool,
                &request.target_conversation_id,
                "assistant",
                &assistant_content,
                Some(assistant_tool_calls_json),
                assistant_reasoning.clone(),
            )
            .await?;
        }

        context_messages.push(ChatMessage {
            role: "assistant".to_string(),
            content: if assistant_content.is_empty() {
                None
            } else {
                Some(assistant_content)
            },
            tool_calls: Some(stream_result.tool_calls.clone()),
            tool_call_id: None,
            reasoning_details: assistant_reasoning_details,
            reasoning: assistant_reasoning,
        });

        update_repeated_signature_rounds(
            &mut last_tool_signature,
            &mut repeated_signature_rounds,
            build_tool_round_signature(&stream_result.tool_calls),
        );

        if repeated_signature_rounds >= 2 {
            guard_stopped = true;
            let guard_text = REPEATED_TOOL_GUARD_TEXT.to_string();
            if request.persist_main_context {
                insert_message(
                    &pool,
                    &request.target_conversation_id,
                    "assistant",
                    &guard_text,
                    None,
                    None,
                )
                .await?;
            }
            return Ok(BackgroundAgentRunResult {
                content: guard_text,
                reasoning: None,
                rounds,
                tool_calls: total_tool_calls,
                blocked_tools,
                guard_stopped,
            });
        }

        for tool_call in stream_result.tool_calls {
            let parsed_arguments = parse_tool_arguments(&tool_call.function.arguments);
            let decision = resolve_background_tool_execution_decision(
                &config,
                &tool_call,
                &parsed_arguments,
                &always_allowed_tools,
                request.tool_whitelist.as_ref(),
            );

            if decision == ToolApprovalDecision::Deny {
                blocked_tools += 1;
                let error_text = format!(
                    "Background scheduler denied tool '{}' by whitelist/policy",
                    tool_call.function.name
                );
                let result_text = format_tool_error_result(&error_text)?;
                if request.persist_main_context {
                    persist_tool_result_message(
                        &pool,
                        &request.target_conversation_id,
                        &mut context_messages,
                        &tool_call,
                        result_text,
                    )
                    .await?;
                } else {
                    push_background_tool_result_message(
                        &mut context_messages,
                        &tool_call,
                        result_text,
                    );
                }
                continue;
            }

            always_allowed_tools.insert(tool_call.function.name.clone());

            let tool_result = execute_tool_call_background(
                &mcp_state,
                &skill_state,
                &config,
                &tool_map,
                &tool_call,
                &workspace_root,
                &request.target_conversation_id,
                &pool,
                &llm_service,
                &model_to_use,
            )
            .await;

            match tool_result {
                Ok(value) => {
                    let result_text =
                        serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string());
                    if request.persist_main_context {
                        persist_tool_result_message(
                            &pool,
                            &request.target_conversation_id,
                            &mut context_messages,
                            &tool_call,
                            result_text,
                        )
                        .await?;
                    } else {
                        push_background_tool_result_message(
                            &mut context_messages,
                            &tool_call,
                            result_text,
                        );
                    }
                }
                Err(error_text) => {
                    let result_text = format_tool_error_result(&error_text)?;
                    if request.persist_main_context {
                        persist_tool_result_message(
                            &pool,
                            &request.target_conversation_id,
                            &mut context_messages,
                            &tool_call,
                            result_text,
                        )
                        .await?;
                    } else {
                        push_background_tool_result_message(
                            &mut context_messages,
                            &tool_call,
                            result_text,
                        );
                    }
                }
            }
        }
    }
}

pub(crate) fn build_tool_round_signature(tool_calls: &[ChatToolCall]) -> String {
    serde_json::to_string(
        &tool_calls
            .iter()
            .map(|call| (call.function.name.clone(), call.function.arguments.clone()))
            .collect::<Vec<_>>(),
    )
    .unwrap_or_default()
}

pub(crate) fn update_repeated_signature_rounds(
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

fn resolve_desktop_permission_action(
    config: &Config,
    tool_name: &str,
    parsed_arguments: &Value,
    fallback: ToolPermissionAction,
) -> ToolPermissionAction {
    if tool_name != DESKTOP_TOOL {
        return fallback;
    }
    if fallback != ToolPermissionAction::Ask {
        return fallback;
    }

    let Some(action) = desktop::action_from_arguments(parsed_arguments) else {
        return ToolPermissionAction::Ask;
    };

    match config.desktop.approval_mode {
        DesktopApprovalMode::AlwaysAllow => ToolPermissionAction::Allow,
        DesktopApprovalMode::AlwaysAsk => ToolPermissionAction::Ask,
        DesktopApprovalMode::HighRiskOnly => {
            if desktop::is_high_risk_action(&action) {
                ToolPermissionAction::Ask
            } else {
                ToolPermissionAction::Allow
            }
        }
    }
}

fn extract_tool_path_argument(arguments: &Value) -> Option<String> {
    read_optional_string_argument(arguments, "path")
        .or_else(|| read_optional_string_argument(arguments, "application_path"))
        .or_else(|| read_optional_string_argument(arguments, "app_path"))
        .or_else(|| read_optional_string_argument(arguments, "executable"))
        .or_else(|| read_optional_string_argument(arguments, "cwd"))
        .or_else(|| read_optional_string_argument(arguments, "directory"))
        .or_else(|| {
            arguments
                .get("params")
                .and_then(Value::as_object)
                .and_then(|params| {
                    params
                        .get("path")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(|value| value.to_string())
                })
        })
        .or_else(|| {
            arguments
                .get("params")
                .and_then(Value::as_object)
                .and_then(|params| {
                    params
                        .get("application_path")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(|value| value.to_string())
                })
        })
        .or_else(|| {
            arguments
                .get("params")
                .and_then(Value::as_object)
                .and_then(|params| {
                    params
                        .get("app_path")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(|value| value.to_string())
                })
        })
        .or_else(|| {
            arguments
                .get("params")
                .and_then(Value::as_object)
                .and_then(|params| {
                    params
                        .get("executable")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(|value| value.to_string())
                })
        })
        .or_else(|| {
            arguments
                .get("params")
                .and_then(Value::as_object)
                .and_then(|params| {
                    params
                        .get("cwd")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(|value| value.to_string())
                })
        })
        .or_else(|| {
            arguments
                .get("params")
                .and_then(Value::as_object)
                .and_then(|params| {
                    params
                        .get("directory")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(|value| value.to_string())
                })
        })
}

pub(crate) fn should_auto_allow_batch_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        WORKSPACE_LIST_TOOL
            | WORKSPACE_READ_TOOL
            | WORKSPACE_PARSE_PDF_TOOL
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

pub(crate) fn is_forbidden_loopback_host(url: &reqwest::Url) -> bool {
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

