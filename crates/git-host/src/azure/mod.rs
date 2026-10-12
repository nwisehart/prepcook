//! Azure DevOps hosting service implementation.

mod cli;

use std::{path::Path, time::Duration};

use async_trait::async_trait;
use backon::{ExponentialBuilder, Retryable};
pub use cli::AzCli;
use cli::{AzCliError, AzureRepoInfo};
use tokio::task;
use tracing::info;

use crate::{
    GitHostProvider,
    types::{
        CreatePrRequest, GitHostError, HeadChecks, JobRun, Mergeability, NoteAnchor, ProviderKind,
        PullRequestDetail, RepoInfo, UnifiedPrComment,
    },
};

#[derive(Debug, Clone)]
pub struct AzureDevOpsProvider {
    az_cli: AzCli,
}

impl AzureDevOpsProvider {
    pub fn new() -> Result<Self, GitHostError> {
        Ok(Self {
            az_cli: AzCli::new(),
        })
    }

    async fn get_repo_info(
        &self,
        repo_path: &Path,
        remote_url: &str,
    ) -> Result<AzureRepoInfo, GitHostError> {
        let cli = self.az_cli.clone();
        let path = repo_path.to_path_buf();
        let url = remote_url.to_string();
        task::spawn_blocking(move || cli.get_repo_info(&path, &url))
            .await
            .map_err(|err| GitHostError::Repository(format!("Failed to get repo info: {err}")))?
            .map_err(Into::into)
    }
}

impl From<AzCliError> for GitHostError {
    fn from(error: AzCliError) -> Self {
        match &error {
            AzCliError::AuthFailed(msg) => GitHostError::AuthFailed(msg.clone()),
            AzCliError::NotAvailable => GitHostError::CliNotInstalled {
                provider: ProviderKind::AzureDevOps,
            },
            AzCliError::CommandFailed(msg) => {
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
            AzCliError::UnexpectedOutput(msg) => GitHostError::UnexpectedOutput(msg.clone()),
        }
    }
}

#[async_trait]
impl GitHostProvider for AzureDevOpsProvider {
    async fn create_pr(
        &self,
        repo_path: &Path,
        remote_url: &str,
        request: &CreatePrRequest,
    ) -> Result<PullRequestDetail, GitHostError> {
        if let Some(head_url) = &request.head_repo_url
            && head_url != remote_url
        {
            return Err(GitHostError::PullRequest(
                "Cross-fork pull requests are not supported for Azure DevOps".to_string(),
            ));
        }

        let repo_info = self.get_repo_info(repo_path, remote_url).await?;

        (|| async {
            let cli = self.az_cli.clone();
            let request_clone = request.clone();
            let organization_url = repo_info.organization_url.clone();
            let project = repo_info.project.clone();
            let repo_name = repo_info.repo_name.clone();

            let cli_result = task::spawn_blocking(move || {
                cli.create_pr(&request_clone, &organization_url, &project, &repo_name)
            })
            .await
            .map_err(|err| {
                GitHostError::PullRequest(format!(
                    "Failed to execute Azure CLI for PR creation: {err}"
                ))
            })?
            .map_err(GitHostError::from)?;

            info!(
                "Created Azure DevOps PR #{} for branch {}",
                cli_result.number, request.head_branch
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
                "Azure DevOps API call failed, retrying after {:.2}s: {}",
                dur.as_secs_f64(),
                err
            );
        })
        .await
    }

