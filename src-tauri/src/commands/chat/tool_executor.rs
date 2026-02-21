use crate::services::desktop::{self, types::DesktopToolRequest};
use crate::commands::mcp::McpState;
use tauri;
use crate::commands::skills::SkillManagerState;
use super::{browser_tools, image_tools, process_tools, web_tools};
use crate::services::llm::{ChatMessage, ChatToolCall};
use crate::commands::chat::{TodoItem, TodoStatus};
use crate::models::config::Config;
use crate::services::llm::LlmService;
use crate::services::pdf_parse::{parse_pdf_to_markdown as parse_pdf_to_markdown_service, ParsePdfOptions};
use crate::services::scheduler::scheduler_manager;
use crate::services::scheduler::models::*;
use crate::commands::chat::todo_store;

use chrono::Utc;
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use regex::{Regex, RegexBuilder};
use serde_json::{json, Value};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::Command as TokioCommand;
use uuid::Uuid;
use walkdir::WalkDir;

// We need an interface or similar to bring in all the tools, but let's first import
// what we need from chat::tool_catalog and chat::llm_provider
// Assuming these are accessible over `super` or `crate::commands::chat`

use super::{
    insert_message, load_conversation_context,
    resolve_clawhub_settings_for_discovery,
    should_auto_allow_batch_tool,
    tool_catalog::*,
};

pub(crate) fn is_probably_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8192).any(|byte| *byte == 0)
}

pub(crate) fn workspace_relative_display_path(workspace_root: &Path, path: &Path) -> String {
    path.strip_prefix(workspace_root)
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|_| path.to_string_lossy().to_string())
}

pub(crate) fn normalize_lexical_path(path: &Path) -> PathBuf {
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::Prefix(prefix) => components.push(std::path::Component::Prefix(prefix)),
            std::path::Component::RootDir => components.push(std::path::Component::RootDir),
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                components.pop();
            }
            std::path::Component::Normal(component) => components.push(std::path::Component::Normal(component)),
        }
    }
    components.iter().collect()
}

pub(crate) fn resolve_workspace_target(
    workspace_root: &Path,
    raw_path: &str,
    allow_missing: bool,
) -> Result<PathBuf, String> {
    let canonical_root = workspace_root.canonicalize().map_err(|e| e.to_string())?;
    let requested_path = {
        let candidate = PathBuf::from(raw_path);
        if candidate.is_absolute() {
            candidate
        } else {
            canonical_root.join(candidate)
        }
    };

    if requested_path.exists() {
        let canonical = requested_path.canonicalize().map_err(|e| e.to_string())?;
        if !canonical.starts_with(&canonical_root) {
            return Err("Path is outside workspace root".to_string());
        }
        return Ok(canonical);
    }

    if !allow_missing {
        return Err(format!("Path does not exist: {}", requested_path.display()));
    }

    let normalized = normalize_lexical_path(&requested_path);
    if !normalized.starts_with(&canonical_root) {
        return Err("Path is outside workspace root".to_string());
    }

    Ok(normalized)
}

pub(crate) fn compile_glob_set(patterns: &[String]) -> Result<GlobSet, String> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let glob = GlobBuilder::new(pattern)
            .literal_separator(false)
            .build()
            .map_err(|e| format!("Invalid glob '{}': {}", pattern, e))?;
        builder.add(glob);
    }
    builder
        .build()
        .map_err(|e| format!("Failed to build glob matcher: {}", e))
}

pub(crate) fn wildcard_match(pattern: &str, value: &str) -> bool {
    let mut regex_pattern = String::from("^");
    for ch in pattern.chars() {
        match ch {
            '*' => regex_pattern.push_str(".*"),
            '?' => regex_pattern.push('.'),
            _ => regex_pattern.push_str(&regex::escape(&ch.to_string())),
        }
    }
    regex_pattern.push('$');
    Regex::new(&regex_pattern)
        .map(|regex| regex.is_match(value))
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Argument Extractors
// ---------------------------------------------------------------------------

pub(crate) fn read_string_argument(arguments: &Value, key: &str) -> Result<String, String> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("'{}' is required and cannot be empty", key))
        .map(str::to_string)
}

pub(crate) fn read_path_argument(arguments: &Value, key: &str) -> Result<String, String> {
    read_string_argument(arguments, key)
}

pub(crate) fn read_optional_string_argument(arguments: &Value, key: &str) -> Option<String> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(crate) fn read_bool_argument(arguments: &Value, key: &str, default_value: bool) -> bool {
    arguments
        .get(key)
        .and_then(Value::as_bool)
        .unwrap_or(default_value)
}

pub(crate) fn read_u64_argument(arguments: &Value, key: &str, default_value: u64) -> u64 {
    arguments
        .get(key)
        .and_then(Value::as_u64)
        .unwrap_or(default_value)
}

// ---------------------------------------------------------------------------
// Tool Implementations
// ---------------------------------------------------------------------------

pub(crate) fn execute_workspace_list_directory(
    arguments: &Value,
    workspace_root: &Path,
) -> Result<Value, String> {
    let raw_path = arguments
        .get("path")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(".");

    let directory = resolve_workspace_target(workspace_root, raw_path, false)?;
    if !directory.is_dir() {
        return Err(format!("Not a directory: {}", directory.display()));
    }

    let mut items = Vec::new();
    let entries = std::fs::read_dir(&directory).map_err(|e| e.to_string())?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let metadata = entry.metadata().map_err(|e| e.to_string())?;
        items.push(json!({
            "name": entry.file_name().to_string_lossy().to_string(),
            "path": entry.path().to_string_lossy().to_string(),
            "is_dir": metadata.is_dir(),
            "size": if metadata.is_file() { Some(metadata.len()) } else { None::<u64> }
        }));
    }

    items.sort_by(|left, right| {
        let left_is_dir = left.get("is_dir").and_then(Value::as_bool).unwrap_or(false);
        let right_is_dir = right
            .get("is_dir")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        match (left_is_dir, right_is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => {
                let left_name = left.get("name").and_then(Value::as_str).unwrap_or_default();
                let right_name = right
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                left_name.cmp(right_name)
            }
        }
    });

    Ok(json!({
        "workspace_root": workspace_root.to_string_lossy().to_string(),
        "directory": directory.to_string_lossy().to_string(),
        "entries": items
    }))
}

pub(crate) fn execute_workspace_read_file(
    arguments: &Value,
    workspace_root: &Path,
) -> Result<Value, String> {
    let raw_path = read_path_argument(arguments, "path")?;
    let file_path = resolve_workspace_target(workspace_root, &raw_path, false)?;
    if !file_path.is_file() {
        return Err(format!("Not a file: {}", file_path.display()));
    }

    let max_bytes = read_u64_argument(arguments, "max_bytes", 200_000).clamp(1, 2_000_000) as usize;
    let offset_bytes = read_u64_argument(arguments, "offset_bytes", 0) as usize;
    let max_lines = arguments
        .get("max_lines")
        .and_then(Value::as_u64)
        .map(|value| value.clamp(1, 200_000) as usize);

    let bytes = fs::read(&file_path).map_err(|e| e.to_string())?;
    let total_bytes = bytes.len();
    let start = offset_bytes.min(total_bytes);
    let end = (start + max_bytes).min(total_bytes);
    let mut content = String::from_utf8_lossy(&bytes[start..end]).to_string();
    let mut lines_truncated = false;
    if let Some(max_lines) = max_lines {
        let lines: Vec<&str> = content.lines().collect();
        if lines.len() > max_lines {
            content = lines[..max_lines].join("\n");
            lines_truncated = true;
        }
    }

    Ok(json!({
        "workspace_root": workspace_root.to_string_lossy().to_string(),
        "path": file_path.to_string_lossy().to_string(),
        "content": content,
        "offset_bytes": start,
        "bytes_read": end.saturating_sub(start),
        "total_bytes": total_bytes,
        "truncated": end < total_bytes || lines_truncated
    }))
}

