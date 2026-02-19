use chrono::{DateTime, Duration, Local, Utc};
use chrono_tz::Tz;
use cron::Schedule;
use std::str::FromStr;

use super::models::{SchedulerJob, SchedulerRunStatus, SchedulerScheduleKind};

pub const DEFAULT_RUN_TIMEOUT_SECONDS: i64 = 600;

pub fn parse_rfc3339(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

pub fn clamp_timeout_seconds(value: Option<i64>) -> i64 {
    value
        .unwrap_or(DEFAULT_RUN_TIMEOUT_SECONDS)
        .clamp(30, 86_400)
}

pub fn validate_schedule_fields(job: &SchedulerJob) -> Result<(), String> {
    match job.schedule_kind {
        SchedulerScheduleKind::At => {
            let at = job
                .schedule_at
                .as_deref()
                .ok_or_else(|| "schedule_at is required for schedule_kind=at".to_string())?;
            if parse_rfc3339(at).is_none() {
                return Err("schedule_at must be RFC3339 timestamp".to_string());
            }
        }
        SchedulerScheduleKind::Every => {
            let every = job
                .every_ms
                .ok_or_else(|| "every_ms is required for schedule_kind=every".to_string())?;
            if every < 1_000 {
                return Err("every_ms must be >= 1000".to_string());
            }
        }
        SchedulerScheduleKind::Cron => {
            let expr = job
                .cron_expr
                .as_deref()
                .ok_or_else(|| "cron_expr is required for schedule_kind=cron".to_string())?;
            Schedule::from_str(expr).map_err(|e| format!("Invalid cron expression: {}", e))?;
            if let Some(tz) = job
                .timezone
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
            {
                Tz::from_str(tz).map_err(|_| format!("Invalid IANA timezone: {}", tz))?;
            }
        }
    }
    Ok(())
}

pub fn compute_next_run_at(job: &SchedulerJob, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    if !job.enabled {
        return None;
    }

    match job.schedule_kind {
        SchedulerScheduleKind::At => {
            let at = job.schedule_at.as_deref().and_then(parse_rfc3339)?;
            if at > now {
                Some(at)
            } else {
                None
            }
        }
        SchedulerScheduleKind::Every => {
            let every_ms = job.every_ms?;
            if every_ms <= 0 {
                return None;
            }
            let interval = Duration::milliseconds(every_ms);
            let anchor = parse_rfc3339(&job.created_at).unwrap_or(now);
            if anchor >= now {
                Some(anchor + interval)
            } else {
                let elapsed_ms = (now - anchor).num_milliseconds().max(0);
                let steps = elapsed_ms / every_ms + 1;
                Some(anchor + Duration::milliseconds(steps * every_ms))
            }
        }
        SchedulerScheduleKind::Cron => {
            let expr = job.cron_expr.as_deref()?;
            let schedule = Schedule::from_str(expr).ok()?;
            if let Some(tz) = job
                .timezone
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
            {
                let tz = Tz::from_str(tz).ok()?;
                let base = now.with_timezone(&tz);
                schedule
                    .after(&base)
                    .next()
                    .map(|dt| dt.with_timezone(&Utc))
            } else {
                let base = now.with_timezone(&Local);
                schedule
                    .after(&base)
                    .next()
                    .map(|dt| dt.with_timezone(&Utc))
            }
        }
    }
}

pub fn error_backoff_duration(consecutive_errors: i64) -> Duration {
    match consecutive_errors {
        i if i <= 1 => Duration::seconds(30),
        2 => Duration::minutes(1),
        3 => Duration::minutes(5),
        4 => Duration::minutes(15),
        _ => Duration::minutes(60),
    }
}

pub fn compute_next_after_result(
    job: &SchedulerJob,
    status: SchedulerRunStatus,
    ended_at: DateTime<Utc>,
    consecutive_errors: i64,
) -> Option<DateTime<Utc>> {
    match job.schedule_kind {
        SchedulerScheduleKind::At => None,
        SchedulerScheduleKind::Every | SchedulerScheduleKind::Cron => {
            if !job.enabled {
                return None;
            }
            let normal_next = compute_next_run_at(job, ended_at);
            if status == SchedulerRunStatus::Error {
                let backoff_next = ended_at + error_backoff_duration(consecutive_errors);
                match normal_next {
                    Some(normal) => Some(normal.max(backoff_next)),
                    None => Some(backoff_next),
                }
            } else {
                normal_next
            }
        }
    }
}

pub fn to_rfc3339(value: Option<DateTime<Utc>>) -> Option<String> {
    value.map(|dt| dt.to_rfc3339())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::scheduler::models::{SchedulerJob, SchedulerSessionTarget};

    fn build_job(kind: SchedulerScheduleKind) -> SchedulerJob {
        SchedulerJob {
            id: "job-1".to_string(),
            name: "test".to_string(),
            description: None,
            enabled: true,
            schedule_kind: kind,
            schedule_at: None,
            every_ms: None,
            cron_expr: None,
            timezone: None,
            session_target: SchedulerSessionTarget::Main,
            target_conversation_id: "conversation-1".to_string(),
            message: "run".to_string(),
            model_override: None,
            workspace_directory: None,
            tool_whitelist: vec![],
            run_timeout_seconds: 600,
            delete_after_run: false,
            next_run_at: None,
            running_at: None,
            last_run_at: None,
            last_status: None,
            last_error: None,
            last_duration_ms: None,
            consecutive_errors: 0,
            created_at: "2026-02-19T00:00:00Z".to_string(),
            updated_at: "2026-02-19T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn at_schedule_past_and_future() {
        let now = parse_rfc3339("2026-02-19T10:00:00Z").expect("valid now");

        let mut future_job = build_job(SchedulerScheduleKind::At);
        future_job.schedule_at = Some("2026-02-19T10:05:00Z".to_string());
        assert_eq!(
            compute_next_run_at(&future_job, now)
                .expect("future next")
                .to_rfc3339(),
            "2026-02-19T10:05:00+00:00"
        );

        let mut past_job = build_job(SchedulerScheduleKind::At);
        past_job.schedule_at = Some("2026-02-19T09:59:00Z".to_string());
        assert!(compute_next_run_at(&past_job, now).is_none());
    }

    #[test]
    fn every_schedule_advances_without_backfill() {
        let now = parse_rfc3339("2026-02-19T10:00:10Z").expect("valid now");
        let mut job = build_job(SchedulerScheduleKind::Every);
        job.created_at = "2026-02-19T10:00:00Z".to_string();
        job.every_ms = Some(5_000);

        let next = compute_next_run_at(&job, now).expect("next run");
        assert_eq!(next.to_rfc3339(), "2026-02-19T10:00:15+00:00");
    }

    #[test]
    fn cron_schedule_with_timezone_returns_future() {
        let now = parse_rfc3339("2026-02-19T10:00:00Z").expect("valid now");
        let mut job = build_job(SchedulerScheduleKind::Cron);
        job.cron_expr = Some("0 * * * * * *".to_string());
        job.timezone = Some("UTC".to_string());
        let next = compute_next_run_at(&job, now).expect("next run");
        assert!(next > now);
    }

    #[test]
    fn backoff_curve_matches_expected_steps() {
        assert_eq!(error_backoff_duration(1), Duration::seconds(30));
        assert_eq!(error_backoff_duration(2), Duration::minutes(1));
        assert_eq!(error_backoff_duration(3), Duration::minutes(5));
        assert_eq!(error_backoff_duration(4), Duration::minutes(15));
        assert_eq!(error_backoff_duration(5), Duration::minutes(60));
    }
}
