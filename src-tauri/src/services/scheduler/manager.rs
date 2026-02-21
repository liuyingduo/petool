use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde_json::json;
use sqlx::SqlitePool;
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

use crate::models::config::{AutomationConfig, Config};
use crate::utils::{load_config, save_config};

use super::executor::{execute_scheduler_job, SchedulerExecutionContext};
use super::models::{
    SchedulerJob, SchedulerJobCreateInput, SchedulerJobPatchInput, SchedulerRun,
    SchedulerRunRequest, SchedulerRunSource, SchedulerRunStatus, SchedulerScheduleKind,
    SchedulerSessionTarget, SchedulerStatus,
};
use super::schedule::{
    clamp_timeout_seconds, compute_next_after_result, compute_next_run_at, to_rfc3339,
    validate_schedule_fields,
};
use super::store;

fn default_job_tool_whitelist() -> Vec<String> {
    vec![
        "workspace_list_directory".to_string(),
        "workspace_read_file".to_string(),
        "workspace_glob".to_string(),
        "workspace_grep".to_string(),
        "workspace_codesearch".to_string(),
        "workspace_lsp_symbols".to_string(),
        "web_fetch".to_string(),
        "web_search".to_string(),
        "sessions_list".to_string(),
        "sessions_history".to_string(),
        "sessions_send".to_string(),
        "sessions_spawn".to_string(),
        "workspace_write_file".to_string(),
        "workspace_edit_file".to_string(),
        "workspace_apply_patch".to_string(),
    ]
}

#[derive(Clone)]
pub struct SchedulerManager {
    app_handle: AppHandle,
    pool: SqlitePool,
    pub(crate) ctx: SchedulerExecutionContext,
    stop_flag: Arc<AtomicBool>,
    running_jobs: Arc<tokio::sync::Mutex<HashSet<String>>>,
    running_conversations: Arc<tokio::sync::Mutex<HashSet<String>>>,
    heartbeat_last_run: Arc<tokio::sync::Mutex<Option<DateTime<Utc>>>>,
}

impl SchedulerManager {
    pub fn new(
        app_handle: AppHandle,
        pool: SqlitePool,
        ctx: SchedulerExecutionContext,
    ) -> Arc<Self> {
        Arc::new(Self {
            app_handle,
            pool,
            ctx,
            stop_flag: Arc::new(AtomicBool::new(false)),
            running_jobs: Arc::new(tokio::sync::Mutex::new(HashSet::new())),
            running_conversations: Arc::new(tokio::sync::Mutex::new(HashSet::new())),
            heartbeat_last_run: Arc::new(tokio::sync::Mutex::new(None)),
        })
    }

