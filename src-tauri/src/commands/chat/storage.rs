use crate::models::chat::TimelineEventType;
use crate::services::llm::{ChatMessage, ChatToolCall};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::OnceLock;
use tauri::{Emitter, Window};
use chrono::Utc;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TodoStatus {
    Pending,
    InProgress,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TodoItem {
    pub(crate) id: String,
    pub(crate) text: String,
    pub(crate) status: TodoStatus,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

pub(crate) type TodoStore = tokio::sync::Mutex<HashMap<String, Vec<TodoItem>>>;

pub(crate) static TODO_STORE: OnceLock<TodoStore> = OnceLock::new();

pub(crate) fn todo_store() -> &'static TodoStore {
    TODO_STORE.get_or_init(|| tokio::sync::Mutex::new(HashMap::new()))
}

#[derive(Debug, Clone)]
pub(crate) struct PendingTimelineEvent {
    pub(crate) turn_id: String,
    pub(crate) seq: i64,
    pub(crate) event_type: TimelineEventType,
    pub(crate) tool_call_id: Option<String>,
    pub(crate) payload: Value,
    pub(crate) created_at: String,
}

pub(crate) async fn insert_timeline_event(
    pool: &SqlitePool,
    conversation_id: &str,
    event: &PendingTimelineEvent,
) -> Result<(), String> {
    let id = Uuid::new_v4().to_string();
    let payload = serde_json::to_string(&event.payload).map_err(|e| e.to_string())?;
    let event_type = serde_json::to_string(&event.event_type)
        .map_err(|e| e.to_string())?
        .trim_matches('"')
        .to_string();

    sqlx::query(
        "INSERT INTO message_events (id, conversation_id, turn_id, seq, event_type, tool_call_id, payload, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(conversation_id)
    .bind(&event.turn_id)
    .bind(event.seq)
    .bind(event_type)
    .bind(event.tool_call_id.as_deref())
    .bind(payload)
    .bind(&event.created_at)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(())
}

pub(crate) async fn insert_timeline_events(
    pool: &SqlitePool,
    conversation_id: &str,
    events: &[PendingTimelineEvent],
) -> Result<(), String> {
    for event in events {
        insert_timeline_event(pool, conversation_id, event).await?;
    }
    Ok(())
}

pub(crate) async fn insert_message(
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

pub(crate) async fn load_conversation_context(
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
                let reasoning = reasoning_raw
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(|value| value.to_string());
                let reasoning_details = reasoning.as_deref().and_then(crate::services::llm::reasoning_details_from_text);

                messages.push(ChatMessage {
                    role,
                    content: if content.is_empty() {
                        None
                    } else {
                        Some(content)
                    },
                    tool_calls,
                    tool_call_id: None,
                    reasoning_details,
                    reasoning,
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
                    reasoning_details: None,
                    reasoning: None,
                });
            }
            _ => {
                messages.push(ChatMessage {
                    role,
                    content: Some(content),
                    tool_calls: None,
                    tool_call_id: None,
                    reasoning_details: None,
                    reasoning: None,
                });
            }
        }
    }

    Ok(messages)
}

pub(crate) fn emit_tool_result_event(
    window: &Window,
    conversation_id: &str,
    turn_id: &str,
    seq: i64,
    created_at: &str,
    tool_call: &ChatToolCall,
    result: Option<&str>,
    error: Option<&str>,
) -> Result<(), String> {
    window
        .emit(
            "chat-tool-result",
            json!({
                "conversationId": conversation_id,
                "turnId": turn_id,
                "seq": seq,
                "eventType": "assistant_tool_result",
                "createdAt": created_at,
                "toolCallId": &tool_call.id,
                "name": &tool_call.function.name,
                "result": result,
                "error": error,
            }),
        )
        .map_err(|e| e.to_string())
}

pub(crate) async fn emit_and_record_tool_result_event(
    pool: &SqlitePool,
    window: &Window,
    conversation_id: &str,
    turn_id: &str,
    seq_counter: &mut i64,
    tool_call: &ChatToolCall,
    result: Option<&str>,
    error: Option<&str>,
) -> Result<(), String> {
    *seq_counter += 1;
    let seq = *seq_counter;
    let created_at = Utc::now().to_rfc3339();
    emit_tool_result_event(
        window,
        conversation_id,
        turn_id,
        seq,
        &created_at,
        tool_call,
        result,
        error,
    )?;

    let event = PendingTimelineEvent {
        turn_id: turn_id.to_string(),
        seq,
        event_type: TimelineEventType::AssistantToolResult,
        tool_call_id: Some(tool_call.id.clone()),
        payload: json!({
            "name": &tool_call.function.name,
            "result": result,
            "error": error
        }),
        created_at,
    };
    insert_timeline_event(pool, conversation_id, &event).await?;
    Ok(())
}

pub(crate) fn build_tool_call_metadata(tool_call: &ChatToolCall) -> Result<String, String> {
    serde_json::to_string(&json!({
        "tool_call_id": &tool_call.id,
        "tool_name": &tool_call.function.name
    }))
    .map_err(|e| e.to_string())
}

pub(crate) fn format_tool_error_result(error_text: &str) -> Result<String, String> {
    serde_json::to_string_pretty(&json!({
        "error": error_text
    }))
    .map_err(|e| e.to_string())
}

pub(crate) async fn persist_tool_result_message(
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
        reasoning_details: None,
        reasoning: None,
    });

    Ok(())
}

pub(crate) fn push_background_tool_result_message(
    context_messages: &mut Vec<ChatMessage>,
    tool_call: &ChatToolCall,
    result_text: String,
) {
    context_messages.push(ChatMessage {
        role: "tool".to_string(),
        content: Some(result_text),
        tool_calls: None,
        tool_call_id: Some(tool_call.id.clone()),
        reasoning_details: None,
        reasoning: None,
    });
}
