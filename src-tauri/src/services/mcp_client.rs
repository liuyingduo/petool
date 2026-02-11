use crate::models::mcp::*;
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::process::{Child, Command, Stdio};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct JsonRpcRequest {
    jsonrpc: String,
    id: Option<String>,
    method: String,
    params: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct JsonRpcResponse {
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
    async fn send(&mut self, request: JsonRpcRequest) -> Result<JsonRpcResponse>;
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

    fn send_request(&mut self, request: &JsonRpcRequest) -> Result<JsonRpcResponse> {
        let child = self.child.as_mut().ok_or_else(|| anyhow!("Child process not available"))?;

        let stdin = child.stdin.as_mut().ok_or_else(|| anyhow!("Stdin not available"))?;
        let stdout = child.stdout.as_mut().ok_or_else(|| anyhow!("Stdout not available"))?;

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
    async fn send(&mut self, request: JsonRpcRequest) -> Result<JsonRpcResponse> {
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
    async fn send(&mut self, request: JsonRpcRequest) -> Result<JsonRpcResponse> {
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
        if self.capabilities.tools != Some(true) {
            return Ok(());
        }

        let transport = self.transport.as_mut().ok_or_else(|| anyhow!("Transport not available"))?;
        let response = transport.send(JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some("tools-list".to_string()),
            method: "tools/list".to_string(),
            params: None,
        }).await?;

        if let Some(error) = response.error {
            return Err(anyhow!("MCP tools/list error: {}", error.message));
        }

        if let Some(result) = response.result {
            if let Some(tools) = result.get("tools")
                .and_then(|v| serde_json::from_value::<Vec<Tool>>(v.clone()).ok())
            {
                self.tools = tools.into_iter().map(|tool| (tool.name.clone(), tool)).collect();
            }
        }
        Ok(())
    }

    pub async fn refresh_prompts(&mut self) -> Result<()> {
        if self.capabilities.prompts != Some(true) {
            return Ok(());
        }

        let transport = self.transport.as_mut().ok_or_else(|| anyhow!("Transport not available"))?;
        let response = transport.send(JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some("prompts-list".to_string()),
            method: "prompts/list".to_string(),
            params: None,
        }).await?;

        if let Some(error) = response.error {
            return Err(anyhow!("MCP prompts/list error: {}", error.message));
        }

        if let Some(result) = response.result {
            if let Some(prompts) = result.get("prompts")
                .and_then(|v| serde_json::from_value::<Vec<Prompt>>(v.clone()).ok())
            {
                self.prompts = prompts.into_iter().map(|prompt| (prompt.name.clone(), prompt)).collect();
            }
        }
        Ok(())
    }

    pub async fn refresh_resources(&mut self) -> Result<()> {
        if self.capabilities.resources != Some(true) {
            return Ok(());
        }

        let transport = self.transport.as_mut().ok_or_else(|| anyhow!("Transport not available"))?;
        let response = transport.send(JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some("resources-list".to_string()),
            method: "resources/list".to_string(),
            params: None,
        }).await?;

        if let Some(error) = response.error {
            return Err(anyhow!("MCP resources/list error: {}", error.message));
        }

        if let Some(result) = response.result {
            if let Some(resources) = result.get("resources")
                .and_then(|v| serde_json::from_value::<Vec<Resource>>(v.clone()).ok())
            {
                self.resources = resources.into_iter().map(|resource| (resource.uri.clone(), resource)).collect();
            }
        }
        Ok(())
    }

    pub async fn call_tool(&mut self, name: &str, arguments: Value) -> Result<Value> {
        let transport = self.transport.as_mut().ok_or_else(|| anyhow!("Transport not available"))?;
        let response = transport.send(JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some("tool-call".to_string()),
            method: "tools/call".to_string(),
            params: Some(json!({
                "name": name,
                "arguments": arguments
            })),
        }).await?;

        if let Some(error) = response.error {
            return Err(anyhow!("MCP tools/call error: {}", error.message));
        }

        Ok(response.result.unwrap_or_else(|| json!({})))
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

    pub fn get_client_mut(&mut self, name: &str) -> Option<&mut McpClient> {
        self.clients.get_mut(name)
    }

    pub fn remove_client(&mut self, name: &str) -> Option<McpClient> {
        self.clients.remove(name)
    }

    pub fn list_all_tools(&self) -> Vec<(String, Tool)> {
        let mut all_tools = Vec::new();
        for client in self.clients.values() {
            for tool in client.list_tools() {
                all_tools.push((client.name.clone(), tool));
            }
        }
        all_tools
    }

    pub fn list_clients(&self) -> Vec<(String, &McpClient)> {
        self.clients.values().map(|client| (client.name.clone(), client)).collect()
    }

    pub async fn shutdown_all(&mut self) -> Result<()> {
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