    async fn get_pr_status(&self, pr_url: &str) -> Result<PullRequestDetail, GitHostError> {
        (|| async {
            let cli = self.az_cli.clone();
            let url = pr_url.to_string();

            let pr = task::spawn_blocking(move || cli.view_pr(&url))
                .await
                .map_err(|err| {
                    GitHostError::PullRequest(format!(
                        "Failed to execute Azure CLI for viewing PR: {err}"
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
                "Azure DevOps API call failed, retrying after {:.2}s: {}",
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
        let repo_info = self.get_repo_info(repo_path, remote_url).await?;

        (|| async {
            let cli = self.az_cli.clone();
            let organization_url = repo_info.organization_url.clone();
            let project = repo_info.project.clone();
            let repo_name = repo_info.repo_name.clone();
            let branch = branch_name.to_string();

            let prs = task::spawn_blocking(move || {
                cli.list_prs_for_branch(&organization_url, &project, &repo_name, &branch)
            })
            .await
            .map_err(|err| {
                GitHostError::PullRequest(format!(
                    "Failed to execute Azure CLI for listing PRs: {err}"
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
                "Azure DevOps API call failed, retrying after {:.2}s: {}",
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
        let repo_info = self.get_repo_info(repo_path, remote_url).await?;

        (|| async {
            let cli = self.az_cli.clone();
            let organization_url = repo_info.organization_url.clone();
            let project_id = repo_info.project_id.clone();
            let repo_id = repo_info.repo_id.clone();

            let comments = task::spawn_blocking(move || {
                cli.get_pr_threads(&organization_url, &project_id, &repo_id, pr_number)
            })
            .await
            .map_err(|err| {
                GitHostError::PullRequest(format!(
                    "Failed to execute Azure CLI for fetching PR comments: {err}"
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
                "Azure DevOps API call failed, retrying after {:.2}s: {}",
                dur.as_secs_f64(),
                err
            );
        })
        .await
    }

    async fn list_open_prs(
        &self,
        _repo_path: &Path,
        _remote_url: &str,
    ) -> Result<Vec<PullRequestDetail>, GitHostError> {
        // TODO: Implement list_open_prs for Azure DevOps
        Err(GitHostError::UnsupportedProvider)
    }

    async fn checkout_pr(
        &self,
        _repo_path: &Path,
        _remote_url: &str,
        _pr_number: i64,
    ) -> Result<(), GitHostError> {
        // TODO: Implement checkout_pr for Azure DevOps
        Err(GitHostError::UnsupportedProvider)
    }

    fn provider_kind(&self) -> ProviderKind {
        ProviderKind::AzureDevOps
    }
    fn repo_info(&self, _remote_url: &str) -> Result<RepoInfo, GitHostError> {
        Err(unsupported("repository info"))
    }

    fn parse_pr_url(&self, _pr_url: &str) -> Result<(RepoInfo, i64), GitHostError> {
        Err(unsupported("pull request URL parsing"))
    }

    async fn set_pr_reviewers(
        &self,
        _repo: &RepoInfo,
        _number: i64,
        _logins: &[String],
    ) -> Result<(), GitHostError> {
        Err(unsupported("reviewer assignment"))
    }

    async fn set_pr_labels(
        &self,
        _repo: &RepoInfo,
        _number: i64,
        _labels: &[String],
    ) -> Result<(), GitHostError> {
        Err(unsupported("labels"))
    }

    async fn pr_mergeability(
        &self,
        _repo: &RepoInfo,
        _number: i64,
    ) -> Result<Mergeability, GitHostError> {
        Err(unsupported("mergeability"))
    }

    async fn pr_head_checks(
        &self,
        _repo: &RepoInfo,
        _number: i64,
    ) -> Result<HeadChecks, GitHostError> {
        Err(unsupported("pipeline status"))
    }

    async fn latest_successful_job(
        &self,
        _repo: &RepoInfo,
        _job_name: &str,
        _git_ref: &str,
    ) -> Result<Option<JobRun>, GitHostError> {
        Err(unsupported("job history"))
    }

    async fn is_ancestor(
        &self,
        _repo: &RepoInfo,
        _ancestor: &str,
        _descendant: &str,
    ) -> Result<bool, GitHostError> {
        Err(unsupported("commit ancestry"))
    }

    async fn pr_merge_commit(
        &self,
        _repo: &RepoInfo,
        _number: i64,
    ) -> Result<Option<String>, GitHostError> {
        Err(unsupported("merge commit lookup"))
    }

    async fn user_display_name(
        &self,
        _repo: &RepoInfo,
        _login: &str,
    ) -> Result<Option<String>, GitHostError> {
        Err(unsupported("user lookup"))
    }

    async fn create_pr_note(
        &self,
        _repo: &RepoInfo,
        _number: i64,
        _body: &str,
    ) -> Result<(), GitHostError> {
        Err(unsupported("pull request comments"))
    }

    async fn create_pr_note_at(
        &self,
        _repo: &RepoInfo,
        _number: i64,
        _anchor: &NoteAnchor,
        _body: &str,
    ) -> Result<(), GitHostError> {
        Err(unsupported("line comments"))
    }

    async fn create_pr_note_unresolvable(
        &self,
        _repo: &RepoInfo,
        _number: i64,
        _body: &str,
    ) -> Result<(), GitHostError> {
        Err(unsupported("pull request comments"))
    }
}

fn unsupported(what: &str) -> GitHostError {
    GitHostError::Unsupported(format!("{what} on Azure DevOps"))
}
