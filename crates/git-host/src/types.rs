use chrono::{DateTime, Utc};
use db::models::merge::{MergeStatus, PullRequestInfo};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    GitHub,
    GitLab,
    AzureDevOps,
    Unknown,
}

impl std::fmt::Display for ProviderKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderKind::GitHub => write!(f, "GitHub"),
            ProviderKind::GitLab => write!(f, "GitLab"),
            ProviderKind::AzureDevOps => write!(f, "Azure DevOps"),
            ProviderKind::Unknown => write!(f, "Unknown"),
        }
    }
}

impl ProviderKind {
    /// The CLI command an agent runs to post a plain comment on a pull/merge
    /// request, for instructions handed to it. `None` where the host's CLI
    /// has no such command.
    pub fn note_command(&self) -> Option<&'static str> {
        match self {
            ProviderKind::GitHub => Some("gh pr comment"),
            ProviderKind::GitLab => Some("glab mr note"),
            ProviderKind::AzureDevOps | ProviderKind::Unknown => None,
        }
    }

    /// The CLI command that rebases a request's branch on the host's side.
    pub fn rebase_command(&self) -> Option<&'static str> {
        match self {
            ProviderKind::GitHub => Some("gh pr update-branch --rebase"),
            ProviderKind::GitLab => Some("glab mr rebase"),
            ProviderKind::AzureDevOps | ProviderKind::Unknown => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CreatePrRequest {
    pub title: String,
    pub body: Option<String>,
    pub head_branch: String,
    pub base_branch: String,
    pub draft: Option<bool>,
    /// URL of the repo containing the head branch (for cross-fork PRs).
    pub head_repo_url: Option<String>,
}

#[derive(Debug, Error)]
pub enum GitHostError {
    #[error("Repository error: {0}")]
    Repository(String),
    #[error("Pull request error: {0}")]
    PullRequest(String),
    #[error("Authentication failed: {0}")]
    AuthFailed(String),
    #[error("Insufficient permissions: {0}")]
    InsufficientPermissions(String),
    #[error("Repository not found or no access: {0}")]
    RepoNotFoundOrNoAccess(String),
    #[error("{provider} CLI is not installed or not available in PATH")]
    CliNotInstalled { provider: ProviderKind },
    #[error("Not a git repository: {0}")]
    NotAGitRepository(String),
    #[error("Unsupported git hosting provider")]
    UnsupportedProvider,
    #[error("CLI returned unexpected output: {0}")]
    UnexpectedOutput(String),
    #[error("Not supported by this git host: {0}")]
    Unsupported(String),
}

impl GitHostError {
    pub fn should_retry(&self) -> bool {
        !matches!(
            self,
            GitHostError::AuthFailed(_)
                | GitHostError::InsufficientPermissions(_)
                | GitHostError::RepoNotFoundOrNoAccess(_)
                | GitHostError::CliNotInstalled { .. }
                | GitHostError::NotAGitRepository(_)
                | GitHostError::UnsupportedProvider
                | GitHostError::Unsupported(_)
        )
    }

    /// Whether a failed anchored note should be retried at a coarser anchor.
    ///
    /// Only the anchor gets a second chance. An auth, network or wrong-number
    /// failure would fail again at file level, or worse succeed as a plain
    /// note and read as though the anchor was the problem — so those abort and
    /// keep the host's own error.
    ///
    /// GitLab answers a line outside the latest diff version with HTTP 400 and
    /// a `position`/`line_code` complaint; GitHub answers HTTP 422 "Validation
    /// Failed" with "must be part of the diff" or "could not be resolved".
    ///
    /// The unknown-flag case is deliberate: `glab mr note create`'s `--file`
    /// family is marked experimental in glab's own help ("might be unstable or
    /// removed at any time"), so a glab upgrade can turn every anchored note
    /// into a usage error. Reading that as a rejected anchor degrades the
    /// review to plain notes instead of losing it.
    pub fn anchor_was_rejected(&self) -> bool {
        anchor_rejected_message(&self.to_string())
    }
}

/// The text test behind [`GitHostError::anchor_was_rejected`], split out so
/// it can be checked against raw CLI output.
pub(crate) fn anchor_rejected_message(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    [
        "400",
        "422",
        "validation failed",
        "position",
        "line_code",
        "diff",
        "could not be resolved",
        "unknown flag",
        "unknown shorthand",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

/// Bytes of a failed CI job's log kept for the agent that has to fix it.
pub(crate) const CI_LOG_TAIL_BYTES: usize = 1500;

/// The last `max_bytes` of `text`, never splitting a UTF-8 character.
pub(crate) fn tail_bytes(text: String, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text;
    }
    let start = text.len() - max_bytes;
    let start = (start..text.len())
        .find(|i| text.is_char_boundary(*i))
        .unwrap_or(text.len());
    text[start..].to_string()
}

/// A repository on a git host, parsed from a remote or web URL without a
/// network round trip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoInfo {
    pub provider: ProviderKind,
    /// Hostname, e.g. `github.com`, `gitlab.com` or a self-managed host.
    pub host: String,
    /// GitHub: the user or organisation. GitLab: the full namespace, nested
    /// groups included (`group/subgroup`).
    pub owner: String,
    /// The repository (GitHub) or project (GitLab) name.
    pub name: String,
    /// `https://<host>/<owner>/<name>`, no trailing slash or `.git`.
    pub web_url: String,
}

impl RepoInfo {
    pub fn new(provider: ProviderKind, host: &str, owner: &str, name: &str) -> Self {
        Self {
            provider,
            host: host.to_string(),
            owner: owner.to_string(),
            name: name.to_string(),
            web_url: format!("https://{host}/{owner}/{name}"),
        }
    }

    /// `owner/name` — GitHub's repo spec, GitLab's project path.
    pub fn path(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }

    /// The web page of pull/merge request `number`.
    pub fn pr_web_url(&self, number: i64) -> String {
        match self.provider {
            ProviderKind::GitLab => format!("{}/-/merge_requests/{number}", self.web_url),
            _ => format!("{}/pull/{number}", self.web_url),
        }
    }
}

/// What the host says about merging a pull/merge request right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mergeability {
    /// The host would merge it as it stands.
    pub mergeable: bool,
    /// The branch no longer applies to its target (or the project demands it
    /// be up to date first) and only a rebase fixes it. Never true for a
    /// request that is merged or closed.
    pub needs_rebase: bool,
    /// A textual conflict with the target, as opposed to merely being behind.
    pub has_conflicts: bool,
    /// The host has not finished computing the answer; the other flags are
    /// not a verdict yet.
    pub checking: bool,
    /// The host's own word for the state — GitLab's `detailed_merge_status`
    /// (`conflict`, `need_rebase`, `ci_must_pass`, …), GitHub's
    /// `mergeStateStatus` (`DIRTY`, `BEHIND`, `BLOCKED`, …).
    pub detail: String,
    /// The head commit the host judged.
    pub head_sha: String,
}

