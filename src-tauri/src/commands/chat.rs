use crate::commands::mcp::McpState;
use crate::models::chat::*;
use crate::models::config::{Config, McpTransport};
use crate::services::llm::{
    ChatMessage, ChatTool, ChatToolCall, ChatToolFunction, LlmService, LlmStreamEvent,
};
use crate::services::mcp_client::{HttpTransport, McpClient, StdioTransport};
use crate::state::AppState;
use chrono::Utc;
use serde_json::{Value, json};
use sqlx::SqlitePool;
use std::collections::HashMap;
use tauri::{Emitter, State, Window};
use uuid::Uuid;

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
                    content: if content.is_empty() { None } else { Some(content) },
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

        let transport: Box<dyn crate::services::mcp_client::McpTransport> = match &server.transport {
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

async fn collect_mcp_tools(
    mcp_state: &McpState,
) -> (Vec<ChatTool>, HashMap<String, (String, String)>) {
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

            tool_map.insert(alias, (server_name.clone(), tool.name.clone()));
        }
    }

    (tools, tool_map)
}

async fn execute_mcp_tool_call(
    mcp_state: &McpState,
    tool_map: &HashMap<String, (String, String)>,
    tool_call: &ChatToolCall,
) -> Result<Value, String> {
    let alias = &tool_call.function.name;
    let (server_name, tool_name) = tool_map
        .get(alias)
        .cloned()
        .ok_or_else(|| format!("Unknown tool: {}", alias))?;

    let arguments = if tool_call.function.arguments.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str::<Value>(&tool_call.function.arguments).unwrap_or_else(|_| {
            json!({
                "raw_arguments": tool_call.function.arguments
            })
        })
    };

    let mut manager = mcp_state.lock().await;
    let client = manager
        .get_client_mut(&server_name)
        .ok_or_else(|| format!("MCP server '{}' is not connected", server_name))?;

    client
        .call_tool(&tool_name, arguments)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn send_message(
    state: State<'_, AppState>,
    conversation_id: String,
    content: String,
) -> Result<String, String> {
    let config = crate::utils::load_config::<Config>().map_err(|e| e.to_string())?;
    let api_key = config.api_key.ok_or("API key not set".to_string())?;
    let llm_service = LlmService::new(api_key, config.api_base.clone());

    let pool = {
        let guard = state.lock().map_err(|e| e.to_string())?;
        guard.db().pool().clone()
    };

    insert_message(&pool, &conversation_id, "user", &content, None).await?;

    let mut messages = load_conversation_context(&pool, &conversation_id).await?;
    if let Some(system_prompt) = config.system_prompt {
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
        .chat(&config.model, messages)
        .await
        .map_err(|e| e.to_string())?;

    insert_message(&pool, &conversation_id, "assistant", &response, None).await?;

    Ok(response)
}

#[tauri::command]
pub async fn stream_message(
    state: State<'_, AppState>,
    mcp_manager: State<'_, McpState>,
    window: Window,
    conversation_id: String,
    content: String,
) -> Result<(), String> {
    let config = crate::utils::load_config::<Config>().map_err(|e| e.to_string())?;
    let api_key = config.api_key.ok_or("API key not set".to_string())?;
    let llm_service = LlmService::new(api_key, config.api_base.clone());
    let mcp_state = mcp_manager.inner();

    ensure_mcp_servers_connected(mcp_state, &config).await?;
    let (mcp_tools, tool_map) = collect_mcp_tools(mcp_state).await;

    let pool = {
        let guard = state.lock().map_err(|e| e.to_string())?;
        guard.db().pool().clone()
    };

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

    let max_tool_rounds = 4usize;
    for _round in 0..max_tool_rounds {
        let window_for_stream = window.clone();
        let stream_result = llm_service
            .chat_stream_with_tools(
                &config.model,
                context_messages.clone(),
                if mcp_tools.is_empty() {
                    None
                } else {
                    Some(mcp_tools.clone())
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
            insert_message(&pool, &conversation_id, "assistant", &assistant_content, None).await?;
            window.emit("chat-end", &()).map_err(|e| e.to_string())?;
            return Ok(());
        }

        let assistant_tool_calls_json = serde_json::to_string(&stream_result.tool_calls)
            .map_err(|e| e.to_string())?;
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

        for tool_call in stream_result.tool_calls {
            let tool_result = execute_mcp_tool_call(mcp_state, &tool_map, &tool_call).await;
            match tool_result {
                Ok(value) => {
                    let result_text = serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string());
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

    window.emit("chat-end", &()).map_err(|e| e.to_string())?;
    Err("Exceeded maximum tool-calling rounds".to_string())
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
        .map(|(id, title, model, created_at_raw, updated_at_raw)| Conversation {
            id,
            title,
            model,
            created_at: created_at_raw.parse().unwrap_or_else(|_| Utc::now()),
            updated_at: updated_at_raw.parse().unwrap_or_else(|_| Utc::now()),
        })
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

                let tool_calls: Option<Vec<ToolCall>> = tool_calls_raw.as_deref().and_then(|value| {
                    serde_json::from_str::<Vec<ToolCall>>(value).ok().or_else(|| {
                        serde_json::from_str::<Vec<ChatToolCall>>(value).ok().map(|calls| {
                            calls
                                .into_iter()
                                .map(|call| ToolCall {
                                    id: call.id,
                                    tool_name: call.function.name,
                                    arguments: serde_json::from_str(&call.function.arguments)
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
