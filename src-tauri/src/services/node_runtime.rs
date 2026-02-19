use anyhow::{anyhow, Context, Result};
use crate::models::config::Config;
use crate::utils::{
    ensure_writable_directory, load_config, resolve_effective_downloads_dir,
    resolve_node_download_cache_dir, resolve_node_runtime_root,
};
use serde::Deserialize;
use serde_json::Value;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use tokio::sync::Mutex;
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct NodeRuntime {
    pub node_command: PathBuf,
    pub bin_dir: Option<PathBuf>,
}

impl NodeRuntime {
    pub fn apply_to_command(&self, command: &mut Command) {
        let Some(bin_dir) = self.bin_dir.as_ref() else {
            return;
        };
        let path_key = if cfg!(windows) { "Path" } else { "PATH" };
        let mut next_path = OsString::new();
        next_path.push(bin_dir.as_os_str());
        if let Some(current_path) = env::var_os(path_key).or_else(|| env::var_os("PATH")) {
            let separator = if cfg!(windows) { ";" } else { ":" };
            next_path.push(separator);
            next_path.push(current_path);
        }
        command.env(path_key, &next_path);
        if cfg!(windows) {
            command.env("PATH", &next_path);
        }
    }
}

static NODE_INSTALL_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn install_lock() -> &'static Mutex<()> {
    NODE_INSTALL_LOCK.get_or_init(|| Mutex::new(()))
}

fn node_executable_name() -> &'static str {
    if cfg!(windows) {
        "node.exe"
    } else {
        "node"
    }
}

fn can_run_node(program: &Path) -> bool {
    Command::new(program)
        .arg("-v")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn managed_runtime_root() -> Result<PathBuf> {
    let config = load_config::<Config>().unwrap_or_default();
    let downloads_dir = resolve_effective_downloads_dir(config.downloads_directory.as_deref());
    let root = resolve_node_runtime_root(&downloads_dir);
    ensure_writable_directory(&root)?;
    Ok(root)
}

fn managed_current_dir() -> Result<PathBuf> {
    Ok(managed_runtime_root()?.join("current"))
}

fn managed_node_executable() -> Result<PathBuf> {
    let current = managed_current_dir()?;
    if cfg!(windows) {
        return Ok(current.join("node.exe"));
    }
    Ok(current.join("bin").join("node"))
}

pub fn detect_available_node_runtime() -> Option<NodeRuntime> {
    if can_run_node(Path::new("node")) {
        return Some(NodeRuntime {
            node_command: PathBuf::from("node"),
            bin_dir: None,
        });
    }

    let node_path = managed_node_executable().ok()?;
    if !node_path.exists() || !can_run_node(&node_path) {
        return None;
    }
    let bin_dir = node_path.parent().map(Path::to_path_buf);
    Some(NodeRuntime {
        node_command: node_path,
        bin_dir,
    })
}

#[derive(Debug, Deserialize)]
struct NodeDistEntry {
    version: String,
    lts: Value,
    #[serde(default)]
    files: Vec<String>,
}

fn is_lts_channel(value: &Value) -> bool {
    if let Some(text) = value.as_str() {
        return !text.trim().is_empty();
    }
    value.as_bool().unwrap_or(false)
}

#[cfg(target_os = "windows")]
fn windows_download_target() -> Result<(&'static str, &'static str)> {
    match env::consts::ARCH {
        "x86_64" => Ok(("win-x64", "win-x64-zip")),
        "aarch64" => Ok(("win-arm64", "win-arm64-zip")),
        other => Err(anyhow!(
            "Automatic Node runtime install is not supported on Windows architecture '{}'",
            other
        )),
    }
}

async fn resolve_latest_node_lts_version(required_file: &str) -> Result<String> {
    let client = reqwest::Client::builder()
        .user_agent("petool/0.1")
        .timeout(std::time::Duration::from_secs(20))
        .build()?;
    let entries = client
        .get("https://nodejs.org/dist/index.json")
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<NodeDistEntry>>()
        .await?;
    for entry in entries {
        if !is_lts_channel(&entry.lts) {
            continue;
        }
        if entry.files.iter().any(|item| item == required_file) {
            return Ok(entry.version);
        }
    }
    Err(anyhow!(
        "Unable to find a Node.js LTS build for target '{}'",
        required_file
    ))
}

fn extract_zip_archive(archive_path: &Path, destination: &Path) -> Result<()> {
    let file = fs::File::open(archive_path)?;
    let mut zip = zip::ZipArchive::new(file)?;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index)?;
        let Some(safe_path) = entry.enclosed_name().map(|value| value.to_owned()) else {
            continue;
        };
        let output_path = destination.join(safe_path);
        if entry.is_dir() {
            fs::create_dir_all(&output_path)?;
            continue;
        }
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut out_file = fs::File::create(&output_path)?;
        io::copy(&mut entry, &mut out_file)?;
    }
    Ok(())
}

