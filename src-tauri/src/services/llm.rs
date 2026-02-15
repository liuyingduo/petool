use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
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

fn append_with_overlap(base: &mut String, chunk: &str) {
    if chunk.is_empty() {
        return;
    }
    if base.is_empty() {
        base.push_str(chunk);
        return;
    }
    if base.contains(chunk) {
        return;
    }
    if chunk.starts_with(base.as_str()) {
        *base = chunk.to_string();
        return;
    }

    let base_chars: Vec<char> = base.chars().collect();
    let chunk_chars: Vec<char> = chunk.chars().collect();
    let max = base_chars.len().min(chunk_chars.len());
    for len in (6..=max).rev() {
        let base_suffix: String = base_chars[base_chars.len() - len..].iter().collect();
        let chunk_prefix: String = chunk_chars[..len].iter().collect();
        if base_suffix == chunk_prefix {
            let chunk_rest: String = chunk_chars[len..].iter().collect();
            base.push_str(&chunk_rest);
            return;
        }
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

    pub async fn chat_stream_with_tools<'a>(
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
                    append_with_overlap(&mut reasoning, reasoning_text);
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
