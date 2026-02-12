use std::collections::HashSet;
use std::time::Duration;

use regex::Regex;
use serde_json::{json, Value};

use super::{
    is_forbidden_loopback_host, read_optional_string_argument, read_string_argument,
    read_u64_argument, DEFAULT_EXA_MCP_ENDPOINT, DEFAULT_WEB_ACCEPT_LANGUAGE, DEFAULT_WEB_USER_AGENT,
};

fn decode_basic_html_entities(value: &str) -> String {
    let mut decoded = value
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ");

    if let Ok(decimal_re) = Regex::new(r"&#(\d+);") {
        decoded = decimal_re
            .replace_all(&decoded, |caps: &regex::Captures| {
                caps.get(1)
                    .and_then(|value| value.as_str().parse::<u32>().ok())
                    .and_then(char::from_u32)
                    .unwrap_or(' ')
                    .to_string()
            })
            .to_string();
    }

    if let Ok(hex_re) = Regex::new(r"&#x([0-9a-fA-F]+);") {
        decoded = hex_re
            .replace_all(&decoded, |caps: &regex::Captures| {
                caps.get(1)
                    .and_then(|value| u32::from_str_radix(value.as_str(), 16).ok())
                    .and_then(char::from_u32)
                    .unwrap_or(' ')
                    .to_string()
            })
            .to_string();
    }

    decoded
}

fn normalize_multiline_text(value: &str) -> String {
    let mut lines = Vec::new();
    let mut blank_line = false;
    for line in value.lines() {
        let compact = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if compact.is_empty() {
            if !blank_line {
                lines.push(String::new());
                blank_line = true;
            }
            continue;
        }
        blank_line = false;
        lines.push(compact);
    }
    lines.join("\n").trim().to_string()
}

fn clean_html_fragment(fragment: &str) -> String {
    let stripped = Regex::new(r"(?is)<[^>]+>")
        .ok()
        .map(|regex| regex.replace_all(fragment, " ").to_string())
        .unwrap_or_else(|| fragment.to_string());
    normalize_multiline_text(&decode_basic_html_entities(&stripped))
}

fn html_to_text_basic(html: &str) -> String {
    let mut value = html.to_string();
    for pattern in [
        r"(?is)<script[^>]*>.*?</script>",
        r"(?is)<style[^>]*>.*?</style>",
        r"(?is)<noscript[^>]*>.*?</noscript>",
    ] {
        if let Ok(regex) = Regex::new(pattern) {
            value = regex.replace_all(&value, " ").to_string();
        }
    }

    for pattern in [
        r"(?is)<br\s*/?>",
        r"(?is)</p>",
        r"(?is)</div>",
        r"(?is)</li>",
        r"(?is)</h[1-6]>",
        r"(?is)</tr>",
    ] {
        if let Ok(regex) = Regex::new(pattern) {
            value = regex.replace_all(&value, "\n").to_string();
        }
    }

    clean_html_fragment(&value)
}

fn html_to_markdown_basic(html: &str) -> String {
    let mut value = html.to_string();
    for pattern in [
        r"(?is)<script[^>]*>.*?</script>",
        r"(?is)<style[^>]*>.*?</style>",
        r"(?is)<noscript[^>]*>.*?</noscript>",
    ] {
        if let Ok(regex) = Regex::new(pattern) {
            value = regex.replace_all(&value, " ").to_string();
        }
    }

    if let Ok(anchor_re) = Regex::new(r#"(?is)<a[^>]*href=["']([^"']+)["'][^>]*>(.*?)</a>"#) {
        value = anchor_re
            .replace_all(&value, |caps: &regex::Captures| {
                let href = caps.get(1).map(|value| value.as_str()).unwrap_or("");
                let text = caps.get(2).map(|value| value.as_str()).unwrap_or("");
                let clean = clean_html_fragment(text);
                if clean.is_empty() {
                    format!(" {}", href)
                } else {
                    format!("[{}]({})", clean, href)
                }
            })
            .to_string();
    }

    for pattern in [r"(?is)<br\s*/?>", r"(?is)</p>", r"(?is)</div>", r"(?is)</li>"] {
        if let Ok(regex) = Regex::new(pattern) {
            value = regex.replace_all(&value, "\n").to_string();
        }
    }

    clean_html_fragment(&value)
}

fn truncate_by_chars(content: &str, max_chars: usize) -> (String, bool) {
    let char_count = content.chars().count();
    if char_count > max_chars {
        (content.chars().take(max_chars).collect::<String>(), true)
    } else {
        (content.to_string(), false)
    }
}

