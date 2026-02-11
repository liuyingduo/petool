use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const DEFAULT_API_BASE: &str = "https://open.bigmodel.cn/api/paas/v4";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ChatToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
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
    pub tool_calls: Vec<ChatToolCall>,
    pub finish_reason: Option<String>,
}

#[derive(Default)]
struct ToolCallBuilder {
    id: Option<String>,
    name: Option<String>,
    arguments: String,
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

    pub async fn chat(&self, model: &str, messages: Vec<ChatMessage>) -> Result<String> {
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

    pub async fn chat_stream<'a>(
        &'a self,
        model: &'a str,
        messages: Vec<ChatMessage>,
        mut callback: impl FnMut(String) + Send + 'a,
    ) -> Result<()> {
        let _ = self
            .chat_stream_with_tools(model, messages, None, move |event| {
                if let LlmStreamEvent::Content(chunk) = event {
                    callback(chunk);
                }
            })
            .await?;

        Ok(())
    }

    pub async fn chat_stream_with_tools<'a>(
        &'a self,
        model: &'a str,
        messages: Vec<ChatMessage>,
        tools: Option<Vec<ChatTool>>,
        mut callback: impl FnMut(LlmStreamEvent) + Send + 'a,
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
        let mut finish_reason: Option<String> = None;
        let mut tool_call_builders: BTreeMap<usize, ToolCallBuilder> = BTreeMap::new();

        while let Some(item) = stream.next().await {
            let chunk = item?;
            let text = String::from_utf8_lossy(&chunk);
            buffer.push_str(&text);

            while let Some(pos) = buffer.find('\n') {
                let line = buffer[..pos].trim_end_matches('\r').to_string();
                buffer = buffer[pos + 1..].to_string();

                if !line.starts_with("data: ") {
                    continue;
                }

                let data = &line[6..];
                if data == "[DONE]" {
                    break;
                }

                let value: serde_json::Value = match serde_json::from_str(data) {
                    Ok(v) => v,
                    Err(_) => continue,
                };

                if finish_reason.is_none() {
                    finish_reason = value
                        .pointer("/choices/0/finish_reason")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                }

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
                            entry.arguments.push_str(value);
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
            tool_calls,
            finish_reason,
        })
    }
}
