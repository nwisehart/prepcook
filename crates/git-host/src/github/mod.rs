//! GitHub hosting service implementation.

mod cli;

use std::{path::Path, time::Duration};

use async_trait::async_trait;
use backon::{ExponentialBuilder, Retryable};
pub use cli::GhCli;
use cli::{GhCliError, GitHubRepoInfo};
pub(crate) use cli::{parse_pr_url, parse_remote_url};
use tokio::task;
use tracing::info;

use crate::{
    GitHostProvider,
    types::{
        CI_LOG_TAIL_BYTES, CreatePrRequest, GitHostError, HeadChecks, JobRun, Mergeability,
        NoteAnchor, PrComment, PrReviewComment, ProviderKind, PullRequestDetail, RepoInfo,
        UnifiedPrComment,
    },
};

#[derive(Debug, Clone)]
pub struct GitHubProvider {
    gh_cli: GhCli,
}

impl GitHubProvider {
    pub fn new() -> Result<Self, GitHostError> {
        Ok(Self {
            gh_cli: GhCli::new(),
        })
    }

    /// Runs one blocking `gh` operation off the async runtime.
    async fn blocking<T, F>(&self, what: &'static str, op: F) -> Result<T, GitHostError>
    where
        T: Send + 'static,
        F: FnOnce(&GhCli) -> Result<T, GhCliError> + Send + 'static,
    {
        let cli = self.gh_cli.clone();
        task::spawn_blocking(move || op(&cli))
            .await
            .map_err(|err| {
                GitHostError::PullRequest(format!("Failed to execute GitHub CLI for {what}: {err}"))
            })?
            .map_err(GitHostError::from)
    }

    async fn get_repo_info(
        &self,
        remote_url: &str,
        repo_path: &Path,
    ) -> Result<GitHubRepoInfo, GitHostError> {
        let cli = self.gh_cli.clone();
        let url = remote_url.to_string();
        let path = repo_path.to_path_buf();
        task::spawn_blocking(move || cli.get_repo_info(&url, &path))
            .await
            .map_err(|err| {
                GitHostError::Repository(format!("Failed to get repo info from URL: {err}"))
            })?
            .map_err(Into::into)
    }

    async fn fetch_general_comments(
        &self,
        cli: &GhCli,
        repo_info: &GitHubRepoInfo,
        pr_number: i64,
    ) -> Result<Vec<PrComment>, GitHostError> {
        let cli = cli.clone();
        let repo_info = repo_info.clone();

        (|| async {
            let cli = cli.clone();
            let repo_info = repo_info.clone();

            let comments = task::spawn_blocking(move || cli.get_pr_comments(&repo_info, pr_number))
                .await
                .map_err(|err| {
                    GitHostError::PullRequest(format!(
                        "Failed to execute GitHub CLI for fetching PR comments: {err}"
                    ))
                })?;
            comments.map_err(GitHostError::from)
        })
        .retry(
            &ExponentialBuilder::default()
                .with_min_delay(Duration::from_secs(1))
                .with_max_delay(Duration::from_secs(30))
                .with_max_times(3)
                .with_jitter(),
        )
        .when(|e: &GitHostError| e.should_retry())
        .notify(|err: &GitHostError, dur: Duration| {
            tracing::warn!(
                "GitHub API call failed, retrying after {:.2}s: {}",
                dur.as_secs_f64(),
                err
            );
        })
        .await
    }

    async fn fetch_review_comments(
        &self,
        cli: &GhCli,
        repo_info: &GitHubRepoInfo,
        pr_number: i64,
    ) -> Result<Vec<PrReviewComment>, GitHostError> {
        let cli = cli.clone();
        let repo_info = repo_info.clone();

        (|| async {
            let cli = cli.clone();
            let repo_info = repo_info.clone();

            let comments =
                task::spawn_blocking(move || cli.get_pr_review_comments(&repo_info, pr_number))
                    .await
                    .map_err(|err| {
                        GitHostError::PullRequest(format!(
                            "Failed to execute GitHub CLI for fetching review comments: {err}"
                        ))
                    })?;
            comments.map_err(GitHostError::from)
        })
        .retry(
            &ExponentialBuilder::default()
                .with_min_delay(Duration::from_secs(1))
                .with_max_delay(Duration::from_secs(30))
                .with_max_times(3)
                .with_jitter(),
        )
        .when(|e: &GitHostError| e.should_retry())
        .notify(|err: &GitHostError, dur: Duration| {
            tracing::warn!(
                "GitHub API call failed, retrying after {:.2}s: {}",
                dur.as_secs_f64(),
                err
            );
        })
        .await
    }
}

impl From<GhCliError> for GitHostError {
    fn from(error: GhCliError) -> Self {
        match &error {
            GhCliError::AuthFailed(msg) => GitHostError::AuthFailed(msg.clone()),
            GhCliError::NotAvailable => GitHostError::CliNotInstalled {
                provider: ProviderKind::GitHub,
            },
            GhCliError::CommandFailed(msg) => {
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
            GhCliError::UnexpectedOutput(msg) => GitHostError::UnexpectedOutput(msg.clone()),
        }
    }
}

#[async_trait]
impl GitHostProvider for GitHubProvider {
    async fn create_pr(
        &self,
        repo_path: &Path,
        remote_url: &str,
        request: &CreatePrRequest,
    ) -> Result<PullRequestDetail, GitHostError> {
        // Get owner/repo from the remote URL (target repo for the PR).
        let target_repo_info = self.get_repo_info(remote_url, repo_path).await?;

        // For cross-fork PRs, get the head repo info to format head_branch as "owner:branch".
        let head_branch = if let Some(head_url) = &request.head_repo_url {
            let head_repo_info = self.get_repo_info(head_url, repo_path).await?;
            if head_repo_info.owner != target_repo_info.owner {
                format!("{}:{}", head_repo_info.owner, request.head_branch)
            } else {
                request.head_branch.clone()
            }
        } else {
            request.head_branch.clone()
        };

        let mut request_clone = request.clone();
        request_clone.head_branch = head_branch;

        (|| async {
            let cli = self.gh_cli.clone();
            let request = request_clone.clone();
            let target_repo = target_repo_info.clone();
            let repo_path = repo_path.to_path_buf();

            let cli_result =
                task::spawn_blocking(move || cli.create_pr(&request, &target_repo, &repo_path))
                    .await
                    .map_err(|err| {
                        GitHostError::PullRequest(format!(
                            "Failed to execute GitHub CLI for PR creation: {err}"
                        ))
                    })?
                    .map_err(GitHostError::from)?;

            info!(
                "Created GitHub PR #{} for branch {}",
                cli_result.number, request_clone.head_branch
            );

            Ok(cli_result)
        })
        .retry(
            &ExponentialBuilder::default()
                .with_min_delay(Duration::from_secs(1))
                .with_max_delay(Duration::from_secs(30))
                .with_max_times(3)
                .with_jitter(),
        )
        .when(|e: &GitHostError| e.should_retry())
        .notify(|err: &GitHostError, dur: Duration| {
            tracing::warn!(
                "GitHub API call failed, retrying after {:.2}s: {}",
                dur.as_secs_f64(),
                err
            );
        })
        .await
    }

