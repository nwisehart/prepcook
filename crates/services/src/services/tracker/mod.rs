//! The issue tracker seam: where tickets come from and where the
//! orchestrator writes back to.
//!
//! Everything outside this module reaches the tracker through
//! [`Tracker`], obtained from a [`TrackerSource`]. Tracker-specific parsing
//! (Jira's document format, mention syntax, transition lookup) stays in the
//! implementation module; the types here are what every tracker answers in.
//!
//! Only Jira is implemented. GitHub Issues and Linear exist as kinds so a
//! config can name them, and answer every operation with
//! [`TrackerError::NotSupported`].

use std::{collections::BTreeMap, path::Path, sync::Arc};

use async_trait::async_trait;
use serde::Serialize;
use thiserror::Error;
use tokio::sync::RwLock;

use crate::services::config::{Config, TrackerConfig, TrackerKind};

pub mod github_issues;
pub mod jira;
pub mod linear;

#[derive(Debug, Error)]
pub enum TrackerError {
    #[error("{0} is not yet supported as a tracker")]
    NotSupported(TrackerKind),
    #[error("the tracker is not configured: {0}")]
    NotConfigured(String),
    #[error("the tracker rejected the credentials")]
    AuthFailed,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type TrackerResult<T> = Result<T, TrackerError>;

/// One ticket as returned by [`Tracker::search`].
#[derive(Debug, Clone)]
pub struct FetchedIssue {
    pub key: String,
    pub summary: String,
    pub issue_type: String,
    /// The description reduced to plain text.
    pub description_text: String,
    /// The parent's key (an epic for epic children, a story for sub-tasks).
    pub parent_key: Option<String>,
    /// Issue-type name of the parent, when the tracker includes it.
    pub parent_type: Option<String>,
    /// Priority name, e.g. "P0".
    pub priority_name: Option<String>,
    pub created: Option<chrono::DateTime<chrono::FixedOffset>>,
    /// An assignment or edit bumps this, never `created`, which is why
    /// assigned pickup watermarks on it.
    pub updated: Option<chrono::DateTime<chrono::FixedOffset>>,
    /// Assignee's tracker account id, when assigned.
    pub assignee_account_id: Option<String>,
    /// Status category: "new", "indeterminate", or "done".
    pub status_category: Option<String>,
    /// Status name, e.g. "Merged". Carried alongside the category because
    /// the two do not agree about finished work (see
    /// [`crate::services::config::StatusMapping::shipped`]).
    pub status_name: Option<String>,
    /// Labels, lowercased.
    pub labels: Vec<String>,
}

/// The fields read off one ticket by [`Tracker::get_issue`].
#[derive(Debug, Clone)]
pub struct IssueFields {
    pub summary: String,
    pub issue_type: Option<String>,
    pub priority: Option<String>,
    /// `done` once the ticket is resolved. A ticket the orchestrator closed
    /// that is no longer `done` has been reopened by hand.
    pub status_category: Option<String>,
    pub assignee_account_id: Option<String>,
    /// Who filed the ticket. For a customer-reported bug this is the person a
    /// follow-up has to reach.
    pub reporter_account_id: Option<String>,
    /// The status's own name, not just its category: putting a ticket back
    /// exactly where it was needs the name.
    pub status_name: Option<String>,
    pub labels: Vec<String>,
}

impl IssueFields {
    /// Whether the tracker still considers this resolved.
    pub fn is_done(&self) -> bool {
        self.status_category.as_deref() == Some("done")
    }
}

/// One comment reduced to plain text.
#[derive(Debug, Clone)]
pub struct TrackerComment {
    pub author: String,
    /// Lets callers recognise the agent's own comments regardless of display
    /// name.
    pub author_account_id: Option<String>,
    /// Account ids @-mentioned in the body. Matching on the id rather than
    /// the rendered label is what makes "did this comment tag the agent?"
    /// answerable — display names are neither unique nor stable.
    pub mentions: Vec<String>,
    pub created: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub body_text: String,
}

/// A tracker user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackerUser {
    pub account_id: String,
    pub display_name: String,
}

/// The account the orchestrator acts as on the tracker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackerIdentity {
    pub account_id: String,
    pub display_name: String,
}

/// One downloaded attachment (see [`Tracker::download_attachments`]).
#[derive(Debug, Clone, Serialize)]
pub struct TrackerAttachment {
    pub filename: String,
    /// Absolute path the file was written to.
    pub path: String,
    pub size_bytes: u64,
    pub mime_type: Option<String>,
}