// ... more methods to be copied ...


pub(crate) fn execute_workspace_parse_pdf_markdown(
    arguments: &Value,
    workspace_root: &Path,
) -> Result<Value, String> {
    let raw_path = read_path_argument(arguments, "path")?;
    let pdf_path = resolve_workspace_target(workspace_root, &raw_path, false)?;
    if !pdf_path.is_file() {
        return Err(format!("Not a file: {}", pdf_path.display()));
    }

    let export_images = read_bool_argument(arguments, "export_images", true);
    let max_pages_value = read_u64_argument(arguments, "max_pages", 0);
    let max_pages = if max_pages_value == 0 {
        None
    } else {
        Some(max_pages_value.min(5_000) as usize)
    };

    let parsed = parse_pdf_to_markdown_service(
        &pdf_path,
        ParsePdfOptions {
            export_images,
            max_pages,
        },
    )?;

    let image_paths: Vec<String> = parsed
        .image_paths
        .iter()
        .map(|path| workspace_relative_display_path(workspace_root, path))
        .collect();

    Ok(json!({
        "workspace_root": workspace_root.to_string_lossy().to_string(),
        "path": workspace_relative_display_path(workspace_root, &pdf_path),
        "page_count": parsed.page_count,
        "markdown": parsed.markdown,
        "image_paths": image_paths,
        "truncated": parsed.truncated
    }))
}

pub(crate) fn write_file_atomic(path: &Path, content: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "Invalid target path".to_string())?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let temp_name = format!(
        ".{}.tmp-{}",
        path.file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("petool"),
        Uuid::new_v4()
    );
    let temp_path = parent.join(temp_name);
    fs::write(&temp_path, content).map_err(|e| e.to_string())?;
    match fs::rename(&temp_path, path) {
        Ok(_) => Ok(()),
        Err(rename_error) => {
            fs::copy(&temp_path, path).map_err(|copy_error| {
                format!(
                    "Failed to atomically write file (rename: {}; copy: {})",
                    rename_error, copy_error
                )
            })?;
            let _ = fs::remove_file(&temp_path);
            Ok(())
        }
    }
}

pub(crate) fn execute_workspace_write_file(
    arguments: &Value,
    workspace_root: &Path,
) -> Result<Value, String> {
    let raw_path = read_path_argument(arguments, "path")?;
    let content = arguments
        .get("content")
        .and_then(Value::as_str)
        .ok_or_else(|| "'content' is required".to_string())?
        .to_string();
    let append = arguments
        .get("append")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let file_path = resolve_workspace_target(workspace_root, &raw_path, true)?;

    if append {
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file_path)
            .map_err(|e| e.to_string())?;
        file.write_all(content.as_bytes())
            .map_err(|e| e.to_string())?;
    } else {
        write_file_atomic(&file_path, content.as_bytes())?;
    }

    let metadata = fs::metadata(&file_path).map_err(|e| e.to_string())?;
    Ok(json!({
        "workspace_root": workspace_root.to_string_lossy().to_string(),
        "path": file_path.to_string_lossy().to_string(),
        "bytes_written": metadata.len(),
        "append": append
    }))
}

pub(crate) fn execute_workspace_edit_file(
    arguments: &Value,
    workspace_root: &Path,
) -> Result<Value, String> {
    let raw_path = read_path_argument(arguments, "path")?;
    let old_string = read_string_argument(arguments, "old_string")?;
    let new_string = arguments
        .get("new_string")
        .and_then(Value::as_str)
        .ok_or_else(|| "'new_string' is required".to_string())?
        .to_string();
    let replace_all = read_bool_argument(arguments, "replace_all", false);

    let file_path = resolve_workspace_target(workspace_root, &raw_path, false)?;
    if !file_path.is_file() {
        return Err(format!("Not a file: {}", file_path.display()));
    }

    let content = fs::read_to_string(&file_path).map_err(|e| e.to_string())?;
    let updated = if replace_all {
        content.replace(&old_string, &new_string)
    } else {
        content.replacen(&old_string, &new_string, 1)
    };

    if updated == content {
        return Err("No matching content found to edit".to_string());
    }

    write_file_atomic(&file_path, updated.as_bytes())?;

    Ok(json!({
        "workspace_root": workspace_root.to_string_lossy().to_string(),
        "path": file_path.to_string_lossy().to_string(),
        "replace_all": replace_all,
        "status": "updated"
    }))
}

pub(crate) async fn execute_workspace_run_command(
    arguments: &Value,
    workspace_root: &Path,
) -> Result<Value, String> {
    let command = arguments
        .get("command")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "'command' is required".to_string())?
        .to_string();

    let timeout_ms = read_u64_argument(arguments, "timeout_ms", 20_000).clamp(1_000, 120_000);

    let mut cmd = if cfg!(target_os = "windows") {
        let mut process = TokioCommand::new("powershell");
        process.args(["-NoProfile", "-Command", &command]);
        process
    } else {
        let mut process = TokioCommand::new("sh");
        process.args(["-lc", &command]);
        process
    };

    cmd.current_dir(workspace_root);
    let output = tokio::time::timeout(Duration::from_millis(timeout_ms), cmd.output())
        .await
        .map_err(|_| format!("Command timed out after {} ms", timeout_ms))?
        .map_err(|e| e.to_string())?;

    Ok(json!({
        "workspace_root": workspace_root.to_string_lossy().to_string(),
        "command": command,
        "timeout_ms": timeout_ms,
        "exit_code": output.status.code(),
        "success": output.status.success(),
        "stdout": String::from_utf8_lossy(&output.stdout).to_string(),
        "stderr": String::from_utf8_lossy(&output.stderr).to_string()
    }))
}
pub(crate) fn collect_workspace_files(
    workspace_root: &Path,
    base_directory: &Path,
    glob_pattern: Option<&str>,
    max_files: usize,
) -> Result<Vec<PathBuf>, String> {
    let patterns = if let Some(pattern) = glob_pattern {
        vec![pattern.to_string()]
    } else {
        vec!["**/*".to_string()]
    };
    let glob_set = compile_glob_set(&patterns)?;
    let mut results = Vec::new();

    for entry in WalkDir::new(base_directory).follow_links(false) {
        let entry = entry.map_err(|e| e.to_string())?;
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path().to_path_buf();
        let relative_to_root = workspace_relative_display_path(workspace_root, &path);
        if glob_set.is_match(relative_to_root.as_str()) {
            results.push(path);
            if results.len() >= max_files {
                break;
            }
        }
    }

    Ok(results)
}

pub(crate) fn execute_workspace_glob(
    arguments: &Value,
    workspace_root: &Path,
) -> Result<Value, String> {
    let pattern = read_string_argument(arguments, "pattern")?;
    let raw_path =
        read_optional_string_argument(arguments, "path").unwrap_or_else(|| ".".to_string());
    let max_results = read_u64_argument(arguments, "max_results", 200).clamp(1, 5_000) as usize;
    let include_directories = read_bool_argument(arguments, "include_directories", false);
    let base_dir = resolve_workspace_target(workspace_root, &raw_path, false)?;
    if !base_dir.is_dir() {
        return Err(format!("Not a directory: {}", base_dir.display()));
    }

    let glob_set = compile_glob_set(&[pattern])?;
    let mut matches = Vec::new();
    for entry in WalkDir::new(&base_dir).follow_links(false) {
        let entry = entry.map_err(|e| e.to_string())?;
        if !include_directories && !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path().to_path_buf();
        let relative = workspace_relative_display_path(workspace_root, &path);
        if glob_set.is_match(relative.as_str()) {
            let metadata = fs::metadata(&path).map_err(|e| e.to_string())?;
            matches.push(json!({
                "path": relative,
                "absolute_path": path.to_string_lossy().to_string(),
                "is_dir": metadata.is_dir(),
                "size": if metadata.is_file() { Some(metadata.len()) } else { None::<u64> }
            }));
            if matches.len() >= max_results {
                break;
            }
        }
    }

    Ok(json!({
        "workspace_root": workspace_root.to_string_lossy().to_string(),
        "base_directory": base_dir.to_string_lossy().to_string(),
        "matches": matches,
        "truncated": matches.len() >= max_results
    }))
}

