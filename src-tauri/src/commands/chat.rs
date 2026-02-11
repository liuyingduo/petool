use crate::models::chat::*;
use crate::services::llm::{LlmService, ChatMessage};
use crate::state::AppState;
use chrono::Utc;
use std::sync::{Arc, Mutex};
use tauri::{State, Window, Emitter};
use uuid::Uuid;

#[tauri::command]
pub async fn send_message(
    state: State<'_, AppState>,
    conversation_id: String,
    content: String,
) -> Result<String, String> {
    let config = crate::utils::load_config::<crate::models::config::Config>()
        .map_err(|e| e.to_string())?;

    let api_key = config.api_key.ok_or("API key not set".to_string())?;
    let llm_service = LlmService::new(api_key, config.api_base);

    let messages = vec![
        ChatMessage {
            role: "user".to_string(),
            content: content.clone(),
        }
    ];

    let response = llm_service.chat(&config.model, messages).await
        .map_err(|e| e.to_string())?;

    let pool = {
        let guard = state.lock().map_err(|e| e.to_string())?;
        guard.db().pool().clone()
    };

    // Save messages to database
    let user_msg_id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO messages (id, conversation_id, role, content, created_at) VALUES (?, ?, ?, ?, ?)"
    )
    .bind(&user_msg_id)
    .bind(&conversation_id)
    .bind("user")
    .bind(&content)
    .bind(&now)
    .execute(&pool)
    .await
    .map_err(|e| e.to_string())?;

    let assistant_msg_id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO messages (id, conversation_id, role, content, created_at) VALUES (?, ?, ?, ?, ?)"
    )
    .bind(&assistant_msg_id)
    .bind(&conversation_id)
    .bind("assistant")
    .bind(&response)
    .bind(&now)
    .execute(&pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(response)
}

#[tauri::command]
pub async fn stream_message(
    state: State<'_, AppState>,
    window: Window,
    conversation_id: String,
    content: String,
) -> Result<(), String> {
    let config = crate::utils::load_config::<crate::models::config::Config>()
        .map_err(|e| e.to_string())?;

    let api_key = config.api_key.ok_or("API key not set".to_string())?;
    let llm_service = LlmService::new(api_key, config.api_base);

    let messages = vec![
        ChatMessage {
            role: "user".to_string(),
            content: content.clone(),
        }
    ];

    let pool = {
        let guard = state.lock().map_err(|e| e.to_string())?;
        guard.db().pool().clone()
    };

    // Save user message to database
    let user_msg_id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO messages (id, conversation_id, role, content, created_at) VALUES (?, ?, ?, ?, ?)"
    )
    .bind(&user_msg_id)
    .bind(&conversation_id)
    .bind("user")
    .bind(&content)
    .bind(&now)
    .execute(&pool)
    .await
    .map_err(|e| e.to_string())?;

    let response_content = Arc::new(Mutex::new(String::new()));
    let response_content_for_stream = Arc::clone(&response_content);
    let window_for_stream = window.clone();
    let conversation_id_clone = conversation_id.clone();

    llm_service.chat_stream(&config.model, messages, move |chunk| {
        if let Ok(mut full_response) = response_content_for_stream.lock() {
            full_response.push_str(&chunk);
        }
        let _ = window_for_stream.emit("chat-chunk", &chunk);
    }).await.map_err(|e| e.to_string())?;

    let response_content = response_content
        .lock()
        .map_err(|e| e.to_string())?
        .clone();

    // Save assistant message to database
    let assistant_msg_id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO messages (id, conversation_id, role, content, created_at) VALUES (?, ?, ?, ?, ?)"
    )
    .bind(&assistant_msg_id)
    .bind(&conversation_id_clone)
    .bind("assistant")
    .bind(&response_content)
    .bind(&now)
    .execute(&pool)
    .await
    .map_err(|e| e.to_string())?;

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
        "SELECT id, title, model, created_at, updated_at FROM conversations ORDER BY updated_at DESC"
    )
    .fetch_all(&pool)
    .await
    .map_err(|e| e.to_string())?;

    let conversations = rows.into_iter().map(|(id, title, model, created_at_raw, updated_at_raw)| {
        Conversation {
            id,
            title,
            model,
            created_at: created_at_raw.parse().unwrap_or_else(|_| Utc::now()),
            updated_at: updated_at_raw.parse().unwrap_or_else(|_| Utc::now()),
        }
    }).collect();

    Ok(conversations)
}

#[tauri::command]
pub async fn get_messages(state: State<'_, AppState>, conversation_id: String) -> Result<Vec<Message>, String> {
    let pool = {
        let guard = state.lock().map_err(|e| e.to_string())?;
        guard.db().pool().clone()
    };

    let rows = sqlx::query_as::<_, (String, String, String, String, String, Option<String>)>(
        "SELECT id, conversation_id, role, content, created_at, tool_calls FROM messages WHERE conversation_id = ? ORDER BY created_at ASC"
    )
    .bind(&conversation_id)
    .fetch_all(&pool)
    .await
    .map_err(|e| e.to_string())?;

    let messages = rows.into_iter().map(|(id, conversation_id, role_raw, content, created_at_raw, tool_calls_raw)| {
        let role = match role_raw.as_str() {
            "user" => MessageRole::User,
            "assistant" => MessageRole::Assistant,
            "system" => MessageRole::System,
            "tool" => MessageRole::Tool,
            _ => MessageRole::User,
        };

        let tool_calls: Option<Vec<ToolCall>> = tool_calls_raw
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok());

        Message {
            id,
            conversation_id,
            role,
            content,
            created_at: created_at_raw.parse().unwrap_or_else(|_| Utc::now()),
            tool_calls,
        }
    }).collect();

    Ok(messages)
}

#[tauri::command]
pub async fn create_conversation(state: State<'_, AppState>, title: String, model: String) -> Result<Conversation, String> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    let pool = {
        let guard = state.lock().map_err(|e| e.to_string())?;
        guard.db().pool().clone()
    };

    sqlx::query(
        "INSERT INTO conversations (id, title, model, created_at, updated_at) VALUES (?, ?, ?, ?, ?)"
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
