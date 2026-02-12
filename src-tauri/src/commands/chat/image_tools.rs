use std::fs;
use std::io::Read;
use std::path::Path;
use std::time::Duration;

use serde_json::{json, Value};

use super::{
    is_forbidden_loopback_host, read_optional_string_argument, read_u64_argument,
    resolve_workspace_target, workspace_relative_display_path,
};

fn parse_jpeg_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 4 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return None;
    }
    let mut index = 2usize;
    while index + 9 < bytes.len() {
        if bytes[index] != 0xFF {
            index += 1;
            continue;
        }
        let marker = bytes[index + 1];
        index += 2;
        if marker == 0xD9 || marker == 0xDA {
            break;
        }
        if index + 1 >= bytes.len() {
            break;
        }
        let segment_len = u16::from_be_bytes([bytes[index], bytes[index + 1]]) as usize;
        if segment_len < 2 || index + segment_len > bytes.len() {
            break;
        }
        if (0xC0..=0xC3).contains(&marker)
            || (0xC5..=0xC7).contains(&marker)
            || (0xC9..=0xCB).contains(&marker)
            || (0xCD..=0xCF).contains(&marker)
        {
            if index + 7 < bytes.len() {
                let height = u16::from_be_bytes([bytes[index + 3], bytes[index + 4]]) as u32;
                let width = u16::from_be_bytes([bytes[index + 5], bytes[index + 6]]) as u32;
                return Some((width, height));
            }
            break;
        }
        index += segment_len;
    }
    None
}

fn detect_image_metadata(bytes: &[u8]) -> (String, Option<u32>, Option<u32>) {
    if bytes.len() >= 24 && bytes.starts_with(&[137, 80, 78, 71, 13, 10, 26, 10]) {
        let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
        let height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
        return ("png".to_string(), Some(width), Some(height));
    }
    if bytes.len() >= 10 && (bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a")) {
        let width = u16::from_le_bytes([bytes[6], bytes[7]]) as u32;
        let height = u16::from_le_bytes([bytes[8], bytes[9]]) as u32;
        return ("gif".to_string(), Some(width), Some(height));
    }
    if bytes.len() >= 26 && bytes[0] == b'B' && bytes[1] == b'M' {
        let width = i32::from_le_bytes([bytes[18], bytes[19], bytes[20], bytes[21]]).unsigned_abs();
        let height =
            i32::from_le_bytes([bytes[22], bytes[23], bytes[24], bytes[25]]).unsigned_abs();
        return ("bmp".to_string(), Some(width), Some(height));
    }
    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        return ("webp".to_string(), None, None);
    }
    if bytes.len() >= 8 && bytes.starts_with(&[0, 0, 1, 0]) {
        let width = if bytes[6] == 0 { 256 } else { bytes[6] as u32 };
        let height = if bytes[7] == 0 { 256 } else { bytes[7] as u32 };
        return ("ico".to_string(), Some(width), Some(height));
    }
    if let Some((width, height)) = parse_jpeg_dimensions(bytes) {
        return ("jpeg".to_string(), Some(width), Some(height));
    }
    ("unknown".to_string(), None, None)
}

pub(super) async fn execute_image_probe(
    arguments: &Value,
    workspace_root: &Path,
) -> Result<Value, String> {
    let max_bytes =
        read_u64_argument(arguments, "max_bytes", 512 * 1024).clamp(1_024, 4_194_304) as usize;

    let source = if let Some(path) = read_optional_string_argument(arguments, "path") {
        let resolved = resolve_workspace_target(workspace_root, &path, false)?;
        let mut file = fs::File::open(&resolved).map_err(|e| e.to_string())?;
        let mut bytes = vec![0u8; max_bytes];
        let size = file.read(&mut bytes).map_err(|e| e.to_string())?;
        bytes.truncate(size);
        json!({
            "kind": "path",
            "value": workspace_relative_display_path(workspace_root, &resolved),
            "bytes": bytes
        })
    } else if let Some(url) = read_optional_string_argument(arguments, "url") {
        let parsed = reqwest::Url::parse(&url).map_err(|e| format!("Invalid URL: {}", e))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err("Only http/https URLs are supported".to_string());
        }
        if is_forbidden_loopback_host(&parsed) {
            return Err("Local/private hosts are not allowed".to_string());
        }

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .build()
            .map_err(|e| e.to_string())?;
        let response = client
            .get(parsed.clone())
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let status = response.status();
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let mut bytes = response.bytes().await.map_err(|e| e.to_string())?.to_vec();
        if bytes.len() > max_bytes {
            bytes.truncate(max_bytes);
        }
        json!({
            "kind": "url",
            "value": parsed.to_string(),
            "status": status.as_u16(),
            "content_type": content_type,
            "bytes": bytes
        })
    } else {
        return Err("Either 'path' or 'url' is required".to_string());
    };

    let bytes = source
        .get("bytes")
        .and_then(Value::as_array)
        .ok_or_else(|| "Failed to load image bytes".to_string())?
        .iter()
        .filter_map(Value::as_u64)
        .map(|value| value as u8)
        .collect::<Vec<_>>();
    let (format, width, height) = detect_image_metadata(&bytes);

    let mut result = json!({
        "source": source.get("value").cloned().unwrap_or(Value::Null),
        "source_kind": source.get("kind").cloned().unwrap_or(Value::Null),
        "byte_length": bytes.len(),
        "format": format,
        "width": width,
        "height": height
    });
    if let Some(status) = source.get("status") {
        result["status"] = status.clone();
    }
    if let Some(content_type) = source.get("content_type") {
        result["content_type"] = content_type.clone();
    }
    Ok(result)
}