pub(crate) fn build_search_regex(
    pattern: &str,
    use_regex: bool,
    case_sensitive: bool,
) -> Result<Regex, String> {
    let source = if use_regex {
        pattern.to_string()
    } else {
        regex::escape(pattern)
    };
    RegexBuilder::new(&source)
        .case_insensitive(!case_sensitive)
        .build()
        .map_err(|e| format!("Invalid search pattern: {}", e))
}

pub(crate) fn execute_workspace_grep(
    arguments: &Value,
    workspace_root: &Path,
) -> Result<Value, String> {
    let pattern = read_string_argument(arguments, "pattern")?;
    let use_regex = read_bool_argument(arguments, "regex", false);
    let case_sensitive = read_bool_argument(arguments, "case_sensitive", false);
    let max_results = read_u64_argument(arguments, "max_results", 200).clamp(1, 5_000) as usize;
    let raw_path =
        read_optional_string_argument(arguments, "path").unwrap_or_else(|| ".".to_string());
    let glob = read_optional_string_argument(arguments, "glob");
    let file_max_bytes = read_u64_argument(arguments, "file_max_bytes", 2_000_000) as usize;
    let regex = build_search_regex(&pattern, use_regex, case_sensitive)?;
    let base_dir = resolve_workspace_target(workspace_root, &raw_path, false)?;
    if !base_dir.is_dir() {
        return Err(format!("Not a directory: {}", base_dir.display()));
    }

    let files = collect_workspace_files(workspace_root, &base_dir, glob.as_deref(), 20_000)?;
    let mut matches = Vec::new();

    for file_path in files {
        let bytes = fs::read(&file_path).map_err(|e| e.to_string())?;
        if bytes.is_empty() || is_probably_binary(&bytes) {
            continue;
        }
        if bytes.len() > file_max_bytes {
            continue;
        }

        let text = String::from_utf8_lossy(&bytes);
        for (index, line) in text.lines().enumerate() {
            if regex.is_match(line) {
                matches.push(json!({
                    "path": workspace_relative_display_path(workspace_root, &file_path),
                    "line_number": index + 1,
                    "line": line
                }));
                if matches.len() >= max_results {
                    break;
                }
            }
        }

        if matches.len() >= max_results {
            break;
        }
    }

    Ok(json!({
        "pattern": pattern,
        "regex": use_regex,
        "case_sensitive": case_sensitive,
        "matches": matches,
        "truncated": matches.len() >= max_results
    }))
}

pub(crate) fn execute_workspace_codesearch(
    arguments: &Value,
    workspace_root: &Path,
) -> Result<Value, String> {
    let query = read_string_argument(arguments, "query")?;
    let raw_path =
        read_optional_string_argument(arguments, "path").unwrap_or_else(|| ".".to_string());
    let glob = read_optional_string_argument(arguments, "glob");
    let context_lines = read_u64_argument(arguments, "context_lines", 2).clamp(0, 20) as usize;
    let max_results = read_u64_argument(arguments, "max_results", 100).clamp(1, 2_000) as usize;
    let regex = build_search_regex(&query, false, false)?;
    let base_dir = resolve_workspace_target(workspace_root, &raw_path, false)?;
    if !base_dir.is_dir() {
        return Err(format!("Not a directory: {}", base_dir.display()));
    }

    let files = collect_workspace_files(workspace_root, &base_dir, glob.as_deref(), 20_000)?;
    let mut matches = Vec::new();

    for file_path in files {
        let bytes = fs::read(&file_path).map_err(|e| e.to_string())?;
        if bytes.is_empty() || is_probably_binary(&bytes) {
            continue;
        }

        let text = String::from_utf8_lossy(&bytes);
        let lines: Vec<&str> = text.lines().collect();
        for index in 0..lines.len() {
            if regex.is_match(lines[index]) {
                let start = index.saturating_sub(context_lines);
                let end = (index + context_lines + 1).min(lines.len());
                let snippet = lines[start..end]
                    .iter()
                    .enumerate()
                    .map(|(offset, line)| format!("{}: {}", start + offset + 1, line))
                    .collect::<Vec<_>>()
                    .join("\n");
                matches.push(json!({
                    "path": workspace_relative_display_path(workspace_root, &file_path),
                    "line_number": index + 1,
                    "snippet": snippet
                }));
                if matches.len() >= max_results {
                    break;
                }
            }
        }
        if matches.len() >= max_results {
            break;
        }
    }

    Ok(json!({
        "query": query,
        "results": matches,
        "truncated": matches.len() >= max_results
    }))
}

pub(crate) fn execute_workspace_lsp_symbols(
    arguments: &Value,
    workspace_root: &Path,
) -> Result<Value, String> {
    let raw_path =
        read_optional_string_argument(arguments, "path").unwrap_or_else(|| ".".to_string());
    let query = read_optional_string_argument(arguments, "query")
        .map(|value| value.to_lowercase())
        .unwrap_or_default();
    let max_results = read_u64_argument(arguments, "max_results", 200).clamp(1, 2_000) as usize;
    let base_dir = resolve_workspace_target(workspace_root, &raw_path, false)?;
    if !base_dir.is_dir() {
        return Err(format!("Not a directory: {}", base_dir.display()));
    }

    let symbol_patterns = vec![
        (
            "function",
            Regex::new(r"^\s*(?:pub\s+)?(?:async\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)")
                .map_err(|e| e.to_string())?,
        ),
        (
            "class",
            Regex::new(r"^\s*class\s+([A-Za-z_][A-Za-z0-9_]*)").map_err(|e| e.to_string())?,
        ),
        (
            "type",
            Regex::new(
                r"^\s*(?:export\s+)?(?:interface|type|struct|enum)\s+([A-Za-z_][A-Za-z0-9_]*)",
            )
            .map_err(|e| e.to_string())?,
        ),
    ];

    let files = collect_workspace_files(workspace_root, &base_dir, None, 40_000)?;
    let mut symbols = Vec::new();

    for file_path in files {
        let relative = workspace_relative_display_path(workspace_root, &file_path);
        let extension = file_path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_lowercase();
        if !matches!(
            extension.as_str(),
            "rs" | "ts"
                | "tsx"
                | "js"
                | "jsx"
                | "py"
                | "go"
                | "java"
                | "c"
                | "cpp"
                | "h"
                | "hpp"
                | "vue"
        ) {
            continue;
        }

        let bytes = fs::read(&file_path).map_err(|e| e.to_string())?;
        if bytes.is_empty() || is_probably_binary(&bytes) {
            continue;
        }

        let text = String::from_utf8_lossy(&bytes);
        for (line_idx, line) in text.lines().enumerate() {
            for (kind, regex) in &symbol_patterns {
                if let Some(captures) = regex.captures(line) {
                    let symbol_name = captures
                        .get(1)
                        .map(|value| value.as_str())
                        .unwrap_or_default()
                        .to_string();
                    if !query.is_empty() && !symbol_name.to_lowercase().contains(&query) {
                        continue;
                    }
                    symbols.push(json!({
                        "name": symbol_name,
                        "kind": kind,
                        "path": relative,
                        "line_number": line_idx + 1
                    }));
                    if symbols.len() >= max_results {
                        break;
                    }
                }
            }
            if symbols.len() >= max_results {
                break;
            }
        }
        if symbols.len() >= max_results {
            break;
        }
    }

    Ok(json!({
        "query": query,
        "symbols": symbols,
        "truncated": symbols.len() >= max_results
    }))
}
pub(crate) enum PatchOperation {
    Add {
        path: String,
        lines: Vec<String>,
    },
    Delete {
        path: String,
    },
    Update {
        path: String,
        move_to: Option<String>,
        hunks: Vec<Vec<String>>,
    },
}

