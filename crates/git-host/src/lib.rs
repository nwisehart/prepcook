mod detection;
mod remote;
mod types;

pub mod azure;
pub mod github;
pub mod gitlab;

use std::path::Path;

use async_trait::async_trait;
use detection::detect_provider_from_url;
use enum_dispatch::enum_dispatch;
pub use remote::{parse_pr_url, repo_info};
pub use types::{
    CheckStatus, CreatePrRequest, FailedJob, GitHostError, HeadChecks, JobRun, Mergeability,
    NoteAnchor, NoteSide, PrComment, PrCommentAuthor, PrReviewComment, ProviderKind,
    PullRequestDetail, RepoInfo, ReviewCommentUser, UnifiedPrComment,
};

use self::{azure::AzureDevOpsProvider, github::GitHubProvider, gitlab::GitLabProvider};

#[async_trait]
#[enum_dispatch(GitHostService)]
pub trait GitHostProvider: Send + Sync {
    async fn create_pr(
        &self,
        repo_path: &Path,
        remote_url: &str,
        request: &CreatePrRequest,
    ) -> Result<PullRequestDetail, GitHostError>;

    async fn get_pr_status(&self, pr_url: &str) -> Result<PullRequestDetail, GitHostError>;

    async fn list_prs_for_branch(
        &self,
        repo_path: &Path,
        remote_url: &str,
        branch_name: &str,
    ) -> Result<Vec<PullRequestDetail>, GitHostError>;

    async fn get_pr_comments(
        &self,
        repo_path: &Path,
        remote_url: &str,
        pr_number: i64,
    ) -> Result<Vec<UnifiedPrComment>, GitHostError>;

    async fn list_open_prs(
        &self,
        repo_path: &Path,
        remote_url: &str,
    ) -> Result<Vec<PullRequestDetail>, GitHostError>;

    /// Fetch a PR/MR's head branch and check it out in `repo_path`
    /// (used when creating a workspace from an existing PR/MR).
    async fn checkout_pr(
        &self,
        repo_path: &Path,
        remote_url: &str,
        pr_number: i64,
    ) -> Result<(), GitHostError>;

    fn provider_kind(&self) -> ProviderKind;

    /// Parses a git remote of this host into [`RepoInfo`], without a network
    /// round trip.
    fn repo_info(&self, remote_url: &str) -> Result<RepoInfo, GitHostError>;

    /// Parses a pull/merge request web URL of this host into its repository
    /// and number.
    fn parse_pr_url(&self, pr_url: &str) -> Result<(RepoInfo, i64), GitHostError>;

    /// Requests review from the given host logins. An empty list is a no-op.
    async fn set_pr_reviewers(
        &self,
        repo: &RepoInfo,
        number: i64,
        logins: &[String],
    ) -> Result<(), GitHostError>;

    /// Applies labels to an existing request. An empty list is a no-op.
    async fn set_pr_labels(
        &self,
        repo: &RepoInfo,
        number: i64,
        labels: &[String],
    ) -> Result<(), GitHostError>;

    /// Whether the host can merge the request as it stands.
    async fn pr_mergeability(
        &self,
        repo: &RepoInfo,
        number: i64,
    ) -> Result<Mergeability, GitHostError>;

    /// CI on the request's head commit, with the failed jobs and the tail of
    /// their logs when it failed.
    async fn pr_head_checks(
        &self,
        repo: &RepoInfo,
        number: i64,
    ) -> Result<HeadChecks, GitHostError>;

    /// The newest successful run of the CI job (GitLab) or workflow (GitHub)
    /// named `job_name` on `git_ref`.
    async fn latest_successful_job(
        &self,
        repo: &RepoInfo,
        job_name: &str,
        git_ref: &str,
    ) -> Result<Option<JobRun>, GitHostError>;

    /// Whether commit `ancestor` is contained in `descendant`'s history,
    /// asked of the host so no local clone needs both commits.
    async fn is_ancestor(
        &self,
        repo: &RepoInfo,
        ancestor: &str,
        descendant: &str,
    ) -> Result<bool, GitHostError>;

    /// The commit a merged request put on its target branch; `None` while it
    /// is open or after it was closed unmerged.
    async fn pr_merge_commit(
        &self,
        repo: &RepoInfo,
        number: i64,
    ) -> Result<Option<String>, GitHostError>;

    /// A user's display name from their login; `None` when the host has no
    /// such user or the user set no name.
    async fn user_display_name(
        &self,
        repo: &RepoInfo,
        login: &str,
    ) -> Result<Option<String>, GitHostError>;

    /// Posts a plain comment on the request.
    async fn create_pr_note(
        &self,
        repo: &RepoInfo,
        number: i64,
        body: &str,
    ) -> Result<(), GitHostError>;

    /// Posts a comment anchored to a file, and optionally a line, of the
    /// request's diff. A rejected anchor surfaces as an error for which
    /// [`GitHostError::anchor_was_rejected`] is true.
    async fn create_pr_note_at(
        &self,
        repo: &RepoInfo,
        number: i64,
        anchor: &NoteAnchor,
        body: &str,
    ) -> Result<(), GitHostError>;

    /// Posts a request-level comment nobody has to resolve — praise, status,
    /// anything that should not block a merge on a project requiring every
    /// thread resolved.
    async fn create_pr_note_unresolvable(
        &self,
        repo: &RepoInfo,
        number: i64,
        body: &str,
    ) -> Result<(), GitHostError>;
}

#[enum_dispatch]
pub enum GitHostService {
    GitHub(GitHubProvider),
    GitLab(GitLabProvider),
    AzureDevOps(AzureDevOpsProvider),
}

impl GitHostService {
    pub fn from_url(url: &str) -> Result<Self, GitHostError> {
        match detect_provider_from_url(url) {
            ProviderKind::GitHub => Ok(Self::GitHub(GitHubProvider::new()?)),
            ProviderKind::GitLab => Ok(Self::GitLab(GitLabProvider::new()?)),
            ProviderKind::AzureDevOps => Ok(Self::AzureDevOps(AzureDevOpsProvider::new()?)),
            ProviderKind::Unknown => Err(GitHostError::UnsupportedProvider),
        }
    }
}
