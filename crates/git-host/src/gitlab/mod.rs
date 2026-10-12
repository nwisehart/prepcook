//! GitLab hosting service implementation (via the `glab` CLI).

mod cli;

use std::{path::Path, time::Duration};

use async_trait::async_trait;
use backon::{ExponentialBuilder, Retryable};
use cli::GlabCliError;
pub use cli::{GitLabRepoInfo, GlabCli, MrMergeability};
use tokio::task;
use tracing::info;

use crate::{
    GitHostProvider,
    types::{
        CI_LOG_TAIL_BYTES, CheckStatus, CreatePrRequest, GitHostError, HeadChecks, JobRun,
        Mergeability, NoteAnchor, ProviderKind, PullRequestDetail, RepoInfo, UnifiedPrComment,
    },
};

/// Parses a GitLab remote into [`RepoInfo`]; nested groups stay in `owner`.
pub(crate) fn parse_remote_url(remote_url: &str) -> Result<RepoInfo, GitHostError> {
    cli::parse_remote_url(remote_url)
        .map(|info| info.to_repo_info())
        .map_err(Into::into)
}

/// Parses `https://<host>/<group>/<project>/-/merge_requests/<iid>`.
pub(crate) fn parse_pr_url(pr_url: &str) -> Result<(RepoInfo, i64), GitHostError> {
    cli::parse_mr_url(pr_url)
        .map(|(info, iid)| (info.to_repo_info(), iid))
        .map_err(Into::into)
}

/// GitLab's pipeline status words, folded into the host-neutral set.
fn pipeline_status(status: &str) -> CheckStatus {
    match status {
        "success" => CheckStatus::Success,
        "failed" => CheckStatus::Failed,
        "canceled" | "skipped" => CheckStatus::Canceled,
        // created, waiting_for_resource, preparing, pending, running,
        // scheduled, manual: not finished.
        _ => CheckStatus::Pending,
    }
}

fn retry_policy() -> ExponentialBuilder {
    ExponentialBuilder::default()
        .with_min_delay(Duration::from_secs(1))
        .with_max_delay(Duration::from_secs(30))
        .with_max_times(3)
        .with_jitter()
}

#[derive(Debug, Clone)]
pub struct GitLabProvider {
    glab_cli: GlabCli,
}

impl GitLabProvider {
    pub fn new() -> Result<Self, GitHostError> {
        Ok(Self {
            glab_cli: GlabCli::new(),
        })
    }

    /// Repo info comes from parsing the remote URL (project paths map 1:1
    /// onto GitLab API paths), so no CLI round-trip is needed.
    fn get_repo_info(&self, remote_url: &str) -> Result<GitLabRepoInfo, GitHostError> {
        cli::parse_remote_url(remote_url).map_err(Into::into)
    }

