use serde_json::{json, Value};

use crate::models::config::Config;
use crate::services::browser;
use crate::services::browser::types::BrowserToolRequest;

use super::{read_optional_string_argument, read_string_argument, read_u64_argument};

pub(super) async fn execute_browser(arguments: &Value) -> Result<Value, String> {
    let action = read_string_argument(arguments, "action")?;
    let profile = read_optional_string_argument(arguments, "profile");
    let target_id = read_optional_string_argument(arguments, "target_id");
    let mut params = arguments
        .get("params")
        .cloned()
        .unwrap_or_else(|| json!({}));

    if params.is_string() {
        if let Some(s) = params.as_str() {
            if let Ok(parsed) = serde_json::from_str::<Value>(s) {
                if parsed.is_object() {
                    params = parsed;
                }
            }
        }
    }

    if !params.is_object() {
        return Err(format!("'params' must be an object, got: {}", params));
    }

    let config = crate::utils::load_config::<Config>().map_err(|e| e.to_string())?;
    if action == "evaluate" && !config.browser.evaluate_enabled {
        return Ok(json!({
            "ok": false,
            "data": Value::Null,
            "error": "evaluate is disabled by policy (browser.evaluate_enabled=false)",
            "meta": {
                "status": 403
            }
        }));
    }

    let request = BrowserToolRequest {
        action,
        profile,
        target_id,
        params,
    };
    browser::execute_browser_request(&request, &config.browser).await
}

pub(super) async fn execute_browser_navigate_compat(arguments: &Value) -> Result<Value, String> {
    let url = read_string_argument(arguments, "url")?;
    let max_links = read_u64_argument(arguments, "max_links", 30).clamp(1, 200);
    let profile = read_optional_string_argument(arguments, "profile");
    let target_id = read_optional_string_argument(arguments, "target_id");

    let config = crate::utils::load_config::<Config>().map_err(|e| e.to_string())?;
    let request = BrowserToolRequest {
        action: "navigate".to_string(),
        profile,
        target_id,
        params: json!({
            "url": url,
            "max_links": max_links,
            "include_links": true
        }),
    };

    let envelope = browser::execute_browser_request(&request, &config.browser).await?;
    let ok = envelope.get("ok").and_then(Value::as_bool).unwrap_or(false);
    if !ok {
        let message = envelope
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("Browser navigation failed");
        return Err(message.to_string());
    }
    let data = envelope.get("data").cloned().unwrap_or_else(|| json!({}));

    Ok(json!({
        "url": data.get("url").cloned().unwrap_or_else(|| json!(url)),
        "status": data.get("status").cloned().unwrap_or_else(|| json!(200)),
        "content_type": data.get("content_type").cloned().unwrap_or_else(|| json!("text/html")),
        "title": data.get("title").cloned().unwrap_or_else(|| json!("")),
        "links": data.get("links").cloned().unwrap_or_else(|| json!([])),
        "content_truncated": data.get("content_truncated").cloned().unwrap_or_else(|| json!(false)),
        "target_id": data.get("target_id").cloned().unwrap_or(Value::Null)
    }))
}