async fn fetch_web_raw(
    url_raw: &str,
    timeout_ms: u64,
    max_redirects: u64,
    retries: u64,
    user_agent: &str,
    accept_language: &str,
    accept: &str,
) -> Result<(reqwest::StatusCode, String, Option<String>, String, u64), String> {
    let url = reqwest::Url::parse(url_raw).map_err(|e| format!("Invalid URL: {}", e))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("Only http/https URLs are supported".to_string());
    }
    if is_forbidden_loopback_host(&url) {
        return Err("Local/private hosts are not allowed".to_string());
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(timeout_ms))
        .redirect(reqwest::redirect::Policy::limited(max_redirects as usize))
        .build()
        .map_err(|e| e.to_string())?;

    let max_attempts = retries.saturating_add(1).clamp(1, 4);
    let mut last_error = "Request failed".to_string();

    for attempt in 1..=max_attempts {
        let response = client
            .get(url.clone())
            .header(reqwest::header::USER_AGENT, user_agent)
            .header(reqwest::header::ACCEPT_LANGUAGE, accept_language)
            .header(reqwest::header::ACCEPT, accept)
            .send()
            .await;

        let response = match response {
            Ok(value) => value,
            Err(error) => {
                last_error = error.to_string();
                if attempt < max_attempts {
                    tokio::time::sleep(Duration::from_millis(250 * attempt)).await;
                    continue;
                }
                return Err(last_error);
            }
        };

        let status = response.status();
        let final_url = response.url().to_string();
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let body = response.text().await.map_err(|e| e.to_string())?;

        if status.is_success() {
            return Ok((status, final_url, content_type, body, attempt));
        }

        let short_body = body.chars().take(240).collect::<String>();
        last_error = format!(
            "HTTP {} from {}{}",
            status.as_u16(),
            final_url,
            if short_body.is_empty() {
                String::new()
            } else {
                format!(": {}", short_body)
            }
        );
        if (status.as_u16() == 429 || status.is_server_error()) && attempt < max_attempts {
            tokio::time::sleep(Duration::from_millis(250 * attempt)).await;
            continue;
        }
        return Err(last_error);
    }

    Err(last_error)
}

async fn fetch_web_content(
    url_raw: &str,
    timeout_ms: u64,
    max_chars: usize,
) -> Result<(reqwest::StatusCode, Option<String>, String, bool), String> {
    let (status, _, content_type, body, _) = fetch_web_raw(
        url_raw,
        timeout_ms,
        5,
        1,
        DEFAULT_WEB_USER_AGENT,
        DEFAULT_WEB_ACCEPT_LANGUAGE,
        "*/*",
    )
    .await?;
    let (content, truncated) = truncate_by_chars(&body, max_chars);
    Ok((status, content_type, content, truncated))
}

pub(super) async fn execute_web_fetch(arguments: &Value) -> Result<Value, String> {
    let url = read_string_argument(arguments, "url")?;
    let timeout_ms = read_u64_argument(arguments, "timeout_ms", 18_000).clamp(1_000, 90_000);
    let max_chars = read_u64_argument(arguments, "max_chars", 50_000).clamp(500, 500_000) as usize;
    let retries = read_u64_argument(arguments, "retries", 1).clamp(0, 3);
    let max_redirects = read_u64_argument(arguments, "max_redirects", 5).clamp(0, 10);
    let format = read_optional_string_argument(arguments, "format")
        .unwrap_or_else(|| "auto".to_string())
        .to_lowercase();
    let user_agent = read_optional_string_argument(arguments, "user_agent")
        .unwrap_or_else(|| DEFAULT_WEB_USER_AGENT.to_string());
    let accept_language = read_optional_string_argument(arguments, "accept_language")
        .unwrap_or_else(|| DEFAULT_WEB_ACCEPT_LANGUAGE.to_string());
    let accept_header = if format == "html" {
        "text/html,application/xhtml+xml;q=0.9,*/*;q=0.1"
    } else if format == "markdown" || format == "text" {
        "text/html,text/plain,application/json;q=0.9,*/*;q=0.1"
    } else {
        "*/*"
    };

    let (status, final_url, content_type, raw_content, attempts) = fetch_web_raw(
        &url,
        timeout_ms,
        max_redirects,
        retries,
        &user_agent,
        &accept_language,
        accept_header,
    )
    .await?;
    let normalized_type = content_type
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let is_html = normalized_type.contains("text/html");
    let is_json = normalized_type.contains("application/json");
    let (rendered, extractor) = match format.as_str() {
        "html" => (raw_content, "raw_html"),
        "markdown" => {
            if is_html {
                (html_to_markdown_basic(&raw_content), "html_to_markdown")
            } else {
                (raw_content, "raw")
            }
        }
        "text" => {
            if is_html {
                (html_to_text_basic(&raw_content), "html_to_text")
            } else if is_json {
                let formatted = serde_json::from_str::<Value>(&raw_content)
                    .map(|value| {
                        serde_json::to_string_pretty(&value).unwrap_or(raw_content.clone())
                    })
                    .unwrap_or(raw_content);
                (formatted, "json_to_text")
            } else {
                (raw_content, "raw")
            }
        }
        _ => {
            if is_html {
                (html_to_text_basic(&raw_content), "html_to_text")
            } else {
                (raw_content, "raw")
            }
        }
    };
    let (content, truncated) = truncate_by_chars(&rendered, max_chars);

    Ok(json!({
        "url": url,
        "final_url": final_url,
        "status": status.as_u16(),
        "content_type": content_type,
        "format": format,
        "extractor": extractor,
        "attempts": attempts,
        "content": content,
        "truncated": truncated
    }))
}