pub(crate) fn parse_patch_envelope(patch: &str) -> Result<Vec<PatchOperation>, String> {
    let lines: Vec<&str> = patch.lines().collect();
    if lines.is_empty() || lines.first().copied() != Some("*** Begin Patch") {
        return Err("Patch must start with '*** Begin Patch'".to_string());
    }

    let mut index = 1usize;
    let mut operations = Vec::new();

    while index < lines.len() {
        let line = lines[index];
        if line == "*** End Patch" {
            return Ok(operations);
        }

        if let Some(path) = line.strip_prefix("*** Add File: ") {
            index += 1;
            let mut add_lines = Vec::new();
            while index < lines.len() && !lines[index].starts_with("*** ") {
                if !lines[index].starts_with('+') {
                    return Err(format!(
                        "Add file operation expects '+' lines, found: {}",
                        lines[index]
                    ));
                }
                add_lines.push(lines[index][1..].to_string());
                index += 1;
            }
            operations.push(PatchOperation::Add {
                path: path.trim().to_string(),
                lines: add_lines,
            });
            continue;
        }

        if let Some(path) = line.strip_prefix("*** Delete File: ") {
            operations.push(PatchOperation::Delete {
                path: path.trim().to_string(),
            });
            index += 1;
            continue;
        }

        if let Some(path) = line.strip_prefix("*** Update File: ") {
            index += 1;
            let mut move_to = None;
            if index < lines.len() {
                if let Some(target) = lines[index].strip_prefix("*** Move to: ") {
                    move_to = Some(target.trim().to_string());
                    index += 1;
                }
            }

            let mut hunks = Vec::new();
            while index < lines.len() && !lines[index].starts_with("*** ") {
                if !lines[index].starts_with("@@") {
                    return Err(format!("Expected hunk header '@@', got: {}", lines[index]));
                }
                index += 1;
                let mut hunk_lines = Vec::new();
                while index < lines.len()
                    && !lines[index].starts_with("@@")
                    && !lines[index].starts_with("*** ")
                {
                    if lines[index] == "*** End of File" {
                        index += 1;
                        break;
                    }
                    let prefix = lines[index].chars().next().unwrap_or(' ');
                    if prefix != ' ' && prefix != '+' && prefix != '-' {
                        return Err(format!("Invalid hunk line prefix: {}", lines[index]));
                    }
                    hunk_lines.push(lines[index].to_string());
                    index += 1;
                }
                hunks.push(hunk_lines);
            }

            operations.push(PatchOperation::Update {
                path: path.trim().to_string(),
                move_to,
                hunks,
            });
            continue;
        }

        return Err(format!("Unknown patch operation: {}", line));
    }

    Err("Patch is missing '*** End Patch'".to_string())
}

pub(crate) fn find_subsequence(
    haystack: &[String],
    needle: &[String],
    start_index: usize,
) -> Option<usize> {
    if needle.is_empty() {
        return Some(start_index.min(haystack.len()));
    }
    if haystack.len() < needle.len() {
        return None;
    }
    (start_index..=haystack.len().saturating_sub(needle.len()))
        .find(|index| haystack[*index..*index + needle.len()] == *needle)
}

pub(crate) fn apply_hunks_to_content(
    content: &str,
    hunks: &[Vec<String>],
) -> Result<String, String> {
    let had_trailing_newline = content.ends_with('\n');
    let mut lines: Vec<String> = content.lines().map(|line| line.to_string()).collect();
    let mut cursor = 0usize;

    for hunk in hunks {
        let old_lines: Vec<String> = hunk
            .iter()
            .filter_map(|line| match line.chars().next().unwrap_or(' ') {
                ' ' | '-' => Some(line[1..].to_string()),
                _ => None,
            })
            .collect();
        let new_lines: Vec<String> = hunk
            .iter()
            .filter_map(|line| match line.chars().next().unwrap_or(' ') {
                ' ' | '+' => Some(line[1..].to_string()),
                _ => None,
            })
            .collect();

        let Some(position) = find_subsequence(&lines, &old_lines, cursor)
            .or_else(|| find_subsequence(&lines, &old_lines, 0))
        else {
            return Err("Failed to locate hunk context in file".to_string());
        };

        let replace_end = position + old_lines.len();
        lines.splice(position..replace_end, new_lines.clone());
        cursor = position + new_lines.len();
    }

    let mut result = lines.join("\n");
    if had_trailing_newline {
        result.push('\n');
    }
    Ok(result)
}

pub(crate) fn execute_workspace_apply_patch(
    arguments: &Value,
    workspace_root: &Path,
) -> Result<Value, String> {
    let patch_text = read_string_argument(arguments, "patch")?;
    let dry_run = read_bool_argument(arguments, "dry_run", false);
    let operations = parse_patch_envelope(&patch_text)?;
    let mut applied = Vec::new();

    for operation in operations {
        match operation {
            PatchOperation::Add { path, lines } => {
                let target_path = resolve_workspace_target(workspace_root, &path, true)?;
                if target_path.exists() {
                    return Err(format!("Cannot add file that already exists: {}", path));
                }
                let content = if lines.is_empty() {
                    String::new()
                } else {
                    format!("{}\n", lines.join("\n"))
                };
                if !dry_run {
                    if let Some(parent) = target_path.parent() {
                        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                    }
                    write_file_atomic(&target_path, content.as_bytes())?;
                }
                applied.push(json!({
                    "operation": "add",
                    "path": workspace_relative_display_path(workspace_root, &target_path)
                }));
            }
            PatchOperation::Delete { path } => {
                let target_path = resolve_workspace_target(workspace_root, &path, false)?;
                if !target_path.exists() {
                    return Err(format!("Cannot delete missing file: {}", path));
                }
                if !dry_run {
                    fs::remove_file(&target_path).map_err(|e| e.to_string())?;
                }
                applied.push(json!({
                    "operation": "delete",
                    "path": workspace_relative_display_path(workspace_root, &target_path)
                }));
            }
            PatchOperation::Update {
                path,
                move_to,
                hunks,
            } => {
                let source_path = resolve_workspace_target(workspace_root, &path, false)?;
                let content = fs::read_to_string(&source_path).map_err(|e| e.to_string())?;
                let updated = apply_hunks_to_content(&content, &hunks)?;
                if !dry_run {
                    write_file_atomic(&source_path, updated.as_bytes())?;
                }

                let final_path = if let Some(move_target_raw) = move_to {
                    let target_path =
                        resolve_workspace_target(workspace_root, &move_target_raw, true)?;
                    if !dry_run {
                        if let Some(parent) = target_path.parent() {
                            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                        }
                        fs::rename(&source_path, &target_path).map_err(|e| e.to_string())?;
                    }
                    target_path
                } else {
                    source_path
                };

                applied.push(json!({
                    "operation": "update",
                    "path": workspace_relative_display_path(workspace_root, &final_path)
                }));
            }
        }
    }

    Ok(json!({
        "dry_run": dry_run,
        "operations": applied
    }))
}

pub(crate) fn parse_todo_status(status: Option<&str>) -> Result<TodoStatus, String> {
    let Some(value) = status else {
        return Ok(TodoStatus::Pending);
    };

    match value.trim().to_ascii_lowercase().as_str() {
        "pending" => Ok(TodoStatus::Pending),
        "in_progress" | "inprogress" | "in-progress" => Ok(TodoStatus::InProgress),
        "completed" | "done" => Ok(TodoStatus::Completed),
        _ => Err(format!("Invalid TODO status: {}", value)),
    }
}

