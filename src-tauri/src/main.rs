// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod models;
mod services;
mod state;
mod utils;

use commands::{chat, config, fs, mcp, skills};
use services::database::Database;
use services::mcp_client::McpManager;
use services::skill_manager::SkillManager;
use state::AppState;
use state::AppStateInner;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use tauri::Manager;

#[cfg(target_os = "windows")]
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::HWND;
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Gdi::{
    CombineRgn, CreateRectRgn, CreateRoundRectRgn, DeleteObject, SetWindowRgn, HGDIOBJ, RGN_OR,
};

#[cfg(target_os = "windows")]
fn is_webview_unavailable_error(message: &str) -> bool {
    let normalized = message.to_ascii_lowercase();
    normalized.contains("failed to receive message from webview")
        || normalized.contains("window has been closed")
}

#[cfg(target_os = "windows")]
fn apply_pet_window_shape(window: &tauri::WebviewWindow) -> Result<(), String> {
    let size = match window.inner_size() {
        Ok(size) => size,
        Err(err) => {
            let message = err.to_string();
            if is_webview_unavailable_error(&message) {
                return Ok(());
            }
            return Err(message);
        }
    };
    let width = size.width as i32;
    let height = size.height as i32;

    if width <= 0 || height <= 0 {
        return Ok(());
    }

    // Keep native hit-region aligned with frontend geometry in app-shell.css.
    const TOP_OFFSET: i32 = 14;
    const CORNER_RADIUS: i32 = 68;
    const EAR_WIDTH: i32 = 84;
    const EAR_HEIGHT: i32 = 50;
    const LEFT_EAR_OFFSET: i32 = -128;
    const RIGHT_EAR_OFFSET: i32 = 44;

    let body_top = TOP_OFFSET.min(height.saturating_sub(1)).max(0);
    let center_x = width / 2;
    let left_ear_x = (center_x + LEFT_EAR_OFFSET).max(0);
    let right_ear_x = (center_x + RIGHT_EAR_OFFSET).min(width.saturating_sub(EAR_WIDTH));

    let body = unsafe {
        CreateRoundRectRgn(
            0,
            body_top,
            width + 1,
            height + 1,
            CORNER_RADIUS,
            CORNER_RADIUS,
        )
    };
    let left_ear = unsafe {
        CreateRoundRectRgn(
            left_ear_x,
            0,
            left_ear_x + EAR_WIDTH,
            EAR_HEIGHT,
            EAR_WIDTH,
            EAR_HEIGHT,
        )
    };
    let right_ear = unsafe {
        CreateRoundRectRgn(
            right_ear_x,
            0,
            right_ear_x + EAR_WIDTH,
            EAR_HEIGHT,
            EAR_WIDTH,
            EAR_HEIGHT,
        )
    };
    let combined = unsafe { CreateRectRgn(0, 0, 0, 0) };

    if body.0.is_null() || left_ear.0.is_null() || right_ear.0.is_null() || combined.0.is_null() {
        return Err("failed to create window region".to_string());
    }

    unsafe {
        CombineRgn(combined, body, left_ear, RGN_OR);
        CombineRgn(combined, combined, right_ear, RGN_OR);
    }

    let hwnd = match window
        .window_handle()
        .map_err(|err| err.to_string())?
        .as_raw()
    {
        RawWindowHandle::Win32(handle) => HWND(handle.hwnd.get() as *mut std::ffi::c_void),
        _ => return Err("unsupported window handle type".to_string()),
    };

    let result = unsafe { SetWindowRgn(hwnd, combined, true) };

    unsafe {
        let _ = DeleteObject(HGDIOBJ(body.0));
        let _ = DeleteObject(HGDIOBJ(left_ear.0));
        let _ = DeleteObject(HGDIOBJ(right_ear.0));
    }

    if result == 0 {
        unsafe {
            let _ = DeleteObject(HGDIOBJ(combined.0));
        }
        return Err("failed to apply window region".to_string());
    }

    Ok(())
}

#[tokio::main]
async fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|app| {
            // Initialize database
            let config_dir = app.path().app_config_dir().unwrap();
            let db_path = config_dir.join("petool.db");
            let skills_dir = config_dir.join("skills");

            // Create app state
            let app_state: AppState = Arc::new(StdMutex::new(AppStateInner::new()));
            app.manage(app_state.clone());

            // Create MCP manager
            let mcp_manager: Arc<tokio::sync::Mutex<McpManager>> =
                Arc::new(tokio::sync::Mutex::new(McpManager::new()));
            app.manage(mcp_manager);

            // Create skill manager
            let skill_manager_result = SkillManager::new(skills_dir);
            if let Ok(skill_manager) = skill_manager_result {
                let skill_manager: Arc<tokio::sync::Mutex<SkillManager>> =
                    Arc::new(tokio::sync::Mutex::new(skill_manager));
                app.manage(skill_manager.clone());

                tauri::async_runtime::spawn(async move {
                    let mut manager = skill_manager.lock().await;
                    if let Err(err) = manager.load_skills().await {
                        eprintln!("Failed to load skills: {}", err);
                    }
                });
            }

            // Initialize database in background
            let app_state_clone = app_state.clone();
            tauri::async_runtime::spawn(async move {
                if let Ok(db) = Database::new(db_path).await {
                    if let Ok(mut state) = app_state_clone.lock() {
                        state.set_db(db);
                    } else {
                        eprintln!("Failed to acquire app state lock while setting database");
                    }
                }
            });

            #[cfg(target_os = "windows")]
            {
                if let Some(main_window) = app.get_webview_window("main") {
                    if let Err(err) = apply_pet_window_shape(&main_window) {
                        if !is_webview_unavailable_error(&err) {
                            eprintln!("Failed to apply shaped window region: {}", err);
                        }
                    }

                    let window_for_events = main_window.clone();
                    main_window.on_window_event(move |event| {
                        if matches!(
                            event,
                            tauri::WindowEvent::Resized(_)
                                | tauri::WindowEvent::ScaleFactorChanged { .. }
                        ) {
                            if let Err(err) = apply_pet_window_shape(&window_for_events) {
                                if !is_webview_unavailable_error(&err) {
                                    eprintln!("Failed to update shaped window region: {}", err);
                                }
                            }
                        }
                    });
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Config commands
            config::get_config,
            config::set_config,
            config::validate_api_key,
            // Chat commands
            chat::send_message,
            chat::stream_message,
            chat::resolve_tool_approval,
            chat::get_conversations,
            chat::get_messages,
            chat::create_conversation,
            chat::delete_conversation,
            // File system commands
            fs::select_folder,
            fs::scan_directory,
            fs::read_file,
            fs::write_file,
            fs::get_path_info,
            // MCP commands
            mcp::connect_server,
            mcp::disconnect_server,
            mcp::list_tools,
            mcp::call_tool,
            mcp::list_prompts,
            mcp::list_resources,
            mcp::list_servers,
            mcp::disconnect_all_servers,
            mcp::read_resource,
            // Skills commands
            skills::list_skills,
            skills::install_skill,
            skills::uninstall_skill,
            skills::execute_skill,
            skills::toggle_skill,
            skills::update_skill,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
