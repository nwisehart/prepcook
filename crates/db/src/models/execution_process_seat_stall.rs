//! Sessions the Claude seat's usage limit stopped mid-turn.
//!
//! A turn that ends on a seat limit leaves the session parked: the agent said
//! nothing, decided nothing, and will not continue on its own. Recovering the
//! board means sending each parked session back in once the window reopens,
//! and doing that needs a durable answer to "which ones stopped this way".
//!
//! The executor's log parser is what recognises the limit (it types the turn's
//! error entry), and the container records it here when the process exits.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ExecutionProcessSeatStall {
    pub execution_process_id: Uuid,
    pub detected_at: DateTime<Utc>,
    pub resumed_at: Option<DateTime<Utc>>,
}

/// A session waiting to be sent back in, with what the caller needs to do it.
#[derive(Debug, Clone)]
pub struct PendingStall {
    pub execution_process_id: Uuid,
    pub session_id: Uuid,
    pub workspace_id: Uuid,
}

impl ExecutionProcessSeatStall {
    /// Records that this process's turn ended on a seat limit. Idempotent, so
    /// a re-parse of the same process cannot queue a second resume.
    pub async fn record(pool: &SqlitePool, execution_process_id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"INSERT INTO execution_process_seat_stall (execution_process_id)
               VALUES (?)
               ON CONFLICT(execution_process_id) DO NOTHING"#,
            execution_process_id
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    /// The longest-waiting session that still needs sending back in.
    ///
    /// Only when the stalled process is still the session's most recent one: a
    /// later process means the session moved on under its own steam or a human
    /// pushed it, and resuming would talk over whatever is there now.
    pub async fn oldest_pending(pool: &SqlitePool) -> Result<Option<PendingStall>, sqlx::Error> {
        sqlx::query_as!(
            PendingStall,
            r#"SELECT
                    st.execution_process_id as "execution_process_id!: Uuid",
                    s.id                    as "session_id!: Uuid",
                    s.workspace_id          as "workspace_id!: Uuid"
               FROM execution_process_seat_stall st
               JOIN execution_processes ep ON ep.id = st.execution_process_id
               JOIN sessions s ON s.id = ep.session_id
              WHERE st.resumed_at IS NULL
                AND ep.id = (
                        SELECT inner_ep.id
                          FROM execution_processes inner_ep
                         WHERE inner_ep.session_id = ep.session_id
                         ORDER BY inner_ep.created_at DESC
                         LIMIT 1
                    )
              ORDER BY st.detected_at ASC
              LIMIT 1"#
        )
        .fetch_optional(pool)
        .await
    }

    /// How many sessions are waiting, for the banner's count.
    pub async fn pending_count(pool: &SqlitePool) -> Result<i64, sqlx::Error> {
        let row = sqlx::query!(
            r#"SELECT COUNT(*) as "count!: i64"
                 FROM execution_process_seat_stall st
                 JOIN execution_processes ep ON ep.id = st.execution_process_id
                WHERE st.resumed_at IS NULL
                  AND ep.id = (
                        SELECT inner_ep.id
                          FROM execution_processes inner_ep
                         WHERE inner_ep.session_id = ep.session_id
                         ORDER BY inner_ep.created_at DESC
                         LIMIT 1
                    )"#
        )
        .fetch_one(pool)
        .await?;
        Ok(row.count)
    }

    /// Stamps a stall as handled. Called after the resume turn is launched,
    /// not before: a launch that fails leaves the row pending so the next
    /// sweep tries again.
    pub async fn mark_resumed(
        pool: &SqlitePool,
        execution_process_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"UPDATE execution_process_seat_stall
                  SET resumed_at = CURRENT_TIMESTAMP
                WHERE execution_process_id = ?"#,
            execution_process_id
        )
        .execute(pool)
        .await?;
        Ok(())
    }
}
