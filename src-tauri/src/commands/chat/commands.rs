use crate::commands::mcp::McpState;
use crate::commands::skills::SkillManagerState;
use crate::models::chat::*;
use crate::models::config::Config;
use crate::services::llm::{reasoning_details_from_text, ChatMessage, ChatToolCall};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

use tauri::{Emitter, State, Window};
use tauri_plugin_notification::NotificationExt;

use uuid::Uuid;
use chrono::Utc;
use crate::AppState;
use super::*;
use super::storage::*;
use super::llm_provider::*;
use super::tool_executor::*;

static TOOL_APPROVAL_WAITERS: OnceLock<tokio::sync::Mutex<HashMap<String, ToolApprovalSender>>> =
    OnceLock::new();
static STREAM_STOP_FLAGS: OnceLock<tokio::sync::Mutex<HashMap<String, Arc<AtomicBool>>>> =
    OnceLock::new();

pub(crate) fn tool_approval_waiters() -> &'static tokio::sync::Mutex<HashMap<String, ToolApprovalSender>> {
    TOOL_APPROVAL_WAITERS.get_or_init(|| tokio::sync::Mutex::new(HashMap::new()))
}

pub(crate) fn stream_stop_flags() -> &'static tokio::sync::Mutex<HashMap<String, Arc<AtomicBool>>> {
    STREAM_STOP_FLAGS.get_or_init(|| tokio::sync::Mutex::new(HashMap::new()))
}

pub(crate) async fn register_stream_stop_flag(conversation_id: &str) -> Arc<AtomicBool> {
    let flag = Arc::new(AtomicBool::new(false));
    let mut flags = stream_stop_flags().lock().await;
    flags.insert(conversation_id.to_string(), flag.clone());
    flag
}

pub(crate) async fn clear_stream_stop_flag(conversation_id: &str) {
    let mut flags = stream_stop_flags().lock().await;
    flags.remove(conversation_id);
}

pub(crate) async fn request_stream_stop(conversation_id: &str) -> bool {
    let flags = stream_stop_flags().lock().await;
    if let Some(flag) = flags.get(conversation_id) {
        flag.store(true, Ordering::Relaxed);
        return true;
    }
    false
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
        let guard = state.lock().await;
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

    let image_turn_id = Uuid::new_v4().to_string();
    insert_timeline_event(
        &pool,
        &conversation_id,
        &PendingTimelineEvent {
            turn_id: image_turn_id.clone(),
            seq: 1,
            event_type: TimelineEventType::UserMessage,
            tool_call_id: None,
            payload: json!({ "content": user_content.clone() }),
            created_at: user_created_at.to_rfc3339(),
        },
    )
    .await?;
    insert_timeline_event(
        &pool,
        &conversation_id,
        &PendingTimelineEvent {
            turn_id: image_turn_id,
            seq: 2,
            event_type: TimelineEventType::AssistantText,
            tool_call_id: None,
            payload: json!({ "text": assistant_content.clone() }),
            created_at: assistant_created_at.to_rfc3339(),
        },
    )
    .await?;

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
        let guard = state.lock().await;
        guard.db().pool().clone()
    };
    let model_to_use = resolve_conversation_model(&pool, &conversation_id, &config.model).await?;
    let llm_service = resolve_text_llm_service(&config, &model_to_use)?;
    let turn_id = Uuid::new_v4().to_string();
    let mut seq: i64 = 0;

    insert_message(&pool, &conversation_id, "user", &content, None, None).await?;
    seq += 1;
    insert_timeline_event(
        &pool,
        &conversation_id,
        &PendingTimelineEvent {
            turn_id: turn_id.clone(),
            seq,
            event_type: TimelineEventType::UserMessage,
            tool_call_id: None,
            payload: json!({ "content": content }),
            created_at: Utc::now().to_rfc3339(),
        },
    )
    .await?;

    let mut messages = load_conversation_context(&pool, &conversation_id).await?;
    prepend_system_prompt(&mut messages, config.system_prompt.as_deref());
    if let Some(memory_prompt) =
        maybe_prepare_memory_prompt(&pool, &config, &model_to_use, &conversation_id, &content).await
    {
        prepend_system_prompt(&mut messages, Some(memory_prompt.as_str()));
    }

    let response = llm_service
        .chat(&model_to_use, messages)
        .await
        .map_err(|e| e.to_string())?;

    insert_message(&pool, &conversation_id, "assistant", &response, None, None).await?;
    seq += 1;
    insert_timeline_event(
        &pool,
        &conversation_id,
        &PendingTimelineEvent {
            turn_id,
            seq,
            event_type: TimelineEventType::AssistantText,
            tool_call_id: None,
            payload: json!({ "text": response.clone() }),
            created_at: Utc::now().to_rfc3339(),
        },
    )
    .await?;

    Ok(response)
}

