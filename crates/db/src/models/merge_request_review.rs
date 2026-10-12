//! Human feedback on the merge requests the orchestrator opens.
//!
//! The comments are already fetched — [`poll_mr_comments`] reads them every
//! five minutes and hands them to the implementing agent — and until now the
//! content was discarded the moment it had been delivered. What survived was a
//! timestamp and a login, which is enough to know that feedback happened and
//! nothing about what it said.
//!
//! Keeping it turns the project's central claim into something measurable.
//! "Reviewers send back fewer changes" is an assertion without this table and a
//! query with it, and the difference matters most before a capability ships
//! rather than after.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use uuid::Uuid;

/// Which comment surface a note came from. GitLab calls both "notes"; the
/// distinction that matters is whether it is anchored to a line of the diff.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommentKind {
    /// A note on the merge request as a whole.
    General,
    /// A note anchored to a path, and usually a line.
    Review,
}

impl CommentKind {
    fn as_str(self) -> &'static str {
        match self {
            CommentKind::General => "general",
            CommentKind::Review => "review",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeRequestReview {
    pub id: Uuid,
    pub project_id: Uuid,
    pub issue_id: Uuid,
    pub jira_key: Option<String>,
    pub mr_url: String,
    pub mr_number: i64,
    pub comment_kind: String,
    pub comment_id: String,
    pub author: String,
    pub is_bot: bool,
    pub body: String,
    pub path: Option<String>,
    pub line: Option<i64>,
    pub commented_at: DateTime<Utc>,
    pub recorded_at: DateTime<Utc>,
}

/// One comment as the caller has it, before it is given an id and a row.
#[derive(Debug, Clone)]
pub struct NewMergeRequestReview {
    pub project_id: Uuid,
    pub issue_id: Uuid,
    pub jira_key: Option<String>,
    pub mr_url: String,
    pub mr_number: i64,
    pub kind: CommentKind,
    pub comment_id: String,
    pub author: String,
    pub is_bot: bool,
    pub body: String,
    pub path: Option<String>,
    pub line: Option<i64>,
    pub commented_at: DateTime<Utc>,
}

impl MergeRequestReview {
    /// Records one comment, ignoring it if the host's note id is already
    /// stored.
    ///
    /// Idempotence is not a nicety here. Comments are recorded before they are
    /// injected, and a failed injection deliberately leaves the high-water mark
    /// where it was so the same comments are re-fetched next tick — so the
    /// second write of a note is the normal case, not the exceptional one. The
    /// backfill relies on it for the same reason.
    ///
    /// Returns whether a row was actually written, which is what lets the
    /// backfill report how much history it found rather than how much it saw.
    pub async fn record(
        pool: &SqlitePool,
        new: &NewMergeRequestReview,
    ) -> Result<bool, sqlx::Error> {
        let id = Uuid::new_v4();
        let kind = new.kind.as_str();
        let rows = sqlx::query!(
            r#"INSERT INTO merge_request_reviews (
                   id, project_id, issue_id, jira_key, mr_url, mr_number,
                   comment_kind, comment_id, author, is_bot, body, path, line,
                   commented_at
               )
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
               ON CONFLICT(project_id, mr_number, comment_kind, comment_id)
               DO NOTHING"#,
            id,
            new.project_id,
            new.issue_id,
            new.jira_key,
            new.mr_url,
            new.mr_number,
            kind,
            new.comment_id,
            new.author,
            new.is_bot,
            new.body,
            new.path,
            new.line,
            new.commented_at,
        )
        .execute(pool)
        .await?
        .rows_affected();
        Ok(rows > 0)
    }
}