    /// Runs one blocking `glab` operation off the async runtime.
    async fn blocking<T, F>(&self, what: &'static str, op: F) -> Result<T, GitHostError>
    where
        T: Send + 'static,
        F: FnOnce(&GlabCli) -> Result<T, GlabCliError> + Send + 'static,
    {
        let cli = self.glab_cli.clone();
        task::spawn_blocking(move || op(&cli))
            .await
            .map_err(|err| {
                GitHostError::PullRequest(format!("Failed to execute GitLab CLI for {what}: {err}"))
            })?
            .map_err(GitHostError::from)
    }
}

impl From<GlabCliError> for GitHostError {
    fn from(error: GlabCliError) -> Self {
        match &error {
            GlabCliError::AuthFailed(msg) => GitHostError::AuthFailed(msg.clone()),
            GlabCliError::NotAvailable => GitHostError::CliNotInstalled {
                provider: ProviderKind::GitLab,
            },
            GlabCliError::CommandFailed(msg) => {
                let lower = msg.to_ascii_lowercase();
                if lower.contains("403") || lower.contains("forbidden") {
                    GitHostError::InsufficientPermissions(msg.clone())
                } else if lower.contains("404") || lower.contains("not found") {
                    GitHostError::RepoNotFoundOrNoAccess(msg.clone())
                } else if lower.contains("not a git repository") {
                    GitHostError::NotAGitRepository(msg.clone())
                } else {
                    GitHostError::PullRequest(msg.clone())
                }
            }
            GlabCliError::UnexpectedOutput(msg) => GitHostError::UnexpectedOutput(msg.clone()),
        }
    }
}

#[async_trait]
impl GitHostProvider for GitLabProvider {
    async fn create_pr(
        &self,
        repo_path: &Path,
        remote_url: &str,
        request: &CreatePrRequest,
    ) -> Result<PullRequestDetail, GitHostError> {
        let repo_info = self.get_repo_info(remote_url)?;

        // Cross-project MRs need a source project id; not needed for the
        // team's same-repo workflow, so reject clearly rather than guessing.
        if let Some(head_url) = &request.head_repo_url {
            let head_info = self.get_repo_info(head_url)?;
            if head_info.project_path != repo_info.project_path {
                return Err(GitHostError::PullRequest(
                    "Cross-project merge requests are not supported yet".to_string(),
                ));
            }
        }

        (|| async {
            let cli = self.glab_cli.clone();
            let request = request.clone();
            let repo_info = repo_info.clone();
            let repo_path = repo_path.to_path_buf();

            let detail =
                task::spawn_blocking(move || cli.create_mr(&request, &repo_info, &repo_path))
                    .await
                    .map_err(|err| {
                        GitHostError::PullRequest(format!(
                            "Failed to execute GitLab CLI for MR creation: {err}"
                        ))
                    })?
                    .map_err(GitHostError::from)?;

            info!(
                "Created GitLab MR !{} for branch {}",
                detail.number, detail.head_branch
            );
            Ok(detail)
        })
        .retry(&retry_policy())
        .when(|e: &GitHostError| e.should_retry())
        .notify(|err: &GitHostError, dur: Duration| {
            tracing::warn!(
                "GitLab API call failed, retrying after {:.2}s: {}",
                dur.as_secs_f64(),
                err
            );
        })
        .await
    }

    async fn get_pr_status(&self, pr_url: &str) -> Result<PullRequestDetail, GitHostError> {
        let cli = self.glab_cli.clone();
        let url = pr_url.to_string();

        (|| async {
            let cli = cli.clone();
            let url = url.clone();
            let mr = task::spawn_blocking(move || cli.view_mr(&url))
                .await
                .map_err(|err| {
                    GitHostError::PullRequest(format!(
                        "Failed to execute GitLab CLI for viewing MR: {err}"
                    ))
                })?;
            mr.map_err(GitHostError::from)
        })
        .retry(&retry_policy())
        .when(|e: &GitHostError| e.should_retry())
        .notify(|err: &GitHostError, dur: Duration| {
            tracing::warn!(
                "GitLab API call failed, retrying after {:.2}s: {}",
                dur.as_secs_f64(),
                err
            );
        })
        .await
    }

    async fn list_prs_for_branch(
        &self,
        repo_path: &Path,
        remote_url: &str,
        branch_name: &str,
    ) -> Result<Vec<PullRequestDetail>, GitHostError> {
        let repo_info = self.get_repo_info(remote_url)?;
        let cli = self.glab_cli.clone();
        let branch = branch_name.to_string();
        let repo_path = repo_path.to_path_buf();

        (|| async {
            let cli = cli.clone();
            let repo_info = repo_info.clone();
            let branch = branch.clone();
            let repo_path = repo_path.clone();

            let mrs = task::spawn_blocking(move || {
                cli.list_mrs_for_branch(&repo_info, &branch, &repo_path)
            })
            .await
            .map_err(|err| {
                GitHostError::PullRequest(format!(
                    "Failed to execute GitLab CLI for listing MRs: {err}"
                ))
            })?;
            mrs.map_err(GitHostError::from)
        })
        .retry(&retry_policy())
        .when(|e: &GitHostError| e.should_retry())
        .notify(|err: &GitHostError, dur: Duration| {
            tracing::warn!(
                "GitLab API call failed, retrying after {:.2}s: {}",
                dur.as_secs_f64(),
                err
            );
        })
        .await
    }

