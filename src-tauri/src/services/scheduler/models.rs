use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SchedulerScheduleKind {
    At,
    Every,
    Cron,
}

impl SchedulerScheduleKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::At => "at",
            Self::Every => "every",
            Self::Cron => "cron",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "at" => Some(Self::At),
            "every" => Some(Self::Every),
            "cron" => Some(Self::Cron),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SchedulerSessionTarget {
    Main,
    Isolated,
    Heartbeat,
}

impl SchedulerSessionTarget {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::Isolated => "isolated",
            Self::Heartbeat => "heartbeat",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "main" => Some(Self::Main),
            "isolated" => Some(Self::Isolated),
            "heartbeat" => Some(Self::Heartbeat),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SchedulerRunSource {
    Job,
    Heartbeat,
}

impl SchedulerRunSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Job => "job",
            Self::Heartbeat => "heartbeat",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "job" => Some(Self::Job),
            "heartbeat" => Some(Self::Heartbeat),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SchedulerRunStatus {
    Ok,
    Error,
    Skipped,
}

impl SchedulerRunStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Error => "error",
            Self::Skipped => "skipped",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "ok" => Some(Self::Ok),
            "error" => Some(Self::Error),
            "skipped" => Some(Self::Skipped),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchedulerJob {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub enabled: bool,
    pub schedule_kind: SchedulerScheduleKind,
    pub schedule_at: Option<String>,
    pub every_ms: Option<i64>,
    pub cron_expr: Option<String>,
    pub timezone: Option<String>,
    pub session_target: SchedulerSessionTarget,
    pub target_conversation_id: String,
    pub message: String,
    pub model_override: Option<String>,
    pub workspace_directory: Option<String>,
    pub tool_whitelist: Vec<String>,
    pub run_timeout_seconds: i64,
    pub delete_after_run: bool,
    pub next_run_at: Option<String>,
    pub running_at: Option<String>,
    pub last_run_at: Option<String>,
    pub last_status: Option<SchedulerRunStatus>,
    pub last_error: Option<String>,
    pub last_duration_ms: Option<i64>,
    pub consecutive_errors: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchedulerRun {
    pub id: String,
    pub source: SchedulerRunSource,
    pub job_id: Option<String>,
    pub job_name_snapshot: String,
    pub target_conversation_id: String,
    pub session_target: SchedulerSessionTarget,
    pub triggered_at: String,
    pub started_at: String,
    pub ended_at: String,
    pub status: SchedulerRunStatus,
    pub error: Option<String>,
    pub summary: Option<String>,
    pub output_text: Option<String>,
    pub detail_json: serde_json::Value,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchedulerStatus {
    pub enabled: bool,
    pub heartbeat_enabled: bool,
    pub running_jobs: usize,
    pub next_wake_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchedulerJobCreateInput {
    pub name: String,
    pub description: Option<String>,
    pub enabled: Option<bool>,
    pub schedule_kind: SchedulerScheduleKind,
    pub schedule_at: Option<String>,
    pub every_ms: Option<i64>,
    pub cron_expr: Option<String>,
    pub timezone: Option<String>,
    pub session_target: SchedulerSessionTarget,
    pub target_conversation_id: String,
    pub message: String,
    pub model_override: Option<String>,
    pub workspace_directory: Option<String>,
    pub tool_whitelist: Option<Vec<String>>,
    pub run_timeout_seconds: Option<i64>,
    pub delete_after_run: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SchedulerJobPatchInput {
    pub name: Option<String>,
    pub description: Option<Option<String>>,
    pub enabled: Option<bool>,
    pub schedule_kind: Option<SchedulerScheduleKind>,
    pub schedule_at: Option<Option<String>>,
    pub every_ms: Option<Option<i64>>,
    pub cron_expr: Option<Option<String>>,
    pub timezone: Option<Option<String>>,
    pub session_target: Option<SchedulerSessionTarget>,
    pub target_conversation_id: Option<String>,
    pub message: Option<String>,
    pub model_override: Option<Option<String>>,
    pub workspace_directory: Option<Option<String>>,
    pub tool_whitelist: Option<Vec<String>>,
    pub run_timeout_seconds: Option<i64>,
    pub delete_after_run: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchedulerRunRequest {
    pub accepted: bool,
    pub reason: Option<String>,
}
