use serde_json::Value;
use sqlx::{Row, SqlitePool};

use super::models::{
    SchedulerJob, SchedulerRun, SchedulerRunSource, SchedulerRunStatus, SchedulerScheduleKind,
    SchedulerSessionTarget,
};

fn bool_to_i64(value: bool) -> i64 {
    if value {
        1
    } else {
        0
    }
}

fn parse_whitelist(raw: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(raw).unwrap_or_default()
}

fn encode_whitelist(values: &[String]) -> String {
    serde_json::to_string(values).unwrap_or_else(|_| "[]".to_string())
}

fn row_to_job(row: &sqlx::sqlite::SqliteRow) -> Result<SchedulerJob, String> {
    let last_status = row
        .get::<Option<String>, _>("last_status")
        .as_deref()
        .and_then(SchedulerRunStatus::from_str);

    let schedule_kind = SchedulerScheduleKind::from_str(&row.get::<String, _>("schedule_kind"))
        .ok_or_else(|| "Invalid schedule kind in scheduler_jobs".to_string())?;

    let session_target = SchedulerSessionTarget::from_str(&row.get::<String, _>("session_target"))
        .ok_or_else(|| "Invalid session target in scheduler_jobs".to_string())?;

    Ok(SchedulerJob {
        id: row.get("id"),
        name: row.get("name"),
        description: row.get("description"),
        enabled: row.get::<i64, _>("enabled") != 0,
        schedule_kind,
        schedule_at: row.get("schedule_at"),
        every_ms: row.get("every_ms"),
        cron_expr: row.get("cron_expr"),
        timezone: row.get("timezone"),
        session_target,
        target_conversation_id: row.get("target_conversation_id"),
        message: row.get("message"),
        model_override: row.get("model_override"),
        workspace_directory: row.get("workspace_directory"),
        tool_whitelist: parse_whitelist(&row.get::<String, _>("tool_whitelist")),
        run_timeout_seconds: row.get("run_timeout_seconds"),
        delete_after_run: row.get::<i64, _>("delete_after_run") != 0,
        next_run_at: row.get("next_run_at"),
        running_at: row.get("running_at"),
        last_run_at: row.get("last_run_at"),
        last_status,
        last_error: row.get("last_error"),
        last_duration_ms: row.get("last_duration_ms"),
        consecutive_errors: row.get("consecutive_errors"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

fn row_to_run(row: &sqlx::sqlite::SqliteRow) -> Result<SchedulerRun, String> {
    let source = SchedulerRunSource::from_str(&row.get::<String, _>("source"))
        .ok_or_else(|| "Invalid source in scheduler_runs".to_string())?;
    let session_target = SchedulerSessionTarget::from_str(&row.get::<String, _>("session_target"))
        .ok_or_else(|| "Invalid session target in scheduler_runs".to_string())?;
    let status = SchedulerRunStatus::from_str(&row.get::<String, _>("status"))
        .ok_or_else(|| "Invalid status in scheduler_runs".to_string())?;
    let detail_json = {
        let raw = row.get::<String, _>("detail_json");
        serde_json::from_str::<Value>(&raw).unwrap_or_else(|_| serde_json::json!({}))
    };

    Ok(SchedulerRun {
        id: row.get("id"),
        source,
        job_id: row.get("job_id"),
        job_name_snapshot: row.get("job_name_snapshot"),
        target_conversation_id: row.get("target_conversation_id"),
        session_target,
        triggered_at: row.get("triggered_at"),
        started_at: row.get("started_at"),
        ended_at: row.get("ended_at"),
        status,
        error: row.get("error"),
        summary: row.get("summary"),
        output_text: row.get("output_text"),
        detail_json,
        created_at: row.get("created_at"),
    })
}

pub async fn insert_job(pool: &SqlitePool, job: &SchedulerJob) -> Result<(), String> {
    sqlx::query(
        "INSERT INTO scheduler_jobs (
            id, name, description, enabled, schedule_kind, schedule_at, every_ms, cron_expr, timezone,
            session_target, target_conversation_id, message, model_override, workspace_directory,
            tool_whitelist, run_timeout_seconds, delete_after_run, next_run_at, running_at,
            last_run_at, last_status, last_error, last_duration_ms, consecutive_errors,
            created_at, updated_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&job.id)
    .bind(&job.name)
    .bind(&job.description)
    .bind(bool_to_i64(job.enabled))
    .bind(job.schedule_kind.as_str())
    .bind(&job.schedule_at)
    .bind(job.every_ms)
    .bind(&job.cron_expr)
    .bind(&job.timezone)
    .bind(job.session_target.as_str())
    .bind(&job.target_conversation_id)
    .bind(&job.message)
    .bind(&job.model_override)
    .bind(&job.workspace_directory)
    .bind(encode_whitelist(&job.tool_whitelist))
    .bind(job.run_timeout_seconds)
    .bind(bool_to_i64(job.delete_after_run))
    .bind(&job.next_run_at)
    .bind(&job.running_at)
    .bind(&job.last_run_at)
    .bind(job.last_status.as_ref().map(SchedulerRunStatus::as_str))
    .bind(&job.last_error)
    .bind(job.last_duration_ms)
    .bind(job.consecutive_errors)
    .bind(&job.created_at)
    .bind(&job.updated_at)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn update_job(pool: &SqlitePool, job: &SchedulerJob) -> Result<(), String> {
    sqlx::query(
        "UPDATE scheduler_jobs SET
            name = ?,
            description = ?,
            enabled = ?,
            schedule_kind = ?,
            schedule_at = ?,
            every_ms = ?,
            cron_expr = ?,
            timezone = ?,
            session_target = ?,
            target_conversation_id = ?,
            message = ?,
            model_override = ?,
            workspace_directory = ?,
            tool_whitelist = ?,
            run_timeout_seconds = ?,
            delete_after_run = ?,
            next_run_at = ?,
            running_at = ?,
            last_run_at = ?,
            last_status = ?,
            last_error = ?,
            last_duration_ms = ?,
            consecutive_errors = ?,
            updated_at = ?
        WHERE id = ?",
    )
    .bind(&job.name)
    .bind(&job.description)
    .bind(bool_to_i64(job.enabled))
    .bind(job.schedule_kind.as_str())
    .bind(&job.schedule_at)
    .bind(job.every_ms)
    .bind(&job.cron_expr)
    .bind(&job.timezone)
    .bind(job.session_target.as_str())
    .bind(&job.target_conversation_id)
    .bind(&job.message)
    .bind(&job.model_override)
    .bind(&job.workspace_directory)
    .bind(encode_whitelist(&job.tool_whitelist))
    .bind(job.run_timeout_seconds)
    .bind(bool_to_i64(job.delete_after_run))
    .bind(&job.next_run_at)
    .bind(&job.running_at)
    .bind(&job.last_run_at)
    .bind(job.last_status.as_ref().map(SchedulerRunStatus::as_str))
    .bind(&job.last_error)
    .bind(job.last_duration_ms)
    .bind(job.consecutive_errors)
    .bind(&job.updated_at)
    .bind(&job.id)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn get_job(pool: &SqlitePool, job_id: &str) -> Result<Option<SchedulerJob>, String> {
    let row = sqlx::query("SELECT * FROM scheduler_jobs WHERE id = ?")
        .bind(job_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?;

    row.map(|item| row_to_job(&item)).transpose()
}

pub async fn list_jobs(
    pool: &SqlitePool,
    include_disabled: bool,
) -> Result<Vec<SchedulerJob>, String> {
    let rows = if include_disabled {
        sqlx::query("SELECT * FROM scheduler_jobs ORDER BY created_at DESC")
            .fetch_all(pool)
            .await
            .map_err(|e| e.to_string())?
    } else {
        sqlx::query("SELECT * FROM scheduler_jobs WHERE enabled = 1 ORDER BY created_at DESC")
            .fetch_all(pool)
            .await
            .map_err(|e| e.to_string())?
    };

    rows.iter().map(row_to_job).collect()
}

pub async fn list_due_jobs(
    pool: &SqlitePool,
    now_rfc3339: &str,
    limit: i64,
) -> Result<Vec<SchedulerJob>, String> {
    let rows = sqlx::query(
        "SELECT * FROM scheduler_jobs
         WHERE enabled = 1
           AND running_at IS NULL
           AND next_run_at IS NOT NULL
           AND next_run_at <= ?
         ORDER BY next_run_at ASC
         LIMIT ?",
    )
    .bind(now_rfc3339)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    rows.iter().map(row_to_job).collect()
}

pub async fn delete_job(pool: &SqlitePool, job_id: &str) -> Result<bool, String> {
    let result = sqlx::query("DELETE FROM scheduler_jobs WHERE id = ?")
        .bind(job_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(result.rows_affected() > 0)
}

pub async fn insert_run(pool: &SqlitePool, run: &SchedulerRun) -> Result<(), String> {
    sqlx::query(
        "INSERT INTO scheduler_runs (
            id, source, job_id, job_name_snapshot, target_conversation_id, session_target,
            triggered_at, started_at, ended_at, status, error, summary, output_text,
            detail_json, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&run.id)
    .bind(run.source.as_str())
    .bind(&run.job_id)
    .bind(&run.job_name_snapshot)
    .bind(&run.target_conversation_id)
    .bind(run.session_target.as_str())
    .bind(&run.triggered_at)
    .bind(&run.started_at)
    .bind(&run.ended_at)
    .bind(run.status.as_str())
    .bind(&run.error)
    .bind(&run.summary)
    .bind(&run.output_text)
    .bind(serde_json::to_string(&run.detail_json).unwrap_or_else(|_| "{}".to_string()))
    .bind(&run.created_at)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn list_runs(
    pool: &SqlitePool,
    job_id: Option<&str>,
    limit: i64,
) -> Result<Vec<SchedulerRun>, String> {
    let rows = if let Some(job_id) = job_id {
        sqlx::query(
            "SELECT * FROM scheduler_runs WHERE job_id = ? ORDER BY created_at DESC LIMIT ?",
        )
        .bind(job_id)
        .bind(limit)
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?
    } else {
        sqlx::query("SELECT * FROM scheduler_runs ORDER BY created_at DESC LIMIT ?")
            .bind(limit)
            .fetch_all(pool)
            .await
            .map_err(|e| e.to_string())?
    };

    rows.iter().map(row_to_run).collect()
}

pub async fn get_run(pool: &SqlitePool, run_id: &str) -> Result<Option<SchedulerRun>, String> {
    let row = sqlx::query("SELECT * FROM scheduler_runs WHERE id = ?")
        .bind(run_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?;
    row.map(|item| row_to_run(&item)).transpose()
}

pub async fn next_wake_at(pool: &SqlitePool) -> Result<Option<String>, String> {
    sqlx::query_scalar::<_, String>(
        "SELECT next_run_at FROM scheduler_jobs WHERE enabled = 1 AND next_run_at IS NOT NULL ORDER BY next_run_at ASC LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())
}

pub async fn set_job_running(
    pool: &SqlitePool,
    job_id: &str,
    running_at: Option<&str>,
) -> Result<(), String> {
    sqlx::query("UPDATE scheduler_jobs SET running_at = ?, updated_at = ? WHERE id = ?")
        .bind(running_at)
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(job_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}