pub(crate) async fn execute_todo_write(
    arguments: &Value,
    conversation_id: &str,
) -> Result<Value, String> {
    let action = read_string_argument(arguments, "action")?;
    let mut store = todo_store().lock().await;
    let items = store.entry(conversation_id.to_string()).or_default();
    let now = Utc::now().to_rfc3339();

    match action.as_str() {
        "add" => {
            let text = read_string_argument(arguments, "text")?;
            let status = parse_todo_status(
                arguments
                    .get("status")
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty()),
            )?;
            let id = read_optional_string_argument(arguments, "id")
                .unwrap_or_else(|| Uuid::new_v4().to_string());
            items.push(TodoItem {
                id,
                text,
                status,
                created_at: now.clone(),
                updated_at: now.clone(),
            });
        }
        "set" => {
            let raw_items = arguments
                .get("items")
                .and_then(Value::as_array)
                .ok_or_else(|| "'items' is required for action 'set'".to_string())?;
            items.clear();
            for raw_item in raw_items {
                let text = raw_item
                    .get("text")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| "Each TODO item requires non-empty 'text'".to_string())?
                    .to_string();
                let id = raw_item
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
                    .unwrap_or_else(|| Uuid::new_v4().to_string());
                let status = parse_todo_status(raw_item.get("status").and_then(Value::as_str))?;
                items.push(TodoItem {
                    id,
                    text,
                    status,
                    created_at: now.clone(),
                    updated_at: now.clone(),
                });
            }
        }
        "update" => {
            let id = read_string_argument(arguments, "id")?;
            let item = items
                .iter_mut()
                .find(|candidate| candidate.id == id)
                .ok_or_else(|| format!("TODO item not found: {}", id))?;
            if let Some(text) = read_optional_string_argument(arguments, "text") {
                item.text = text;
            }
            if let Some(status_raw) = read_optional_string_argument(arguments, "status") {
                item.status = parse_todo_status(Some(status_raw.as_str()))?;
            }
            item.updated_at = now.clone();
        }
        "remove" => {
            let id = read_string_argument(arguments, "id")?;
            let before = items.len();
            items.retain(|candidate| candidate.id != id);
            if items.len() == before {
                return Err(format!("TODO item not found: {}", id));
            }
        }
        "clear" => {
            items.clear();
        }
        _ => {
            return Err("Unsupported action. Use add|set|update|remove|clear".to_string());
        }
    }

    Ok(json!({
        "conversation_id": conversation_id,
        "action": action,
        "items": items
    }))
}

pub(crate) async fn execute_todo_read(
    arguments: &Value,
    conversation_id: &str,
) -> Result<Value, String> {
    let include_done = read_bool_argument(arguments, "include_completed", true);
    let store = todo_store().lock().await;
    let mut items = store.get(conversation_id).cloned().unwrap_or_default();
    if !include_done {
        items.retain(|item| !matches!(item.status, TodoStatus::Completed));
    }
    Ok(json!({
        "conversation_id": conversation_id,
        "items": items
    }))
}

pub(crate) async fn execute_web_fetch(arguments: &Value) -> Result<Value, String> {
    web_tools::execute_web_fetch(arguments).await
}

pub(crate) async fn execute_web_search(arguments: &Value) -> Result<Value, String> {
    web_tools::execute_web_search(arguments).await
}

pub(crate) async fn execute_browser(arguments: &Value) -> Result<Value, String> {
    browser_tools::execute_browser(arguments).await
}

pub(crate) async fn execute_browser_navigate(arguments: &Value) -> Result<Value, String> {
    browser_tools::execute_browser_navigate_compat(arguments).await
}

pub(crate) async fn execute_desktop(
    arguments: &Value,
    conversation_id: &str,
    config: &Config,
) -> Result<Value, String> {
    let action = read_string_argument(arguments, "action")?;
    let params = arguments
        .get("params")
        .cloned()
        .unwrap_or_else(|| json!({}));
    if !params.is_object() {
        return Err("'params' must be an object".to_string());
    }

    let request = DesktopToolRequest { action, params };
    desktop::execute_desktop_request(conversation_id, &request, &config.desktop).await
}

pub(crate) async fn execute_image_probe(
    arguments: &Value,
    workspace_root: &Path,
) -> Result<Value, String> {
    image_tools::execute_image_probe(arguments, workspace_root).await
}

pub(crate) async fn execute_image_understand(
    arguments: &Value,
    workspace_root: &Path,
    llm_service: &LlmService,
    default_model: &str,
) -> Result<Value, String> {
    image_tools::execute_image_understand(arguments, workspace_root, llm_service, default_model)
        .await
}

pub(crate) async fn execute_workspace_process_start(
    arguments: &Value,
    workspace_root: &Path,
) -> Result<Value, String> {
    process_tools::execute_workspace_process_start(arguments, workspace_root).await
}

pub(crate) async fn execute_workspace_process_list(arguments: &Value) -> Result<Value, String> {
    process_tools::execute_workspace_process_list(arguments).await
}

pub(crate) async fn execute_workspace_process_read(arguments: &Value) -> Result<Value, String> {
    process_tools::execute_workspace_process_read(arguments).await
}

pub(crate) async fn execute_workspace_process_terminate(arguments: &Value) -> Result<Value, String> {
    process_tools::execute_workspace_process_terminate(arguments).await
}
pub(crate) async fn execute_skill_list(
    skill_manager_state: &SkillManagerState,
) -> Result<Value, String> {
    let manager = skill_manager_state.lock().await;
    Ok(json!({
        "skills": manager.list_skills()
    }))
}

pub(crate) async fn execute_skill_discover(
    arguments: &Value,
    skill_manager_state: &SkillManagerState,
) -> Result<Value, String> {
    let query = read_optional_string_argument(arguments, "query");
    let limit = read_u64_argument(arguments, "limit", 8).clamp(1, 20) as usize;
    let (clawhub_api_key, clawhub_api_base) = resolve_clawhub_settings_for_discovery();
    let manager = skill_manager_state.lock().await;
    let results = manager
        .discover_skills(
            query.as_deref(),
            limit,
            clawhub_api_key.as_deref(),
            clawhub_api_base.as_deref(),
        )
        .await
        .map_err(|e| e.to_string())?;
    Ok(json!({
        "query": query.unwrap_or_default(),
        "limit": limit,
        "results": results
    }))
}

pub(crate) async fn execute_skill_execute(
    arguments: &Value,
    skill_manager_state: &SkillManagerState,
) -> Result<Value, String> {
    let preferred_skill_id = read_optional_string_argument(arguments, "skill_id");
    let preferred_skill_name = read_optional_string_argument(arguments, "skill_name");
    let params = arguments
        .get("params")
        .and_then(Value::as_object)
        .map(|object| {
            object
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect::<HashMap<String, Value>>()
        })
        .unwrap_or_default();

    let manager = skill_manager_state.lock().await;
    let resolved_skill_id = if let Some(skill_id) = preferred_skill_id {
        skill_id
    } else if let Some(skill_name) = preferred_skill_name {
        manager
            .list_skills()
            .into_iter()
            .find(|item| item.name.eq_ignore_ascii_case(&skill_name))
            .map(|item| item.id)
            .ok_or_else(|| format!("Skill not found by name: {}", skill_name))?
    } else {
        return Err("Either 'skill_id' or 'skill_name' is required".to_string());
    };

    let result = manager
        .execute_skill(&resolved_skill_id, params)
        .await
        .map_err(|e| e.to_string())?;
    Ok(json!({
        "skill_id": resolved_skill_id,
        "result": result
    }))
}