    async fn get_pr_comments(
        &self,
        repo_path: &Path,
        remote_url: &str,
        pr_number: i64,
    ) -> Result<Vec<UnifiedPrComment>, GitHostError> {
        let repo_info = self.get_repo_info(remote_url)?;
        let cli = self.glab_cli.clone();
        let repo_path = repo_path.to_path_buf();

        (|| async {
            let cli = cli.clone();
            let repo_info = repo_info.clone();
            let repo_path = repo_path.clone();

            let comments = task::spawn_blocking(move || {
                cli.get_mr_comments(&repo_info, pr_number, &repo_path)
            })
            .await
            .map_err(|err| {
                GitHostError::PullRequest(format!(
                    "Failed to execute GitLab CLI for fetching MR comments: {err}"
                ))
            })?;
            comments.map_err(GitHostError::from)
        })
        .retry(&retry_policy())
        .when(|e: &GitHostError| e.should_retry())
        .notify(|err: &GitHostError, dur: Duration| {
            tracing::warn!(
                "GitLab API call failed, retrying after {:.2}s: {}",
                dur.as_secs_f64(),
                err
            );
        })
        .await
    }

    async fn list_open_prs(
        &self,
        repo_path: &Path,
        remote_url: &str,
    ) -> Result<Vec<PullRequestDetail>, GitHostError> {
        let repo_info = self.get_repo_info(remote_url)?;
        let cli = self.glab_cli.clone();
        let repo_path = repo_path.to_path_buf();

        (|| async {
            let cli = cli.clone();
            let repo_info = repo_info.clone();
            let repo_path = repo_path.clone();

            let mrs = task::spawn_blocking(move || cli.list_open_mrs(&repo_info, &repo_path))
                .await
                .map_err(|err| {
                    GitHostError::PullRequest(format!(
                        "Failed to execute GitLab CLI for listing MRs: {err}"
                    ))
                })?;
            mrs.map_err(GitHostError::from)
        })
        .retry(&retry_policy())
        .when(|e: &GitHostError| e.should_retry())
        .notify(|err: &GitHostError, dur: Duration| {
            tracing::warn!(
                "GitLab API call failed, retrying after {:.2}s: {}",
                dur.as_secs_f64(),
                err
            );
        })
        .await
    }

    async fn checkout_pr(
        &self,
        repo_path: &Path,
        remote_url: &str,
        pr_number: i64,
    ) -> Result<(), GitHostError> {
        let repo_info = self.get_repo_info(remote_url)?;
        let cli = self.glab_cli.clone();
        let repo_path = repo_path.to_path_buf();

        // No retry: checkout mutates the worktree, so a retried half-applied
        // attempt would compound the failure rather than recover from it.
        task::spawn_blocking(move || cli.checkout_mr(&repo_info, pr_number, &repo_path))
            .await
            .map_err(|err| {
                GitHostError::PullRequest(format!(
                    "Failed to execute GitLab CLI for MR checkout: {err}"
                ))
            })?
            .map_err(GitHostError::from)
    }

    fn provider_kind(&self) -> ProviderKind {
        ProviderKind::GitLab
    }
    fn repo_info(&self, remote_url: &str) -> Result<RepoInfo, GitHostError> {
        parse_remote_url(remote_url)
    }

    fn parse_pr_url(&self, pr_url: &str) -> Result<(RepoInfo, i64), GitHostError> {
        parse_pr_url(pr_url)
    }

    async fn set_pr_reviewers(
        &self,
        repo: &RepoInfo,
        number: i64,
        logins: &[String],
    ) -> Result<(), GitHostError> {
        let info = GitLabRepoInfo::from(repo);
        let logins = logins.to_vec();
        self.blocking("assigning reviewers", move |cli| {
            cli.set_mr_reviewers(&info, number, &logins)
        })
        .await
    }

    async fn set_pr_labels(
        &self,
        repo: &RepoInfo,
        number: i64,
        labels: &[String],
    ) -> Result<(), GitHostError> {
        let info = GitLabRepoInfo::from(repo);
        let labels = labels.to_vec();
        self.blocking("labelling an MR", move |cli| {
            cli.set_mr_labels(&info, number, &labels)
        })
        .await
    }