fn maybe_notify_stream_end(window: &Window, conversation_id: &str) {
    let focused = window.is_focused().unwrap_or(true);
    if focused {
        return;
    }

    if let Err(error) = window
        .notification()
        .builder()
        .title("PETool")
        .body("AI 回复已结束")
        .show()
    {
        eprintln!(
            "failed to send stream end notification for conversation {}: {}",
            conversation_id, error
        );
    }
}

fn emit_chat_end_with_notification(window: &Window, conversation_id: &str) -> Result<(), String> {
    window
        .emit("chat-end", json!({ "conversationId": conversation_id }))
        .map_err(|e| e.to_string())?;
    maybe_notify_stream_end(window, conversation_id);
    Ok(())
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
    attachments: Option<Vec<UploadedAttachmentInput>>,
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
        let uploaded_attachments = normalize_uploaded_attachments(attachments, &workspace_root)?;

        let pool = {
            let guard = state.lock().await;
            guard.db().pool().clone()
        };
        let model_to_use =
            resolve_conversation_model(&pool, &conversation_id, &config.model).await?;
        let llm_service = resolve_text_llm_service(&config, &model_to_use)?;
        let turn_id = Uuid::new_v4().to_string();
        let mut seq: i64 = 0;

        insert_message(&pool, &conversation_id, "user", &content, None, None).await?;
        seq += 1;
        let user_event_created_at = Utc::now().to_rfc3339();
        let user_event = PendingTimelineEvent {
            turn_id: turn_id.clone(),
            seq,
            event_type: TimelineEventType::UserMessage,
            tool_call_id: None,
            payload: json!({ "content": content }),
            created_at: user_event_created_at.clone(),
        };
        insert_timeline_event(&pool, &conversation_id, &user_event).await?;
        let _ = window.emit(
            "chat-user-message",
            json!({
                "conversationId": conversation_id,
                "turnId": turn_id,
                "seq": seq,
                "eventType": "user_message",
                "createdAt": user_event_created_at,
                "content": content
            }),
        );

        let mut context_messages = load_conversation_context(&pool, &conversation_id).await?;
        prepend_system_prompt(&mut context_messages, config.system_prompt.as_deref());
        if let Some(memory_prompt) =
            maybe_prepare_memory_prompt(&pool, &config, &model_to_use, &conversation_id, &content)
                .await
        {
            prepend_system_prompt(&mut context_messages, Some(memory_prompt.as_str()));
        }
        prepend_tool_usage_guidance(&mut context_messages);
        prepend_skills_usage_guidance(&mut context_messages, &skills_guidance);
        prepend_uploaded_attachments_guidance(
            &mut context_messages,
            &uploaded_attachments,
            &workspace_root,
        );

        let mut always_allowed_tools = HashSet::<String>::new();
        let mut last_tool_signature: Option<String> = None;
        let mut repeated_signature_rounds = 0usize;

        loop {
            if stop_flag.load(Ordering::Relaxed) {
                emit_chat_end_with_notification(&window, &conversation_id)?;
                return Ok(());
            }

            let stream_round = run_stream_round(
                &llm_service,
                &window,
                &conversation_id,
                &turn_id,
                &mut seq,
                &model_to_use,
                context_messages.clone(),
                &available_tools,
                stop_flag.clone(),
            )
            .await?;
            insert_timeline_events(&pool, &conversation_id, &stream_round.timeline_events).await?;
            let stream_result = stream_round.stream_result;

            let assistant_content = stream_result.content.clone();
            let assistant_reasoning = if stream_result.reasoning.trim().is_empty() {
                None
            } else {
                Some(stream_result.reasoning.clone())
            };
            let assistant_reasoning_details =
                stream_result.reasoning_details.clone().or_else(|| {
                    assistant_reasoning
                        .as_deref()
                        .and_then(reasoning_details_from_text)
                });

            if stream_result.cancelled {
                for tool_call in &stream_result.tool_calls {
                    let _ = emit_and_record_tool_result_event(
                        &pool,
                        &window,
                        &conversation_id,
                        &turn_id,
                        &mut seq,
                        tool_call,
                        None,
                        Some("Paused by user"),
                    )
                    .await;
                }

                let paused_content = if assistant_content.trim().is_empty() {
                    seq += 1;
                    let paused_created_at = Utc::now().to_rfc3339();
                    window
                        .emit(
                            "chat-chunk",
                            json!({
                                "conversationId": conversation_id,
                                "turnId": turn_id,
                                "seq": seq,
                                "eventType": "assistant_text",
                                "createdAt": paused_created_at,
                                "chunk": STREAM_PAUSED_TEXT
                            }),
                        )
                        .map_err(|e| e.to_string())?;
                    insert_timeline_event(
                        &pool,
                        &conversation_id,
                        &PendingTimelineEvent {
                            turn_id: turn_id.clone(),
                            seq,
                            event_type: TimelineEventType::AssistantText,
                            tool_call_id: None,
                            payload: json!({ "text": STREAM_PAUSED_TEXT }),
                            created_at: paused_created_at,
                        },
                    )
                    .await?;
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
                emit_chat_end_with_notification(&window, &conversation_id)?;
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
                emit_chat_end_with_notification(&window, &conversation_id)?;
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
                reasoning_details: assistant_reasoning_details,
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
                seq += 1;
                let guard_created_at = Utc::now().to_rfc3339();
                window
                    .emit(
                        "chat-chunk",
                        json!({
                            "conversationId": conversation_id,
                            "turnId": turn_id,
                            "seq": seq,
                            "eventType": "assistant_text",
                            "createdAt": guard_created_at,
                            "chunk": guard_text
                        }),
                    )
                    .map_err(|e| e.to_string())?;
                insert_timeline_event(
                    &pool,
                    &conversation_id,
                    &PendingTimelineEvent {
                        turn_id: turn_id.clone(),
                        seq,
                        event_type: TimelineEventType::AssistantText,
                        tool_call_id: None,
                        payload: json!({ "text": guard_text }),
                        created_at: guard_created_at,
                    },
                )
                .await?;
                insert_message(&pool, &conversation_id, "assistant", guard_text, None, None)
                    .await?;
                emit_chat_end_with_notification(&window, &conversation_id)?;
                return Ok(());
            }

            let mut cancelled_during_tools = false;
            for tool_call in stream_result.tool_calls {
                if stop_flag.load(Ordering::Relaxed) {
                    cancelled_during_tools = true;
                    emit_and_record_tool_result_event(
                        &pool,
                        &window,
                        &conversation_id,
                        &turn_id,
                        &mut seq,
                        &tool_call,
                        None,
                        Some("Paused by user"),
                    )
                    .await?;
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
                    emit_and_record_tool_result_event(
                        &pool,
                        &window,
                        &conversation_id,
                        &turn_id,
                        &mut seq,
                        &tool_call,
                        None,
                        Some("Paused by user"),
                    )
                    .await?;
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
                        emit_and_record_tool_result_event(
                            &pool,
                            &window,
                            &conversation_id,
                            &turn_id,
                            &mut seq,
                            &tool_call,
                            None,
                            Some(&error_text),
                        )
                        .await?;
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
                        emit_and_record_tool_result_event(
                            &pool,
                            &window,
                            &conversation_id,
                            &turn_id,
                            &mut seq,
                            &tool_call,
                            Some(&result_text),
                            None,
                        )
                        .await?;
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
                        emit_and_record_tool_result_event(
                            &pool,
                            &window,
                            &conversation_id,
                            &turn_id,
                            &mut seq,
                            &tool_call,
                            None,
                            Some(&error_text),
                        )
                        .await?;
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
                emit_chat_end_with_notification(&window, &conversation_id)?;
                return Ok(());
            }
        }
    }
    .await;

    clear_stream_stop_flag(&conversation_id).await;
    if result.is_err() {
        let _ = emit_chat_end_with_notification(&window, &conversation_id);
    }
    result
}

#[tauri::command]
pub async fn get_conversations(state: State<'_, AppState>) -> Result<Vec<Conversation>, String> {
    let pool = {
        let guard = state.lock().await;
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
        let guard = state.lock().await;
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

fn parse_timeline_event_type(raw: &str) -> TimelineEventType {
    match raw {
        "user_message" => TimelineEventType::UserMessage,
        "assistant_reasoning" => TimelineEventType::AssistantReasoning,
        "assistant_tool_call" => TimelineEventType::AssistantToolCall,
        "assistant_tool_result" => TimelineEventType::AssistantToolResult,
        _ => TimelineEventType::AssistantText,
    }
}

#[tauri::command]
pub async fn get_conversation_timeline(
    state: State<'_, AppState>,
    conversation_id: String,
) -> Result<ConversationTimeline, String> {
    let pool = {
        let guard = state.lock().await;
        guard.db().pool().clone()
    };

    let event_rows = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            i64,
            String,
            Option<String>,
            String,
            String,
        ),
    >(
        "SELECT id, conversation_id, turn_id, seq, event_type, tool_call_id, payload, created_at
         FROM message_events
         WHERE conversation_id = ?
         ORDER BY created_at ASC, seq ASC",
    )
    .bind(&conversation_id)
    .fetch_all(&pool)
    .await
    .map_err(|e| e.to_string())?;

    if !event_rows.is_empty() {
        let events = event_rows
            .into_iter()
            .map(
                |(
                    id,
                    event_conversation_id,
                    turn_id,
                    seq,
                    event_type_raw,
                    tool_call_id,
                    payload_raw,
                    created_at_raw,
                )| TimelineEvent {
                    id,
                    conversation_id: event_conversation_id,
                    turn_id,
                    seq,
                    event_type: parse_timeline_event_type(&event_type_raw),
                    tool_call_id,
                    payload: serde_json::from_str(&payload_raw).unwrap_or_else(|_| json!({})),
                    created_at: created_at_raw.parse().unwrap_or_else(|_| Utc::now()),
                },
            )
            .collect();

        return Ok(ConversationTimeline {
            events,
            legacy: false,
        });
    }

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
        "SELECT id, role, content, created_at, conversation_id, tool_calls, reasoning
         FROM messages
         WHERE conversation_id = ?
         ORDER BY created_at ASC",
    )
    .bind(&conversation_id)
    .fetch_all(&pool)
    .await
    .map_err(|e| e.to_string())?;

    let mut events: Vec<TimelineEvent> = Vec::new();
    let mut seq: i64 = 0;
    let mut turn_number: i64 = 0;
    let mut current_turn_id = "legacy-turn-0".to_string();

    for (
        message_id,
        role_raw,
        content,
        created_at_raw,
        event_conversation_id,
        tool_calls_raw,
        reasoning_raw,
    ) in rows
    {
        let created_at = created_at_raw.parse().unwrap_or_else(|_| Utc::now());
        match role_raw.as_str() {
            "user" => {
                turn_number += 1;
                current_turn_id = format!("legacy-turn-{}", turn_number);
                seq += 1;
                events.push(TimelineEvent {
                    id: format!("legacy-{}-{}", message_id, seq),
                    conversation_id: event_conversation_id,
                    turn_id: current_turn_id.clone(),
                    seq,
                    event_type: TimelineEventType::UserMessage,
                    tool_call_id: None,
                    payload: json!({ "content": content }),
                    created_at: created_at.clone(),
                });
            }
            "assistant" => {
                if turn_number == 0 {
                    turn_number = 1;
                    current_turn_id = "legacy-turn-1".to_string();
                }

                let reasoning = reasoning_raw.unwrap_or_default().trim().to_string();
                if !reasoning.is_empty() {
                    seq += 1;
                    events.push(TimelineEvent {
                        id: format!("legacy-{}-{}", message_id, seq),
                        conversation_id: event_conversation_id.clone(),
                        turn_id: current_turn_id.clone(),
                        seq,
                        event_type: TimelineEventType::AssistantReasoning,
                        tool_call_id: None,
                        payload: json!({ "text": reasoning }),
                        created_at,
                    });
                }

                if let Some(raw) = tool_calls_raw.as_deref() {
                    if let Ok(tool_calls) = serde_json::from_str::<Vec<ToolCall>>(raw) {
                        for (index, call) in tool_calls.into_iter().enumerate() {
                            seq += 1;
                            let arguments_chunk = serde_json::to_string(&call.arguments)
                                .unwrap_or_else(|_| "{}".to_string());
                            events.push(TimelineEvent {
                                id: format!("legacy-{}-{}", message_id, seq),
                                conversation_id: event_conversation_id.clone(),
                                turn_id: current_turn_id.clone(),
                                seq,
                                event_type: TimelineEventType::AssistantToolCall,
                                tool_call_id: Some(call.id),
                                payload: json!({
                                    "index": index,
                                    "name": call.tool_name,
                                    "argumentsChunk": arguments_chunk
                                }),
                                created_at: created_at.clone(),
                            });
                        }
                    } else if let Ok(tool_calls) = serde_json::from_str::<Vec<ChatToolCall>>(raw) {
                        for (index, call) in tool_calls.into_iter().enumerate() {
                            seq += 1;
                            events.push(TimelineEvent {
                                id: format!("legacy-{}-{}", message_id, seq),
                                conversation_id: event_conversation_id.clone(),
                                turn_id: current_turn_id.clone(),
                                seq,
                                event_type: TimelineEventType::AssistantToolCall,
                                tool_call_id: Some(call.id),
                                payload: json!({
                                    "index": index,
                                    "name": call.function.name,
                                    "argumentsChunk": call.function.arguments
                                }),
                                created_at: created_at.clone(),
                            });
                        }
                    }
                }

                if !content.trim().is_empty() {
                    seq += 1;
                    events.push(TimelineEvent {
                        id: format!("legacy-{}-{}", message_id, seq),
                        conversation_id: event_conversation_id,
                        turn_id: current_turn_id.clone(),
                        seq,
                        event_type: TimelineEventType::AssistantText,
                        tool_call_id: None,
                        payload: json!({ "text": content }),
                        created_at: created_at.clone(),
                    });
                }
            }
            "tool" => {
                if turn_number == 0 {
                    turn_number = 1;
                    current_turn_id = "legacy-turn-1".to_string();
                }

                let metadata = tool_calls_raw
                    .as_deref()
                    .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
                    .unwrap_or_else(|| json!({}));
                let tool_call_id = metadata
                    .get("tool_call_id")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                let tool_name = metadata
                    .get("tool_name")
                    .and_then(Value::as_str)
                    .unwrap_or("tool");
                let error = serde_json::from_str::<Value>(&content)
                    .ok()
                    .and_then(|value| {
                        value
                            .get("error")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                    });

                seq += 1;
                events.push(TimelineEvent {
                    id: format!("legacy-{}-{}", message_id, seq),
                    conversation_id: event_conversation_id,
                    turn_id: current_turn_id.clone(),
                    seq,
                    event_type: TimelineEventType::AssistantToolResult,
                    tool_call_id,
                    payload: json!({
                        "name": tool_name,
                        "result": content,
                        "error": error
                    }),
                    created_at: created_at.clone(),
                });
            }
            _ => {}
        }
    }

    Ok(ConversationTimeline {
        events,
        legacy: true,
    })
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
        let guard = state.lock().await;
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
        let guard = state.lock().await;
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
        let guard = state.lock().await;
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
        let guard = state.lock().await;
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
