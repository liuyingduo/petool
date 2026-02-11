use crate::models::chat::*;
use crate::services::llm::{LlmService, ChatMessage};
use crate::state::AppState;
use chrono::Utc;
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
            content,
        }
    ];

    let response = llm_service.chat(&config.model, messages).await
        .map_err(|e| e.to_string())?;

    // Save messages to database
    let guard = state.lock();
    let db = guard.db();

    // Save user message
    let user_msg_id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let _ = sqlx::query(
        "INSERT INTO messages (id, conversation_id, role, content, created_at) VALUES (?, ?, ?, ?, ?)"
    )
    .bind(&user_msg_id)
    .bind(&conversation_id)
    .bind("user")
    .bind(&content)
    .bind(&now)
    .execute(db.pool())
    .await
    .map_err(|e| e.to_string());

    // Save assistant message
    let assistant_msg_id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let _ = sqlx::query(
        "INSERT INTO messages (id, conversation_id, role, content, created_at) VALUES (?, ?, ?, ?, ?)"
    )
    .bind(&assistant_msg_id)
    .bind(&conversation_id)
    .bind("assistant")
    .bind(&response)
    .bind(&now)
    .execute(db.pool())
    .await
    .map_err(|e| e.to_string());

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
            content,
        }
    ];

    // Save user message to database
    let guard = state.lock();
    let db = guard.db();
    let user_msg_id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let _ = sqlx::query(
        "INSERT INTO messages (id, conversation_id, role, content, created_at) VALUES (?, ?, ?, ?, ?)"
    )
    .bind(&user_msg_id)
    .bind(&conversation_id)
    .bind("user")
    .bind(&content)
    .bind(&now)
    .execute(db.pool())
    .await
    .map_err(|e| e.to_string());
    drop(guard);

    let mut response_content = String::new();
    let conversation_id_clone = conversation_id.clone();
    let state_clone = state.clone();

    llm_service.chat_stream(&config.model, messages, move |chunk| {
        response_content.push_str(&chunk);
        let _ = window.emit("chat-chunk", &chunk);
    }).await.map_err(|e| e.to_string())?;

    // Save assistant message to database
    let guard = state_clone.lock();
    let db = guard.db();
    let assistant_msg_id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let _ = sqlx::query(
        "INSERT INTO messages (id, conversation_id, role, content, created_at) VALUES (?, ?, ?, ?, ?)"
    )
    .bind(&assistant_msg_id)
    .bind(&conversation_id_clone)
    .bind("assistant")
    .bind(&response_content)
    .bind(&now)
    .execute(db.pool())
    .await
    .map_err(|e| e.to_string());

    window.emit("chat-end", &()).map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn get_conversations(state: State<'_, AppState>) -> Result<Vec<Conversation>, String> {
    let guard = state.lock();
    let db = guard.db();

    let rows = sqlx::query_as::<_, (String, String, String, String, String)>(
        "SELECT id, title, model, created_at, updated_at FROM conversations ORDER BY updated_at DESC"
    )
    .fetch_all(db.pool())
    .await
    .map_err(|e| e.to_string())?;

    let conversations = rows.into_iter().map(|(id, title, model, created_at, updated_at)| {
        Conversation {
            id,
            title,
            model,
            created_at: created_at.parse().unwrap_or_else(|_| Utc::now()),
            updated_at: updated_at.parse().unwrap_or_else(|_| Utc::now()),
        }
    }).collect();

    Ok(conversations)
}

#[tauri::command]
pub async fn get_messages(state: State<'_, AppState>, conversation_id: String) -> Result<Vec<Message>, String> {
    let guard = state.lock();
    let db = guard.db();

    let rows = sqlx::query_as::<_, (String, String, String, String, String, Option<String>)>(
        "SELECT id, conversation_id, role, content, created_at, tool_calls FROM messages WHERE conversation_id = ? ORDER BY created_at ASC"
    )
    .bind(&conversation_id)
    .fetch_all(db.pool())
    .await
    .map_err(|e| e.to_string())?;

    let messages = rows.into_iter().map(|(id, conversation_id, role, content, created_at, tool_calls)| {
        let role = match role.as_str() {
            "user" => MessageRole::User,
            "assistant" => MessageRole::Assistant,
            "system" => MessageRole::System,
            "tool" => MessageRole::Tool,
            _ => MessageRole::User,
        };

        let tool_calls = tool_calls.as_deref().and_then(|s| serde_json::from_str(s).ok());

        Message {
            id,
            conversation_id,
            role,
            content,
            created_at: created_at.parse().unwrap_or_else(|_| Utc::now()),
            tool_calls,
        }
    }).collect();

    Ok(messages)
}

#[tauri::command]
pub async fn create_conversation(state: State<'_, AppState>, title: String, model: String) -> Result<Conversation, String> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    let guard = state.lock();
    let db = guard.db();

    let _ = sqlx::query(
        "INSERT INTO conversations (id, title, model, created_at, updated_at) VALUES (?, ?, ?, ?, ?)"
    )
    .bind(&id)
    .bind(&title)
    .bind(&model)
    .bind(&now)
    .bind(&now)
    .execute(db.pool())
    .await
    .map_err(|e| e.to_string());

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
    let guard = state.lock();
    let db = guard.db();

    let _ = sqlx::query("DELETE FROM conversations WHERE id = ?")
        .bind(&id)
        .execute(db.pool())
        .await
        .map_err(|e: e.to_string());

    Ok(())
}
