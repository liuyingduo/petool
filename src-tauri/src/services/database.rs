use anyhow::Result;
use sqlx::{sqlite::SqliteConnectOptions, SqlitePool};
use std::str::FromStr;

pub struct Database {
    pool: SqlitePool,
}

impl Database {
    pub async fn new(db_path: std::path::PathBuf) -> Result<Self> {
        // Ensure directory exists
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let options = SqliteConnectOptions::from_str(&format!("sqlite:{}", db_path.display()))?
            .create_if_missing(true);

        let pool = SqlitePool::connect_with(options).await?;

        // Run migrations
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS conversations (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                model TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                conversation_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                created_at TEXT NOT NULL,
                tool_calls TEXT,
                reasoning TEXT,
                FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS skills (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                version TEXT NOT NULL,
                enabled INTEGER DEFAULT 1,
                installed_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS message_events (
                id TEXT PRIMARY KEY,
                conversation_id TEXT NOT NULL,
                turn_id TEXT NOT NULL,
                seq INTEGER NOT NULL,
                event_type TEXT NOT NULL,
                tool_call_id TEXT,
                payload TEXT NOT NULL,
                created_at TEXT NOT NULL,
                FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS scheduler_jobs (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT,
                enabled INTEGER NOT NULL,
                schedule_kind TEXT NOT NULL,
                schedule_at TEXT,
                every_ms INTEGER,
                cron_expr TEXT,
                timezone TEXT,
                session_target TEXT NOT NULL,
                target_conversation_id TEXT NOT NULL,
                message TEXT NOT NULL,
                model_override TEXT,
                workspace_directory TEXT,
                tool_whitelist TEXT NOT NULL,
                run_timeout_seconds INTEGER NOT NULL,
                delete_after_run INTEGER NOT NULL,
                next_run_at TEXT,
                running_at TEXT,
                last_run_at TEXT,
                last_status TEXT,
                last_error TEXT,
                last_duration_ms INTEGER,
                consecutive_errors INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS scheduler_runs (
                id TEXT PRIMARY KEY,
                source TEXT NOT NULL,
                job_id TEXT,
                job_name_snapshot TEXT NOT NULL,
                target_conversation_id TEXT NOT NULL,
                session_target TEXT NOT NULL,
                triggered_at TEXT NOT NULL,
                started_at TEXT NOT NULL,
                ended_at TEXT NOT NULL,
                status TEXT NOT NULL,
                error TEXT,
                summary TEXT,
                output_text TEXT,
                detail_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                FOREIGN KEY (job_id) REFERENCES scheduler_jobs(id) ON DELETE SET NULL
            );

            CREATE INDEX IF NOT EXISTS idx_messages_conversation ON messages(conversation_id);
            CREATE INDEX IF NOT EXISTS idx_message_events_conversation_turn_seq ON message_events(conversation_id, turn_id, seq);
            CREATE INDEX IF NOT EXISTS idx_message_events_conversation_created ON message_events(conversation_id, created_at);
            CREATE INDEX IF NOT EXISTS idx_scheduler_jobs_enabled_next_run ON scheduler_jobs(enabled, next_run_at);
            CREATE INDEX IF NOT EXISTS idx_scheduler_runs_job_created ON scheduler_runs(job_id, created_at);
            CREATE INDEX IF NOT EXISTS idx_scheduler_runs_source_created ON scheduler_runs(source, created_at);
            "#,
        )
        .execute(&pool)
        .await?;

        let has_reasoning_column = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM pragma_table_info('messages') WHERE name = 'reasoning'",
        )
        .fetch_one(&pool)
        .await?
            > 0;

        if !has_reasoning_column {
            sqlx::query("ALTER TABLE messages ADD COLUMN reasoning TEXT")
                .execute(&pool)
                .await?;
        }

        Ok(Self { pool })
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}
