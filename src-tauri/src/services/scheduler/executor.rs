use crate::commands::chat::{
    run_agent_turn_background, BackgroundAgentRunRequest, BackgroundAgentRunResult,
};
use crate::commands::mcp::McpState;
use crate::commands::skills::SkillManagerState;
use std::sync::Arc;
use super::manager::SchedulerManager;
use crate::state::AppState;
use chrono::Utc;
use serde_json::json;
use sqlx::SqlitePool;
use uuid::Uuid;

use super::models::{SchedulerJob, SchedulerRunSource, SchedulerRunStatus, SchedulerSessionTarget};

#[derive(Clone)]
pub struct SchedulerExecutionContext {
    pub app_state: AppState,
    pub mcp_state: McpState,
    pub skill_state: SkillManagerState,
}

#[derive(Debug, Clone)]
pub struct SchedulerExecutionResult {
    pub status: SchedulerRunStatus,
    pub error: Option<String>,
    pub summary: Option<String>,
    pub output_text: Option<String>,
    pub detail_json: serde_json::Value,
}

fn summarize_text(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return "(empty)".to_string();
    }
    if let Some(line) = trimmed.lines().find(|line| !line.trim().is_empty()) {
        let line = line.trim();
        if line.chars().count() > 220 {
            return line.chars().take(220).collect::<String>();
        }
        return line.to_string();
    }
    trimmed.chars().take(220).collect::<String>()
}

async fn insert_summary_message(
    pool: &SqlitePool,
    conversation_id: &str,
    summary: &str,
) -> Result<(), String> {
    let message_id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO messages (id, conversation_id, role, content, created_at, tool_calls, reasoning) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&message_id)
    .bind(conversation_id)
    .bind("assistant")
    .bind(summary)
    .bind(&now)
    .bind(Option::<String>::None)
    .bind(Option::<String>::None)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    sqlx::query("UPDATE conversations SET updated_at = ? WHERE id = ?")
        .bind(&now)
        .bind(conversation_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

pub async fn execute_scheduler_job(
    manager: Arc<SchedulerManager>,
    source: SchedulerRunSource,
    job: SchedulerJob,
) -> SchedulerExecutionResult {
    let persist_main_context = matches!(job.session_target, SchedulerSessionTarget::Main)
        || matches!(source, SchedulerRunSource::Heartbeat);

    let (app_state, mcp_state, skill_state) = {
        (
            manager.ctx.app_state.clone(),
            manager.ctx.mcp_state.clone(),
            manager.ctx.skill_state.clone(),
        )
    };

    let run_result = run_agent_turn_background(
        app_state.clone(),
        mcp_state,
        skill_state,
        BackgroundAgentRunRequest {
            target_conversation_id: job.target_conversation_id.clone(),
            content: job.message.clone(),
            workspace_directory: job.workspace_directory.clone(),
            model_override: job.model_override.clone(),
            persist_main_context,
            tool_whitelist: Some(job.tool_whitelist.iter().cloned().collect()),
        },
    )
    .await;

    match run_result {
        Ok(BackgroundAgentRunResult {
            content,
            reasoning,
            rounds,
            tool_calls,
            blocked_tools,
            guard_stopped,
        }) => {
            let summary = summarize_text(&content);
            if matches!(job.session_target, SchedulerSessionTarget::Isolated)
                && !matches!(source, SchedulerRunSource::Heartbeat)
            {
                let pool = {
                    let guard = app_state.lock().await;
                    guard.db().pool().clone()
                };
                let summary_text = format!("[Scheduled isolated run] {}", summary);
                if let Err(error) =
                    insert_summary_message(&pool, &job.target_conversation_id, &summary_text).await
                {
                    return SchedulerExecutionResult {
                        status: SchedulerRunStatus::Error,
                        error: Some(error),
                        summary: Some(summary),
                        output_text: Some(content),
                        detail_json: json!({
                            "reasoning": reasoning,
                            "rounds": rounds,
                            "toolCalls": tool_calls,
                            "blockedTools": blocked_tools,
                            "guardStopped": guard_stopped,
                            "failedToWriteSummary": true
                        }),
                    };
                }
            }

            SchedulerExecutionResult {
                status: SchedulerRunStatus::Ok,
                error: None,
                summary: Some(summary),
                output_text: Some(content),
                detail_json: json!({
                    "reasoning": reasoning,
                    "rounds": rounds,
                    "toolCalls": tool_calls,
                    "blockedTools": blocked_tools,
                    "guardStopped": guard_stopped,
                    "sessionTarget": job.session_target.as_str(),
                    "source": source.as_str()
                }),
            }
        }
        Err(error) => SchedulerExecutionResult {
            status: SchedulerRunStatus::Error,
            error: Some(error.clone()),
            summary: None,
            output_text: None,
            detail_json: json!({
                "error": error,
                "sessionTarget": job.session_target.as_str(),
                "source": source.as_str()
            }),
        },
    }
}
