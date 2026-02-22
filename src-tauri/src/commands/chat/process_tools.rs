use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex as StdMutex, OnceLock};

use chrono::Utc;
use serde_json::{json, Value};
use uuid::Uuid;

use super::{
    read_bool_argument, read_optional_string_argument, read_string_argument, read_u64_argument,
    resolve_workspace_target, workspace_relative_display_path,
};

#[derive(Debug, Default)]
struct ProcessOutputBuffer {
    stdout: String,
    stderr: String,
}

#[derive(Debug)]
struct ManagedProcess {
    id: String,
    command: String,
    started_at: String,
    pid: u32,
    child: Child,
    output: Arc<StdMutex<ProcessOutputBuffer>>,
}

type ProcessStore = tokio::sync::Mutex<HashMap<String, ManagedProcess>>;

static PROCESS_STORE: OnceLock<ProcessStore> = OnceLock::new();

fn process_store() -> &'static ProcessStore {
    PROCESS_STORE.get_or_init(|| tokio::sync::Mutex::new(HashMap::new()))
}

fn trim_process_output(buffer: &mut String, max_chars: usize) {
    if buffer.chars().count() <= max_chars {
        return;
    }
    let tail = buffer
        .chars()
        .rev()
        .take(max_chars)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<String>();
    *buffer = tail;
}

fn spawn_process_reader<R: Read + Send + 'static>(
    reader: R,
    output: Arc<StdMutex<ProcessOutputBuffer>>,
    is_stdout: bool,
) {
    std::thread::spawn(move || {
        let mut reader = BufReader::new(reader);
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    if let Ok(mut guard) = output.lock() {
                        if is_stdout {
                            guard.stdout.push_str(&line);
                            trim_process_output(&mut guard.stdout, 200_000);
                        } else {
                            guard.stderr.push_str(&line);
                            trim_process_output(&mut guard.stderr, 200_000);
                        }
                    } else {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
}

pub(super) async fn execute_workspace_process_start(
    arguments: &Value,
    workspace_root: &Path,
) -> Result<Value, String> {
    let command = read_string_argument(arguments, "command")?;
    let cwd_raw =
        read_optional_string_argument(arguments, "cwd").unwrap_or_else(|| ".".to_string());
    let cwd = resolve_workspace_target(workspace_root, &cwd_raw, false)?;
    if !cwd.is_dir() {
        return Err(format!("Not a directory: {}", cwd.display()));
    }

    let mut cmd = if cfg!(target_os = "windows") {
        let wrapped_command = format!(
            "$OutputEncoding = [Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false); [Console]::InputEncoding = [System.Text.UTF8Encoding]::new($false); chcp 65001 > $null; {}",
            command
        );
        let mut process = Command::new("powershell");
        process.args(["-NoProfile", "-NonInteractive", "-Command", &wrapped_command]);
        process
    } else {
        let mut process = Command::new("sh");
        process.args(["-lc", &command]);
        process
    };
    cmd.current_dir(&cwd);
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    let pid = child.id();
    let output = Arc::new(StdMutex::new(ProcessOutputBuffer::default()));

    if let Some(stdout) = child.stdout.take() {
        spawn_process_reader(stdout, Arc::clone(&output), true);
    }
    if let Some(stderr) = child.stderr.take() {
        spawn_process_reader(stderr, Arc::clone(&output), false);
    }

    let process_id = Uuid::new_v4().to_string();
    let managed = ManagedProcess {
        id: process_id.clone(),
        command: command.clone(),
        started_at: Utc::now().to_rfc3339(),
        pid,
        child,
        output,
    };

    let mut store = process_store().lock().await;
    store.insert(process_id.clone(), managed);

    Ok(json!({
        "process_id": process_id,
        "pid": pid,
        "command": command,
        "cwd": workspace_relative_display_path(workspace_root, &cwd)
    }))
}

pub(super) async fn execute_workspace_process_list(arguments: &Value) -> Result<Value, String> {
    let cleanup_exited = read_bool_argument(arguments, "cleanup_exited", false);
    let mut store = process_store().lock().await;
    let mut exited_ids = Vec::<String>::new();
    let mut items = Vec::<Value>::new();

    for (process_id, process) in store.iter_mut() {
        let status = match process.child.try_wait() {
            Ok(Some(status)) => {
                if cleanup_exited {
                    exited_ids.push(process_id.clone());
                }
                json!({
                    "running": false,
                    "exit_code": status.code(),
                    "success": status.success()
                })
            }
            Ok(None) => json!({
                "running": true,
                "exit_code": Value::Null,
                "success": Value::Null
            }),
            Err(error) => json!({
                "running": false,
                "error": error.to_string()
            }),
        };

        let (stdout_chars, stderr_chars) = match process.output.lock() {
            Ok(buffer) => (buffer.stdout.chars().count(), buffer.stderr.chars().count()),
            Err(_) => (0usize, 0usize),
        };

        items.push(json!({
            "id": process.id,
            "process_id": process_id,
            "pid": process.pid,
            "command": process.command,
            "started_at": process.started_at,
            "status": status,
            "stdout_chars": stdout_chars,
            "stderr_chars": stderr_chars
        }));
    }

    if cleanup_exited {
        for id in exited_ids {
            store.remove(&id);
        }
    }

    Ok(json!({
        "processes": items
    }))
}

pub(super) async fn execute_workspace_process_read(arguments: &Value) -> Result<Value, String> {
    let process_id = read_string_argument(arguments, "process_id")?;
    let max_chars = read_u64_argument(arguments, "max_chars", 10_000).clamp(200, 200_000) as usize;
    let mut store = process_store().lock().await;
    let process = store
        .get_mut(&process_id)
        .ok_or_else(|| format!("Unknown process id: {}", process_id))?;
    let status = process.child.try_wait().map_err(|e| e.to_string())?;

    let (stdout, stderr) = process
        .output
        .lock()
        .map(|buffer| (buffer.stdout.clone(), buffer.stderr.clone()))
        .map_err(|_| "Failed to lock process output".to_string())?;

    let trim_tail = |content: String| -> String {
        if content.chars().count() <= max_chars {
            return content;
        }
        content
            .chars()
            .rev()
            .take(max_chars)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<String>()
    };

    Ok(json!({
        "process_id": process_id,
        "stdout": trim_tail(stdout),
        "stderr": trim_tail(stderr),
        "status": match status {
            Some(exit) => json!({
                "running": false,
                "exit_code": exit.code(),
                "success": exit.success()
            }),
            None => json!({
                "running": true
            })
        }
    }))
}

pub(super) async fn execute_workspace_process_terminate(
    arguments: &Value,
) -> Result<Value, String> {
    let process_id = read_string_argument(arguments, "process_id")?;
    let mut store = process_store().lock().await;
    let mut process = store
        .remove(&process_id)
        .ok_or_else(|| format!("Unknown process id: {}", process_id))?;

    let mut result = json!({
        "process_id": process_id,
        "terminated": false
    });

    match process.child.try_wait().map_err(|e| e.to_string())? {
        Some(status) => {
            result["terminated"] = json!(true);
            result["already_exited"] = json!(true);
            result["exit_code"] = json!(status.code());
            result["success"] = json!(status.success());
        }
        None => {
            process.child.kill().map_err(|e| e.to_string())?;
            let status = process.child.wait().map_err(|e| e.to_string())?;
            result["terminated"] = json!(true);
            result["already_exited"] = json!(false);
            result["exit_code"] = json!(status.code());
            result["success"] = json!(status.success());
        }
    }

    Ok(result)
}