fn unwrap_duckduckgo_redirect(raw_url: &str) -> String {
    let normalized = if raw_url.starts_with("//") {
        format!("https:{}", raw_url)
    } else {
        raw_url.to_string()
    };
    if let Ok(parsed) = reqwest::Url::parse(&normalized) {
        if parsed.host_str() == Some("duckduckgo.com") && parsed.path() == "/l/" {
            if let Some((_, value)) = parsed.query_pairs().find(|(key, _)| key == "uddg") {
                return value.into_owned();
            }
        }
    }
    normalized
}

fn push_search_result(
    results: &mut Vec<Value>,
    seen: &mut HashSet<String>,
    title: &str,
    url: &str,
    snippet: &str,
    max_results: usize,
) {
    if results.len() >= max_results {
        return;
    }
    let clean_title = title.trim();
    let clean_url = url.trim();
    if clean_title.is_empty() || clean_url.is_empty() {
        return;
    }
    if !clean_url.starts_with("http://") && !clean_url.starts_with("https://") {
        return;
    }
    if seen.contains(clean_url) {
        return;
    }
    seen.insert(clean_url.to_string());
    results.push(json!({
        "title": clean_title,
        "url": clean_url,
        "snippet": snippet.trim()
    }));
}

fn parse_exa_text_results(raw_text: &str, max_results: usize) -> Vec<Value> {
    let text = raw_text.replace("\r\n", "\n");
    let mut results = Vec::new();
    let mut seen = HashSet::<String>::new();
    let pattern = r"(?ms)Title:\s*(?P<title>.*?)\n(?:Author:.*?\n)?(?:Published Date:.*?\n)?URL:\s*(?P<url>.*?)\nText:\s*(?P<text>.*?)(?=\nTitle:|\z)";
    if let Ok(regex) = Regex::new(pattern) {
        for captures in regex.captures_iter(&text) {
            let title = captures
                .name("title")
                .map(|value| clean_html_fragment(value.as_str()))
                .unwrap_or_default();
            let url = captures
                .name("url")
                .map(|value| value.as_str().trim().to_string())
                .unwrap_or_default();
            let snippet = captures
                .name("text")
                .map(|value| clean_html_fragment(value.as_str()))
                .unwrap_or_default();
            push_search_result(&mut results, &mut seen, &title, &url, &snippet, max_results);
            if results.len() >= max_results {
                return results;
            }
        }
    }
    results
}