    pub fn start(self: &Arc<Self>) {
        let manager = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            manager.run_loop().await;
        });
    }

    async fn run_loop(self: Arc<Self>) {
        if let Err(error) = self.bootstrap_jobs().await {
            eprintln!("scheduler bootstrap failed: {}", error);
        }

        loop {
            if self.stop_flag.load(Ordering::Relaxed) {
                break;
            }

            if let Err(error) = self.clone().tick_once().await {
                eprintln!("scheduler tick failed: {}", error);
            }

            tokio::time::sleep(Duration::from_millis(1000)).await;
        }
    }

    async fn bootstrap_jobs(&self) -> Result<(), String> {
        let now = Utc::now();
        let jobs = store::list_jobs(&self.pool, true).await?;

        for mut job in jobs {
            let mut changed = false;

            if job.running_at.is_some() {
                job.running_at = None;
                changed = true;
            }

            if job.enabled {
                if matches!(job.schedule_kind, SchedulerScheduleKind::At) {
                    let next = compute_next_run_at(&job, now);
                    if next.is_none() {
                        job.enabled = false;
                        job.next_run_at = None;
                        job.last_status = Some(SchedulerRunStatus::Skipped);
                        job.last_error = Some("missed_at_on_startup".to_string());
                        changed = true;
                    } else {
                        let next_rfc3339 = to_rfc3339(next);
                        if job.next_run_at != next_rfc3339 {
                            job.next_run_at = next_rfc3339;
                            changed = true;
                        }
                    }
                } else {
                    let next_rfc3339 = to_rfc3339(compute_next_run_at(&job, now));
                    if job.next_run_at != next_rfc3339 {
                        job.next_run_at = next_rfc3339;
                        changed = true;
                    }
                }
            }

            if changed {
                job.updated_at = now.to_rfc3339();
                store::update_job(&self.pool, &job).await?;
            }
        }

        let mut heartbeat_last_run = self.heartbeat_last_run.lock().await;
        if heartbeat_last_run.is_none() {
            *heartbeat_last_run = Some(now);
        }

        Ok(())
    }

    async fn tick_once(self: Arc<Self>) -> Result<(), String> {
        let config = load_config::<Config>().map_err(|e| e.to_string())?;
        if !config.automation.enabled {
            return Ok(());
        }

        self.ensure_heartbeat_target(&config).await?;
        let manager = self.clone();
        manager.poll_due_jobs(&config.automation).await?;
        let manager = self.clone();
        manager.poll_heartbeat(&config).await?;
        self.emit_status().await?;
        Ok(())
    }

    async fn emit_status(&self) -> Result<(), String> {
        let payload = self.get_status().await?;
        let _ = self.app_handle.emit("scheduler-status", payload);
        Ok(())
    }

    async fn poll_due_jobs(self: Arc<Self>, automation: &AutomationConfig) -> Result<(), String> {
        let max_concurrent = automation.max_concurrent_runs.max(1) as usize;
        let running_count = self.running_jobs.lock().await.len();
        if running_count >= max_concurrent {
            return Ok(());
        }

        let now = Utc::now();
        let limit = (max_concurrent - running_count) as i64;
        let due_jobs = store::list_due_jobs(&self.pool, &now.to_rfc3339(), limit).await?;

        for job in due_jobs {
            let manager = self.clone();
            manager.start_job_execution(job, SchedulerRunSource::Job, now.to_rfc3339())
                .await?;
        }

        Ok(())
    }

    async fn poll_heartbeat(self: Arc<Self>, config: &Config) -> Result<(), String> {
        if !config.automation.heartbeat.enabled {
            return Ok(());
        }

        let interval = config.automation.heartbeat.every_minutes.max(1) as i64;
        let now = Utc::now();
        {
            let last_run = self.heartbeat_last_run.lock().await;
            if let Some(last_run) = *last_run {
                if now.signed_duration_since(last_run).num_minutes() < interval {
                    return Ok(());
                }
            }
        }

        let target_conversation_id = config
            .automation
            .heartbeat
            .target_conversation_id
            .clone()
            .ok_or_else(|| "Heartbeat target conversation is not configured".to_string())?;

        let heartbeat_job = SchedulerJob {
            id: format!("heartbeat-{}", Uuid::new_v4()),
            name: "Heartbeat".to_string(),
            description: Some("Periodic heartbeat task".to_string()),
            enabled: true,
            schedule_kind: SchedulerScheduleKind::Every,
            schedule_at: None,
            every_ms: Some(interval * 60_000),
            cron_expr: None,
            timezone: None,
            session_target: SchedulerSessionTarget::Heartbeat,
            target_conversation_id,
            message: config.automation.heartbeat.prompt.clone(),
            model_override: config.automation.heartbeat.model.clone(),
            workspace_directory: config.automation.heartbeat.workspace_directory.clone(),
            tool_whitelist: if config.automation.heartbeat.tool_whitelist.is_empty() {
                default_job_tool_whitelist()
            } else {
                config.automation.heartbeat.tool_whitelist.clone()
            },
            run_timeout_seconds: 600,
            delete_after_run: false,
            next_run_at: None,
            running_at: None,
            last_run_at: None,
            last_status: None,
            last_error: None,
            last_duration_ms: None,
            consecutive_errors: 0,
            created_at: now.to_rfc3339(),
            updated_at: now.to_rfc3339(),
        };

        let manager = self.clone();
        manager.execute_heartbeat_job(heartbeat_job, now.to_rfc3339())
            .await?;

        {
            let mut last_run = self.heartbeat_last_run.lock().await;
            *last_run = Some(now);
        }
        Ok(())
    }

    async fn execute_heartbeat_job(
        self: Arc<Self>,
        heartbeat_job: SchedulerJob,
        triggered_at: String,
    ) -> Result<(), String> {
        let source = SchedulerRunSource::Heartbeat;
        let job_id = heartbeat_job.id.clone();

        {
            let mut running_jobs = self.running_jobs.lock().await;
            if running_jobs.contains(&job_id) {
                return Ok(());
            }
            running_jobs.insert(job_id.clone());
        }

        if let Err(error) =
            store::set_job_running(&self.pool, &job_id, Some(&triggered_at)).await
        {
            {
                let mut running_jobs = self.running_jobs.lock().await;
                running_jobs.remove(&job_id);
            }
            return Err(error);
        }

        let manager = self.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(error) = manager.run_job_execution(heartbeat_job, source, triggered_at).await {
                eprintln!("scheduler heartbeat task failed: {}", error);
            }
        });

        Ok(())
    }

    async fn start_job_execution(
        self: Arc<Self>,
        job: SchedulerJob,
        source: SchedulerRunSource,
        triggered_at: String,
    ) -> Result<SchedulerRunRequest, String> {
        let job_id = job.id.clone();
        {
            let mut running_jobs = self.running_jobs.lock().await;
            if running_jobs.contains(&job_id) {
                return Ok(SchedulerRunRequest {
                    accepted: false,
                    reason: Some("already_running".to_string()),
                });
            }
            running_jobs.insert(job_id.clone());
        }

        if let Err(error) =
            store::set_job_running(&self.pool, &job_id, Some(&Utc::now().to_rfc3339())).await
        {
            {
                let mut running_jobs = self.running_jobs.lock().await;
                running_jobs.remove(&job_id);
            }
            return Err(error);
        }

        let manager = self.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(error) = manager.run_job_execution(job, source, triggered_at).await {
                eprintln!("scheduler job execution failed: {}", error);
            }
        });

        Ok(SchedulerRunRequest {
            accepted: true,
            reason: None,
        })
    }

    pub async fn run_job_execution(
        self: Arc<Self>,
        mut job: SchedulerJob,
        source: SchedulerRunSource,
        triggered_at: String,
    ) -> Result<(), String> {
        let started_at = Utc::now();
        let session_acquired = {
            if job.session_target == SchedulerSessionTarget::Isolated {
                let mut running_conversations = self.running_conversations.lock().await;
                if running_conversations.contains(&job.target_conversation_id) {
                    false
                } else {
                    running_conversations.insert(job.target_conversation_id.clone());
                    true
                }
            } else {
                true
            }
        };

        if !session_acquired {
            let _ = self.running_jobs.lock().await.remove(&job.id);
            if source == SchedulerRunSource::Job {
                let _ = store::set_job_running(&self.pool, &job.id, None).await;
            }

            let run = SchedulerRun {
                id: Uuid::new_v4().to_string(),
                source: source.clone(),
                job_id: if source == SchedulerRunSource::Job { Some(job.id.clone()) } else { None },
                job_name_snapshot: job.name.clone(),
                target_conversation_id: job.target_conversation_id.clone(),
                session_target: job.session_target.clone(),
                triggered_at: triggered_at.clone(),
                started_at: started_at.to_rfc3339(),
                ended_at: Utc::now().to_rfc3339(),
                status: SchedulerRunStatus::Skipped,
                error: Some("session_busy".to_string()),
                summary: Some("session busy".to_string()),
                output_text: None,
                detail_json: json!({ "error": "session_busy" }),
                created_at: Utc::now().to_rfc3339(),
            };
            store::insert_run(&self.pool, &run).await?;
            let _ = self.app_handle.emit("scheduler-run-event", &run);

            return Err("session-busy".to_string());
        }

        let result = execute_scheduler_job(self.clone(), source.clone(), job.clone()).await;
        let ended_at = Utc::now();

        let mut should_delete_job = false;
        if source == SchedulerRunSource::Job {
            if let Some(latest) = store::get_job(&self.pool, &job.id).await? {
                job = latest;
                job.running_at = None;
                job.last_run_at = Some(started_at.to_rfc3339());
                job.last_status = Some(result.status.clone());
                job.last_error = result.error.clone();
                job.last_duration_ms = Some(
                    ended_at
                        .signed_duration_since(started_at)
                        .num_milliseconds(),
                );
                job.updated_at = ended_at.to_rfc3339();

                if result.status == SchedulerRunStatus::Error {
                    job.consecutive_errors += 1;
                } else {
                    job.consecutive_errors = 0;
                }

                if job.schedule_kind == SchedulerScheduleKind::At {
                    if result.status == SchedulerRunStatus::Ok && job.delete_after_run {
                        should_delete_job = true;
                    } else {
                        job.enabled = false;
                        job.next_run_at = None;
                    }
                } else {
                    let next = compute_next_after_result(
                        &job,
                        result.status.clone(),
                        ended_at,
                        job.consecutive_errors,
                    );
                    job.next_run_at = to_rfc3339(next);
                }

                if should_delete_job {
                    let _ = store::delete_job(&self.pool, &job.id).await;
                } else {
                    let _ = store::update_job(&self.pool, &job).await;
                }
            }
        }

        let run = SchedulerRun {
            id: Uuid::new_v4().to_string(),
            source: source.clone(),
            job_id: if source == SchedulerRunSource::Job { Some(job.id.clone()) } else { None },
            job_name_snapshot: job.name.clone(),
            target_conversation_id: job.target_conversation_id.clone(),
            session_target: job.session_target.clone(),
            triggered_at: triggered_at.clone(),
            started_at: started_at.to_rfc3339(),
            ended_at: ended_at.to_rfc3339(),
            status: result.status.clone(),
            error: result.error.clone(),
            summary: result.summary.clone(),
            output_text: result.output_text,
            detail_json: result.detail_json,
            created_at: ended_at.to_rfc3339(),
        };
        store::insert_run(&self.pool, &run).await?;

        let _ = self.app_handle.emit("scheduler-run-event", &run);
        if source == SchedulerRunSource::Job {
            if should_delete_job {
                let _ = self.app_handle.emit(
                    "scheduler-job-event",
                    json!({
                        "event": "removed",
                        "jobId": job.id,
                    }),
                );
            } else if let Some(updated) = store::get_job(&self.pool, &job.id).await? {
                let _ = self.app_handle.emit(
                    "scheduler-job-event",
                    json!({
                        "event": "updated",
                        "job": updated,
                    }),
                );
            }
        }

        if source == SchedulerRunSource::Job {
            let _ = store::set_job_running(&self.pool, &job.id, None).await;
        }
        let _ = self.running_jobs.lock().await.remove(&job.id);
        if job.session_target == SchedulerSessionTarget::Isolated {
            let _ = self
                .running_conversations
                .lock()
                .await
                .remove(&job.target_conversation_id);
        }

        result.error.map(Err).unwrap_or(Ok(()))
    }

    async fn ensure_heartbeat_target(&self, config: &Config) -> Result<(), String> {
        if !config.automation.heartbeat.enabled {
            return Ok(());
        }

        let target = config
            .automation
            .heartbeat
            .target_conversation_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);

        if let Some(conversation_id) = target {
            let exists =
                sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM conversations WHERE id = ?")
                    .bind(&conversation_id)
                    .fetch_one(&self.pool)
                    .await
                    .map_err(|e| e.to_string())?
                    > 0;
            if exists {
                return Ok(());
            }
        }

        let conversation_id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO conversations (id, title, model, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&conversation_id)
        .bind("Heartbeat")
        .bind(&config.model)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|e| e.to_string())?;

        let mut next_config = config.clone();
        next_config.automation.heartbeat.target_conversation_id = Some(conversation_id);
        save_config(&next_config).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub async fn get_status(&self) -> Result<SchedulerStatus, String> {
        let config = load_config::<Config>().map_err(|e| e.to_string())?;
        let next_wake_at = store::next_wake_at(&self.pool).await?;
        Ok(SchedulerStatus {
            enabled: config.automation.enabled,
            heartbeat_enabled: config.automation.heartbeat.enabled,
            running_jobs: self.running_jobs.lock().await.len(),
            next_wake_at,
        })
    }

    pub async fn list_jobs(&self, include_disabled: bool) -> Result<Vec<SchedulerJob>, String> {
        store::list_jobs(&self.pool, include_disabled).await
    }

    pub async fn get_job(&self, job_id: &str) -> Result<Option<SchedulerJob>, String> {
        store::get_job(&self.pool, job_id).await
    }

    pub async fn create_job(&self, input: SchedulerJobCreateInput) -> Result<SchedulerJob, String> {
        let now = Utc::now();
        let is_at_schedule = matches!(input.schedule_kind, SchedulerScheduleKind::At);
        let mut job = SchedulerJob {
            id: Uuid::new_v4().to_string(),
            name: input.name.trim().to_string(),
            description: input
                .description
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
            enabled: input.enabled.unwrap_or(true),
            schedule_kind: input.schedule_kind.clone(),
            schedule_at: input.schedule_at,
            every_ms: input.every_ms,
            cron_expr: input.cron_expr,
            timezone: input
                .timezone
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
            session_target: input.session_target,
            target_conversation_id: input.target_conversation_id.trim().to_string(),
            message: input.message.trim().to_string(),
            model_override: input
                .model_override
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
            workspace_directory: input
                .workspace_directory
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
            tool_whitelist: input
                .tool_whitelist
                .unwrap_or_else(default_job_tool_whitelist),
            run_timeout_seconds: clamp_timeout_seconds(input.run_timeout_seconds),
            delete_after_run: input.delete_after_run.unwrap_or(is_at_schedule),
            next_run_at: None,
            running_at: None,
            last_run_at: None,
            last_status: None,
            last_error: None,
            last_duration_ms: None,
            consecutive_errors: 0,
            created_at: now.to_rfc3339(),
            updated_at: now.to_rfc3339(),
        };

        if job.name.is_empty() {
            return Err("name is required".to_string());
        }
        if job.target_conversation_id.is_empty() {
            return Err("target_conversation_id is required".to_string());
        }
        if job.message.is_empty() {
            return Err("message is required".to_string());
        }
        validate_schedule_fields(&job)?;

        job.next_run_at = to_rfc3339(compute_next_run_at(&job, now));
        if job.next_run_at.is_none() && matches!(job.schedule_kind, SchedulerScheduleKind::At) {
            job.enabled = false;
        }

        store::insert_job(&self.pool, &job).await?;
        let _ = self.app_handle.emit(
            "scheduler-job-event",
            json!({ "event": "added", "job": &job }),
        );
        Ok(job)
    }

    pub async fn update_job(
        &self,
        job_id: &str,
        patch: SchedulerJobPatchInput,
    ) -> Result<SchedulerJob, String> {
        let mut job = store::get_job(&self.pool, job_id)
            .await?
            .ok_or_else(|| format!("Job not found: {}", job_id))?;

        if let Some(name) = patch
            .name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            job.name = name.to_string();
        }
        if let Some(description) = patch.description {
            job.description = description
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
        }
        if let Some(enabled) = patch.enabled {
            job.enabled = enabled;
        }
        if let Some(schedule_kind) = patch.schedule_kind {
            job.schedule_kind = schedule_kind;
        }
        if let Some(schedule_at) = patch.schedule_at {
            job.schedule_at = schedule_at
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
        }
        if let Some(every_ms) = patch.every_ms {
            job.every_ms = every_ms;
        }
        if let Some(cron_expr) = patch.cron_expr {
            job.cron_expr = cron_expr
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
        }
        if let Some(timezone) = patch.timezone {
            job.timezone = timezone
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
        }
        if let Some(session_target) = patch.session_target {
            job.session_target = session_target;
        }
        if let Some(target) = patch
            .target_conversation_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            job.target_conversation_id = target.to_string();
        }
        if let Some(message) = patch
            .message
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            job.message = message.to_string();
        }
        if let Some(model_override) = patch.model_override {
            job.model_override = model_override
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
        }
        if let Some(workspace_directory) = patch.workspace_directory {
            job.workspace_directory = workspace_directory
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
        }
        if let Some(tool_whitelist) = patch.tool_whitelist {
            job.tool_whitelist = tool_whitelist;
        }
        if patch.run_timeout_seconds.is_some() {
            job.run_timeout_seconds = clamp_timeout_seconds(patch.run_timeout_seconds);
        }
        if let Some(delete_after_run) = patch.delete_after_run {
            job.delete_after_run = delete_after_run;
        }

        validate_schedule_fields(&job)?;

        let now = Utc::now();
        if job.running_at.is_none() {
            job.next_run_at = to_rfc3339(compute_next_run_at(&job, now));
        }
        job.updated_at = now.to_rfc3339();

        store::update_job(&self.pool, &job).await?;
        let _ = self.app_handle.emit(
            "scheduler-job-event",
            json!({ "event": "updated", "job": &job }),
        );
        Ok(job)
    }

    pub async fn delete_job(&self, job_id: &str) -> Result<bool, String> {
        let removed = store::delete_job(&self.pool, job_id).await?;
        if removed {
            let _ = self.app_handle.emit(
                "scheduler-job-event",
                json!({ "event": "removed", "jobId": job_id }),
            );
        }
        Ok(removed)
    }

    pub async fn run_job_now(self: Arc<Self>, job_id: &str) -> Result<SchedulerRunRequest, String> {
        let job = store::get_job(&self.pool, job_id)
            .await?
            .ok_or_else(|| format!("Job not found: {}", job_id))?;
        self.start_job_execution(job, SchedulerRunSource::Job, Utc::now().to_rfc3339())
            .await
    }

    pub async fn list_runs(
        &self,
        job_id: Option<&str>,
        limit: i64,
    ) -> Result<Vec<SchedulerRun>, String> {
        store::list_runs(&self.pool, job_id, limit.clamp(1, 1000)).await
    }

    pub async fn get_run(&self, run_id: &str) -> Result<Option<SchedulerRun>, String> {
        store::get_run(&self.pool, run_id).await
    }

    pub async fn run_heartbeat_now(self: Arc<Self>) -> Result<SchedulerRunRequest, String> {
        let config = load_config::<Config>().map_err(|e| e.to_string())?;
        if !config.automation.enabled || !config.automation.heartbeat.enabled {
            return Ok(SchedulerRunRequest {
                accepted: false,
                reason: Some("heartbeat_disabled".to_string()),
            });
        }

        self.ensure_heartbeat_target(&config).await?;

        let target_conversation_id = config
            .automation
            .heartbeat
            .target_conversation_id
            .clone()
            .ok_or_else(|| "Heartbeat target conversation is not configured".to_string())?;
        let now = Utc::now();
        let heartbeat_job = SchedulerJob {
            id: format!("heartbeat-{}", Uuid::new_v4()),
            name: "Heartbeat".to_string(),
            description: Some("Periodic heartbeat task".to_string()),
            enabled: true,
            schedule_kind: SchedulerScheduleKind::Every,
            schedule_at: None,
            every_ms: Some((config.automation.heartbeat.every_minutes.max(1) as i64) * 60_000),
            cron_expr: None,
            timezone: None,
            session_target: SchedulerSessionTarget::Heartbeat,
            target_conversation_id,
            message: config.automation.heartbeat.prompt.clone(),
            model_override: config.automation.heartbeat.model.clone(),
            workspace_directory: config.automation.heartbeat.workspace_directory.clone(),
            tool_whitelist: if config.automation.heartbeat.tool_whitelist.is_empty() {
                default_job_tool_whitelist()
            } else {
                config.automation.heartbeat.tool_whitelist.clone()
            },
            run_timeout_seconds: 600,
            delete_after_run: false,
            next_run_at: None,
            running_at: None,
            last_run_at: None,
            last_status: None,
            last_error: None,
            last_duration_ms: None,
            consecutive_errors: 0,
            created_at: now.to_rfc3339(),
            updated_at: now.to_rfc3339(),
        };

        let manager = self.clone();
        manager.execute_heartbeat_job(heartbeat_job, now.to_rfc3339())
            .await?;
        let mut last_run = self.heartbeat_last_run.lock().await;
        *last_run = Some(now);
        Ok(SchedulerRunRequest {
            accepted: true,
            reason: None,
        })
    }
}
