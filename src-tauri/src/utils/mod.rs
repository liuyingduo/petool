use anyhow::Result;
use std::fs;
use std::path::PathBuf;

pub fn get_config_path() -> Result<PathBuf> {
    let config_dir = dirs::config_dir()
        .ok_or_else(|| anyhow::anyhow!("Could not find config directory"))?;

    let app_config_dir = config_dir.join("petool");
    fs::create_dir_all(&app_config_dir)?;

    Ok(app_config_dir.join("config.json"))
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
