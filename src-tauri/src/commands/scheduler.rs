use crate::services::scheduler::models::{
    SchedulerJob, SchedulerJobCreateInput, SchedulerJobPatchInput, SchedulerRun,
    SchedulerRunRequest, SchedulerStatus,
};
use crate::services::scheduler::scheduler_manager;

fn manager() -> Result<std::sync::Arc<crate::services::scheduler::manager::SchedulerManager>, String>
{
    scheduler_manager().ok_or_else(|| "Scheduler is not initialized yet".to_string())
}

#[tauri::command]
pub async fn scheduler_get_status() -> Result<SchedulerStatus, String> {
    manager()?.get_status().await
}

#[tauri::command]
pub async fn scheduler_list_jobs(
    include_disabled: Option<bool>,
) -> Result<Vec<SchedulerJob>, String> {
    manager()?
        .list_jobs(include_disabled.unwrap_or(false))
        .await
}

#[tauri::command]
pub async fn scheduler_get_job(job_id: String) -> Result<Option<SchedulerJob>, String> {
    manager()?.get_job(&job_id).await
}

#[tauri::command]
pub async fn scheduler_create_job(input: SchedulerJobCreateInput) -> Result<SchedulerJob, String> {
    manager()?.create_job(input).await
}

#[tauri::command]
pub async fn scheduler_update_job(
    job_id: String,
    patch: SchedulerJobPatchInput,
) -> Result<SchedulerJob, String> {
    manager()?.update_job(&job_id, patch).await
}

#[tauri::command]
pub async fn scheduler_delete_job(job_id: String) -> Result<bool, String> {
    manager()?.delete_job(&job_id).await
}

#[tauri::command]
pub async fn scheduler_run_job_now(job_id: String) -> Result<SchedulerRunRequest, String> {
    manager()?.run_job_now(&job_id).await
}

#[tauri::command]
pub async fn scheduler_run_heartbeat_now() -> Result<SchedulerRunRequest, String> {
    manager()?.run_heartbeat_now().await
}

#[tauri::command]
pub async fn scheduler_list_runs(
    job_id: Option<String>,
    limit: Option<u32>,
) -> Result<Vec<SchedulerRun>, String> {
    manager()?
        .list_runs(job_id.as_deref(), limit.unwrap_or(50) as i64)
        .await
}

#[tauri::command]
pub async fn scheduler_get_run(run_id: String) -> Result<Option<SchedulerRun>, String> {
    manager()?.get_run(&run_id).await
}
