use crate::models::mcp::*;
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    id: Option<String>,
    method: String,
    params: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    id: Option<String>,
    result: Option<Value>,
    error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JsonRpcError {
    code: i32,
    message: String,
    data: Option<Value>,
}

#[async_trait]
pub trait McpTransport: Send + Sync {
    async fn send(&self, request: JsonRpcRequest) -> Result<JsonRpcResponse>;
    async fn initialize(&mut self) -> Result<ServerCapabilities>;
    async fn shutdown(&mut self) -> Result<()>;
}

pub struct StdioTransport {
    child: Option<Child>,
}

impl StdioTransport {
    pub fn new(command: &str, args: &[String]) -> Result<Self> {
        let child = Command::new(command)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        Ok(Self { child: Some(child) })
    }

    fn send_request(&self, request: &JsonRpcRequest) -> Result<JsonRpcResponse> {
        let child = self.child.as_ref().ok_or_else(|| anyhow!("Child process not available"))?;

        let mut stdin = child.stdin.as_ref().ok_or_else(|| anyhow!("Stdin not available"))?;
        let stdout = child.stdout.as_ref().ok_or_else(|| anyhow!("Stdout not available"))?;

        // Send request
        let request_str = serde_json::to_string(request)?;
        writeln!(stdin, "Content-Length: {}", request_str.len())?;
        writeln!(stdin)?;
        writeln!(stdin, "{}", request_str)?;
        stdin.flush()?;

        // Read response (simple single-line JSON for now)
        let mut stdout_reader = std::io::BufReader::new(stdout);
        let mut line = String::new();
        stdout_reader.read_line(&mut line)?;

        if let Some(json_start) = line.find('{') {
            let json_str = &line[json_start..];
            let response: JsonRpcResponse = serde_json::from_str(json_str)?;
            return Ok(response);
        }

        Err(anyhow!("No valid JSON response"))
    }
}

#[async_trait]
impl McpTransport for StdioTransport {
    async fn send(&self, request: JsonRpcRequest) -> Result<JsonRpcResponse> {
        self.send_request(&request)
    }

    async fn initialize(&mut self) -> Result<ServerCapabilities> {
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some("init".to_string()),
            method: "initialize".to_string(),
            params: Some(json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "petool",
                    "version": "0.1.0"
                }
            })),
        };

        let response = self.send_request(&request)?;

        if let Some(error) = response.error {
            return Err(anyhow!("MCP initialization error: {}", error.message));
        }

        Ok(ServerCapabilities {
            tools: Some(true),
            prompts: Some(true),
            resources: Some(true),
        })
    }

    async fn shutdown(&mut self) -> Result<()> {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        Ok(())
    }
}

pub struct HttpTransport {
    url: String,
    client: reqwest::Client,
}

impl HttpTransport {
    pub fn new(url: String) -> Self {
        Self {
            url,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl McpTransport for HttpTransport {
    async fn send(&self, request: JsonRpcRequest) -> Result<JsonRpcResponse> {
        let response = self.client
            .post(&self.url)
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow!("HTTP error: {}", response.status()));
        }

        let rpc_response: JsonRpcResponse = response.json().await?;
        Ok(rpc_response)
    }

    async fn initialize(&mut self) -> Result<ServerCapabilities> {
        Ok(ServerCapabilities {
            tools: Some(true),
            prompts: Some(true),
            resources: Some(true),
        })
    }

    async fn shutdown(&mut self) -> Result<()> {
        Ok(())
    }
}

pub struct McpClient {
    pub name: String,
    transport: Option<Box<dyn McpTransport>>,
    pub capabilities: ServerCapabilities,
    pub tools: HashMap<String, Tool>,
    pub prompts: HashMap<String, Prompt>,
    pub resources: HashMap<String, Resource>,
}

impl McpClient {
    pub async fn new(name: String, mut transport: Box<dyn McpTransport>) -> Result<Self> {
        let capabilities = transport.initialize().await?;

        Ok(Self {
            name,
            transport: Some(transport),
            capabilities,
            tools: HashMap::new(),
            prompts: HashMap::new(),
            resources: HashMap::new(),
        })
    }

    pub async fn refresh_tools(&mut self) -> Result<()> {
        let _ = self.transport.as_ref().ok_or_else(|| anyhow!("Transport not available"))?;
        // TODO: Implement actual tools refresh
        Ok(())
    }

    pub async fn refresh_prompts(&mut self) -> Result<()> {
        let _ = self.transport.as_ref().ok_or_else(|| anyhow!("Transport not available"))?;
        // TODO: Implement actual prompts refresh
        Ok(())
    }

    pub async fn refresh_resources(&mut self) -> Result<()> {
        let _ = self.transport.as_ref().ok_or_else(|| anyhow!("Transport not available"))?;
        // TODO: Implement actual resources refresh
        Ok(())
    }

    pub async fn call_tool(&self, _name: &str, _arguments: Value) -> Result<Value> {
        // TODO: Implement actual tool calling
        Ok(json!({"result": "tool_called"}))
    }

    pub fn list_tools(&self) -> Vec<Tool> {
        self.tools.values().cloned().collect()
    }

    pub fn list_prompts(&self) -> Vec<Prompt> {
        self.prompts.values().cloned().collect()
    }

    pub fn list_resources(&self) -> Vec<Resource> {
        self.resources.values().cloned().collect()
    }

    pub async fn shutdown(mut self) -> Result<()> {
        if let Some(mut transport) = self.transport.take() {
            transport.shutdown().await?;
        }
        Ok(())
    }
}

pub struct McpManager {
    clients: HashMap<String, McpClient>,
}

impl McpManager {
    pub fn new() -> Self {
        Self {
            clients: HashMap::new(),
        }
    }

    pub async fn add_client(&mut self, name: String, mut client: McpClient) -> Result<()> {
        client.refresh_tools().await?;
        client.refresh_prompts().await?;
        client.refresh_resources().await?;
        self.clients.insert(name, client);
        Ok(())
    }

    pub fn get_client(&self, name: &str) -> Option<&McpClient> {
        self.clients.get(name)
    }

    pub fn remove_client(&mut self, name: &str) -> Option<McpClient> {
        self.clients.remove(name)
    }

    pub fn list_all_tools(&self) -> Vec<(String, Tool)> {
        let mut all_tools = Vec::new();
        for (server_name, client) in &self.clients {
            for tool in client.list_tools() {
                all_tools.push((server_name.clone(), tool));
            }
        }
        all_tools
    }

    pub fn list_clients(&self) -> Vec<(String, &McpClient)> {
        self.clients.iter().map(|(k, v)| (k.clone(), v)).collect()
    }

    pub async fn shutdown_all(mut self) -> Result<()> {
        for (_name, client) in self.clients.drain() {
            client.shutdown().await?;
        }
        Ok(())
    }
}

impl Default for McpManager {
    fn default() -> Self {
        Self::new()
    }
}
