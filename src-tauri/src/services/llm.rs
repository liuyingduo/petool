use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

const DEFAULT_API_BASE: &str = "https://open.bigmodel.cn/api/paas/v4";
const ANTHROPIC_API_VERSION: &str = "2023-06-01";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ChatToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing, skip_deserializing, default)]
    pub reasoning: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatTool {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: ChatToolFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatToolFunction {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: ChatToolCallFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatToolCallFunction {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ChatTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_stream: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<Choice>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Choice {
    pub index: i32,
    pub message: AssistantMessage,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantMessage {
    pub role: String,
    pub content: Option<String>,
    pub tool_calls: Option<Vec<ChatToolCall>>,
}

#[derive(Debug, Clone)]
pub struct ToolCallDelta {
    pub index: usize,
    pub id: Option<String>,
    pub name: Option<String>,
    pub arguments_chunk: Option<String>,
}

#[derive(Debug, Clone)]
pub enum LlmStreamEvent {
    Content(String),
    Reasoning(String),
    ToolCallDelta(ToolCallDelta),
}

#[derive(Debug, Clone)]
pub struct LlmStreamResult {
    pub content: String,
    pub reasoning: String,
    pub tool_calls: Vec<ChatToolCall>,
    pub cancelled: bool,
}

#[derive(Default)]
struct ToolCallBuilder {
    id: Option<String>,
    name: Option<String>,
    arguments: String,
}

fn append_tool_arguments_chunk(base: &mut String, chunk: &str) {
    if chunk.is_empty() {
        return;
    }
    if base.is_empty() {
        base.push_str(chunk);
        return;
    }

    // Tool arguments are JSON-like payloads. Keep merging conservative:
    // prefer full snapshots, ignore exact/contained repeats, otherwise append raw.
    if chunk == base.as_str() {
        return;
    }
    if chunk.starts_with(base.as_str()) || chunk.contains(base.as_str()) {
        *base = chunk.to_string();
        return;
    }
    if base.contains(chunk) || base.ends_with(chunk) {
        return;
    }
    base.push_str(chunk);
}

fn extract_assistant_content_text(content: &Value) -> Option<String> {
    if let Some(text) = content.as_str() {
        return Some(text.to_string());
    }

    let parts = content.as_array()?;
    let merged = parts
        .iter()
        .filter_map(|part| {
            let part_type = part.get("type").and_then(Value::as_str)?;
            if part_type != "text" {
                return None;
            }
            part.get("text")
                .and_then(Value::as_str)
                .map(|text| text.to_string())
        })
        .collect::<Vec<_>>()
        .join("");

    if merged.is_empty() {
        None
    } else {
        Some(merged)
    }
}

pub struct LlmService {
    client: Client,
    api_key: String,
    api_base: String,
}

impl LlmService {
    pub fn new(api_key: String, api_base: Option<String>) -> Self {
        Self {
            client: Client::new(),
            api_key,
            api_base: api_base.unwrap_or_else(|| DEFAULT_API_BASE.to_string()),
        }
    }

    fn is_anthropic_compatible(&self) -> bool {
        let base = self.api_base.to_ascii_lowercase();
        base.contains("/anthropic")
    }

    pub async fn chat(&self, model: &str, messages: Vec<ChatMessage>) -> Result<String> {
        if self.is_anthropic_compatible() {
            return self.chat_anthropic(model, messages).await;
        }
        self.chat_openai_compatible(model, messages).await
    }

    async fn chat_openai_compatible(&self, model: &str, messages: Vec<ChatMessage>) -> Result<String> {
        let url = format!("{}/chat/completions", self.api_base.trim_end_matches('/'));

        let request = ChatRequest {
            model: model.to_string(),
            messages,
            stream: false,
            tools: None,
            tool_choice: None,
            tool_stream: None,
        };

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let error = response.text().await?;
            return Err(anyhow!("API error: {}", error));
        }

        let chat_response: ChatResponse = response.json().await?;
        let choice = chat_response
            .choices
            .first()
            .ok_or_else(|| anyhow!("No response from API"))?;

        Ok(choice.message.content.clone().unwrap_or_default())
    }

    async fn chat_anthropic(&self, model: &str, messages: Vec<ChatMessage>) -> Result<String> {
        let url = format!("{}/v1/messages", self.api_base.trim_end_matches('/'));
        let request = self.build_anthropic_request(model, messages, None, false)?;

        let response = self
            .client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_API_VERSION)
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let error = response.text().await?;
            return Err(anyhow!("Anthropic API error: {}", error));
        }

        let response_json: Value = response.json().await?;
        let content_blocks = response_json
            .get("content")
            .and_then(Value::as_array)
            .ok_or_else(|| anyhow!("Anthropic response missing content blocks"))?;

        let mut text = String::new();
        for block in content_blocks {
            if block.get("type").and_then(Value::as_str) == Some("text") {
                if let Some(part) = block.get("text").and_then(Value::as_str) {
                    text.push_str(part);
                }
            }
        }
        Ok(text)
    }

    pub async fn chat_stream_with_tools<'a>(
        &'a self,
        model: &'a str,
        messages: Vec<ChatMessage>,
        tools: Option<Vec<ChatTool>>,
        callback: impl FnMut(LlmStreamEvent) + Send + 'a,
        should_cancel: impl Fn() -> bool + Send + Sync + 'a,
    ) -> Result<LlmStreamResult> {
        if self.is_anthropic_compatible() {
            return self
                .chat_stream_with_tools_anthropic(model, messages, tools, callback, should_cancel)
                .await;
        }
        self.chat_stream_with_tools_openai(model, messages, tools, callback, should_cancel)
            .await
    }

    async fn chat_stream_with_tools_openai<'a>(
        &'a self,
        model: &'a str,
        messages: Vec<ChatMessage>,
        tools: Option<Vec<ChatTool>>,
        mut callback: impl FnMut(LlmStreamEvent) + Send + 'a,
        should_cancel: impl Fn() -> bool + Send + Sync + 'a,
    ) -> Result<LlmStreamResult> {
        let url = format!("{}/chat/completions", self.api_base.trim_end_matches('/'));

        let request = ChatRequest {
            model: model.to_string(),
            messages,
            stream: true,
            tools: tools.clone(),
            tool_choice: tools.as_ref().map(|_| "auto".to_string()),
            tool_stream: tools.as_ref().map(|_| true),
        };

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let error = response.text().await?;
            return Err(anyhow!("API error: {}", error));
        }

        let mut stream = response.bytes_stream();
        use futures_util::StreamExt;

        let mut buffer = String::new();
        let mut content = String::new();
        let mut reasoning = String::new();
        let mut tool_call_builders: BTreeMap<usize, ToolCallBuilder> = BTreeMap::new();
        let mut cancelled = false;
        let mut done = false;

        while let Some(item) = stream.next().await {
            if should_cancel() {
                cancelled = true;
                break;
            }
            let chunk = item?;
            let text = String::from_utf8_lossy(&chunk);
            buffer.push_str(&text);

            while let Some(pos) = buffer.find('\n') {
                if should_cancel() {
                    cancelled = true;
                    break;
                }
                let line = buffer[..pos].trim_end_matches('\r').to_string();
                buffer = buffer[pos + 1..].to_string();

                if !line.starts_with("data: ") {
                    continue;
                }

                let data = &line[6..];
                if data == "[DONE]" {
                    done = true;
                    break;
                }

                let value: serde_json::Value = match serde_json::from_str(data) {
                    Ok(v) => v,
                    Err(_) => continue,
                };

                if let Some(chunk_text) = value
                    .pointer("/choices/0/delta/content")
                    .and_then(|v| v.as_str())
                {
                    let part = chunk_text.to_string();
                    content.push_str(&part);
                    callback(LlmStreamEvent::Content(part));
                }

                if let Some(reasoning_text) = value
                    .pointer("/choices/0/delta/reasoning_content")
                    .and_then(|v| v.as_str())
                {
                    reasoning.push_str(reasoning_text);
                    callback(LlmStreamEvent::Reasoning(reasoning_text.to_string()));
                }

                if let Some(tool_calls) = value
                    .pointer("/choices/0/delta/tool_calls")
                    .and_then(|v| v.as_array())
                {
                    for item in tool_calls {
                        let index =
                            item.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                        let entry = tool_call_builders.entry(index).or_default();

                        let id = item
                            .get("id")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        if let Some(ref value) = id {
                            entry.id = Some(value.clone());
                        }

                        let name = item
                            .pointer("/function/name")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        if let Some(ref value) = name {
                            entry.name = Some(value.clone());
                        }

                        let arguments_chunk = item
                            .pointer("/function/arguments")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        if let Some(ref value) = arguments_chunk {
                            append_tool_arguments_chunk(&mut entry.arguments, value);
                        }

                        callback(LlmStreamEvent::ToolCallDelta(ToolCallDelta {
                            index,
                            id,
                            name,
                            arguments_chunk,
                        }));
                    }
                }
            }

            if cancelled || done {
                break;
            }
        }

        let tool_calls = tool_call_builders
            .into_iter()
            .filter_map(|(index, builder)| {
                let name = builder.name.unwrap_or_default();
                if name.is_empty() {
                    return None;
                }

                Some(ChatToolCall {
                    id: builder.id.unwrap_or_else(|| format!("tool_call_{}", index)),
                    call_type: "function".to_string(),
                    function: ChatToolCallFunction {
                        name,
                        arguments: builder.arguments,
                    },
                })
            })
            .collect();

        Ok(LlmStreamResult {
            content,
            reasoning,
            tool_calls,
            cancelled,
        })
    }

    async fn chat_stream_with_tools_anthropic<'a>(
        &'a self,
        model: &'a str,
        messages: Vec<ChatMessage>,
        tools: Option<Vec<ChatTool>>,
        mut callback: impl FnMut(LlmStreamEvent) + Send + 'a,
        should_cancel: impl Fn() -> bool + Send + Sync + 'a,
    ) -> Result<LlmStreamResult> {
        use futures_util::StreamExt;

        let url = format!("{}/v1/messages", self.api_base.trim_end_matches('/'));
        let request = self.build_anthropic_request(model, messages, tools.clone(), true)?;

        let response = self
            .client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_API_VERSION)
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let error = response.text().await?;
            return Err(anyhow!("Anthropic API error: {}", error));
        }

        let mut stream = response.bytes_stream();
        let mut buffer = String::new();
        let mut content = String::new();
        let mut reasoning = String::new();
        let mut tool_call_builders: BTreeMap<usize, ToolCallBuilder> = BTreeMap::new();
        let mut cancelled = false;
        let mut done = false;

        while let Some(item) = stream.next().await {
            if should_cancel() {
                cancelled = true;
                break;
            }

            let chunk = item?;
            let text = String::from_utf8_lossy(&chunk);
            buffer.push_str(&text);

            while let Some(pos) = buffer.find('\n') {
                if should_cancel() {
                    cancelled = true;
                    break;
                }
                let line = buffer[..pos].trim_end_matches('\r').to_string();
                buffer = buffer[pos + 1..].to_string();

                if !line.starts_with("data: ") {
                    continue;
                }

                let data = &line[6..];
                if data == "[DONE]" {
                    done = true;
                    break;
                }

                let value: Value = match serde_json::from_str(data) {
                    Ok(v) => v,
                    Err(_) => continue,
                };

                let event_type = value.get("type").and_then(Value::as_str).unwrap_or_default();

                if event_type == "content_block_start" {
                    let index = value.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                    if let Some(block) = value.get("content_block") {
                        let block_type = block.get("type").and_then(Value::as_str).unwrap_or_default();
                        if block_type == "tool_use" {
                            let entry = tool_call_builders.entry(index).or_default();
                            if let Some(id) = block.get("id").and_then(Value::as_str) {
                                entry.id = Some(id.to_string());
                            }
                            if let Some(name) = block.get("name").and_then(Value::as_str) {
                                entry.name = Some(name.to_string());
                            }
                            if let Some(input) = block.get("input") {
                                if !input.is_null() {
                                    let raw = serde_json::to_string(input)
                                        .unwrap_or_else(|_| "{}".to_string());
                                    append_tool_arguments_chunk(&mut entry.arguments, &raw);
                                    callback(LlmStreamEvent::ToolCallDelta(ToolCallDelta {
                                        index,
                                        id: entry.id.clone(),
                                        name: entry.name.clone(),
                                        arguments_chunk: Some(raw),
                                    }));
                                }
                            }
                        }
                    }
                    continue;
                }

                if event_type == "content_block_delta" {
                    let index = value.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                    let Some(delta) = value.get("delta") else {
                        continue;
                    };
                    let delta_type = delta.get("type").and_then(Value::as_str).unwrap_or_default();

                    if delta_type == "text_delta" {
                        if let Some(part) = delta.get("text").and_then(Value::as_str) {
                            content.push_str(part);
                            callback(LlmStreamEvent::Content(part.to_string()));
                        }
                        continue;
                    }

                    if delta_type == "thinking_delta" {
                        if let Some(part) = delta.get("thinking").and_then(Value::as_str) {
                            reasoning.push_str(part);
                            callback(LlmStreamEvent::Reasoning(part.to_string()));
                        }
                        continue;
                    }

                    if delta_type == "input_json_delta" {
                        if let Some(part) = delta.get("partial_json").and_then(Value::as_str) {
                            let entry = tool_call_builders.entry(index).or_default();
                            append_tool_arguments_chunk(&mut entry.arguments, part);
                            callback(LlmStreamEvent::ToolCallDelta(ToolCallDelta {
                                index,
                                id: entry.id.clone(),
                                name: entry.name.clone(),
                                arguments_chunk: Some(part.to_string()),
                            }));
                        }
                    }
                    continue;
                }

                if event_type == "message_stop" {
                    done = true;
                    break;
                }
            }

            if cancelled || done {
                break;
            }
        }

        let tool_calls = tool_call_builders
            .into_iter()
            .filter_map(|(index, builder)| {
                let name = builder.name.unwrap_or_default();
                if name.is_empty() {
                    return None;
                }
                let arguments = if builder.arguments.trim().is_empty() {
                    "{}".to_string()
                } else {
                    builder.arguments
                };
                Some(ChatToolCall {
                    id: builder.id.unwrap_or_else(|| format!("tool_call_{}", index)),
                    call_type: "function".to_string(),
                    function: ChatToolCallFunction { name, arguments },
                })
            })
            .collect::<Vec<_>>();

        Ok(LlmStreamResult {
            content,
            reasoning,
            tool_calls,
            cancelled,
        })
    }

    fn build_anthropic_request(
        &self,
        model: &str,
        messages: Vec<ChatMessage>,
        tools: Option<Vec<ChatTool>>,
        stream: bool,
    ) -> Result<Value> {
        let mut system_parts: Vec<String> = Vec::new();
        let mut anthropic_messages: Vec<Value> = Vec::new();

        for message in messages {
            let role = message.role.trim().to_ascii_lowercase();
            if role == "system" {
                if let Some(content) = message.content {
                    let trimmed = content.trim();
                    if !trimmed.is_empty() {
                        system_parts.push(trimmed.to_string());
                    }
                }
                continue;
            }

            let mut content_blocks: Vec<Value> = Vec::new();

            if role == "assistant" {
                if let Some(reasoning_text) = message.reasoning {
                    let trimmed = reasoning_text.trim();
                    if !trimmed.is_empty() {
                        content_blocks.push(json!({
                            "type": "thinking",
                            "thinking": trimmed
                        }));
                    }
                }
                if let Some(text) = message.content {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        content_blocks.push(json!({
                            "type": "text",
                            "text": trimmed
                        }));
                    }
                }
                if let Some(tool_calls) = message.tool_calls {
                    for tool_call in tool_calls {
                        let input = serde_json::from_str::<Value>(&tool_call.function.arguments)
                            .unwrap_or_else(|_| json!({ "raw": tool_call.function.arguments }));
                        content_blocks.push(json!({
                            "type": "tool_use",
                            "id": tool_call.id,
                            "name": tool_call.function.name,
                            "input": input
                        }));
                    }
                }
                if content_blocks.is_empty() {
                    continue;
                }
                self.push_or_merge_anthropic_message(
                    &mut anthropic_messages,
                    "assistant",
                    content_blocks,
                );
                continue;
            }

            if role == "tool" {
                let tool_use_id = message.tool_call_id.unwrap_or_default();
                if tool_use_id.is_empty() {
                    continue;
                }
                let result_text = message.content.unwrap_or_default();
                content_blocks.push(json!({
                    "type": "tool_result",
                    "tool_use_id": tool_use_id,
                    "content": result_text
                }));
                self.push_or_merge_anthropic_message(&mut anthropic_messages, "user", content_blocks);
                continue;
            }

            if let Some(text) = message.content {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    content_blocks.push(json!({
                        "type": "text",
                        "text": trimmed
                    }));
                }
            }
            if content_blocks.is_empty() {
                continue;
            }
            self.push_or_merge_anthropic_message(&mut anthropic_messages, "user", content_blocks);
        }

        let mut request = json!({
            "model": model,
            "max_tokens": 8192,
            "stream": stream,
            "messages": anthropic_messages
        });

        let system = system_parts.join("\n\n");
        if !system.is_empty() {
            request["system"] = json!(system);
        }

        if let Some(tools) = tools {
            if !tools.is_empty() {
                let mapped_tools = tools
                    .into_iter()
                    .map(|tool| {
                        json!({
                            "name": tool.function.name,
                            "description": tool.function.description,
                            "input_schema": tool.function.parameters
                        })
                    })
                    .collect::<Vec<_>>();
                request["tools"] = json!(mapped_tools);
                request["tool_choice"] = json!({ "type": "auto" });
            }
        }

        Ok(request)
    }

    fn push_or_merge_anthropic_message(
        &self,
        messages: &mut Vec<Value>,
        role: &str,
        blocks: Vec<Value>,
    ) {
        if let Some(last) = messages.last_mut() {
            let same_role = last
                .get("role")
                .and_then(Value::as_str)
                .map(|value| value.eq_ignore_ascii_case(role))
                .unwrap_or(false);
            if same_role {
                if let Some(content) = last.get_mut("content").and_then(Value::as_array_mut) {
                    content.extend(blocks);
                    return;
                }
            }
        }

        messages.push(json!({
            "role": role,
            "content": blocks
        }));
    }

    pub async fn chat_with_image_url(
        &self,
        model: &str,
        prompt: &str,
        image_url: &str,
        enable_thinking: bool,
    ) -> Result<String> {
        let url = format!("{}/chat/completions", self.api_base.trim_end_matches('/'));

        let mut request = json!({
            "model": model,
            "stream": false,
            "messages": [
                {
                    "role": "user",
                    "content": [
                        {
                            "type": "image_url",
                            "image_url": {
                                "url": image_url
                            }
                        },
                        {
                            "type": "text",
                            "text": prompt
                        }
                    ]
                }
            ]
        });

        if enable_thinking {
            request["thinking"] = json!({
                "type": "enabled"
            });
        }

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let error = response.text().await?;
            return Err(anyhow!("API error: {}", error));
        }

        let response_json: Value = response.json().await?;
        let content = response_json
            .pointer("/choices/0/message/content")
            .and_then(extract_assistant_content_text)
            .unwrap_or_default();

        Ok(content)
    }

    pub async fn generate_image(
        &self,
        model: &str,
        prompt: &str,
        size: &str,
        watermark: bool,
    ) -> Result<String> {
        let url = format!("{}/images/generations", self.api_base.trim_end_matches('/'));
        let request = json!({
            "model": model,
            "prompt": prompt,
            "size": size,
            "response_format": "url",
            "extra_body": {
                "watermark": watermark
            }
        });

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let error = response.text().await?;
            return Err(anyhow!("Image API error: {}", error));
        }

        let response_json: Value = response.json().await?;
        let image_url = response_json
            .pointer("/data/0/url")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| anyhow!("Image API returned empty URL"))?;

        Ok(image_url.to_string())
    }
}
