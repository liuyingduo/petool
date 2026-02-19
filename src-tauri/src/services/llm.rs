use anyhow::{anyhow, Result};
use async_openai::{config::OpenAIConfig, traits::RequestOptionsBuilder, Client as OpenAiClient};
use futures_util::StreamExt;
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_details: Option<Vec<ChatReasoningDetail>>,
    #[serde(skip_serializing, skip_deserializing, default)]
    pub reasoning: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChatReasoningDetail {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub detail_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<usize>,
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
    pub extra_body: Option<Value>,
    // #[serde(skip_serializing_if = "Option::is_none")]
    // pub tool_stream: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<Choice>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct OpenAiCompatStreamResponse {
    #[serde(default)]
    choices: Vec<OpenAiCompatStreamChoice>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct OpenAiCompatStreamChoice {
    #[serde(default)]
    delta: OpenAiCompatStreamDelta,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct OpenAiCompatStreamDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    reasoning_content: Option<String>,
    #[serde(default)]
    reasoning_details: Option<Vec<ChatReasoningDetail>>,
    #[serde(default)]
    tool_calls: Option<Vec<OpenAiCompatToolCallChunk>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct OpenAiCompatToolCallChunk {
    #[serde(default)]
    index: usize,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: Option<OpenAiCompatFunctionCallChunk>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct OpenAiCompatFunctionCallChunk {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
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
    pub reasoning_details: Option<Vec<ChatReasoningDetail>>,
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
    let chunk_trimmed = chunk.trim();
    if base.is_empty() {
        base.push_str(chunk);
        return;
    }

    // Recover malformed concatenated objects from a single provider chunk, e.g.:
    //   {}{"action":"start"}
    if chunk_trimmed.contains("}{") {
        if let Some(start) = chunk_trimmed.rfind('{') {
            let tail = chunk_trimmed[start..].trim();
            if tail.starts_with('{') && serde_json::from_str::<Value>(tail).is_ok() {
                *base = tail.to_string();
                return;
            }
        }
    }

    // Some providers stream partial_json as full snapshots at each delta.
    // If the incoming chunk is already a valid JSON object, prefer replacing
    // current buffer to avoid corrupt concatenation like "}{".
    if chunk_trimmed.starts_with('{') && serde_json::from_str::<Value>(chunk_trimmed).is_ok() {
        *base = chunk_trimmed.to_string();
        return;
    }

    // Keep safe dedupe rules for incremental fragments.
    if chunk == base.as_str() {
        return;
    }
    if chunk_trimmed == base.as_str() {
        return;
    }
    if chunk.starts_with(base.as_str()) {
        *base = chunk.to_string();
        return;
    }
    if chunk_trimmed.starts_with(base.as_str()) {
        *base = chunk_trimmed.to_string();
        return;
    }

    base.push_str(chunk);
}

fn append_reasoning_chunk(base: &mut String, chunk: &str) -> Option<String> {
    if chunk.is_empty() {
        return None;
    }
    if base.is_empty() {
        base.push_str(chunk);
        return Some(chunk.to_string());
    }
    if chunk == base.as_str() {
        return None;
    }
    if chunk.starts_with(base.as_str()) {
        let delta = chunk[base.len()..].to_string();
        *base = chunk.to_string();
        if delta.is_empty() {
            return None;
        }
        return Some(delta);
    }
    base.push_str(chunk);
    Some(chunk.to_string())
}

fn extract_reasoning_text_from_details(details: Option<&[ChatReasoningDetail]>) -> Option<String> {
    let details = details?;
    let text = details
        .iter()
        .filter_map(|detail| detail.text.as_deref())
        .collect::<Vec<_>>()
        .join("");
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

pub fn reasoning_details_from_text(text: &str) -> Option<Vec<ChatReasoningDetail>> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(vec![ChatReasoningDetail {
        text: Some(trimmed.to_string()),
        detail_type: Some("reasoning.text".to_string()),
        format: Some("MiniMax-response-v1".to_string()),
        id: Some("reasoning-text-1".to_string()),
        index: Some(0),
    }])
}

fn minimax_reasoning_split_extra_body(model: &str) -> Option<Value> {
    if is_minimax_model(model) {
        Some(json!({ "reasoning_split": true }))
    } else {
        None
    }
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

fn is_system_role(role: &str) -> bool {
    role.trim().eq_ignore_ascii_case("system")
}

fn is_minimax_model(model: &str) -> bool {
    model.trim().to_ascii_lowercase().starts_with("minimax-")
}

fn merge_leading_system_messages(messages: Vec<ChatMessage>) -> Vec<ChatMessage> {
    let leading_system_count = messages
        .iter()
        .take_while(|message| is_system_role(&message.role))
        .count();
    if leading_system_count <= 1 {
        return messages;
    }

    let mut merged_parts: Vec<String> = Vec::new();
    for message in messages.iter().take(leading_system_count) {
        if let Some(content) = message.content.as_deref() {
            let trimmed = content.trim();
            if !trimmed.is_empty() {
                merged_parts.push(trimmed.to_string());
            }
        }
    }

    let mut normalized: Vec<ChatMessage> = Vec::new();
    if !merged_parts.is_empty() {
        normalized.push(ChatMessage {
            role: "system".to_string(),
            content: Some(merged_parts.join("\n\n")),
            tool_calls: None,
            tool_call_id: None,
            reasoning_details: None,
            reasoning: None,
        });
    }

    normalized.extend(messages.into_iter().skip(leading_system_count));
    normalized
}

pub struct LlmService {
    client: OpenAiClient<OpenAIConfig>,
    api_key: String,
    api_base: String,
}

impl LlmService {
    pub fn new(api_key: String, api_base: Option<String>) -> Self {
        let resolved_api_base = api_base
            .unwrap_or_else(|| DEFAULT_API_BASE.to_string())
            .trim_end_matches('/')
            .to_string();
        let config = OpenAIConfig::new()
            .with_api_key(api_key.clone())
            .with_api_base(resolved_api_base.clone());
        Self {
            client: OpenAiClient::with_config(config),
            api_key,
            api_base: resolved_api_base,
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

    async fn chat_openai_compatible(
        &self,
        model: &str,
        messages: Vec<ChatMessage>,
    ) -> Result<String> {
        let messages = if is_minimax_model(model) {
            merge_leading_system_messages(messages)
        } else {
            messages
        };

        let request = ChatRequest {
            model: model.to_string(),
            messages,
            stream: false,
            tools: None,
            tool_choice: None,
            extra_body: minimax_reasoning_split_extra_body(model),
            // tool_stream: None,
        };

        let chat_response: ChatResponse =
            self.client.chat().create_byot(request).await.map_err(|e| {
                eprintln!(
                    "[llm] non-stream request failed: model={}, error={}",
                    model, e
                );
                anyhow!("API error: {}", e)
            })?;
        let choice = chat_response
            .choices
            .first()
            .ok_or_else(|| anyhow!("No response from API"))?;

        Ok(choice.message.content.clone().unwrap_or_default())
    }

    async fn chat_anthropic(&self, model: &str, messages: Vec<ChatMessage>) -> Result<String> {
        let request = self.build_anthropic_request(model, messages, None, false)?;

        let chat_api = self
            .client
            .chat()
            .path("/v1/messages")
            .map_err(|e| anyhow!("Anthropic API error: {}", e))?
            .header("x-api-key", &self.api_key)
            .map_err(|e| anyhow!("Anthropic API error: {}", e))?
            .header("anthropic-version", ANTHROPIC_API_VERSION)
            .map_err(|e| anyhow!("Anthropic API error: {}", e))?;

        let response_json: Value = chat_api
            .create_byot(request)
            .await
            .map_err(|e| anyhow!("Anthropic API error: {}", e))?;
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
        let messages = if is_minimax_model(model) {
            merge_leading_system_messages(messages)
        } else {
            messages
        };
        let request = ChatRequest {
            model: model.to_string(),
            messages,
            stream: true,
            tool_choice: if is_minimax_model(model) {
                None
            } else {
                tools.as_ref().map(|_| "auto".to_string())
            },
            tools: tools.clone(),
            extra_body: minimax_reasoning_split_extra_body(model),
            // Some OpenAI-compatible providers (including MiniMax /v1) reject tool_stream.
            // tool_stream: None,
        };
        let primary_stream_result = self.client.chat().create_stream_byot(request.clone()).await;
        let mut stream = match primary_stream_result {
            Ok(stream) => stream,
            Err(primary_error) => {
                let error_text = primary_error.to_string();
                eprintln!(
                    "[llm] primary stream request failed: model={}, error={}",
                    model, error_text
                );
                let should_retry_without_tools = request.tools.is_some()
                    && error_text
                        .to_ascii_lowercase()
                        .contains("invalid chat setting");

                if !should_retry_without_tools {
                    return Err(anyhow!("API error: {}", primary_error));
                }

                let mut fallback_request = request;
                fallback_request.tools = None;
                fallback_request.tool_choice = None;
                self.client
                    .chat()
                    .create_stream_byot(fallback_request)
                    .await
                    .map_err(|fallback_error| {
                        eprintln!(
                            "[llm] fallback stream request without tools failed: model={}, error={}",
                            model, fallback_error
                        );
                        anyhow!(
                            "API error: {}; fallback without tools failed: {}",
                            error_text,
                            fallback_error
                        )
                    })?
            }
        };

        let mut content = String::new();
        let mut reasoning = String::new();
        let mut tool_call_builders: BTreeMap<usize, ToolCallBuilder> = BTreeMap::new();
        let mut cancelled = false;

        while let Some(item) = stream.next().await {
            if should_cancel() {
                cancelled = true;
                break;
            }
            let value: OpenAiCompatStreamResponse =
                item.map_err(|e| anyhow!("API error: {}", e))?;

            for choice in value.choices {
                if should_cancel() {
                    cancelled = true;
                    break;
                }

                if let Some(chunk_text) = choice.delta.content {
                    if !chunk_text.is_empty() {
                        let part = chunk_text.to_string();
                        content.push_str(&part);
                        callback(LlmStreamEvent::Content(part));
                    }
                }

                if let Some(reasoning_text) = choice.delta.reasoning_content {
                    if let Some(delta_reasoning) = append_reasoning_chunk(&mut reasoning, &reasoning_text)
                    {
                        callback(LlmStreamEvent::Reasoning(delta_reasoning));
                    }
                }

                if let Some(details) = choice.delta.reasoning_details {
                    for detail in details {
                        if let Some(part) = detail.text.as_deref() {
                            if let Some(delta_reasoning) = append_reasoning_chunk(&mut reasoning, part)
                            {
                                callback(LlmStreamEvent::Reasoning(delta_reasoning));
                            }
                        }
                    }
                }

                if let Some(tool_calls) = choice.delta.tool_calls {
                    for item in tool_calls {
                        let index = item.index;
                        let entry = tool_call_builders.entry(index).or_default();

                        let id = item.id;
                        if let Some(ref value) = id {
                            entry.id = Some(value.clone());
                        }

                        let name = item.function.as_ref().and_then(|v| v.name.clone());
                        if let Some(ref value) = name {
                            entry.name = Some(value.clone());
                        }

                        let arguments_chunk = item.function.and_then(|v| v.arguments);
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
            if cancelled {
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

        let reasoning_details = reasoning_details_from_text(&reasoning);
        Ok(LlmStreamResult {
            content,
            reasoning,
            reasoning_details,
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
        let request = self.build_anthropic_request(model, messages, tools.clone(), true)?;

        let chat_api = self
            .client
            .chat()
            .path("/v1/messages")
            .map_err(|e| anyhow!("Anthropic API error: {}", e))?
            .header("x-api-key", &self.api_key)
            .map_err(|e| anyhow!("Anthropic API error: {}", e))?
            .header("anthropic-version", ANTHROPIC_API_VERSION)
            .map_err(|e| anyhow!("Anthropic API error: {}", e))?;

        let mut stream = chat_api
            .create_stream_byot(request)
            .await
            .map_err(|e| anyhow!("Anthropic API error: {}", e))?;
        let mut content = String::new();
        let mut reasoning = String::new();
        let mut tool_call_builders: BTreeMap<usize, ToolCallBuilder> = BTreeMap::new();
        let mut cancelled = false;

        while let Some(item) = stream.next().await {
            if should_cancel() {
                cancelled = true;
                break;
            }

            let value: Value = item.map_err(|e| anyhow!("Anthropic API error: {}", e))?;

            let event_type = value
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or_default();

            if event_type == "content_block_start" {
                let index = value.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                if let Some(block) = value.get("content_block") {
                    let block_type = block
                        .get("type")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
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
                let delta_type = delta
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or_default();

                if delta_type == "text_delta" {
                    if let Some(part) = delta.get("text").and_then(Value::as_str) {
                        if !part.is_empty() {
                            content.push_str(part);
                            callback(LlmStreamEvent::Content(part.to_string()));
                        }
                    }
                    continue;
                }

                if delta_type == "thinking_delta" {
                    if let Some(part) = delta.get("thinking").and_then(Value::as_str) {
                        if !part.is_empty() {
                            reasoning.push_str(part);
                            callback(LlmStreamEvent::Reasoning(part.to_string()));
                        }
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
                break;
            }

            if cancelled {
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

        let reasoning_details = reasoning_details_from_text(&reasoning);
        Ok(LlmStreamResult {
            content,
            reasoning,
            reasoning_details,
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
                if let Some(reasoning_text) = message
                    .reasoning
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(|value| value.to_string())
                    .or_else(|| {
                        extract_reasoning_text_from_details(message.reasoning_details.as_deref())
                    })
                {
                    content_blocks.push(json!({
                        "type": "thinking",
                        "thinking": reasoning_text
                    }));
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
                self.push_or_merge_anthropic_message(
                    &mut anthropic_messages,
                    "user",
                    content_blocks,
                );
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

        let response_json: Value = self
            .client
            .chat()
            .create_byot(request)
            .await
            .map_err(|e| anyhow!("API error: {}", e))?;
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
        let request = json!({
            "model": model,
            "prompt": prompt,
            "size": size,
            "response_format": "url",
            "extra_body": {
                "watermark": watermark
            }
        });

        let response_json: Value = self
            .client
            .images()
            .generate_byot(request)
            .await
            .map_err(|e| anyhow!("Image API error: {}", e))?;
        let image_url = response_json
            .pointer("/data/0/url")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| anyhow!("Image API returned empty URL"))?;

        Ok(image_url.to_string())
    }
}
