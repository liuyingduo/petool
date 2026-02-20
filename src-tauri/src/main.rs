// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod models;
mod services;
mod state;
mod utils;

use commands::{chat, config, fs, mcp, petool_account, scheduler, skills};
use models::config::{AutomationCloseBehavior, Config};
use services::database::Database;
use services::mcp_client::McpManager;
use services::scheduler::initialize_scheduler;
use services::skill_manager::SkillManager;
use state::AppState;
use state::AppStateInner;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager, WindowEvent};
use utils::{load_config, resolve_effective_downloads_dir, resolve_skills_dir};

const TRAY_MENU_OPEN: &str = "tray-open-petool";
const TRAY_MENU_EXIT: &str = "tray-exit-petool";

fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn setup_tray(app: &tauri::App) -> Result<TrayIcon, Box<dyn std::error::Error>> {
    let open_item = MenuItem::with_id(app, TRAY_MENU_OPEN, "打开 PETool", true, None::<&str>)?;
    let exit_item = MenuItem::with_id(app, TRAY_MENU_EXIT, "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open_item, &exit_item])?;

    let tray = TrayIconBuilder::with_id("petool-tray")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("PETool")
        .on_menu_event(|app, event| match event.id().as_ref() {
            TRAY_MENU_OPEN => show_main_window(app),
            TRAY_MENU_EXIT => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(tray)
}

#[tokio::main]
async fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_notification::init())
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let behavior = load_config::<Config>()
                    .map(|config| config.automation.close_behavior)
                    .unwrap_or(AutomationCloseBehavior::Ask);

                match behavior {
                    AutomationCloseBehavior::Exit => {}
                    AutomationCloseBehavior::MinimizeToTray => {
                        api.prevent_close();
                        let _ = window.hide();
                    }
                    AutomationCloseBehavior::Ask => {
                        api.prevent_close();
                        let _ = window.emit("app-close-requested", ());
                    }
                }
            }
        })
        .setup(|app| {
            // Initialize database
            let config_dir = app.path().app_config_dir().unwrap();
            let db_path = config_dir.join("petool.db");
            let initial_config = load_config::<Config>().unwrap_or_default();
            let initial_downloads =
                resolve_effective_downloads_dir(initial_config.downloads_directory.as_deref());
            let skills_dir = resolve_skills_dir(&initial_downloads);
            let app_handle = app.handle().clone();

            // Create app state
            let app_state: AppState = Arc::new(StdMutex::new(AppStateInner::new()));
            app.manage(app_state.clone());

            // Create MCP manager
            let mcp_manager_state: Arc<tokio::sync::Mutex<McpManager>> =
                Arc::new(tokio::sync::Mutex::new(McpManager::new()));
            app.manage(mcp_manager_state.clone());

            // Create skill manager
            let skill_manager = SkillManager::new(skills_dir)?;
            let skill_manager_state: Arc<tokio::sync::Mutex<SkillManager>> =
                Arc::new(tokio::sync::Mutex::new(skill_manager));
            app.manage(skill_manager_state.clone());

            let skill_manager_state_for_load = skill_manager_state.clone();
            tauri::async_runtime::spawn(async move {
                let mut manager = skill_manager_state_for_load.lock().await;
                if let Err(err) = manager.load_skills().await {
                    eprintln!("Failed to load skills: {}", err);
                }
            });

            let tray_icon = setup_tray(app)?;
            app.manage(tray_icon);

            // Initialize database in background
            let app_state_clone = app_state.clone();
            let mcp_state_clone = mcp_manager_state.clone();
            let skill_state_clone = skill_manager_state.clone();
            tauri::async_runtime::spawn(async move {
                if let Ok(db) = Database::new(db_path).await {
                    let pool = db.pool().clone();
                    if let Ok(mut state) = app_state_clone.lock() {
                        state.set_db(db);
                    } else {
                        eprintln!("Failed to acquire app state lock while setting database");
                        return;
                    }

                    if let Err(error) = initialize_scheduler(
                        app_handle,
                        pool,
                        app_state_clone,
                        mcp_state_clone,
                        skill_state_clone,
                    ) {
                        eprintln!("Failed to initialize scheduler: {}", error);
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
            config::submit_feedback,
            config::app_exit_now,
            // Chat commands
            chat::send_message,
            chat::stream_message,
            chat::stop_stream,
            chat::generate_image,
            chat::resolve_tool_approval,
            chat::get_conversations,
            chat::get_messages,
            chat::get_conversation_timeline,
            chat::create_conversation,
            chat::delete_conversation,
            chat::rename_conversation,
            chat::update_conversation_model,
            // File system commands
            fs::select_folder,
            fs::scan_directory,
            fs::read_file,
            fs::write_file,
            fs::get_path_info,
            fs::parse_pdf_to_markdown,
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
            // Scheduler commands
            scheduler::scheduler_get_status,
            scheduler::scheduler_list_jobs,
            scheduler::scheduler_get_job,
            scheduler::scheduler_create_job,
            scheduler::scheduler_update_job,
            scheduler::scheduler_delete_job,
            scheduler::scheduler_run_job_now,
            scheduler::scheduler_run_heartbeat_now,
            scheduler::scheduler_list_runs,
            scheduler::scheduler_get_run,
            // Petool 账户命令
            petool_account::petool_login,
            petool_account::petool_register,
            petool_account::petool_logout,
            petool_account::petool_is_logged_in,
            petool_account::petool_get_profile,
            petool_account::petool_get_quota,
            petool_account::petool_get_usage,
            petool_account::petool_get_orders,
            petool_account::petool_create_order,
            petool_account::petool_query_order,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