/// A ticket to file with [`Tracker::create_issue`].
#[derive(Debug, Clone)]
pub struct NewIssue {
    /// `None` files into the configured project.
    pub project_key: Option<String>,
    pub issue_type: String,
    pub summary: String,
    /// Plain text.
    pub description: String,
    pub priority: String,
    /// Set at creation rather than patched on afterwards: a label is how a
    /// ticket announces what it is to everything that only sees the tracker,
    /// and a ticket that exists for even one poll without it can be imported
    /// as an ordinary one.
    pub labels: Vec<String>,
}

/// One sub-task to create under a parent with [`Tracker::create_subtasks`].
#[derive(Debug, Clone)]
pub struct NewSubtask {
    pub title: String,
    pub description: Option<String>,
}

/// What became of one requested sub-task.
#[derive(Debug, Clone)]
pub struct CreatedSubtask {
    pub title: String,
    /// The new ticket's key; `None` when this one was not created.
    pub key: Option<String>,
}

/// A remote link pinned to a ticket. `id` makes re-pinning idempotent: the
/// same id replaces the link instead of adding a second one.
#[derive(Debug, Clone)]
pub struct RemoteLink<'a> {
    pub id: &'a str,
    pub title: &'a str,
    pub url: &'a str,
}

/// Everything the orchestrator, watcher, routes and agent tools ask of an
/// issue tracker. Object-safe: callers hold an `Arc<dyn Tracker>`.
#[async_trait]
pub trait Tracker: Send + Sync {
    fn kind(&self) -> TrackerKind;

    /// Whether the tracker has what it needs to make calls. Pollers check
    /// this before spending a request finding out.
    fn is_configured(&self) -> bool;

    /// The ticket's web address.
    fn issue_url(&self, key: &str) -> String;

    /// Runs a query in the tracker's own query language (JQL for Jira).
    async fn search(&self, query: &str) -> TrackerResult<Vec<FetchedIssue>>;

    async fn get_issue(&self, key: &str) -> TrackerResult<IssueFields>;

    /// Every comment on the ticket, oldest first.
    async fn comments(&self, key: &str) -> TrackerResult<Vec<TrackerComment>>;

    /// Posts plain text. `[~accountid:<id>]` in the text becomes a mention.
    async fn post_comment(&self, key: &str, text: &str) -> TrackerResult<()>;

    /// Moves the ticket to the status with exactly this name, failing when
    /// that status is not reachable.
    async fn transition(&self, key: &str, status_name: &str) -> TrackerResult<()>;

    /// Moves the ticket to the configured planning status, or failing that,
    /// to any in-progress status.
    async fn transition_to_in_progress(&self, key: &str) -> TrackerResult<()>;

    async fn assignee(&self, key: &str) -> TrackerResult<Option<TrackerUser>>;

    /// Assigns the ticket to [`Tracker::identity`].
    async fn assign_to_self(&self, key: &str) -> TrackerResult<()>;

    async fn set_summary(&self, key: &str, summary: &str) -> TrackerResult<()>;

    /// Records that `blocked` is blocked by `blocker`.
    async fn link_blocks(&self, blocker: &str, blocked: &str) -> TrackerResult<()>;