    async fn pr_mergeability(
        &self,
        repo: &RepoInfo,
        number: i64,
    ) -> Result<Mergeability, GitHostError> {
        let info = GitLabRepoInfo::from(repo);
        self.blocking("reading mergeability", move |cli| {
            cli.mr_mergeability(&info, number)
                .map(|m| m.to_mergeability())
        })
        .await
    }

    async fn pr_head_checks(
        &self,
        repo: &RepoInfo,
        number: i64,
    ) -> Result<HeadChecks, GitHostError> {
        let info = GitLabRepoInfo::from(repo);
        self.blocking("reading the head pipeline", move |cli| {
            let Some((pipeline_id, status, sha)) = cli.mr_head_pipeline(&info, number)? else {
                return Ok(HeadChecks {
                    status: CheckStatus::None,
                    sha: String::new(),
                    failed_jobs: Vec::new(),
                });
            };
            let status = pipeline_status(&status);
            let failed_jobs = if status == CheckStatus::Failed {
                cli.failed_jobs(&info, pipeline_id, CI_LOG_TAIL_BYTES)?
            } else {
                Vec::new()
            };
            Ok(HeadChecks {
                status,
                sha,
                failed_jobs,
            })
        })
        .await
    }

    async fn latest_successful_job(
        &self,
        repo: &RepoInfo,
        job_name: &str,
        git_ref: &str,
    ) -> Result<Option<JobRun>, GitHostError> {
        let info = GitLabRepoInfo::from(repo);
        let job_name = job_name.to_string();
        let git_ref = git_ref.to_string();
        self.blocking("reading jobs", move |cli| {
            cli.latest_successful_job(&info, &job_name, &git_ref)
        })
        .await
    }

    async fn is_ancestor(
        &self,
        repo: &RepoInfo,
        ancestor: &str,
        descendant: &str,
    ) -> Result<bool, GitHostError> {
        let info = GitLabRepoInfo::from(repo);
        let ancestor = ancestor.to_string();
        let descendant = descendant.to_string();
        self.blocking("reading a merge base", move |cli| {
            cli.is_ancestor(&info, &ancestor, &descendant)
        })
        .await
    }

    async fn pr_merge_commit(
        &self,
        repo: &RepoInfo,
        number: i64,
    ) -> Result<Option<String>, GitHostError> {
        let info = GitLabRepoInfo::from(repo);
        self.blocking("reading a merge commit", move |cli| {
            cli.mr_merge_commit(&info, number)
        })
        .await
    }

    async fn user_display_name(
        &self,
        repo: &RepoInfo,
        login: &str,
    ) -> Result<Option<String>, GitHostError> {
        let host = repo.host.clone();
        let login = login.to_string();
        self.blocking("reading a user", move |cli| {
            cli.user_display_name(&host, &login)
        })
        .await
    }

    async fn create_pr_note(
        &self,
        repo: &RepoInfo,
        number: i64,
        body: &str,
    ) -> Result<(), GitHostError> {
        let info = GitLabRepoInfo::from(repo);
        let body = body.to_string();
        self.blocking("posting an MR note", move |cli| {
            cli.create_mr_note(&info, number, &body)
        })
        .await
    }

    async fn create_pr_note_at(
        &self,
        repo: &RepoInfo,
        number: i64,
        anchor: &NoteAnchor,
        body: &str,
    ) -> Result<(), GitHostError> {
        let info = GitLabRepoInfo::from(repo);
        let anchor = anchor.clone();
        let body = body.to_string();
        self.blocking("posting an anchored MR note", move |cli| {
            cli.create_mr_note_at(&info, number, &body, &anchor)
        })
        .await
    }

    async fn create_pr_note_unresolvable(
        &self,
        repo: &RepoInfo,
        number: i64,
        body: &str,
    ) -> Result<(), GitHostError> {
        let info = GitLabRepoInfo::from(repo);
        let body = body.to_string();
        self.blocking("posting an MR note", move |cli| {
            cli.create_mr_note_unresolvable(&info, number, &body)
        })
        .await
    }
}
