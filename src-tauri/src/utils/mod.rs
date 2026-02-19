use anyhow::Result;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

fn detect_install_root() -> Option<PathBuf> {
    let exe_path = std::env::current_exe().ok()?;

    #[cfg(target_os = "macos")]
    {
        let components: Vec<_> = exe_path.components().collect();
        let app_index = components.iter().position(|component| {
            component
                .as_os_str()
                .to_string_lossy()
                .to_ascii_lowercase()
                .ends_with(".app")
        })?;
        let mut app_path = PathBuf::new();
        for component in &components[..=app_index] {
            app_path.push(component.as_os_str());
        }
        return app_path.parent().map(Path::to_path_buf);
    }

    exe_path.parent().map(Path::to_path_buf)
}

pub fn ensure_writable_directory(path: &Path) -> Result<()> {
    fs::create_dir_all(path)?;
    let probe_path = path.join(".petool-write-test");
    {
        let mut file = fs::File::create(&probe_path)?;
        file.write_all(b"ok")?;
        file.flush()?;
    }
    let _ = fs::remove_file(probe_path);
    Ok(())
}

pub fn fallback_downloads_dir() -> PathBuf {
    dirs::data_local_dir()
        .or_else(dirs::config_dir)
        .unwrap_or_else(std::env::temp_dir)
        .join("petool")
        .join("downloads")
}

pub fn resolve_default_downloads_dir() -> PathBuf {
    if let Some(install_root) = detect_install_root() {
        if let Some(parent) = install_root.parent() {
            let sibling = parent.join("petool-data").join("downloads");
            if ensure_writable_directory(&sibling).is_ok() {
                return sibling;
            }
        }
    }

    let fallback = fallback_downloads_dir();
    let _ = ensure_writable_directory(&fallback);
    fallback
}

pub fn resolve_effective_downloads_dir(raw: Option<&str>) -> PathBuf {
    let from_config = raw
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);
    from_config.unwrap_or_else(resolve_default_downloads_dir)
}

pub fn resolve_tools_root(downloads_dir: &Path) -> PathBuf {
    downloads_dir.join("tools")
}

pub fn resolve_skills_dir(downloads_dir: &Path) -> PathBuf {
    resolve_tools_root(downloads_dir).join("skills")
}

pub fn resolve_node_runtime_root(downloads_dir: &Path) -> PathBuf {
    resolve_tools_root(downloads_dir).join("node-runtime")
}

pub fn resolve_skill_download_cache_dir(downloads_dir: &Path) -> PathBuf {
    downloads_dir.join("download-cache").join("skills")
}

pub fn resolve_node_download_cache_dir(downloads_dir: &Path) -> PathBuf {
    downloads_dir.join("download-cache").join("node")
}

pub fn get_app_config_dir() -> Result<PathBuf> {
    let config_dir =
        dirs::config_dir().ok_or_else(|| anyhow::anyhow!("Could not find config directory"))?;

    let app_config_dir = config_dir.join("petool");
    fs::create_dir_all(&app_config_dir)?;

    Ok(app_config_dir)
}

pub fn get_app_log_dir() -> Result<PathBuf> {
    let data_local_dir = dirs::data_local_dir()
        .ok_or_else(|| anyhow::anyhow!("Could not find local data directory"))?;
    let app_log_dir = data_local_dir.join("petool").join("logs");
    fs::create_dir_all(&app_log_dir)?;
    Ok(app_log_dir)
}

pub fn get_config_path() -> Result<PathBuf> {
    Ok(get_app_config_dir()?.join("config.json"))
}

pub fn load_config<T>() -> Result<T>
where
    T: serde::de::DeserializeOwned + Default,
{
    let config_path = get_config_path()?;

    if config_path.exists() {
        let content = fs::read_to_string(config_path)?;
        serde_json::from_str(&content).map_err(Into::into)
    } else {
        Ok(T::default())
    }
}

pub fn save_config<T>(config: &T) -> Result<()>
where
    T: serde::Serialize,
{
    let config_path = get_config_path()?;
    let content = serde_json::to_string_pretty(config)?;
    fs::write(config_path, content)?;
    Ok(())
}

