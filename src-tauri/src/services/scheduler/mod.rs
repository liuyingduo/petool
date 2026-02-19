use std::sync::{Arc, OnceLock};

use sqlx::SqlitePool;
use tauri::AppHandle;

use crate::commands::mcp::McpState;
use crate::commands::skills::SkillManagerState;
use crate::state::AppState;

pub mod executor;
pub mod manager;
pub mod models;
pub mod schedule;
pub mod store;

use executor::SchedulerExecutionContext;
use manager::SchedulerManager;

static SCHEDULER_MANAGER: OnceLock<Arc<SchedulerManager>> = OnceLock::new();

pub fn scheduler_manager() -> Option<Arc<SchedulerManager>> {
    SCHEDULER_MANAGER.get().cloned()
}

pub fn initialize_scheduler(
    app_handle: AppHandle,
    pool: SqlitePool,
    app_state: AppState,
    mcp_state: McpState,
    skill_state: SkillManagerState,
) -> Result<Arc<SchedulerManager>, String> {
    if let Some(existing) = SCHEDULER_MANAGER.get() {
        return Ok(existing.clone());
    }

    let manager = SchedulerManager::new(
        app_handle,
        pool,
        SchedulerExecutionContext {
            app_state,
            mcp_state,
            skill_state,
        },
    );
    manager.start();
    let _ = SCHEDULER_MANAGER.set(manager.clone());
    Ok(manager)
}