pub(crate) async fn execute_sessions_list(
    arguments: &Value,
    pool: &SqlitePool,
) -> Result<Value, String> {
    let limit = read_u64_argument(arguments, "limit", 30).clamp(1, 200) as i64;
    let rows = sqlx::query_as::<_, (String, String, String, String, String)>(
        "SELECT id, title, model, created_at, updated_at FROM conversations ORDER BY updated_at DESC LIMIT ?",
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    let conversations = rows
        .into_iter()
        .map(|(id, title, model, created_at, updated_at)| {
            json!({
                "id": id,
                "title": title,
                "model": model,
                "created_at": created_at,
                "updated_at": updated_at
            })
        })
        .collect::<Vec<_>>();

    Ok(json!({
        "limit": limit,
        "conversations": conversations
    }))
}

pub(crate) async fn execute_sessions_history(
    arguments: &Value,
    pool: &SqlitePool,
) -> Result<Value, String> {
    let conversation_id = read_string_argument(arguments, "conversation_id")?;
    let limit = read_u64_argument(arguments, "limit", 100).clamp(1, 500) as i64;

    let rows = sqlx::query_as::<_, (String, String, String, String, Option<String>)>(
        "SELECT role, content, created_at, id, tool_calls FROM messages WHERE conversation_id = ? ORDER BY created_at DESC LIMIT ?",
    )
    .bind(&conversation_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    let mut messages = rows
        .into_iter()
        .map(|(role, content, created_at, id, tool_calls)| {
            json!({
                "id": id,
                "role": role,
                "content": content,
                "created_at": created_at,
                "tool_calls": tool_calls
                    .as_deref()
                    .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
            })
        })
        .collect::<Vec<_>>();
    messages.reverse();

    Ok(json!({
        "conversation_id": conversation_id,
        "limit": limit,
        "messages": messages
    }))
}

pub(crate) async fn execute_sessions_send(
    arguments: &Value,
    pool: &SqlitePool,
    llm_service: &LlmService,
    default_model: &str,
) -> Result<Value, String> {
    let conversation_id = read_string_argument(arguments, "conversation_id")?;
    let content = read_string_argument(arguments, "content")?;
    let run_assistant = read_bool_argument(arguments, "run_assistant", false);
    let conversation_model =
        sqlx::query_scalar::<_, String>("SELECT model FROM conversations WHERE id = ?")
            .bind(&conversation_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?;
    if conversation_model.is_none() {
        return Err(format!("Conversation not found: {}", conversation_id));
    }
    let selected_model = read_optional_string_argument(arguments, "model")
        .or(conversation_model)
        .unwrap_or_else(|| default_model.to_string());

    insert_message(pool, &conversation_id, "user", &content, None, None).await?;

    let assistant_response = if run_assistant {
        let messages = load_conversation_context(pool, &conversation_id).await?;
        let response = llm_service
            .chat(&selected_model, messages)
            .await
            .map_err(|e| e.to_string())?;
        insert_message(pool, &conversation_id, "assistant", &response, None, None).await?;
        Some(response)
    } else {
        None
    };

    Ok(json!({
        "conversation_id": conversation_id,
        "model": selected_model,
        "run_assistant": run_assistant,
        "assistant_response": assistant_response
    }))
}

pub(crate) async fn execute_sessions_spawn(
    arguments: &Value,
    pool: &SqlitePool,
    llm_service: &LlmService,
    default_model: &str,
) -> Result<Value, String> {
    let title = read_string_argument(arguments, "title")?;
    let model = read_optional_string_argument(arguments, "model")
        .unwrap_or_else(|| default_model.to_string());
    let content = read_optional_string_argument(arguments, "content");
    let run_assistant = read_bool_argument(arguments, "run_assistant", false);
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO conversations (id, title, model, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&title)
    .bind(&model)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    if let Some(user_text) = content.clone() {
        insert_message(pool, &id, "user", &user_text, None, None).await?;
    }

    let assistant_response = if run_assistant {
        if content.is_none() {
            return Err("'content' is required when 'run_assistant' is true".to_string());
        }
        let messages = load_conversation_context(pool, &id).await?;
        let response = llm_service
            .chat(&model, messages)
            .await
            .map_err(|e| e.to_string())?;
        insert_message(pool, &id, "assistant", &response, None, None).await?;
        Some(response)
    } else {
        None
    };

    Ok(json!({
        "conversation_id": id,
        "title": title,
        "model": model,
        "assistant_response": assistant_response
    }))
}

pub(crate) fn execute_agents_list() -> Result<Value, String> {
    Ok(json!({
        "agents": [
            {
                "id": "default",
                "name": "Default Agent",
                "description": "General-purpose coding and assistant workflow."
            },
            {
                "id": "planner",
                "name": "Planner Agent",
                "description": "Task decomposition and structured planning."
            },
            {
                "id": "analyst",
                "name": "Analyst Agent",
                "description": "Investigation and comparative reasoning."
            }
        ]
    }))
}

pub(crate) fn require_scheduler_manager(
) -> Result<std::sync::Arc<crate::services::scheduler::manager::SchedulerManager>, String> {
    scheduler_manager().ok_or_else(|| "Scheduler is not initialized yet".to_string())
}

pub(crate) fn validate_scheduler_session_target(session_target: SchedulerSessionTarget) -> Result<(), String> {
    if matches!(session_target, SchedulerSessionTarget::Heartbeat) {
        return Err("scheduler jobs cannot use session_target=heartbeat".to_string());
    }
    Ok(())
}

pub(crate) async fn execute_scheduler_jobs_list(arguments: &Value) -> Result<Value, String> {
    let include_disabled = read_bool_argument(arguments, "include_disabled", false);
    let jobs = require_scheduler_manager()?
        .list_jobs(include_disabled)
        .await?;
    Ok(json!({ "jobs": jobs }))
}

pub(crate) async fn execute_scheduler_job_create(arguments: &Value) -> Result<Value, String> {
    let input: SchedulerJobCreateInput =
        serde_json::from_value(arguments.clone()).map_err(|e| e.to_string())?;
    validate_scheduler_session_target(input.session_target.clone())?;
    if matches!(input.schedule_kind, SchedulerScheduleKind::At) && input.schedule_at.is_none() {
        return Err("'schedule_at' is required for schedule_kind=at".to_string());
    }
    if matches!(input.schedule_kind, SchedulerScheduleKind::Every) && input.every_ms.is_none() {
        return Err("'every_ms' is required for schedule_kind=every".to_string());
    }
    if matches!(input.schedule_kind, SchedulerScheduleKind::Cron) && input.cron_expr.is_none() {
        return Err("'cron_expr' is required for schedule_kind=cron".to_string());
    }
    let job = require_scheduler_manager()?.create_job(input).await?;
    Ok(json!({ "job": job }))
}

pub(crate) async fn execute_scheduler_job_update(arguments: &Value) -> Result<Value, String> {
    let job_id = read_optional_string_argument(arguments, "job_id")
        .or_else(|| read_optional_string_argument(arguments, "jobId"))
        .ok_or_else(|| "'job_id' is required".to_string())?;

    let patch_value = if let Some(patch) = arguments.get("patch") {
        patch.clone()
    } else {
        let mut object = arguments
            .as_object()
            .cloned()
            .ok_or_else(|| "arguments must be an object".to_string())?;
        object.remove("job_id");
        object.remove("jobId");
        Value::Object(object)
    };
    if !patch_value.is_object() {
        return Err("'patch' must be an object".to_string());
    }

    let patch: SchedulerJobPatchInput =
        serde_json::from_value(patch_value).map_err(|e| e.to_string())?;
    if let Some(session_target) = patch.session_target.clone() {
        validate_scheduler_session_target(session_target)?;
    }

    let job = require_scheduler_manager()?
        .update_job(&job_id, patch)
        .await?;
    Ok(json!({ "job": job }))
}

pub(crate) async fn execute_scheduler_job_delete(arguments: &Value) -> Result<Value, String> {
    let job_id = read_optional_string_argument(arguments, "job_id")
        .or_else(|| read_optional_string_argument(arguments, "jobId"))
        .ok_or_else(|| "'job_id' is required".to_string())?;
    let removed = require_scheduler_manager()?.delete_job(&job_id).await?;
    Ok(json!({
        "deleted": removed,
        "job_id": job_id
    }))
}

pub(crate) fn execute_scheduler_job_run(arguments: &Value) -> Result<Value, String> {
    let job_id = read_optional_string_argument(arguments, "job_id")
        .or_else(|| read_optional_string_argument(arguments, "jobId"))
        .ok_or_else(|| "'job_id' is required".to_string())?;
    let manager = require_scheduler_manager()?;
    // 使用 spawn 触发，避免在 scheduler 内部调用链中形成 Send 循环依赖
    tauri::async_runtime::spawn(async move {
        if let Err(e) = manager.run_job_now(&job_id).await {
            eprintln!("scheduler_job_run tool failed for job {}: {}", job_id, e);
        }
    });
    Ok(json!({
        "accepted": true,
        "reason": null
    }))
}

pub(crate) async fn execute_scheduler_runs_list(arguments: &Value) -> Result<Value, String> {
    let job_id = read_optional_string_argument(arguments, "job_id")
        .or_else(|| read_optional_string_argument(arguments, "jobId"));
    let limit = arguments
        .get("limit")
        .and_then(Value::as_i64)
        .unwrap_or(50)
        .clamp(1, 1000);
    let runs = require_scheduler_manager()?
        .list_runs(job_id.as_deref(), limit)
        .await?;
    Ok(json!({ "runs": runs }))
}

pub(crate) async fn execute_core_task(
    arguments: &Value,
    llm_service: &LlmService,
    default_model: &str,
) -> Result<Value, String> {
    let prompt = read_string_argument(arguments, "prompt")?;
    let model = read_optional_string_argument(arguments, "model")
        .unwrap_or_else(|| default_model.to_string());
    let response = llm_service
        .chat(
            &model,
            vec![ChatMessage {
                role: "user".to_string(),
                content: Some(prompt.clone()),
                tool_calls: None,
                tool_call_id: None,
                reasoning_details: None,
                reasoning: None,
            }],
        )
        .await
        .map_err(|e| e.to_string())?;

    Ok(json!({
        "model": model,
        "prompt": prompt,
        "response": response
    }))
}
pub(crate) async fn execute_core_batch_safe_tool(
    tool_name: &str,
    arguments: &Value,
    workspace_root: &Path,
    conversation_id: &str,
    skill_manager_state: &SkillManagerState,
    pool: &SqlitePool,
    llm_service: &LlmService,
) -> Result<Value, String> {
    match tool_name {
        WORKSPACE_LIST_TOOL => execute_workspace_list_directory(arguments, workspace_root),
        WORKSPACE_READ_TOOL => execute_workspace_read_file(arguments, workspace_root),
        WORKSPACE_PARSE_PDF_TOOL => execute_workspace_parse_pdf_markdown(arguments, workspace_root),
        WORKSPACE_GLOB_TOOL => execute_workspace_glob(arguments, workspace_root),
        WORKSPACE_GREP_TOOL => execute_workspace_grep(arguments, workspace_root),
        WORKSPACE_CODESEARCH_TOOL => execute_workspace_codesearch(arguments, workspace_root),
        WORKSPACE_LSP_SYMBOLS_TOOL => execute_workspace_lsp_symbols(arguments, workspace_root),
        TODO_READ_TOOL => execute_todo_read(arguments, conversation_id).await,
        TODO_WRITE_TOOL => execute_todo_write(arguments, conversation_id).await,
        WEB_FETCH_TOOL => execute_web_fetch(arguments).await,
        WEB_SEARCH_TOOL => execute_web_search(arguments).await,
        BROWSER_TOOL => execute_browser(arguments).await,
        BROWSER_NAVIGATE_TOOL => execute_browser_navigate(arguments).await,
        IMAGE_PROBE_TOOL => execute_image_probe(arguments, workspace_root).await,
        IMAGE_UNDERSTAND_TOOL => {
            execute_image_understand(arguments, workspace_root, llm_service, "glm-4.6v").await
        }
        SESSIONS_LIST_TOOL => execute_sessions_list(arguments, pool).await,
        SESSIONS_HISTORY_TOOL => execute_sessions_history(arguments, pool).await,
        AGENTS_LIST_TOOL => execute_agents_list(),
        SKILL_DISCOVER_TOOL => execute_skill_discover(arguments, skill_manager_state).await,
        SKILL_LIST_TOOL => execute_skill_list(skill_manager_state).await,
        _ => Err(format!("Unsupported batch tool: {}", tool_name)),
    }
}

pub(crate) async fn execute_core_batch(
    arguments: &Value,
    workspace_root: &Path,
    conversation_id: &str,
    skill_manager_state: &SkillManagerState,
    pool: &SqlitePool,
    llm_service: &LlmService,
) -> Result<Value, String> {
    let calls = arguments
        .get("tool_calls")
        .and_then(Value::as_array)
        .ok_or_else(|| "'tool_calls' must be an array".to_string())?;

    let mut results = Vec::<Value>::new();
    for (index, call) in calls.iter().enumerate() {
        let tool_name = call
            .get("tool")
            .or_else(|| call.get("name"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| format!("tool_calls[{}].tool is required", index))?;
        let tool_arguments = call.get("arguments").cloned().unwrap_or_else(|| json!({}));
        if !tool_arguments.is_object() {
            return Err(format!("tool_calls[{}].arguments must be an object", index));
        }
        if !should_auto_allow_batch_tool(tool_name) {
            results.push(json!({
                "index": index,
                "tool": tool_name,
                "result": Value::Null,
                "error": "Tool is not allowed in core_batch"
            }));
            continue;
        }

        match execute_core_batch_safe_tool(
            tool_name,
            &tool_arguments,
            workspace_root,
            conversation_id,
            skill_manager_state,
            pool,
            llm_service,
        )
        .await
        {
            Ok(value) => results.push(json!({
                "index": index,
                "tool": tool_name,
                "result": value,
                "error": Value::Null
            })),
            Err(error) => results.push(json!({
                "index": index,
                "tool": tool_name,
                "result": Value::Null,
                "error": error
            })),
        }
    }

    Ok(json!({
        "results": results
    }))
}

pub(crate) async fn execute_runtime_tool_non_scheduler(
    mcp_state: &McpState,
    skill_manager_state: &SkillManagerState,
    config: &Config,
    runtime_tool: RuntimeTool,
    arguments: &Value,
    workspace_root: &Path,
    conversation_id: &str,
    pool: &SqlitePool,
    llm_service: &LlmService,
    default_model: &str,
) -> Result<Value, String> {
    match runtime_tool {
        RuntimeTool::Mcp {
            server_name,
            tool_name,
        } => {
            let mut manager = mcp_state.lock().await;
            let client = manager
                .get_client_mut(&server_name)
                .ok_or_else(|| format!("MCP server '{}' is not connected", server_name))?;

            client
                .call_tool(&tool_name, arguments.clone())
                .await
                .map_err(|e| e.to_string())
        }
        RuntimeTool::WorkspaceListDirectory => {
            execute_workspace_list_directory(arguments, workspace_root)
        }
        RuntimeTool::WorkspaceReadFile => execute_workspace_read_file(arguments, workspace_root),
        RuntimeTool::WorkspaceParsePdfMarkdown => {
            execute_workspace_parse_pdf_markdown(arguments, workspace_root)
        }
        RuntimeTool::WorkspaceWriteFile => execute_workspace_write_file(arguments, workspace_root),
        RuntimeTool::WorkspaceEditFile => execute_workspace_edit_file(arguments, workspace_root),
        RuntimeTool::WorkspaceGlob => execute_workspace_glob(arguments, workspace_root),
        RuntimeTool::WorkspaceGrep => execute_workspace_grep(arguments, workspace_root),
        RuntimeTool::WorkspaceCodeSearch => execute_workspace_codesearch(arguments, workspace_root),
        RuntimeTool::WorkspaceLspSymbols => {
            execute_workspace_lsp_symbols(arguments, workspace_root)
        }
        RuntimeTool::WorkspaceApplyPatch => {
            execute_workspace_apply_patch(arguments, workspace_root)
        }
        RuntimeTool::WorkspaceRunCommand => {
            execute_workspace_run_command(arguments, workspace_root).await
        }
        RuntimeTool::WorkspaceProcessStart => {
            execute_workspace_process_start(arguments, workspace_root).await
        }
        RuntimeTool::WorkspaceProcessList => execute_workspace_process_list(arguments).await,
        RuntimeTool::WorkspaceProcessRead => execute_workspace_process_read(arguments).await,
        RuntimeTool::WorkspaceProcessTerminate => {
            execute_workspace_process_terminate(arguments).await
        }
        RuntimeTool::SkillInstallFromRepo => {
            let repo_url = read_string_argument(arguments, "repo_url")?;
            let skill_path = read_optional_string_argument(arguments, "skill_path");
            let mut manager = skill_manager_state.lock().await;
            let installed = if let Some(path) = skill_path.as_deref() {
                manager
                    .install_skill_with_path(&repo_url, Some(path))
                    .await
                    .map_err(|e| e.to_string())?
            } else {
                manager
                    .install_skill(&repo_url)
                    .await
                    .map_err(|e| e.to_string())?
            };

            Ok(json!({
                "installed": true,
                "repo_url": repo_url,
                "skill_path": skill_path,
                "skill": installed
            }))
        }
        RuntimeTool::SkillDiscover => {
            execute_skill_discover(arguments, skill_manager_state).await
        }
        RuntimeTool::SkillList => execute_skill_list(skill_manager_state).await,
        RuntimeTool::SkillExecute => {
            execute_skill_execute(arguments, skill_manager_state).await
        }
        RuntimeTool::CoreBatch => {
            execute_core_batch(
                arguments,
                workspace_root,
                conversation_id,
                skill_manager_state,
                pool,
                llm_service,
            )
            .await
        }
        RuntimeTool::CoreTask => execute_core_task(arguments, llm_service, default_model).await,
        RuntimeTool::TodoWrite => execute_todo_write(arguments, conversation_id).await,
        RuntimeTool::TodoRead => execute_todo_read(arguments, conversation_id).await,
        RuntimeTool::WebFetch => execute_web_fetch(arguments).await,
        RuntimeTool::WebSearch => execute_web_search(arguments).await,
        RuntimeTool::Browser => execute_browser(arguments).await,
        RuntimeTool::BrowserNavigate => execute_browser_navigate(arguments).await,
        RuntimeTool::Desktop => execute_desktop(arguments, conversation_id, config).await,
        RuntimeTool::ImageProbe => execute_image_probe(arguments, workspace_root).await,
        RuntimeTool::ImageUnderstand => {
            execute_image_understand(arguments, workspace_root, llm_service, default_model).await
        }
        RuntimeTool::SessionsList => execute_sessions_list(arguments, pool).await,
        RuntimeTool::SessionsHistory => execute_sessions_history(arguments, pool).await,
        RuntimeTool::SessionsSend => {
            execute_sessions_send(arguments, pool, llm_service, default_model).await
        }
        RuntimeTool::SessionsSpawn => {
            execute_sessions_spawn(arguments, pool, llm_service, default_model).await
        }
        RuntimeTool::AgentsList => execute_agents_list(),
        RuntimeTool::SchedulerJobsList => execute_scheduler_jobs_list(arguments).await,
        RuntimeTool::SchedulerJobCreate => execute_scheduler_job_create(arguments).await,
        RuntimeTool::SchedulerJobUpdate => execute_scheduler_job_update(arguments).await,
        RuntimeTool::SchedulerJobDelete => execute_scheduler_job_delete(arguments).await,
        RuntimeTool::SchedulerJobRun => execute_scheduler_job_run(arguments),
        RuntimeTool::SchedulerRunsList => execute_scheduler_runs_list(arguments).await,
    }
}

pub(crate) async fn execute_scheduler_runtime_tool(
    runtime_tool: RuntimeTool,
    arguments: &Value,
) -> Result<Value, String> {
    match runtime_tool {
        RuntimeTool::SchedulerJobsList => execute_scheduler_jobs_list(arguments).await,
        RuntimeTool::SchedulerJobCreate => execute_scheduler_job_create(arguments).await,
        RuntimeTool::SchedulerJobUpdate => execute_scheduler_job_update(arguments).await,
        RuntimeTool::SchedulerJobDelete => execute_scheduler_job_delete(arguments).await,
        RuntimeTool::SchedulerJobRun => execute_scheduler_job_run(arguments),
        RuntimeTool::SchedulerRunsList => execute_scheduler_runs_list(arguments).await,
        _ => Err("Tool is not supported inside scheduler core loop".to_string()),
    }
}

pub(crate) async fn execute_runtime_tool(
    mcp_state: &McpState,
    skill_manager_state: &SkillManagerState,
    config: &Config,
    runtime_tool: RuntimeTool,
    arguments: &Value,
    workspace_root: &Path,
    conversation_id: &str,
    pool: &SqlitePool,
    llm_service: &LlmService,
    default_model: &str,
) -> Result<Value, String> {
    if matches!(
        runtime_tool,
        RuntimeTool::SchedulerJobsList
            | RuntimeTool::SchedulerJobCreate
            | RuntimeTool::SchedulerJobUpdate
            | RuntimeTool::SchedulerJobDelete
            | RuntimeTool::SchedulerJobRun
            | RuntimeTool::SchedulerRunsList
    ) {
        execute_scheduler_runtime_tool(runtime_tool, arguments).await
    } else {
        execute_runtime_tool_non_scheduler(
            mcp_state,
            skill_manager_state,
            config,
            runtime_tool,
            arguments,
            workspace_root,
            conversation_id,
            pool,
            llm_service,
            default_model,
        )
        .await
    }
}

pub(crate) async fn execute_tool_call(
    mcp_state: &McpState,
    skill_manager_state: &SkillManagerState,
    config: &Config,
    tool_map: &HashMap<String, RuntimeTool>,
    tool_call: &ChatToolCall,
    workspace_root: &Path,
    conversation_id: &str,
    pool: &SqlitePool,
    llm_service: &LlmService,
    default_model: &str,
) -> Result<Value, String> {
    let raw_arguments_str = tool_call.function.arguments.clone();
    let (tool_name, raw_arguments) = if tool_call.function.name == "bash" {
        ("workspace_run_command", recover_tool_arguments_candidate(&raw_arguments_str).unwrap_or(raw_arguments_str))
    } else {
        (tool_call.function.name.as_str(), raw_arguments_str)
    };

    let Some(runtime_tool) = tool_map.get(tool_name).cloned() else {
        return Err(format!("Tool '{}' not found", tool_name));
    };

    let arguments = parse_tool_arguments(&raw_arguments);

    execute_runtime_tool(
        mcp_state,
        skill_manager_state,
        config,
        runtime_tool,
        &arguments,
        workspace_root,
        conversation_id,
        pool,
        llm_service,
        default_model,
    )
    .await
}

pub(crate) async fn execute_tool_call_background(
    mcp_state: &McpState,
    skill_manager_state: &SkillManagerState,
    config: &Config,
    tool_map: &HashMap<String, RuntimeTool>,
    tool_call: &ChatToolCall,
    workspace_root: &Path,
    conversation_id: &str,
    pool: &SqlitePool,
    llm_service: &LlmService,
    default_model: &str,
) -> Result<Value, String> {
    let raw_arguments_str = tool_call.function.arguments.clone();
    let (tool_name, raw_arguments) = if tool_call.function.name == "bash" {
        ("workspace_run_command", recover_tool_arguments_candidate(&raw_arguments_str).unwrap_or(raw_arguments_str))
    } else {
        (tool_call.function.name.as_str(), raw_arguments_str)
    };

    let Some(runtime_tool) = tool_map.get(tool_name).cloned() else {
        return Err(format!("Tool '{}' not found in background session!", tool_name));
    };

    let arguments = parse_tool_arguments(&raw_arguments);

    execute_runtime_tool(
        mcp_state,
        skill_manager_state,
        config,
        runtime_tool,
        &arguments,
        workspace_root,
        conversation_id,
        pool,
        llm_service,
        default_model,
    )
    .await
}