fn parse_bing_html_results(html: &str, max_results: usize) -> Vec<Value> {
    let mut results = Vec::new();
    let mut seen = HashSet::<String>::new();
    let Ok(item_re) = Regex::new(r#"(?is)<li[^>]*class="[^"]*\bb_algo\b[^"]*"[^>]*>(.*?)</li>"#)
    else {
        return results;
    };
    let Ok(link_re) = Regex::new(r#"(?is)<h2[^>]*>\s*<a[^>]*href="([^"]+)"[^>]*>(.*?)</a>"#) else {
        return results;
    };
    let Ok(snippet_re) = Regex::new(r#"(?is)<p[^>]*>(.*?)</p>"#) else {
        return results;
    };

    for item in item_re.captures_iter(html) {
        let block = item.get(1).map(|value| value.as_str()).unwrap_or_default();
        let Some(link_caps) = link_re.captures(block) else {
            continue;
        };
        let raw_url = link_caps
            .get(1)
            .map(|value| value.as_str())
            .unwrap_or_default();
        let title_raw = link_caps
            .get(2)
            .map(|value| value.as_str())
            .unwrap_or_default();
        let snippet_raw = snippet_re
            .captures(block)
            .and_then(|caps| caps.get(1))
            .map(|value| value.as_str())
            .unwrap_or_default();
        let url = decode_basic_html_entities(raw_url);
        let title = clean_html_fragment(title_raw);
        let snippet = clean_html_fragment(snippet_raw);
        push_search_result(&mut results, &mut seen, &title, &url, &snippet, max_results);
        if results.len() >= max_results {
            break;
        }
    }

    results
}

fn parse_duckduckgo_html_results(html: &str, max_results: usize) -> Vec<Value> {
    let mut results = Vec::new();
    let mut seen = HashSet::<String>::new();
    let Ok(item_re) =
        Regex::new(r#"(?is)<div[^>]*class="[^"]*\bresult__body\b[^"]*"[^>]*>(.*?)</div>"#)
    else {
        return results;
    };
    let Ok(link_re) = Regex::new(
        r#"(?is)<a[^>]*class="[^"]*\bresult__a\b[^"]*"[^>]*href="([^"]+)"[^>]*>(.*?)</a>"#,
    ) else {
        return results;
    };
    let Ok(snippet_re) =
        Regex::new(r#"(?is)<a[^>]*class="[^"]*\bresult__snippet\b[^"]*"[^>]*>(.*?)</a>"#)
    else {
        return results;
    };

    for item in item_re.captures_iter(html) {
        let block = item.get(1).map(|value| value.as_str()).unwrap_or_default();
        let Some(link_caps) = link_re.captures(block) else {
            continue;
        };
        let raw_url = link_caps
            .get(1)
            .map(|value| value.as_str())
            .unwrap_or_default();
        let title_raw = link_caps
            .get(2)
            .map(|value| value.as_str())
            .unwrap_or_default();
        let snippet_raw = snippet_re
            .captures(block)
            .and_then(|caps| caps.get(1))
            .map(|value| value.as_str())
            .unwrap_or_default();
        let url = unwrap_duckduckgo_redirect(&decode_basic_html_entities(raw_url));
        let title = clean_html_fragment(title_raw);
        let snippet = clean_html_fragment(snippet_raw);
        push_search_result(&mut results, &mut seen, &title, &url, &snippet, max_results);
        if results.len() >= max_results {
            break;
        }
    }

    results
}

fn parse_duckduckgo_instant_results(payload: &Value, max_results: usize) -> Vec<Value> {
    let mut results = Vec::<Value>::new();
    let mut seen = HashSet::<String>::new();

    if let Some(abstract_text) = payload.get("AbstractText").and_then(Value::as_str) {
        let abstract_url = payload
            .get("AbstractURL")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let heading = payload
            .get("Heading")
            .and_then(Value::as_str)
            .unwrap_or("Result");
        push_search_result(
            &mut results,
            &mut seen,
            heading,
            abstract_url,
            abstract_text,
            max_results,
        );
    }

    if let Some(related) = payload.get("RelatedTopics").and_then(Value::as_array) {
        for item in related {
            if let Some(text) = item.get("Text").and_then(Value::as_str) {
                let link = item
                    .get("FirstURL")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                push_search_result(&mut results, &mut seen, text, link, text, max_results);
            } else if let Some(topics) = item.get("Topics").and_then(Value::as_array) {
                for topic in topics {
                    let text = topic
                        .get("Text")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let link = topic
                        .get("FirstURL")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    push_search_result(&mut results, &mut seen, text, link, text, max_results);
                    if results.len() >= max_results {
                        break;
                    }
                }
            }

            if results.len() >= max_results {
                break;
            }
        }
    }

    results
}

async fn run_exa_search(
    query: &str,
    max_results: usize,
    timeout_ms: u64,
    exa_url: &str,
    user_agent: &str,
    accept_language: &str,
) -> Result<Vec<Value>, String> {
    let endpoint = reqwest::Url::parse(exa_url).map_err(|e| format!("Invalid exa_url: {}", e))?;
    if !matches!(endpoint.scheme(), "http" | "https") {
        return Err("exa_url must be http/https".to_string());
    }
    if is_forbidden_loopback_host(&endpoint) {
        return Err("exa_url cannot point to local/private hosts".to_string());
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(timeout_ms))
        .build()
        .map_err(|e| e.to_string())?;

    let response = client
        .post(endpoint)
        .header(
            reqwest::header::ACCEPT,
            "application/json, text/event-stream",
        )
        .header(reqwest::header::USER_AGENT, user_agent)
        .header(reqwest::header::ACCEPT_LANGUAGE, accept_language)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "web_search_exa",
                "arguments": {
                    "query": query,
                    "numResults": max_results as u64,
                    "livecrawl": "fallback",
                    "type": "auto",
                    "contextMaxCharacters": 12000
                }
            }
        }))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !response.status().is_success() {
        let status_code = response.status().as_u16();
        let detail = response
            .text()
            .await
            .unwrap_or_else(|_| "no response detail".to_string());
        return Err(format!(
            "Exa MCP request failed ({}): {}",
            status_code,
            detail.chars().take(240).collect::<String>()
        ));
    }

    let response_text = response.text().await.map_err(|e| e.to_string())?;
    let mut content_chunks = Vec::<String>::new();
    for line in response_text.lines() {
        let Some(payload) = line.strip_prefix("data: ") else {
            continue;
        };
        if payload.trim().is_empty() || payload.trim() == "[DONE]" {
            continue;
        }
        if let Ok(value) = serde_json::from_str::<Value>(payload) {
            if let Some(content_items) = value.pointer("/result/content").and_then(Value::as_array)
            {
                for item in content_items {
                    if item.get("type").and_then(Value::as_str) == Some("text") {
                        if let Some(text) = item.get("text").and_then(Value::as_str) {
                            if !text.trim().is_empty() {
                                content_chunks.push(text.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    let merged = content_chunks.join("\n");
    let mut parsed = parse_exa_text_results(&merged, max_results);
    if parsed.is_empty() && !merged.trim().is_empty() {
        parsed.push(json!({
            "title": format!("Exa search summary: {}", query),
            "url": "https://mcp.exa.ai/",
            "snippet": merged.chars().take(500).collect::<String>()
        }));
    }
    Ok(parsed)
}

pub(super) async fn execute_web_search(arguments: &Value) -> Result<Value, String> {
    let query = read_string_argument(arguments, "query")?;
    let max_results = read_u64_argument(arguments, "max_results", 8).clamp(1, 25) as usize;
    let timeout_ms = read_u64_argument(arguments, "timeout_ms", 18_000).clamp(1_000, 90_000);
    let provider = read_optional_string_argument(arguments, "provider")
        .unwrap_or_else(|| "auto".to_string())
        .to_lowercase();
    let search_lang = read_optional_string_argument(arguments, "search_lang")
        .unwrap_or_else(|| "zh-CN".to_string());
    let ui_lang =
        read_optional_string_argument(arguments, "ui_lang").unwrap_or_else(|| "zh-CN".to_string());
    let country =
        read_optional_string_argument(arguments, "country").unwrap_or_else(|| "CN".to_string());
    let user_agent = read_optional_string_argument(arguments, "user_agent")
        .unwrap_or_else(|| DEFAULT_WEB_USER_AGENT.to_string());
    let accept_language = read_optional_string_argument(arguments, "accept_language")
        .unwrap_or_else(|| DEFAULT_WEB_ACCEPT_LANGUAGE.to_string());
    let exa_url = read_optional_string_argument(arguments, "exa_url")
        .unwrap_or_else(|| DEFAULT_EXA_MCP_ENDPOINT.to_string());

    let providers = match provider.as_str() {
        "auto" => vec!["exa", "bing", "duckduckgo", "duckduckgo_instant"],
        "exa" => vec!["exa"],
        "bing" => vec!["bing"],
        "duckduckgo" => vec!["duckduckgo", "duckduckgo_instant"],
        _ => {
            return Err("provider must be one of: auto|exa|bing|duckduckgo".to_string());
        }
    };

    let mut attempts = Vec::<Value>::new();
    let mut final_results = Vec::<Value>::new();
    let mut provider_used = String::new();

    for provider_name in providers {
        let result = match provider_name {
            "exa" => {
                run_exa_search(
                    &query,
                    max_results,
                    timeout_ms,
                    &exa_url,
                    &user_agent,
                    &accept_language,
                )
                .await
            }
            "bing" => {
                let search_url = reqwest::Url::parse_with_params(
                    "https://www.bing.com/search",
                    &[
                        ("q", query.as_str()),
                        ("setlang", ui_lang.as_str()),
                        ("cc", country.as_str()),
                    ],
                )
                .map_err(|e| e.to_string())?
                .to_string();
                let fetched = fetch_web_raw(
                    &search_url,
                    timeout_ms,
                    4,
                    1,
                    &user_agent,
                    &accept_language,
                    "text/html,*/*;q=0.1",
                )
                .await;
                fetched.map(|(_, _, _, html, _)| parse_bing_html_results(&html, max_results))
            }
            "duckduckgo" => {
                let search_url = reqwest::Url::parse_with_params(
                    "https://duckduckgo.com/html/",
                    &[("q", query.as_str()), ("kl", search_lang.as_str())],
                )
                .map_err(|e| e.to_string())?
                .to_string();
                let fetched = fetch_web_raw(
                    &search_url,
                    timeout_ms,
                    4,
                    1,
                    &user_agent,
                    &accept_language,
                    "text/html,*/*;q=0.1",
                )
                .await;
                fetched.map(|(_, _, _, html, _)| parse_duckduckgo_html_results(&html, max_results))
            }
            "duckduckgo_instant" => {
                let client = reqwest::Client::builder()
                    .timeout(Duration::from_millis(timeout_ms))
                    .build()
                    .map_err(|e| e.to_string())?;
                let payload = client
                    .get("https://api.duckduckgo.com/")
                    .query(&[
                        ("q", query.as_str()),
                        ("format", "json"),
                        ("no_redirect", "1"),
                        ("no_html", "1"),
                        ("skip_disambig", "1"),
                    ])
                    .header(reqwest::header::USER_AGENT, &user_agent)
                    .header(reqwest::header::ACCEPT_LANGUAGE, &accept_language)
                    .send()
                    .await
                    .map_err(|e| e.to_string())?
                    .json::<Value>()
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(parse_duckduckgo_instant_results(&payload, max_results))
            }
            _ => Err("Unknown provider".to_string()),
        };

        match result {
            Ok(results) => {
                attempts.push(json!({
                    "provider": provider_name,
                    "status": "ok",
                    "count": results.len()
                }));
                if !results.is_empty() {
                    provider_used = provider_name.to_string();
                    final_results = results;
                    break;
                }
            }
            Err(error) => {
                attempts.push(json!({
                    "provider": provider_name,
                    "status": "error",
                    "error": error
                }));
            }
        }
    }

    Ok(json!({
        "query": query,
        "provider": if provider_used.is_empty() { Value::Null } else { json!(provider_used) },
        "results": final_results,
        "attempts": attempts
    }))
}

pub(super) async fn execute_browser_navigate(arguments: &Value) -> Result<Value, String> {
    let url = read_string_argument(arguments, "url")?;
    let max_links = read_u64_argument(arguments, "max_links", 30).clamp(1, 200) as usize;
    let (status, content_type, html, truncated) = fetch_web_content(&url, 20_000, 200_000).await?;

    let title_regex = Regex::new(r"(?is)<title[^>]*>(.*?)</title>").map_err(|e| e.to_string())?;
    let link_regex =
        Regex::new(r#"(?is)href\s*=\s*["']([^"'#\s]+)["']"#).map_err(|e| e.to_string())?;

    let title = title_regex
        .captures(&html)
        .and_then(|caps| caps.get(1))
        .map(|value| value.as_str().trim().to_string())
        .unwrap_or_default();

    let base_url = reqwest::Url::parse(&url).map_err(|e| e.to_string())?;
    let mut links = Vec::<String>::new();
    for captures in link_regex.captures_iter(&html) {
        let Some(raw) = captures.get(1).map(|value| value.as_str().trim()) else {
            continue;
        };
        let absolute = base_url
            .join(raw)
            .map(|value| value.to_string())
            .unwrap_or_else(|_| raw.to_string());
        if !links.iter().any(|existing| existing == &absolute) {
            links.push(absolute);
        }
        if links.len() >= max_links {
            break;
        }
    }

    Ok(json!({
        "url": url,
        "status": status.as_u16(),
        "content_type": content_type,
        "title": title,
        "links": links,
        "content_truncated": truncated
    }))
}