/// The overall state of the checks on a request's head commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckStatus {
    /// Still running, queued, or waiting on something.
    Pending,
    Success,
    /// Finished, with at least one failure.
    Failed,
    /// Finished without failing or passing: canceled or skipped.
    Canceled,
    /// No pipeline or checks have run on the head commit.
    None,
}

impl CheckStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            CheckStatus::Pending => "pending",
            CheckStatus::Success => "success",
            CheckStatus::Failed => "failed",
            CheckStatus::Canceled => "canceled",
            CheckStatus::None => "none",
        }
    }
}

impl std::fmt::Display for CheckStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One failed CI job on a request's head commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailedJob {
    pub name: String,
    pub url: Option<String>,
    /// The end of the job's log. Bounded: a full log can run to megabytes and
    /// only its end explains the failure. `None` when the host would not give
    /// it (an external status check has no log to read).
    pub log_tail: Option<String>,
}

/// CI on a request's head commit (GitLab: the head pipeline; GitHub: the
/// check runs and commit statuses).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadChecks {
    pub status: CheckStatus,
    /// The commit the checks ran on; empty when the host did not say.
    pub sha: String,
    /// Populated only when `status` is [`CheckStatus::Failed`].
    pub failed_jobs: Vec<FailedJob>,
}

/// One successful run of a named CI job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobRun {
    pub sha: String,
    pub finished_at: DateTime<Utc>,
    pub url: Option<String>,
}

/// Which side of the diff a line-anchored note refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteSide {
    /// A line the branch removed.
    Old,
    /// A line the branch added or kept.
    New,
}

/// Where a diff-anchored note attaches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteAnchor {
    pub path: String,
    /// `None` anchors to the file as a whole.
    pub line: Option<u32>,
    /// Last line of a range on the new side; `None` for a single line.
    pub end_line: Option<u32>,
    pub side: NoteSide,
}

impl NoteAnchor {
    /// The file as a whole — no line.
    pub fn file(path: &str) -> Self {
        Self {
            path: path.to_string(),
            line: None,
            end_line: None,
            side: NoteSide::New,
        }
    }

    /// A line the branch added or changed, on the new side.
    pub fn new_line(path: &str, line: u32) -> Self {
        Self {
            line: Some(line),
            ..Self::file(path)
        }
    }