fn copy_directory(source: &Path, destination: &Path) -> Result<()> {
    for entry in WalkDir::new(source) {
        let entry = entry?;
        let entry_path = entry.path();
        let relative = entry_path
            .strip_prefix(source)
            .map_err(|error| anyhow!("Failed to resolve relative path: {}", error))?;
        let target = destination.join(relative);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&target)?;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(entry_path, &target)?;
    }
    Ok(())
}

fn find_runtime_root(extracted_dir: &Path) -> Result<PathBuf> {
    let executable_name = node_executable_name();
    let candidate = WalkDir::new(extracted_dir)
        .into_iter()
        .filter_map(Result::ok)
        .find_map(|entry| {
            if !entry.file_type().is_file() {
                return None;
            }
            if entry.file_name() != executable_name {
                return None;
            }
            entry.path().parent().map(Path::to_path_buf)
        })
        .ok_or_else(|| {
            anyhow!(
                "Downloaded Node runtime does not contain '{}'",
                executable_name
            )
        })?;
    Ok(candidate)
}

#[cfg(target_os = "windows")]
async fn install_managed_node_runtime() -> Result<()> {
    let (platform_suffix, file_token) = windows_download_target()?;
    let version = resolve_latest_node_lts_version(file_token).await?;
    let download_url = format!(
        "https://nodejs.org/dist/{}/node-{}-{}.zip",
        version, version, platform_suffix
    );

    let config = load_config::<Config>().unwrap_or_default();
    let downloads_dir = resolve_effective_downloads_dir(config.downloads_directory.as_deref());
    let runtime_root = managed_runtime_root()?;
    fs::create_dir_all(&runtime_root)?;
    let cache_root = resolve_node_download_cache_dir(&downloads_dir);
    fs::create_dir_all(&cache_root)?;

    let temp_dir = cache_root.join(format!("tmp-install-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&temp_dir)?;
    let archive_path = temp_dir.join("node.zip");
    let extract_dir = temp_dir.join("extract");
    fs::create_dir_all(&extract_dir)?;

    let install_result: Result<()> = async {
        let client = reqwest::Client::builder()
            .user_agent("petool/0.1")
            .timeout(std::time::Duration::from_secs(60))
            .build()?;
        let bytes = client
            .get(&download_url)
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;
        fs::write(&archive_path, bytes)?;
        extract_zip_archive(&archive_path, &extract_dir)?;

        let extracted_runtime = find_runtime_root(&extract_dir)?;
        let current_dir = managed_current_dir()?;
        if current_dir.exists() {
            fs::remove_dir_all(&current_dir)?;
        }
        if let Err(error) = fs::rename(&extracted_runtime, &current_dir) {
            let _ = error;
            copy_directory(&extracted_runtime, &current_dir)?;
        }
        let installed_node = managed_node_executable()?;
        if !installed_node.exists() || !can_run_node(&installed_node) {
            return Err(anyhow!("Installed Node runtime validation failed"));
        }
        Ok(())
    }
    .await;

    let _ = fs::remove_dir_all(&temp_dir);
    install_result.with_context(|| format!("Failed to install managed Node from {}", download_url))
}

#[cfg(not(target_os = "windows"))]
async fn install_managed_node_runtime() -> Result<()> {
    Err(anyhow!(
        "Automatic Node runtime install is currently implemented for Windows only"
    ))
}

pub async fn ensure_node_runtime() -> Result<NodeRuntime> {
    if let Some(runtime) = detect_available_node_runtime() {
        return Ok(runtime);
    }

    let lock = install_lock();
    let _guard = lock.lock().await;

    if let Some(runtime) = detect_available_node_runtime() {
        return Ok(runtime);
    }

    install_managed_node_runtime().await?;
    detect_available_node_runtime().ok_or_else(|| {
        anyhow!(
            "Node.js runtime is still unavailable after installation. Please install Node.js manually."
        )
    })
}

