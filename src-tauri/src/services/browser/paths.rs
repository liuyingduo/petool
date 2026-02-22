use anyhow::{anyhow, Result};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::utils::{get_app_config_dir, get_app_log_dir};

#[derive(Debug, Clone)]
pub struct SidecarLaunchSpec {
    pub program: String,
    pub args: Vec<String>,
}

pub fn browser_profiles_root() -> Result<PathBuf> {
    let root = get_app_config_dir()?.join("browser").join("profiles");
    fs::create_dir_all(&root)?;
    Ok(root)
}

pub fn browser_profile_user_data_dir(profile: &str) -> Result<PathBuf> {
    let safe = sanitize_profile_name(profile);
    let path = browser_profiles_root()?.join(safe).join("user-data");
    fs::create_dir_all(&path)?;
    Ok(path)
}

pub fn browser_sidecar_stderr_log_path() -> Result<PathBuf> {
    let path = get_app_log_dir()?.join("browser-sidecar.log");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(path)
}

pub fn sanitize_profile_name(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches('-');
    if trimmed.is_empty() {
        "openclaw".to_string()
    } else {
        trimmed.to_string()
    }
}

fn resolve_node_binary() -> Option<PathBuf> {
    if let Ok(path) = env::var("PETOOL_BROWSER_NODE_BIN") {
        let candidate = PathBuf::from(path);
        if candidate.exists() {
            return Some(candidate);
        }
    }

    let mut candidates = Vec::<PathBuf>::new();
    if let Ok(exe) = env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            let target = env::var("TAURI_ENV_TARGET_TRIPLE")
                .or_else(|_| env::var("TARGET"))
                .unwrap_or_default();
            let extension = if cfg!(target_os = "windows") {
                ".exe"
            } else {
                ""
            };
            candidates.push(exe_dir.join(format!("browser-node{}", extension)));
            if !target.is_empty() {
                candidates.push(exe_dir.join(format!("browser-node-{}{}", target, extension)));
                candidates.push(
                    exe_dir
                        .join("binaries")
                        .join(format!("browser-node-{}{}", target, extension)),
                );
            }
        }
    }

    for candidate in candidates {
        if candidate.exists() {
            return Some(candidate);
        }
    }

    None
}

fn candidate_sidecar_entries() -> Vec<PathBuf> {
    let mut candidates = Vec::<PathBuf>::new();
    if let Ok(path) = env::var("PETOOL_BROWSER_SIDECAR_ENTRY") {
        candidates.push(PathBuf::from(path));
    }

    if let Ok(cwd) = env::current_dir() {
        let src_entry = cwd.join("browser-sidecar").join("src").join("index.mjs");
        let dist_entry = cwd.join("browser-sidecar").join("dist").join("index.mjs");
        let cwd_resource_entry = cwd
            .join("resources")
            .join("browser-sidecar")
            .join("dist")
            .join("index.mjs");
        let parent_src_entry = cwd
            .join("..")
            .join("browser-sidecar")
            .join("src")
            .join("index.mjs");
        let parent_dist_entry = cwd
            .join("..")
            .join("browser-sidecar")
            .join("dist")
            .join("index.mjs");
        let parent_resource_entry = cwd
            .join("..")
            .join("src-tauri")
            .join("resources")
            .join("browser-sidecar")
            .join("dist")
            .join("index.mjs");
        if prefer_src_sidecar_in_dev() {
            candidates.push(src_entry);
            candidates.push(dist_entry);
            candidates.push(parent_src_entry);
            candidates.push(parent_dist_entry);
        } else {
            candidates.push(dist_entry);
            candidates.push(src_entry);
            candidates.push(parent_dist_entry);
            candidates.push(parent_src_entry);
        }
        candidates.push(cwd_resource_entry);
        candidates.push(parent_resource_entry);
    }

    if let Ok(exe) = env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            candidates.push(
                exe_dir
                    .join("resources")
                    .join("browser-sidecar")
                    .join("dist")
                    .join("index.mjs"),
            );
            candidates.push(
                exe_dir
                    .join("browser-sidecar")
                    .join("dist")
                    .join("index.mjs"),
            );
            candidates.push(
                exe_dir
                    .join("..")
                    .join("Resources")
                    .join("browser-sidecar")
                    .join("dist")
                    .join("index.mjs"),
            );
        }
    }

    candidates
}

fn prefer_src_sidecar_in_dev() -> bool {
    if !cfg!(debug_assertions) {
        return false;
    }
    match env::var("PETOOL_BROWSER_DEV_USE_SRC_SIDECAR") {
        Ok(value) => matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        Err(_) => true,
    }
}

pub fn resolve_sidecar_entry() -> Result<PathBuf> {
    for candidate in candidate_sidecar_entries() {
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    Err(anyhow!(
        "Browser sidecar entry not found. Set PETOOL_BROWSER_SIDECAR_ENTRY or run scripts/browser/prepare-sidecar.mjs."
    ))
}

pub fn resolve_sidecar_launch_spec() -> Result<SidecarLaunchSpec> {
    let entry = resolve_sidecar_entry()?;
    let node_program = resolve_node_binary()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| "node".to_string());

    Ok(SidecarLaunchSpec {
        program: node_program,
        args: vec![entry.to_string_lossy().to_string()],
    })
}

pub fn format_launch_help(program: &str, args: &[String], stderr_excerpt: &str) -> String {
    let rendered_args = if args.is_empty() {
        String::new()
    } else {
        format!(" {}", args.join(" "))
    };
    format!(
        "Failed to start browser sidecar.\nProgram: {}{}\nStderr: {}\nHints: run scripts/browser/prepare-sidecar.mjs and set browser executable_path (external Chrome launch + auto attach via --remote-debugging-port), or provide cdp_url for attach-only mode.",
        program,
        rendered_args,
        stderr_excerpt
    )
}

pub fn ensure_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_profile_name_normalizes_invalid_chars() {
        assert_eq!(sanitize_profile_name("Open Claw@123"), "open-claw-123");
        assert_eq!(sanitize_profile_name(""), "openclaw");
        assert_eq!(sanitize_profile_name("___"), "___");
    }
}