    /// A line the branch removed, on the old side.
    pub fn old_line(path: &str, line: u32) -> Self {
        Self {
            line: Some(line),
            side: NoteSide::Old,
            ..Self::file(path)
        }
    }

    /// A range on the new side, `start..=end`.
    pub fn new_range(path: &str, start: u32, end: u32) -> Self {
        Self {
            line: Some(start),
            end_line: Some(end),
            ..Self::file(path)
        }
    }

    pub fn is_file_level(&self) -> bool {
        self.line.is_none()
    }

    /// The same file without the line.
    pub fn to_file_level(&self) -> Self {
        Self::file(&self.path)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct PrCommentAuthor {
    pub login: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PrComment {
    pub id: String,
    pub author: PrCommentAuthor,
    pub author_association: String,
    pub body: String,
    pub created_at: DateTime<Utc>,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ReviewCommentUser {
    pub login: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct PrReviewComment {
    pub id: i64,
    pub user: ReviewCommentUser,
    pub body: String,
    pub created_at: DateTime<Utc>,
    pub html_url: String,
    pub path: String,
    pub line: Option<i64>,
    pub side: Option<String>,
    pub diff_hunk: String,
    pub author_association: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(tag = "comment_type", rename_all = "snake_case")]
#[ts(tag = "comment_type", rename_all = "snake_case")]
pub enum UnifiedPrComment {
    General {
        id: String,
        author: String,
        author_association: Option<String>,
        body: String,
        created_at: DateTime<Utc>,
        url: Option<String>,
    },
    Review {
        id: i64,
        author: String,
        author_association: Option<String>,
        body: String,
        created_at: DateTime<Utc>,
        url: Option<String>,
        path: String,
        line: Option<i64>,
        side: Option<String>,
        diff_hunk: Option<String>,
    },
}

impl UnifiedPrComment {
    pub fn created_at(&self) -> DateTime<Utc> {
        match self {
            UnifiedPrComment::General { created_at, .. } => *created_at,
            UnifiedPrComment::Review { created_at, .. } => *created_at,
        }
    }

    /// The comment text, whichever kind of comment this is. Lets a caller ask
    /// "has this already been said on the merge request?" without caring
    /// whether the existing note was anchored to a line.
    pub fn body(&self) -> &str {
        match self {
            UnifiedPrComment::General { body, .. } => body,
            UnifiedPrComment::Review { body, .. } => body,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct PullRequestDetail {
    pub number: i64,
    pub url: String,
    pub status: MergeStatus,
    pub merged_at: Option<DateTime<Utc>>,
    pub merge_commit_sha: Option<String>,
    pub title: String,
    pub base_branch: String,
    pub head_branch: String,
}

impl From<PullRequestDetail> for PullRequestInfo {
    fn from(d: PullRequestDetail) -> Self {
        PullRequestInfo {
            number: d.number,
            url: d.url,
            status: d.status,
            merged_at: d.merged_at,
            merge_commit_sha: d.merge_commit_sha,
        }
    }
}

#[cfg(test)]
mod anchor_tests {
    use super::{GitHostError, anchor_rejected_message};

    #[test]
    fn only_an_anchor_problem_earns_a_coarser_retry() {
        // Each host's own rejection of a line that isn't in the latest diff,
        // and the shapes the CLIs wrap it in.
        for error in [
            "HTTP 400: position is invalid",
            "failed: 400 Bad Request line_code",
            "could not create note: diff position not found",
            // GitHub's answer to a line outside the diff.
            "gh: Validation Failed (HTTP 422)\npull_request_review_thread.line must be part of the diff",
            "Path could not be resolved",
            // A glab upgrade dropping the experimental flags reads as usage
            // failure, and degrading beats losing the review.
            "unknown flag: --old-line",
            "unknown shorthand flag",
        ] {
            assert!(anchor_rejected_message(error), "should fall back: {error}");
        }

        // These would fail again at file level, or succeed as a plain note
        // and read as though the anchor was at fault.
        for error in [
            "authentication failed: token expired",
            "dial tcp: lookup gitlab.com: no such host",
            "404 Not Found: merge request !9999",
            "context deadline exceeded",
            "gh: Not Found (HTTP 404)",
        ] {
            assert!(!anchor_rejected_message(error), "should abort: {error}");
        }
    }

    #[test]
    fn the_error_method_reads_the_wrapped_message() {
        assert!(
            GitHostError::PullRequest("HTTP 400: position is invalid".into()).anchor_was_rejected()
        );
        assert!(!GitHostError::AuthFailed("token expired".into()).anchor_was_rejected());
    }
}
