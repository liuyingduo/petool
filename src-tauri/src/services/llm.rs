use anyhow::{Result, anyhow};
use reqwest::Client;
use serde::{Deserialize, Serialize};

const DEFAULT_API_BASE: &str = "https://api.openai.com/v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub stream: bool,
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
    pub message: ChatMessage,
    pub finish_reason: String,
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
        };

        let response = self.client
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

        chat_response.choices
            .first()
            .map(|c| c.message.content.clone())
            .ok_or_else(|| anyhow!("No response from API"))
    }

    pub async fn chat_stream<'a>(
        &'a self,
        model: &'a str,
        messages: Vec<ChatMessage>,
        mut callback: impl FnMut(String) + Send + 'a,
    ) -> Result<()> {
        let url = format!("{}/chat/completions", self.api_base.trim_end_matches('/'));

        let request = ChatRequest {
            model: model.to_string(),
            messages,
            stream: true,
        };

        let response = self.client
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

        while let Some(item) = stream.next().await {
            let chunk = item?;
            let text = String::from_utf8_lossy(&chunk);
            buffer.push_str(&text);

            // Process complete lines
            while let Some(pos) = buffer.find('\n') {
                let line = buffer[..pos].to_string();
                buffer = buffer[pos + 1..].to_string();

                if line.starts_with("data: ") {
                    let data = &line[6..];
                    if data == "[DONE]" {
                        return Ok(());
                    }
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(data) {
                        if let Some(content) = value
                            .pointer("/choices/0/delta/content")
                            .and_then(|v| v.as_str())
                        {
                            callback(content.to_string());
                        }
                    }
                }
            }
        }

        Ok(())
    }
}
