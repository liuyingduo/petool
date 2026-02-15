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

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Config commands
            config::get_config,
            config::set_config,
            config::validate_api_key,
            config::open_browser_profile_dir,
            config::reset_browser_profile,
            // Chat commands
            chat::send_message,
            chat::stream_message,
            chat::stop_stream,
            chat::generate_image,
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
            skills::discover_skills,
            skills::install_skill,
            skills::uninstall_skill,
            skills::execute_skill,
            skills::toggle_skill,
            skills::update_skill,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
