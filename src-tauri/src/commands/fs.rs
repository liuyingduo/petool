use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: Option<u64>,
    pub extension: Option<String>,
}

#[tauri::command]
pub async fn select_folder(window: tauri::Window) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = tokio::sync::oneshot::channel();

    window.dialog().file().pick_folder(move |folder_path| {
        let _ = tx.send(folder_path);
    });

    let folder_path = rx.await.map_err(|e| e.to_string())?;

    Ok(folder_path.map(|p| p.to_string()))
}

#[tauri::command]
pub async fn scan_directory(path: String) -> Result<Vec<FileInfo>, String> {
    let path = std::path::PathBuf::from(&path);

    if !path.exists() || !path.is_dir() {
        return Err("Invalid directory path".to_string());
    }

    let mut files = Vec::new();

    let entries = std::fs::read_dir(&path).map_err(|e| e.to_string())?;

    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let metadata = entry.metadata().ok();

        let file_info = FileInfo {
            name: entry.file_name().to_string_lossy().to_string(),
            path: entry.path().to_string_lossy().to_string(),
            is_dir: metadata.as_ref().map(|m| m.is_dir()).unwrap_or(false),
            size: metadata
                .as_ref()
                .and_then(|m| if m.is_file() { Some(m.len()) } else { None }),
            extension: entry
                .path()
                .extension()
                .map(|ext| ext.to_string_lossy().to_string()),
        };

        files.push(file_info);
    }

    // Sort: directories first, then files, alphabetically
    files.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.cmp(&b.name),
    });

    Ok(files)
}

#[tauri::command]
pub async fn read_file(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn write_file(path: String, content: String) -> Result<(), String> {
    std::fs::write(&path, content).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_path_info(path: String) -> Result<FileInfo, String> {
    let path_buf = std::path::PathBuf::from(&path);
    if !path_buf.exists() {
        return Err(format!("Path does not exist: {}", path));
    }

    let metadata = std::fs::metadata(&path_buf).map_err(|e| e.to_string())?;
    let name = path_buf
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| path_buf.to_string_lossy().to_string());

    Ok(FileInfo {
        name,
        path: path_buf.to_string_lossy().to_string(),
        is_dir: metadata.is_dir(),
        size: if metadata.is_file() {
            Some(metadata.len())
        } else {
            None
        },
        extension: path_buf
            .extension()
            .map(|ext| ext.to_string_lossy().to_string()),
    })
}