    async fn set_remote_link(&self, key: &str, link: RemoteLink<'_>) -> TrackerResult<()>;

    /// Adding an existing watcher is a no-op.
    async fn add_watcher(&self, key: &str, account_id: &str) -> TrackerResult<()>;

    /// Returns the new ticket's key.
    async fn create_issue(&self, issue: &NewIssue) -> TrackerResult<String>;

    /// Creates sub-tasks under `parent`, one result per item in order.
    async fn create_subtasks(
        &self,
        parent: &str,
        items: &[NewSubtask],
    ) -> TrackerResult<Vec<CreatedSubtask>>;

    /// Downloads every attachment into `dir` (created if missing).
    async fn download_attachments(
        &self,
        key: &str,
        dir: &Path,
    ) -> TrackerResult<Vec<TrackerAttachment>>;

    /// The account the orchestrator acts as.
    async fn identity(&self) -> TrackerResult<TrackerIdentity>;

    /// People who can be assigned tickets in `project`, the orchestrator's
    /// own account excluded. Backs the approver picker in settings.
    async fn assignable_users(&self, project: &str) -> TrackerResult<Vec<TrackerUser>>;
}

/// Renders comments as a plain-text block for appending to an imported
/// ticket's description. Empty string when there are no comments.
pub fn format_comments(comments: &[TrackerComment]) -> String {
    if comments.is_empty() {
        return String::new();
    }
    let mut out = String::from("\n\n--- Tracker comments ---");
    for comment in comments {
        let when = comment
            .created
            .map(|dt| format!(" · {}", dt.format("%Y-%m-%d %H:%M")))
            .unwrap_or_default();
        out.push_str(&format!(
            "\n\n[{}{}]\n{}",
            comment.author,
            when,
            comment.body_text.trim()
        ));
    }
    out
}

/// Builds the tracker a config describes.
pub fn build(config: &TrackerConfig) -> Arc<dyn Tracker> {
    match config.kind {
        TrackerKind::Jira => Arc::new(jira::JiraTracker::new(config.clone())),
        TrackerKind::GithubIssues => Arc::new(github_issues::GithubIssuesTracker),
        TrackerKind::Linear => Arc::new(linear::LinearTracker),
    }
}

/// The tracker last built, and the settings it was built from.
type Built = Option<(TrackerConfig, Arc<dyn Tracker>)>;

/// Hands out the tracker the live config describes.
///
/// Built once and reused; rebuilt when the `tracker` section of the config
/// changes, the same way the orchestrator re-reads its settings every tick.
/// Anything a tracker caches for its lifetime (the agent's own identity)
/// therefore follows a change of credentials without a restart.
#[derive(Clone)]
pub struct TrackerSource {
    config: Arc<RwLock<Config>>,
    built: Arc<std::sync::Mutex<Built>>,
}

impl TrackerSource {
    pub fn new(config: Arc<RwLock<Config>>) -> Self {
        Self {
            config,
            built: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    /// The tracker for the current config.
    pub async fn current(&self) -> Arc<dyn Tracker> {
        let wanted = self.config.read().await.tracker.clone();
        let mut built = self
            .built
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some((config, tracker)) = built.as_ref()
            && *config == wanted
        {
            return tracker.clone();
        }
        let tracker = build(&wanted);
        *built = Some((wanted, tracker.clone()));
        tracker
    }

    /// The tracker section of the current config.
    pub async fn settings(&self) -> TrackerConfig {
        self.config.read().await.tracker.clone()
    }
}

/// Account id → display name, the shape the settings picker stores.
pub fn users_by_id(users: Vec<TrackerUser>) -> BTreeMap<String, String> {
    users
        .into_iter()
        .map(|user| (user.account_id, user.display_name))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_source_rebuilds_only_when_the_tracker_config_changes() {
        let config = Arc::new(RwLock::new(Config::default()));
        let source = TrackerSource::new(config.clone());

        let first = source.current().await;
        assert_eq!(first.kind(), TrackerKind::Jira);
        assert!(Arc::ptr_eq(&first, &source.current().await));

        // A change elsewhere in the config keeps the same tracker.
        config.write().await.slack_channel = Some("C0TEST".to_string());
        assert!(Arc::ptr_eq(&first, &source.current().await));

        config.write().await.tracker.kind = TrackerKind::Linear;
        let rebuilt = source.current().await;
        assert_eq!(rebuilt.kind(), TrackerKind::Linear);
        assert!(!Arc::ptr_eq(&first, &rebuilt));
    }

    #[tokio::test]
    async fn unsupported_kinds_say_so_on_every_call() {
        for kind in [TrackerKind::GithubIssues, TrackerKind::Linear] {
            let tracker = build(&TrackerConfig {
                kind,
                ..Default::default()
            });
            assert!(!tracker.is_configured());
            assert!(matches!(
                tracker.post_comment("X-1", "hello").await,
                Err(TrackerError::NotSupported(k)) if k == kind
            ));
            assert!(matches!(
                tracker.identity().await,
                Err(TrackerError::NotSupported(k)) if k == kind
            ));
        }
    }

    #[test]
    fn comments_render_oldest_first_with_their_authors() {
        let comment = |author: &str, body: &str| TrackerComment {
            author: author.to_string(),
            author_account_id: None,
            mentions: Vec::new(),
            created: None,
            body_text: body.to_string(),
        };
        assert_eq!(format_comments(&[]), "");
        let block = format_comments(&[comment("Ada", " first "), comment("Lin", "second")]);
        assert_eq!(
            block,
            "\n\n--- Tracker comments ---\n\n[Ada]\nfirst\n\n[Lin]\nsecond"
        );
    }
}
