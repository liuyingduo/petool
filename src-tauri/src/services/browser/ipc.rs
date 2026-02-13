use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};
use tokio::sync::{oneshot, Mutex};

use super::paths::{
    browser_sidecar_stderr_log_path, ensure_parent, format_launch_help, SidecarLaunchSpec,
};
use super::types::{BrowserRpcRequest, BrowserRpcResponse};

fn sanitize_stderr_line(line: &str) -> String {
    let mut rendered = line.to_string();
    for marker in [
        "token",
        "authorization",
        "api_key",
        "apikey",
        "password",
        "cookie",
    ] {
        let marker_lower = marker.to_ascii_lowercase();
        if rendered.to_ascii_lowercase().contains(&marker_lower) {
            rendered = rendered.replace(line, "[redacted]");
            break;
        }
    }
    rendered
}

async fn spawn_stdout_reader(
    stdout: ChildStdout,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Result<BrowserRpcResponse, String>>>>>,
) {
    let mut lines = BufReader::new(stdout).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let parsed = serde_json::from_str::<BrowserRpcResponse>(&line);
        match parsed {
            Ok(response) => {
                let tx = {
                    let mut guard = pending.lock().await;
                    guard.remove(&response.id)
                };
                if let Some(channel) = tx {
                    let _ = channel.send(Ok(response));
                }
            }
            Err(err) => {
                let mut guard = pending.lock().await;
                for (_, tx) in guard.drain() {
                    let _ = tx.send(Err(format!(
                        "Invalid browser sidecar response JSON: {} (line={})",
                        err, line
                    )));
                }
                break;
            }
        }
    }

    let mut guard = pending.lock().await;
    for (_, tx) in guard.drain() {
        let _ = tx.send(Err("Browser sidecar stdout closed".to_string()));
    }
}

async fn spawn_stderr_reader(stderr: ChildStderr) {
    let mut lines = BufReader::new(stderr).lines();
    let log_path = match browser_sidecar_stderr_log_path() {
        Ok(path) => path,
        Err(_) => return,
    };
    let _ = ensure_parent(&log_path);

    while let Ok(Some(line)) = lines.next_line().await {
        let sanitized = sanitize_stderr_line(&line);
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&log_path) {
            let _ = writeln!(file, "{}", sanitized);
        }
    }
}

pub struct BrowserIpcClient {
    child: Arc<Mutex<Child>>,
    stdin: Arc<Mutex<ChildStdin>>,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Result<BrowserRpcResponse, String>>>>>,
    next_id: AtomicU64,
    launch_program: String,
    launch_args: Vec<String>,
}

impl BrowserIpcClient {
    pub async fn spawn(spec: SidecarLaunchSpec) -> Result<Self> {
        let mut command = Command::new(&spec.program);
        command
            .args(&spec.args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = command
            .spawn()
            .map_err(|err| anyhow!("Failed to spawn browser sidecar: {}", err))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("Missing browser sidecar stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("Missing browser sidecar stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| anyhow!("Missing browser sidecar stderr"))?;

        let pending = Arc::new(Mutex::new(HashMap::new()));
        tokio::spawn(spawn_stdout_reader(stdout, Arc::clone(&pending)));
        tokio::spawn(spawn_stderr_reader(stderr));

        Ok(Self {
            child: Arc::new(Mutex::new(child)),
            stdin: Arc::new(Mutex::new(stdin)),
            pending,
            next_id: AtomicU64::new(1),
            launch_program: spec.program,
            launch_args: spec.args,
        })
    }

    pub async fn call(
        &self,
        method: &str,
        params: serde_json::Value,
        timeout: Duration,
    ) -> Result<BrowserRpcResponse> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let request = BrowserRpcRequest {
            id,
            method: method.to_string(),
            params,
        };
        let payload = serde_json::to_string(&request)?;

        let (tx, rx) = oneshot::channel::<Result<BrowserRpcResponse, String>>();
        {
            let mut pending = self.pending.lock().await;
            pending.insert(id, tx);
        }

        {
            let mut stdin = self.stdin.lock().await;
            if let Err(err) = stdin.write_all(payload.as_bytes()).await {
                let mut pending = self.pending.lock().await;
                pending.remove(&id);
                return Err(anyhow!("Failed to write browser sidecar stdin: {}", err));
            }
            if let Err(err) = stdin.write_all(b"\n").await {
                let mut pending = self.pending.lock().await;
                pending.remove(&id);
                return Err(anyhow!("Failed to write browser sidecar newline: {}", err));
            }
            if let Err(err) = stdin.flush().await {
                let mut pending = self.pending.lock().await;
                pending.remove(&id);
                return Err(anyhow!("Failed to flush browser sidecar stdin: {}", err));
            }
        }

        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(Ok(response))) => Ok(response),
            Ok(Ok(Err(err))) => Err(anyhow!(err)),
            Ok(Err(_)) => Err(anyhow!("Browser sidecar response channel dropped")),
            Err(_) => {
                let mut pending = self.pending.lock().await;
                pending.remove(&id);
                Err(anyhow!(
                    "Timed out waiting for browser sidecar response (method={}, timeout_ms={})",
                    method,
                    timeout.as_millis()
                ))
            }
        }
    }

    pub async fn is_running(&self) -> bool {
        let mut child = self.child.lock().await;
        match child.try_wait() {
            Ok(Some(_)) => false,
            Ok(None) => true,
            Err(_) => false,
        }
    }

    pub async fn kill(&self) {
        let mut child = self.child.lock().await;
        let _ = child.kill().await;
    }

    pub fn launch_help_message(&self, stderr_excerpt: &str) -> String {
        format_launch_help(&self.launch_program, &self.launch_args, stderr_excerpt)
    }
}