    async fn get_pr_status(&self, pr_url: &str) -> Result<PullRequestDetail, GitHostError> {
        let cli = self.gh_cli.clone();
        let url = pr_url.to_string();

        (|| async {
            let cli = cli.clone();
            let url = url.clone();
            let pr = task::spawn_blocking(move || cli.view_pr(&url))
                .await
                .map_err(|err| {
                    GitHostError::PullRequest(format!(
                        "Failed to execute GitHub CLI for viewing PR: {err}"
                    ))
                })?;
            pr.map_err(GitHostError::from)
        })
        .retry(
            &ExponentialBuilder::default()
                .with_min_delay(Duration::from_secs(1))
                .with_max_delay(Duration::from_secs(30))
                .with_max_times(3)
                .with_jitter(),
        )
        .when(|err: &GitHostError| err.should_retry())
        .notify(|err: &GitHostError, dur: Duration| {
            tracing::warn!(
                "GitHub API call failed, retrying after {:.2}s: {}",
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
        let repo_info = self.get_repo_info(remote_url, repo_path).await?;

        let cli = self.gh_cli.clone();
        let branch = branch_name.to_string();

        (|| async {
            let cli = cli.clone();
            let repo_info = repo_info.clone();
            let branch = branch.clone();

            let prs = task::spawn_blocking(move || cli.list_prs_for_branch(&repo_info, &branch))
                .await
                .map_err(|err| {
                    GitHostError::PullRequest(format!(
                        "Failed to execute GitHub CLI for listing PRs: {err}"
                    ))
                })?;
            prs.map_err(GitHostError::from)
        })
        .retry(
            &ExponentialBuilder::default()
                .with_min_delay(Duration::from_secs(1))
                .with_max_delay(Duration::from_secs(30))
                .with_max_times(3)
                .with_jitter(),
        )
        .when(|e: &GitHostError| e.should_retry())
        .notify(|err: &GitHostError, dur: Duration| {
            tracing::warn!(
                "GitHub API call failed, retrying after {:.2}s: {}",
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
        let repo_info = self.get_repo_info(remote_url, repo_path).await?;

        // Fetch both types of comments in parallel
        let cli1 = self.gh_cli.clone();
        let cli2 = self.gh_cli.clone();

        let (general_result, review_result) = tokio::join!(
            self.fetch_general_comments(&cli1, &repo_info, pr_number),
            self.fetch_review_comments(&cli2, &repo_info, pr_number)
        );

        let general_comments = general_result?;
        let review_comments = review_result?;

        // Convert and merge into unified timeline
        let mut unified: Vec<UnifiedPrComment> = Vec::new();

        for c in general_comments {
            unified.push(UnifiedPrComment::General {
                id: c.id,
                author: c.author.login,
                author_association: Some(c.author_association),
                body: c.body,
                created_at: c.created_at,
                url: Some(c.url),
            });
        }

        for c in review_comments {
            unified.push(UnifiedPrComment::Review {
                id: c.id,
                author: c.user.login,
                author_association: Some(c.author_association),
                body: c.body,
                created_at: c.created_at,
                url: Some(c.html_url),
                path: c.path,
                line: c.line,
                side: c.side,
                diff_hunk: Some(c.diff_hunk),
            });
        }

        // Sort by creation time
        unified.sort_by_key(|c| c.created_at());

        Ok(unified)
    }

    async fn list_open_prs(
        &self,
        repo_path: &Path,
        remote_url: &str,
    ) -> Result<Vec<PullRequestDetail>, GitHostError> {
        let repo_info = self.get_repo_info(remote_url, repo_path).await?;

        let cli = self.gh_cli.clone();

        (|| async {
            let cli = cli.clone();
            let owner = repo_info.owner.clone();
            let repo_name = repo_info.repo_name.clone();

            let prs = task::spawn_blocking(move || cli.list_prs(&owner, &repo_name))
                .await
                .map_err(|err| {
                    GitHostError::PullRequest(format!(
                        "Failed to execute GitHub CLI for listing PRs: {err}"
                    ))
                })?;
            prs.map_err(GitHostError::from)
        })
        .retry(
            &ExponentialBuilder::default()
                .with_min_delay(Duration::from_secs(1))
                .with_max_delay(Duration::from_secs(30))
                .with_max_times(3)
                .with_jitter(),
        )
        .when(|e: &GitHostError| e.should_retry())
        .notify(|err: &GitHostError, dur: Duration| {
            tracing::warn!(
                "GitHub API call failed, retrying after {:.2}s: {}",
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
        let repo_info = self.get_repo_info(remote_url, repo_path).await?;
        let cli = self.gh_cli.clone();
        let repo_path = repo_path.to_path_buf();

        // No retry: checkout mutates the worktree, so a retried half-applied
        // attempt would compound the failure rather than recover from it.
        task::spawn_blocking(move || cli.pr_checkout(&repo_path, &repo_info, pr_number))
            .await
            .map_err(|err| {
                GitHostError::PullRequest(format!(
                    "Failed to execute GitHub CLI for PR checkout: {err}"
                ))
            })?
            .map_err(GitHostError::from)
    }

    fn provider_kind(&self) -> ProviderKind {
        ProviderKind::GitHub
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
        let info = GitHubRepoInfo::from(repo);
        let logins = logins.to_vec();
        self.blocking("requesting reviewers", move |cli| {
            cli.set_pr_reviewers(&info, number, &logins)
        })
        .await
    }

    async fn set_pr_labels(
        &self,
        repo: &RepoInfo,
        number: i64,
        labels: &[String],
    ) -> Result<(), GitHostError> {
        let info = GitHubRepoInfo::from(repo);
        let labels = labels.to_vec();
        self.blocking("labelling a PR", move |cli| {
            cli.set_pr_labels(&info, number, &labels)
        })
        .await
    }

    async fn pr_mergeability(
        &self,
        repo: &RepoInfo,
        number: i64,
    ) -> Result<Mergeability, GitHostError> {
        let info = GitHubRepoInfo::from(repo);
        self.blocking("reading mergeability", move |cli| {
            cli.pr_mergeability(&info, number)
        })
        .await
    }

    async fn pr_head_checks(
        &self,
        repo: &RepoInfo,
        number: i64,
    ) -> Result<HeadChecks, GitHostError> {
        let info = GitHubRepoInfo::from(repo);
        self.blocking("reading checks", move |cli| {
            cli.pr_head_checks(&info, number, CI_LOG_TAIL_BYTES)
        })
        .await
    }

    async fn latest_successful_job(
        &self,
        repo: &RepoInfo,
        job_name: &str,
        git_ref: &str,
    ) -> Result<Option<JobRun>, GitHostError> {
        let info = GitHubRepoInfo::from(repo);
        let job_name = job_name.to_string();
        let git_ref = git_ref.to_string();
        self.blocking("listing workflow runs", move |cli| {
            cli.latest_successful_run(&info, &job_name, &git_ref)
        })
        .await
    }

    async fn is_ancestor(
        &self,
        repo: &RepoInfo,
        ancestor: &str,
        descendant: &str,
    ) -> Result<bool, GitHostError> {
        let info = GitHubRepoInfo::from(repo);
        let ancestor = ancestor.to_string();
        let descendant = descendant.to_string();
        self.blocking("comparing commits", move |cli| {
            cli.is_ancestor(&info, &ancestor, &descendant)
        })
        .await
    }

    async fn pr_merge_commit(
        &self,
        repo: &RepoInfo,
        number: i64,
    ) -> Result<Option<String>, GitHostError> {
        let info = GitHubRepoInfo::from(repo);
        self.blocking("reading a merge commit", move |cli| {
            cli.pr_merge_commit(&info, number)
        })
        .await
    }

    async fn user_display_name(
        &self,
        repo: &RepoInfo,
        login: &str,
    ) -> Result<Option<String>, GitHostError> {
        let info = GitHubRepoInfo::from(repo);
        let login = login.to_string();
        self.blocking("reading a user", move |cli| {
            cli.user_display_name(&info, &login)
        })
        .await
    }

    async fn create_pr_note(
        &self,
        repo: &RepoInfo,
        number: i64,
        body: &str,
    ) -> Result<(), GitHostError> {
        let info = GitHubRepoInfo::from(repo);
        let body = body.to_string();
        self.blocking("commenting on a PR", move |cli| {
            cli.create_pr_comment(&info, number, &body)
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
        let info = GitHubRepoInfo::from(repo);
        let anchor = anchor.clone();
        let body = body.to_string();
        self.blocking("posting a review comment", move |cli| {
            cli.create_review_comment(&info, number, &anchor, &body)
        })
        .await
    }

    /// Conversation comments are never threads on GitHub, so a plain comment
    /// already blocks nothing.
    async fn create_pr_note_unresolvable(
        &self,
        repo: &RepoInfo,
        number: i64,
        body: &str,
    ) -> Result<(), GitHostError> {
        self.create_pr_note(repo, number, body).await
    }
}
