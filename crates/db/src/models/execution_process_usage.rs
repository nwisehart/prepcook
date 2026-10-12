//! Per-turn token-usage ledger (BUG-9777). One row per execution process,
//! written when the process exits from the billing split the executor parsed
//! out of the turn's final result message. Lives in its own table so the
//! execution_processes queries and generated types stay untouched.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ExecutionProcessUsage {
    pub execution_process_id: Uuid,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cache_read_tokens: Option<i64>,
    pub cache_creation_tokens: Option<i64>,
    pub model: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Per-workspace rollup row for the usage endpoint. Workspace names encode
/// the work ("Review: …", "Knowledge: RCA analysis", or the ticket title),
/// which is the v1 attribution axis.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct WorkspaceUsage {
    pub workspace_id: Uuid,
    pub workspace_name: Option<String>,
    pub branch: String,
    pub turns: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_tokens: i64,
    pub last_activity: DateTime<Utc>,
}

/// Per-model rollup row (verifies the Sonnet-except-planning pinning).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ModelUsage {
    pub model: Option<String>,
    pub turns: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_tokens: i64,
}

impl ExecutionProcessUsage {
    /// Records (or refreshes) a turn's usage. Idempotent per process.
    pub async fn upsert(
        pool: &SqlitePool,
        execution_process_id: Uuid,
        input_tokens: Option<i64>,
        output_tokens: Option<i64>,
        cache_read_tokens: Option<i64>,
        cache_creation_tokens: Option<i64>,
        model: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"INSERT INTO execution_process_usage
                    (execution_process_id, input_tokens, output_tokens,
                     cache_read_tokens, cache_creation_tokens, model)
               VALUES (?, ?, ?, ?, ?, ?)
               ON CONFLICT(execution_process_id) DO UPDATE SET
                    input_tokens = excluded.input_tokens,
                    output_tokens = excluded.output_tokens,
                    cache_read_tokens = excluded.cache_read_tokens,
                    cache_creation_tokens = excluded.cache_creation_tokens,
                    model = excluded.model"#,
            execution_process_id,
            input_tokens,
            output_tokens,
            cache_read_tokens,
            cache_creation_tokens,
            model
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Usage summed per workspace, heaviest first. `since` bounds by the
    /// process start time; `None` = all time.
    pub async fn workspace_rollup(
        pool: &SqlitePool,
        since: Option<DateTime<Utc>>,
    ) -> Result<Vec<WorkspaceUsage>, sqlx::Error> {
        sqlx::query_as!(
            WorkspaceUsage,
            r#"SELECT
                    w.id as "workspace_id!: Uuid",
                    w.name as "workspace_name?: String",
                    w.branch as "branch!: String",
                    COUNT(u.execution_process_id) as "turns!: i64",
                    COALESCE(SUM(u.input_tokens), 0) as "input_tokens!: i64",
                    COALESCE(SUM(u.output_tokens), 0) as "output_tokens!: i64",
                    COALESCE(SUM(u.cache_read_tokens), 0) as "cache_read_tokens!: i64",
                    COALESCE(SUM(u.cache_creation_tokens), 0) as "cache_creation_tokens!: i64",
                    MAX(ep.started_at) as "last_activity!: DateTime<Utc>"
               FROM execution_process_usage u
               JOIN execution_processes ep ON ep.id = u.execution_process_id
               JOIN sessions s ON s.id = ep.session_id
               JOIN workspaces w ON w.id = s.workspace_id
              WHERE (?1 IS NULL OR ep.started_at >= ?1)
              GROUP BY w.id, w.name, w.branch
              ORDER BY (COALESCE(SUM(u.input_tokens), 0) + COALESCE(SUM(u.output_tokens), 0)
                        + COALESCE(SUM(u.cache_creation_tokens), 0)) DESC"#,
            since
        )
        .fetch_all(pool)
        .await
    }

    /// Usage summed per model over the same window.
    pub async fn model_rollup(
        pool: &SqlitePool,
        since: Option<DateTime<Utc>>,
    ) -> Result<Vec<ModelUsage>, sqlx::Error> {
        sqlx::query_as!(
            ModelUsage,
            r#"SELECT
                    u.model as "model?: String",
                    COUNT(u.execution_process_id) as "turns!: i64",
                    COALESCE(SUM(u.input_tokens), 0) as "input_tokens!: i64",
                    COALESCE(SUM(u.output_tokens), 0) as "output_tokens!: i64",
                    COALESCE(SUM(u.cache_read_tokens), 0) as "cache_read_tokens!: i64",
                    COALESCE(SUM(u.cache_creation_tokens), 0) as "cache_creation_tokens!: i64"
               FROM execution_process_usage u
               JOIN execution_processes ep ON ep.id = u.execution_process_id
              WHERE (?1 IS NULL OR ep.started_at >= ?1)
              GROUP BY u.model
              ORDER BY (COALESCE(SUM(u.input_tokens), 0) + COALESCE(SUM(u.output_tokens), 0)) DESC"#,
            since
        )
        .fetch_all(pool)
        .await
    }
}
