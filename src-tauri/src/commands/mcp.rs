use crate::models::config::McpTransport;
use crate::models::mcp::*;
use crate::services::mcp_client::{McpClient, McpManager, StdioTransport, HttpTransport};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;

pub type McpState = Arc<Mutex<McpManager>>;

#[tauri::command]
pub async fn connect_server(
    mcp_manager: tauri::State<'_, McpState>,
    name: String,
    config: McpTransport,
) -> Result<(), String> {
    let transport: Box<dyn crate::services::mcp_client::McpTransport> = match config {
        McpTransport::Stdio { command, args } => {
            Box::new(StdioTransport::new(&command, &args).map_err(|e| e.to_string())?)
        }
        McpTransport::Http { url } => {
            Box::new(HttpTransport::new(url))
        }
    };

    let client = McpClient::new(name.clone(), transport).await
        .map_err(|e| e.to_string())?;

    let mut manager = mcp_manager.lock().await;
    manager.add_client(name, client).await
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn disconnect_server(
    mcp_manager: tauri::State<'_, McpState>,
    name: String,
) -> Result<(), String> {
    let mut manager = mcp_manager.lock().await;
    if let Some(client) = manager.remove_client(&name) {
        client.shutdown().await.map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn list_tools(
    mcp_manager: tauri::State<'_, McpState>,
) -> Result<Vec<(String, Tool)>, String> {
    let manager = mcp_manager.lock().await;
    Ok(manager.list_all_tools())
}

#[tauri::command]
pub async fn call_tool(
    mcp_manager: tauri::State<'_, McpState>,
    server: String,
    name: String,
    arguments: Value,
) -> Result<String, String> {
    let mut manager = mcp_manager.lock().await;
    let client = manager.get_client_mut(&server)
        .ok_or_else(|| format!("Server '{}' not found", server))?;

    let result = client.call_tool(&name, arguments).await
        .map_err(|e| e.to_string())?;

    serde_json::to_string_pretty(&result).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_prompts(
    mcp_manager: tauri::State<'_, McpState>,
) -> Result<Vec<(String, Prompt)>, String> {
    let manager = mcp_manager.lock().await;
    let mut result = Vec::new();
    for (server_name, _client) in manager.list_clients() {
        let client = manager.get_client(&server_name).unwrap();
        for prompt in client.list_prompts() {
            result.push((server_name.clone(), prompt));
        }
    }
    Ok(result)
}

#[tauri::command]
pub async fn list_resources(
    mcp_manager: tauri::State<'_, McpState>,
) -> Result<Vec<(String, Resource)>, String> {
    let manager = mcp_manager.lock().await;
    let mut result = Vec::new();
    for (server_name, _client) in manager.list_clients() {
        let client = manager.get_client(&server_name).unwrap();
        for resource in client.list_resources() {
            result.push((server_name.clone(), resource));
        }
    }
    Ok(result)
}

#[tauri::command]
pub async fn list_servers(
    mcp_manager: tauri::State<'_, McpState>,
) -> Result<Vec<McpServer>, String> {
    let manager = mcp_manager.lock().await;
    let mut servers = Vec::new();

    for (_server_name, client) in manager.list_clients() {
        servers.push(McpServer {
            name: client.name.clone(),
            capabilities: client.capabilities.clone(),
            tools: client.list_tools(),
            prompts: client.list_prompts(),
            resources: client.list_resources(),
        });
    }

    Ok(servers)
}

#[tauri::command]
pub async fn disconnect_all_servers(
    mcp_manager: tauri::State<'_, McpState>,
) -> Result<(), String> {
    let mut manager = mcp_manager.lock().await;
    manager.shutdown_all().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn read_resource(
    mcp_manager: tauri::State<'_, McpState>,
    server: String,
    uri: String,
) -> Result<String, String> {
    let mut manager = mcp_manager.lock().await;
    let client = manager.get_client_mut(&server)
        .ok_or_else(|| format!("Server '{}' not found", server))?;

    let result = client.read_resource(&uri).await
        .map_err(|e| e.to_string())?;

    serde_json::to_string_pretty(&result).map_err(|e| e.to_string())
}
