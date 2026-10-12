//! Minimal helpers around the GitHub CLI (`gh`).
//!
//! This module provides low-level access to the GitHub CLI for operations
//! the REST client does not cover well.

use std::{
    ffi::{OsStr, OsString},
    io::Write,
    path::Path,
    process::Command,
};

use chrono::{DateTime, Utc};
use db::models::merge::MergeStatus;
use serde::Deserialize;
use tempfile::NamedTempFile;
use thiserror::Error;
use url::Url;
use utils::{command_ext::NoWindowExt, shell::resolve_executable_path_blocking};

use crate::types::{
    CheckStatus, CreatePrRequest, FailedJob, HeadChecks, JobRun, Mergeability, NoteAnchor,
    NoteSide, PrComment, PrCommentAuthor, PrReviewComment, PullRequestDetail, RepoInfo,
    ReviewCommentUser,
};

#[derive(Debug, Clone)]
pub struct GitHubRepoInfo {
    pub owner: String,
    pub repo_name: String,
    /// GitHub hostname (e.g., "github.com" or enterprise hostname)
    pub hostname: Option<String>,
}

impl GitHubRepoInfo {
    pub fn repo_spec(&self) -> String {
        match &self.hostname {
            Some(host) => format!("{}/{}/{}", host, self.owner, self.repo_name),
            None => format!("{}/{}", self.owner, self.repo_name),
        }
    }

    /// `gh api <endpoint>`, aimed at an Enterprise host when there is one.
    fn api_args(&self, endpoint: String) -> Vec<String> {
        let mut args = vec!["api".to_string(), endpoint];
        if let Some(host) = &self.hostname {
            args.push("--hostname".to_string());
            args.push(host.clone());
        }
        args
    }
}

impl From<&RepoInfo> for GitHubRepoInfo {
    fn from(repo: &RepoInfo) -> Self {
        Self {
            owner: repo.owner.clone(),
            repo_name: repo.name.clone(),
            // github.com is gh's default; naming it would only add noise.
            hostname: (!repo.host.eq_ignore_ascii_case("github.com")).then(|| repo.host.clone()),
        }
    }
}

#[derive(Deserialize)]
struct GhRepoViewResponse {
    owner: GhRepoOwner,
    name: String,
    url: String,
}

#[derive(Deserialize)]
struct GhRepoOwner {
    login: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhCommentResponse {
    id: String,
    author: Option<GhUserLogin>,
    #[serde(default)]
    author_association: String,
    #[serde(default)]
    body: String,
    created_at: Option<DateTime<Utc>>,
    #[serde(default)]
    url: String,
}

#[derive(Deserialize)]
struct GhCommentsWrapper {
    comments: Vec<GhCommentResponse>,
}

#[derive(Deserialize)]
struct GhUserLogin {
    login: Option<String>,
}

#[derive(Deserialize)]
struct GhReviewCommentResponse {
    id: i64,
    user: Option<GhUserLogin>,
    #[serde(default)]
    body: String,
    created_at: Option<DateTime<Utc>>,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    path: String,
    line: Option<i64>,
    side: Option<String>,
    #[serde(default)]
    diff_hunk: String,
    #[serde(default)]
    author_association: String,
}

#[derive(Deserialize)]
struct GhMergeCommit {
    oid: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhPrResponse {
    number: i64,
    url: String,
    #[serde(default)]
    state: String,
    merged_at: Option<DateTime<Utc>>,
    merge_commit: Option<GhMergeCommit>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    base_ref_name: Option<String>,
    #[serde(default)]
    head_ref_name: Option<String>,
    #[serde(default)]
    updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Error)]
pub enum GhCliError {
    #[error("GitHub CLI (`gh`) executable not found or not runnable")]
    NotAvailable,
    #[error("GitHub CLI command failed: {0}")]
    CommandFailed(String),
    #[error("GitHub CLI authentication failed: {0}")]
    AuthFailed(String),
    #[error("GitHub CLI returned unexpected output: {0}")]
    UnexpectedOutput(String),
}

#[derive(Debug, Clone, Default)]
pub struct GhCli;

impl GhCli {
    pub fn new() -> Self {
        Self {}
    }

    /// Ensure the GitHub CLI binary is discoverable.
    fn ensure_available(&self) -> Result<(), GhCliError> {
        resolve_executable_path_blocking("gh").ok_or(GhCliError::NotAvailable)?;
        Ok(())
    }

    fn run<I, S>(&self, args: I, dir: Option<&Path>) -> Result<String, GhCliError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.ensure_available()?;
        let gh = resolve_executable_path_blocking("gh").ok_or(GhCliError::NotAvailable)?;
        let mut cmd = Command::new(&gh);
        if let Some(d) = dir {
            cmd.current_dir(d);
        }
        for arg in args {
            cmd.arg(arg);
        }
        let output = cmd
            .no_window()
            .output()
            .map_err(|err| GhCliError::CommandFailed(err.to_string()))?;

        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).to_string());
        }

        // `gh api` prints the host's error body on stdout and its own summary
        // ("Validation Failed (HTTP 422)") on stderr; the body is what says
        // which field was wrong, so both are kept.
        let stderr_raw = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout_raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr = if stdout_raw.is_empty() {
            stderr_raw
        } else {
            format!("{stderr_raw}\n{stdout_raw}")
        };

        // Check exit code first - gh CLI uses exit code 4 for auth failures
        if output.status.code() == Some(4) {
            return Err(GhCliError::AuthFailed(stderr));
        }

        // Fall back to string matching for older gh versions or other auth scenarios
        let lower = stderr.to_ascii_lowercase();
        if lower.contains("authentication failed")
            || lower.contains("must authenticate")
            || lower.contains("bad credentials")
            || lower.contains("unauthorized")
            || lower.contains("gh auth login")
        {
            return Err(GhCliError::AuthFailed(stderr));
        }

        Err(GhCliError::CommandFailed(stderr))
    }

    pub fn get_repo_info(
        &self,
        remote_url: &str,
        repo_path: &Path,
    ) -> Result<GitHubRepoInfo, GhCliError> {
        let raw = self.run(
            ["repo", "view", remote_url, "--json", "owner,name,url"],
            Some(repo_path),
        )?;
        Self::parse_repo_info_response(&raw)
    }

    fn parse_repo_info_response(raw: &str) -> Result<GitHubRepoInfo, GhCliError> {
        let resp: GhRepoViewResponse = serde_json::from_str(raw).map_err(|e| {
            GhCliError::UnexpectedOutput(format!("Failed to parse gh repo view response: {e}"))
        })?;

        let hostname = Url::parse(&resp.url)
            .ok()
            .and_then(|u| u.host_str().map(String::from));

        Ok(GitHubRepoInfo {
            owner: resp.owner.login,
            repo_name: resp.name,
            hostname,
        })
    }

    /// Run `gh pr create` and parse the response.
    ///
    /// The `repo_path` parameter specifies the working directory for the command.
    /// This is required for compatibility with older `gh` CLI versions (e.g., v2.4.0)
    /// that require running from within a git repository.
    pub fn create_pr(
        &self,
        request: &CreatePrRequest,
        repo_info: &GitHubRepoInfo,
        repo_path: &Path,
    ) -> Result<PullRequestDetail, GhCliError> {
        // Write body to temp file to avoid shell escaping and length issues
        let body = request.body.as_deref().unwrap_or("");
        let mut body_file = NamedTempFile::new()
            .map_err(|e| GhCliError::CommandFailed(format!("Failed to create temp file: {e}")))?;
        body_file
            .write_all(body.as_bytes())
            .map_err(|e| GhCliError::CommandFailed(format!("Failed to write body: {e}")))?;

        let repo_spec = repo_info.repo_spec();

        let mut args: Vec<OsString> = Vec::with_capacity(14);
        args.push(OsString::from("pr"));
        args.push(OsString::from("create"));
        args.push(OsString::from("--repo"));
        args.push(OsString::from(&repo_spec));
        args.push(OsString::from("--head"));
        args.push(OsString::from(&request.head_branch));
        args.push(OsString::from("--base"));
        args.push(OsString::from(&request.base_branch));
        args.push(OsString::from("--title"));
        args.push(OsString::from(&request.title));
        args.push(OsString::from("--body-file"));
        args.push(body_file.path().as_os_str().to_os_string());

        if request.draft.unwrap_or(false) {
            args.push(OsString::from("--draft"));
        }

        let raw = self.run(args, Some(repo_path))?;
        Self::parse_pr_create_text(&raw, request)
    }

    /// Retrieve details for a pull request by URL.
    pub fn view_pr(&self, pr_url: &str) -> Result<PullRequestDetail, GhCliError> {
        let raw = self.run(
            [
                "pr",
                "view",
                pr_url,
                "--json",
                "number,url,state,mergedAt,mergeCommit,title,baseRefName,headRefName",
            ],
            None,
        )?;
        Self::parse_pr_view(&raw)
    }

    /// List pull requests for a branch (includes closed/merged).
    pub fn list_prs_for_branch(
        &self,
        repo_info: &GitHubRepoInfo,
        branch: &str,
    ) -> Result<Vec<PullRequestDetail>, GhCliError> {
        let repo_spec = repo_info.repo_spec();
        let raw = self.run(
            [
                "pr",
                "list",
                "--repo",
                &repo_spec,
                "--state",
                "all",
                "--head",
                branch,
                "--json",
                "number,url,title,headRefName,baseRefName,state,mergedAt,mergeCommit",
            ],
            None,
        )?;
        Self::parse_pr_list(&raw)
    }

    pub fn list_prs(&self, owner: &str, repo: &str) -> Result<Vec<PullRequestDetail>, GhCliError> {
        let repo_spec = format!("{owner}/{repo}");
        let json_fields =
            "number,url,title,headRefName,baseRefName,state,mergedAt,mergeCommit,updatedAt";

        let open_raw = self.run(
            [
                "pr",
                "list",
                "--repo",
                &repo_spec,
                "--state",
                "open",
                "--json",
                json_fields,
            ],
            None,
        )?;

        let closed_raw = self.run(
            [
                "pr",
                "list",
                "--repo",
                &repo_spec,
                "--state",
                "closed",
                "--limit",
                "20",
                "--json",
                json_fields,
            ],
            None,
        )?;

        let mut open_prs: Vec<GhPrResponse> =
            serde_json::from_str(open_raw.trim()).map_err(|err| {
                GhCliError::UnexpectedOutput(format!(
                    "Failed to parse gh pr list (open) response: {err}; raw: {open_raw}"
                ))
            })?;
        let closed_prs: Vec<GhPrResponse> =
            serde_json::from_str(closed_raw.trim()).map_err(|err| {
                GhCliError::UnexpectedOutput(format!(
                    "Failed to parse gh pr list (closed) response: {err}; raw: {closed_raw}"
                ))
            })?;

        open_prs.extend(closed_prs);
        open_prs.sort_by_key(|pr| std::cmp::Reverse(pr.updated_at));

        Ok(open_prs
            .into_iter()
            .map(Self::pr_response_to_detail)
            .collect())
    }

    /// Fetch comments for a pull request.
    pub fn get_pr_comments(
        &self,
        repo_info: &GitHubRepoInfo,
        pr_number: i64,
    ) -> Result<Vec<PrComment>, GhCliError> {
        let repo_spec = repo_info.repo_spec();
        let raw = self.run(
            [
                "pr",
                "view",
                &pr_number.to_string(),
                "--repo",
                &repo_spec,
                "--json",
                "comments",
            ],
            None,
        )?;
        Self::parse_pr_comments(&raw)
    }

    /// Fetch inline review comments for a pull request via API.
    pub fn get_pr_review_comments(
        &self,
        repo_info: &GitHubRepoInfo,
        pr_number: i64,
    ) -> Result<Vec<PrReviewComment>, GhCliError> {
        let mut args = vec![
            "api".to_string(),
            format!(
                "repos/{}/{}/pulls/{}/comments",
                repo_info.owner, repo_info.repo_name, pr_number
            ),
        ];
        if let Some(ref host) = repo_info.hostname {
            args.push("--hostname".to_string());
            args.push(host.clone());
        }
        let raw = self.run(args, None)?;
        Self::parse_pr_review_comments(&raw)
    }

    pub fn pr_checkout(
        &self,
        repo_path: &Path,
        repo_info: &GitHubRepoInfo,
        pr_number: i64,
    ) -> Result<(), GhCliError> {
        self.run(
            [
                "pr",
                "checkout",
                &pr_number.to_string(),
                "--repo",
                // repo_spec() keeps the hostname for GitHub Enterprise repos.
                &repo_info.repo_spec(),
                "--force",
            ],
            Some(repo_path),
        )?;
        Ok(())
    }
}

impl GhCli {
    /// `gh pr <subcommand> <number> --repo <spec> …`.
    fn pr_args(subcommand: &str, repo: &GitHubRepoInfo, number: i64) -> Vec<String> {
        vec![
            "pr".to_string(),
            subcommand.to_string(),
            number.to_string(),
            "--repo".to_string(),
            repo.repo_spec(),
        ]
    }

    /// Requests review from the given logins (`gh pr edit --add-reviewer`).
    pub fn set_pr_reviewers(
        &self,
        repo: &GitHubRepoInfo,
        number: i64,
        logins: &[String],
    ) -> Result<(), GhCliError> {
        if logins.is_empty() {
            return Ok(());
        }
        let mut args = Self::pr_args("edit", repo, number);
        args.push("--add-reviewer".to_string());
        args.push(logins.join(","));
        self.run(args, None).map(|_| ())
    }

    /// Adds labels to a pull request (`gh pr edit --add-label`).
    pub fn set_pr_labels(
        &self,
        repo: &GitHubRepoInfo,
        number: i64,
        labels: &[String],
    ) -> Result<(), GhCliError> {
        if labels.is_empty() {
            return Ok(());
        }
        let mut args = Self::pr_args("edit", repo, number);
        args.push("--add-label".to_string());
        args.push(labels.join(","));
        self.run(args, None).map(|_| ())
    }

    fn pr_view_json(
        &self,
        repo: &GitHubRepoInfo,
        number: i64,
        fields: &str,
    ) -> Result<String, GhCliError> {
        let mut args = Self::pr_args("view", repo, number);
        args.push("--json".to_string());
        args.push(fields.to_string());
        self.run(args, None)
    }

    pub fn pr_mergeability(
        &self,
        repo: &GitHubRepoInfo,
        number: i64,
    ) -> Result<Mergeability, GhCliError> {
        let raw = self.pr_view_json(repo, number, "state,mergeable,mergeStateStatus,headRefOid")?;
        parse_mergeability(&raw)
    }

    /// The head commit's check runs and commit statuses, with the tail of
    /// each failed GitHub Actions job's log.
    ///
    /// Reads `statusCheckRollup` through `gh pr view` rather than `gh pr
    /// checks`: one call returns the head commit with the checks, and it
    /// exits 0 whatever the checks say (`gh pr checks` signals failing and
    /// pending checks through its exit code).
    pub fn pr_head_checks(
        &self,
        repo: &GitHubRepoInfo,
        number: i64,
        log_tail_bytes: usize,
    ) -> Result<HeadChecks, GhCliError> {
        let raw = self.pr_view_json(repo, number, "headRefOid,statusCheckRollup")?;
        let mut checks = parse_head_checks(&raw)?;
        for job in &mut checks.failed_jobs {
            let Some(job_id) = job.url.as_deref().and_then(actions_job_id) else {
                continue;
            };
            // Best effort: a log that cannot be read leaves the job named.
            job.log_tail = self
                .run(
                    [
                        "run".to_string(),
                        "view".to_string(),
                        "--job".to_string(),
                        job_id,
                        "--log-failed".to_string(),
                        "--repo".to_string(),
                        repo.repo_spec(),
                    ],
                    None,
                )
                .ok()
                .map(|log| crate::types::tail_bytes(log, log_tail_bytes));
        }
        Ok(checks)
    }

    /// The newest successful run of workflow `workflow` on `branch`.
    pub fn latest_successful_run(
        &self,
        repo: &GitHubRepoInfo,
        workflow: &str,
        branch: &str,
    ) -> Result<Option<JobRun>, GhCliError> {
        let raw = self.run(
            [
                "run",
                "list",
                "--repo",
                &repo.repo_spec(),
                "--workflow",
                workflow,
                "--branch",
                branch,
                "--status",
                "success",
                "--limit",
                "20",
                "--json",
                "headSha,updatedAt,url",
            ],
            None,
        )?;
        parse_latest_run(&raw)
    }

    /// Whether `ancestor` is in `descendant`'s history, from the compare API:
    /// `ahead` or `identical` means every commit of `ancestor` is contained.
    pub fn is_ancestor(
        &self,
        repo: &GitHubRepoInfo,
        ancestor: &str,
        descendant: &str,
    ) -> Result<bool, GhCliError> {
        let mut args = repo.api_args(format!(
            "repos/{}/{}/compare/{ancestor}...{descendant}",
            repo.owner, repo.repo_name
        ));
        args.push("--jq".to_string());
        args.push(".status".to_string());
        let status = self.run(args, None)?;
        Ok(matches!(status.trim(), "ahead" | "identical"))
    }

    pub fn pr_merge_commit(
        &self,
        repo: &GitHubRepoInfo,
        number: i64,
    ) -> Result<Option<String>, GhCliError> {
        let raw = self.pr_view_json(repo, number, "state,mergeCommit")?;
        parse_merge_commit(&raw)
    }

    pub fn user_display_name(
        &self,
        repo: &GitHubRepoInfo,
        login: &str,
    ) -> Result<Option<String>, GhCliError> {
        let raw = self.run(repo.api_args(format!("users/{login}")), None)?;
        let user: serde_json::Value = serde_json::from_str(raw.trim())
            .map_err(|e| GhCliError::UnexpectedOutput(format!("Bad user JSON: {e}")))?;
        Ok(user
            .get("name")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string))
    }

    /// Posts a conversation comment (`gh pr comment`). These are never
    /// threads, so nothing about them blocks a merge.
    pub fn create_pr_comment(
        &self,
        repo: &GitHubRepoInfo,
        number: i64,
        body: &str,
    ) -> Result<(), GhCliError> {
        let mut body_file = NamedTempFile::new()
            .map_err(|e| GhCliError::CommandFailed(format!("Failed to create temp file: {e}")))?;
        body_file
            .write_all(body.as_bytes())
            .map_err(|e| GhCliError::CommandFailed(format!("Failed to write body: {e}")))?;
        let mut args: Vec<OsString> = Self::pr_args("comment", repo, number)
            .into_iter()
            .map(OsString::from)
            .collect();
        args.push(OsString::from("--body-file"));
        args.push(body_file.path().as_os_str().to_os_string());
        self.run(args, None).map(|_| ())
    }

    /// Posts a review comment on a file, or a line or range of it, at the
    /// pull request's head commit.
    pub fn create_review_comment(
        &self,
        repo: &GitHubRepoInfo,
        number: i64,
        anchor: &NoteAnchor,
        body: &str,
    ) -> Result<(), GhCliError> {
        let head = self.pr_view_json(repo, number, "headRefOid")?;
        let head: serde_json::Value = serde_json::from_str(head.trim())
            .map_err(|e| GhCliError::UnexpectedOutput(format!("Bad PR JSON: {e}")))?;
        let head_sha = head
            .get("headRefOid")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| GhCliError::UnexpectedOutput("PR has no head commit".to_string()))?
            .to_string();

        let mut args = repo.api_args(format!(
            "repos/{}/{}/pulls/{number}/comments",
            repo.owner, repo.repo_name
        ));
        args.extend(["--method".to_string(), "POST".to_string()]);
        args.extend(review_comment_fields(anchor, &head_sha, body));
        self.run(args, None).map(|_| ())
    }
}

/// The `gh api` field flags for one review comment. `-f` sends strings
/// verbatim (no `@file` expansion); `-F` sends the line numbers as integers.
fn review_comment_fields(anchor: &NoteAnchor, head_sha: &str, body: &str) -> Vec<String> {
    let mut fields = vec![
        "-f".to_string(),
        format!("body={body}"),
        "-f".to_string(),
        format!("commit_id={head_sha}"),
        "-f".to_string(),
        format!("path={}", anchor.path),
    ];
    let side = match anchor.side {
        NoteSide::Old => "LEFT",
        NoteSide::New => "RIGHT",
    };
    match (anchor.line, anchor.end_line) {
        (None, _) => {
            fields.push("-f".to_string());
            fields.push("subject_type=file".to_string());
        }
        (Some(start), Some(end)) if end > start => {
            fields.extend([
                "-F".to_string(),
                format!("start_line={start}"),
                "-f".to_string(),
                format!("start_side={side}"),
                "-F".to_string(),
                format!("line={end}"),
                "-f".to_string(),
                format!("side={side}"),
            ]);
        }
        (Some(line), _) => {
            fields.extend([
                "-F".to_string(),
                format!("line={line}"),
                "-f".to_string(),
                format!("side={side}"),
            ]);
        }
    }
    fields
}

/// Parses a git remote of a GitHub (or Enterprise) host. The repository is
/// the first two path segments, so a remote with a trailing path still names
/// its repository.
pub fn parse_remote_url(remote_url: &str) -> Result<RepoInfo, crate::GitHostError> {
    let (host, path) = crate::remote::split_remote(remote_url)?;
    let mut segments = path.split('/').filter(|segment| !segment.is_empty());
    match (segments.next(), segments.next()) {
        (Some(owner), Some(name)) => Ok(RepoInfo::new(
            crate::ProviderKind::GitHub,
            &host,
            owner,
            name.trim_end_matches(".git"),
        )),
        _ => Err(crate::GitHostError::Repository(format!(
            "Cannot derive a GitHub owner/repo from '{remote_url}'"
        ))),
    }
}

/// Parses `https://<host>/<owner>/<repo>/pull/<number>[/files|#…]`.
pub fn parse_pr_url(pr_url: &str) -> Result<(RepoInfo, i64), crate::GitHostError> {
    let (repo_part, number_part) = pr_url.trim().split_once("/pull/").ok_or_else(|| {
        crate::GitHostError::UnexpectedOutput(format!("Not a GitHub pull request URL: {pr_url}"))
    })?;
    let number = crate::remote::trailing_number(number_part, pr_url)?;
    Ok((parse_remote_url(repo_part)?, number))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhMergeabilityResponse {
    #[serde(default)]
    state: String,
    #[serde(default)]
    mergeable: String,
    #[serde(default)]
    merge_state_status: String,
    #[serde(default)]
    head_ref_oid: String,
}

/// `gh pr view --json state,mergeable,mergeStateStatus,headRefOid`.
///
/// `mergeable` is `MERGEABLE`, `CONFLICTING` or `UNKNOWN` (still computing).
/// `mergeStateStatus` adds the branch-protection view: `CLEAN`, `UNSTABLE`
/// (mergeable with failing optional checks), `HAS_HOOKS`, `BLOCKED` (reviews
/// or required checks), `BEHIND` (protection demands an up-to-date branch),
/// `DIRTY` (conflicts), `DRAFT`, `UNKNOWN`. Both `DIRTY` and `BEHIND` are
/// fixed only by bringing the branch onto its target.
pub(crate) fn parse_mergeability(raw: &str) -> Result<Mergeability, GhCliError> {
    let pr: GhMergeabilityResponse = serde_json::from_str(raw.trim())
        .map_err(|e| GhCliError::UnexpectedOutput(format!("Bad mergeability JSON: {e}")))?;
    let open = pr.state.eq_ignore_ascii_case("OPEN");
    let mergeable = pr.mergeable.to_ascii_uppercase();
    let merge_state = pr.merge_state_status.to_ascii_uppercase();
    let has_conflicts = mergeable == "CONFLICTING" || merge_state == "DIRTY";
    Ok(Mergeability {
        mergeable: open
            && mergeable == "MERGEABLE"
            && matches!(merge_state.as_str(), "CLEAN" | "UNSTABLE" | "HAS_HOOKS"),
        needs_rebase: open && (has_conflicts || merge_state == "BEHIND"),
        has_conflicts,
        checking: mergeable == "UNKNOWN" || merge_state == "UNKNOWN",
        detail: merge_state,
        head_sha: pr.head_ref_oid,
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhChecksResponse {
    #[serde(default)]
    head_ref_oid: String,
    #[serde(default)]
    status_check_rollup: Vec<GhRollupItem>,
}

/// One entry of `statusCheckRollup`: a `CheckRun` (Actions and apps) or a
/// `StatusContext` (the older commit-status API external CI posts to).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhRollupItem {
    #[serde(rename = "__typename", default)]
    typename: String,
    // CheckRun
    #[serde(default)]
    name: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    conclusion: String,
    #[serde(default)]
    details_url: Option<String>,
    // StatusContext
    #[serde(default)]
    context: String,
    #[serde(default)]
    state: String,
    #[serde(default)]
    target_url: Option<String>,
}

enum RollupOutcome {
    Pending,
    Success,
    Failed,
    Canceled,
}

impl GhRollupItem {
    fn outcome(&self) -> RollupOutcome {
        if self.typename == "StatusContext" {
            return match self.state.to_ascii_uppercase().as_str() {
                "SUCCESS" => RollupOutcome::Success,
                "FAILURE" | "ERROR" => RollupOutcome::Failed,
                _ => RollupOutcome::Pending, // PENDING, EXPECTED
            };
        }
        if !self.status.eq_ignore_ascii_case("COMPLETED") {
            return RollupOutcome::Pending;
        }
        match self.conclusion.to_ascii_uppercase().as_str() {
            // A job skipped by its own condition is part of a green run.
            "SUCCESS" | "NEUTRAL" | "SKIPPED" => RollupOutcome::Success,
            "FAILURE" | "TIMED_OUT" | "STARTUP_FAILURE" | "ACTION_REQUIRED" => {
                RollupOutcome::Failed
            }
            "CANCELLED" | "STALE" => RollupOutcome::Canceled,
            _ => RollupOutcome::Pending,
        }
    }

    fn display_name(&self) -> String {
        if self.typename == "StatusContext" {
            self.context.clone()
        } else {
            self.name.clone()
        }
    }

    fn link(&self) -> Option<String> {
        self.details_url
            .clone()
            .or_else(|| self.target_url.clone())
            .filter(|url| !url.is_empty())
    }
}

/// `gh pr view --json headRefOid,statusCheckRollup`, folded into one status
/// the way a GitLab pipeline reports one: anything unfinished keeps the whole
/// `Pending` (a pipeline is not `failed` while jobs still run), then any
/// failure is `Failed`, then any cancellation is `Canceled`. Logs are not
/// read here.
pub(crate) fn parse_head_checks(raw: &str) -> Result<HeadChecks, GhCliError> {
    let pr: GhChecksResponse = serde_json::from_str(raw.trim())
        .map_err(|e| GhCliError::UnexpectedOutput(format!("Bad checks JSON: {e}")))?;
    if pr.status_check_rollup.is_empty() {
        return Ok(HeadChecks {
            status: CheckStatus::None,
            sha: pr.head_ref_oid,
            failed_jobs: Vec::new(),
        });
    }
    let (mut pending, mut canceled) = (false, false);
    let mut failed_jobs = Vec::new();
    for item in &pr.status_check_rollup {
        match item.outcome() {
            RollupOutcome::Pending => pending = true,
            RollupOutcome::Canceled => canceled = true,
            RollupOutcome::Success => {}
            RollupOutcome::Failed => failed_jobs.push(FailedJob {
                name: item.display_name(),
                url: item.link(),
                log_tail: None,
            }),
        }
    }
    let status = if pending {
        CheckStatus::Pending
    } else if !failed_jobs.is_empty() {
        CheckStatus::Failed
    } else if canceled {
        CheckStatus::Canceled
    } else {
        CheckStatus::Success
    };
    if status != CheckStatus::Failed {
        failed_jobs.clear();
    }
    Ok(HeadChecks {
        status,
        sha: pr.head_ref_oid,
        failed_jobs,
    })
}

/// The job id in an Actions job URL,
/// `https://github.com/<owner>/<repo>/actions/runs/<run>/job/<job>`.
fn actions_job_id(url: &str) -> Option<String> {
    let (_, rest) = url.split_once("/actions/runs/")?;
    let (_, job) = rest.split_once("/job/")?;
    let id = job.split(['/', '?', '#']).next()?;
    (!id.is_empty() && id.chars().all(|c| c.is_ascii_digit())).then(|| id.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhRunResponse {
    #[serde(default)]
    head_sha: String,
    updated_at: Option<DateTime<Utc>>,
    #[serde(default)]
    url: String,
}

/// `gh run list --json headSha,updatedAt,url`, newest by finish time. A
/// successful run's `updatedAt` is when it finished; the list order is not
/// relied on.
pub(crate) fn parse_latest_run(raw: &str) -> Result<Option<JobRun>, GhCliError> {
    let runs: Vec<GhRunResponse> = serde_json::from_str(raw.trim())
        .map_err(|e| GhCliError::UnexpectedOutput(format!("Bad run list JSON: {e}")))?;
    Ok(runs
        .into_iter()
        .filter(|run| !run.head_sha.is_empty())
        .filter_map(|run| {
            Some(JobRun {
                finished_at: run.updated_at?,
                sha: run.head_sha,
                url: (!run.url.is_empty()).then_some(run.url),
            })
        })
        .max_by_key(|run| run.finished_at))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhMergeCommitResponse {
    #[serde(default)]
    state: String,
    merge_commit: Option<GhMergeCommit>,
}

/// Only a merged pull request has put a commit on its base branch; for a
/// squash or rebase merge `mergeCommit` is that squashed or last commit.
pub(crate) fn parse_merge_commit(raw: &str) -> Result<Option<String>, GhCliError> {
    let pr: GhMergeCommitResponse = serde_json::from_str(raw.trim())
        .map_err(|e| GhCliError::UnexpectedOutput(format!("Bad PR JSON: {e}")))?;
    if !pr.state.eq_ignore_ascii_case("MERGED") {
        return Ok(None);
    }
    Ok(pr.merge_commit.and_then(|commit| commit.oid))
}

impl GhCli {
    fn parse_pr_create_text(
        raw: &str,
        request: &CreatePrRequest,
    ) -> Result<PullRequestDetail, GhCliError> {
        let pr_url = raw
            .lines()
            .rev()
            .flat_map(|line| line.split_whitespace())
            .map(|token| token.trim_matches(|c: char| c == '<' || c == '>'))
            .find(|token| token.starts_with("http") && token.contains("/pull/"))
            .ok_or_else(|| {
                GhCliError::UnexpectedOutput(format!(
                    "gh pr create did not return a pull request URL; raw output: {raw}"
                ))
            })?
            .trim_end_matches(['.', ',', ';'])
            .to_string();

        let number = pr_url
            .rsplit('/')
            .next()
            .ok_or_else(|| {
                GhCliError::UnexpectedOutput(format!(
                    "Failed to extract PR number from URL '{pr_url}'"
                ))
            })?
            .trim_end_matches(|c: char| !c.is_ascii_digit())
            .parse::<i64>()
            .map_err(|err| {
                GhCliError::UnexpectedOutput(format!(
                    "Failed to parse PR number from URL '{pr_url}': {err}"
                ))
            })?;

        Ok(PullRequestDetail {
            number,
            url: pr_url,
            status: MergeStatus::Open,
            merged_at: None,
            merge_commit_sha: None,
            title: request.title.clone(),
            base_branch: request.base_branch.clone(),
            head_branch: request.head_branch.clone(),
        })
    }

    fn parse_pr_view(raw: &str) -> Result<PullRequestDetail, GhCliError> {
        let pr: GhPrResponse = serde_json::from_str(raw.trim()).map_err(|err| {
            GhCliError::UnexpectedOutput(format!(
                "Failed to parse gh pr view response: {err}; raw: {raw}"
            ))
        })?;
        Ok(Self::pr_response_to_detail(pr))
    }

    fn parse_pr_list(raw: &str) -> Result<Vec<PullRequestDetail>, GhCliError> {
        let prs: Vec<GhPrResponse> = serde_json::from_str(raw.trim()).map_err(|err| {
            GhCliError::UnexpectedOutput(format!(
                "Failed to parse gh pr list response: {err}; raw: {raw}"
            ))
        })?;
        Ok(prs.into_iter().map(Self::pr_response_to_detail).collect())
    }

    fn pr_response_to_detail(pr: GhPrResponse) -> PullRequestDetail {
        let state = if pr.state.is_empty() {
            "OPEN"
        } else {
            &pr.state
        };
        PullRequestDetail {
            number: pr.number,
            url: pr.url,
            status: match state.to_ascii_uppercase().as_str() {
                "OPEN" => MergeStatus::Open,
                "MERGED" => MergeStatus::Merged,
                "CLOSED" => MergeStatus::Closed,
                _ => MergeStatus::Unknown,
            },
            merged_at: pr.merged_at,
            merge_commit_sha: pr.merge_commit.and_then(|c| c.oid),
            title: pr.title.unwrap_or_default(),
            base_branch: pr.base_ref_name.unwrap_or_default(),
            head_branch: pr.head_ref_name.unwrap_or_default(),
        }
    }

    fn parse_pr_comments(raw: &str) -> Result<Vec<PrComment>, GhCliError> {
        let wrapper: GhCommentsWrapper = serde_json::from_str(raw.trim()).map_err(|err| {
            GhCliError::UnexpectedOutput(format!(
                "Failed to parse gh pr view --json comments response: {err}; raw: {raw}"
            ))
        })?;

        Ok(wrapper
            .comments
            .into_iter()
            .map(|c| PrComment {
                id: c.id,
                author: PrCommentAuthor {
                    login: c
                        .author
                        .and_then(|a| a.login)
                        .unwrap_or_else(|| "unknown".to_string()),
                },
                author_association: c.author_association,
                body: c.body,
                created_at: c.created_at.unwrap_or_else(Utc::now),
                url: c.url,
            })
            .collect())
    }

    fn parse_pr_review_comments(raw: &str) -> Result<Vec<PrReviewComment>, GhCliError> {
        let items: Vec<GhReviewCommentResponse> =
            serde_json::from_str(raw.trim()).map_err(|err| {
                GhCliError::UnexpectedOutput(format!(
                    "Failed to parse review comments API response: {err}; raw: {raw}"
                ))
            })?;

        Ok(items
            .into_iter()
            .map(|c| PrReviewComment {
                id: c.id,
                user: ReviewCommentUser {
                    login: c
                        .user
                        .and_then(|u| u.login)
                        .unwrap_or_else(|| "unknown".to_string()),
                },
                body: c.body,
                created_at: c.created_at.unwrap_or_else(Utc::now),
                html_url: c.html_url,
                path: c.path,
                line: c.line,
                side: c.side,
                diff_hunk: c.diff_hunk,
                author_association: c.author_association,
            })
            .collect())
    }
}

#[cfg(test)]
mod host_tests {
    use super::*;
    use crate::ProviderKind;

    #[test]
    fn parses_github_remotes_of_every_shape() {
        for remote in [
            "https://github.com/acme/widgets.git",
            "https://github.com/acme/widgets",
            "https://github.com/acme/widgets/",
            "git@github.com:acme/widgets.git",
            "git@github.com:acme/widgets",
            "ssh://git@github.com/acme/widgets.git",
        ] {
            let info = parse_remote_url(remote).unwrap();
            assert_eq!(info.provider, ProviderKind::GitHub, "{remote}");
            assert_eq!(info.host, "github.com", "{remote}");
            assert_eq!(info.owner, "acme", "{remote}");
            assert_eq!(info.name, "widgets", "{remote}");
            assert_eq!(info.web_url, "https://github.com/acme/widgets", "{remote}");
        }
        assert!(parse_remote_url("https://github.com/acme").is_err());
    }

    #[test]
    fn an_enterprise_remote_keeps_its_host() {
        let info = parse_remote_url("git@github.example.com:team/tool.git").unwrap();
        assert_eq!(info.host, "github.example.com");
        assert_eq!(
            info.pr_web_url(3),
            "https://github.example.com/team/tool/pull/3"
        );
        let gh = GitHubRepoInfo::from(&info);
        assert_eq!(gh.repo_spec(), "github.example.com/team/tool");
        let dotcom = GitHubRepoInfo::from(&parse_remote_url("https://github.com/a/b").unwrap());
        assert_eq!(dotcom.repo_spec(), "a/b");
    }

    #[test]
    fn parses_pull_request_urls() {
        let (info, number) = parse_pr_url("https://github.com/acme/widgets/pull/42").unwrap();
        assert_eq!(info.path(), "acme/widgets");
        assert_eq!(number, 42);
        let (_, number) =
            parse_pr_url("https://github.com/acme/widgets/pull/42/files#diff-1").unwrap();
        assert_eq!(number, 42);
        assert_eq!(
            info.pr_web_url(number),
            "https://github.com/acme/widgets/pull/42"
        );
        assert!(parse_pr_url("https://github.com/acme/widgets/issues/42").is_err());
        assert!(parse_pr_url("https://github.com/acme/widgets/pull/abc").is_err());
    }

    #[test]
    fn decodes_mergeability() {
        let clean = parse_mergeability(
            r#"{"state":"OPEN","mergeable":"MERGEABLE","mergeStateStatus":"CLEAN","headRefOid":"abc"}"#,
        )
        .unwrap();
        assert!(clean.mergeable && !clean.needs_rebase && !clean.checking);
        assert_eq!(clean.head_sha, "abc");

        let dirty = parse_mergeability(
            r#"{"state":"OPEN","mergeable":"CONFLICTING","mergeStateStatus":"DIRTY","headRefOid":"def"}"#,
        )
        .unwrap();
        assert!(!dirty.mergeable && dirty.needs_rebase && dirty.has_conflicts);
        assert_eq!(dirty.detail, "DIRTY");

        let behind = parse_mergeability(
            r#"{"state":"OPEN","mergeable":"MERGEABLE","mergeStateStatus":"BEHIND","headRefOid":"x"}"#,
        )
        .unwrap();
        assert!(behind.needs_rebase && !behind.has_conflicts);

        let blocked = parse_mergeability(
            r#"{"state":"OPEN","mergeable":"MERGEABLE","mergeStateStatus":"BLOCKED","headRefOid":"x"}"#,
        )
        .unwrap();
        assert!(!blocked.mergeable && !blocked.needs_rebase);

        let computing = parse_mergeability(
            r#"{"state":"OPEN","mergeable":"UNKNOWN","mergeStateStatus":"UNKNOWN","headRefOid":"x"}"#,
        )
        .unwrap();
        assert!(computing.checking && !computing.needs_rebase);

        // A merged request never needs a rebase, whatever the flags say.
        let merged = parse_mergeability(
            r#"{"state":"MERGED","mergeable":"CONFLICTING","mergeStateStatus":"DIRTY","headRefOid":"x"}"#,
        )
        .unwrap();
        assert!(!merged.needs_rebase && !merged.mergeable);
    }

    #[test]
    fn decodes_head_checks() {
        let failed = parse_head_checks(
            r#"{"headRefOid":"abc123","statusCheckRollup":[
                {"__typename":"CheckRun","name":"lint","status":"COMPLETED","conclusion":"SUCCESS",
                 "detailsUrl":"https://github.com/a/b/actions/runs/11/job/21"},
                {"__typename":"CheckRun","name":"test","status":"COMPLETED","conclusion":"FAILURE",
                 "detailsUrl":"https://github.com/a/b/actions/runs/11/job/22"},
                {"__typename":"StatusContext","context":"ci/external","state":"ERROR",
                 "targetUrl":"https://ci.example.com/build/9"}
            ]}"#,
        )
        .unwrap();
        assert_eq!(failed.status, CheckStatus::Failed);
        assert_eq!(failed.sha, "abc123");
        let names: Vec<_> = failed.failed_jobs.iter().map(|j| j.name.as_str()).collect();
        assert_eq!(names, ["test", "ci/external"]);
        assert_eq!(
            actions_job_id(failed.failed_jobs[0].url.as_deref().unwrap()).as_deref(),
            Some("22")
        );
        assert_eq!(
            actions_job_id(failed.failed_jobs[1].url.as_deref().unwrap()),
            None
        );

        // Anything still running holds the whole at pending, failures and all.
        let running = parse_head_checks(
            r#"{"headRefOid":"abc","statusCheckRollup":[
                {"__typename":"CheckRun","name":"test","status":"COMPLETED","conclusion":"FAILURE"},
                {"__typename":"CheckRun","name":"build","status":"IN_PROGRESS","conclusion":""}
            ]}"#,
        )
        .unwrap();
        assert_eq!(running.status, CheckStatus::Pending);
        assert!(running.failed_jobs.is_empty());

        let green = parse_head_checks(
            r#"{"headRefOid":"abc","statusCheckRollup":[
                {"__typename":"CheckRun","name":"test","status":"COMPLETED","conclusion":"SUCCESS"},
                {"__typename":"CheckRun","name":"docs","status":"COMPLETED","conclusion":"SKIPPED"},
                {"__typename":"StatusContext","context":"ci","state":"SUCCESS"}
            ]}"#,
        )
        .unwrap();
        assert_eq!(green.status, CheckStatus::Success);

        let canceled = parse_head_checks(
            r#"{"headRefOid":"abc","statusCheckRollup":[
                {"__typename":"CheckRun","name":"test","status":"COMPLETED","conclusion":"CANCELLED"}
            ]}"#,
        )
        .unwrap();
        assert_eq!(canceled.status, CheckStatus::Canceled);

        let none = parse_head_checks(r#"{"headRefOid":"abc","statusCheckRollup":[]}"#).unwrap();
        assert_eq!(none.status, CheckStatus::None);
    }

    #[test]
    fn picks_the_newest_successful_run() {
        let run = parse_latest_run(
            r#"[
                {"headSha":"old","updatedAt":"2026-09-01T10:00:00Z","url":"https://github.com/a/b/actions/runs/1"},
                {"headSha":"new","updatedAt":"2026-09-02T10:00:00Z","url":"https://github.com/a/b/actions/runs/2"}
            ]"#,
        )
        .unwrap()
        .unwrap();
        assert_eq!(run.sha, "new");
        assert_eq!(
            run.url.as_deref(),
            Some("https://github.com/a/b/actions/runs/2")
        );
        assert_eq!(parse_latest_run("[]").unwrap(), None);
    }

    #[test]
    fn only_a_merged_request_reports_its_merge_commit() {
        assert_eq!(
            parse_merge_commit(r#"{"state":"MERGED","mergeCommit":{"oid":"abc"}}"#).unwrap(),
            Some("abc".to_string())
        );
        assert_eq!(
            parse_merge_commit(r#"{"state":"OPEN","mergeCommit":null}"#).unwrap(),
            None
        );
        assert_eq!(
            parse_merge_commit(r#"{"state":"CLOSED","mergeCommit":{"oid":"abc"}}"#).unwrap(),
            None
        );
    }

    #[test]
    fn review_comment_fields_follow_the_anchor() {
        let line = review_comment_fields(&NoteAnchor::new_line("src/a.rs", 118), "sha", "hi");
        assert!(line.contains(&"line=118".to_string()));
        assert!(line.contains(&"side=RIGHT".to_string()));
        let old = review_comment_fields(&NoteAnchor::old_line("src/a.rs", 7), "sha", "hi");
        assert!(old.contains(&"side=LEFT".to_string()));
        let range = review_comment_fields(&NoteAnchor::new_range("src/a.rs", 10, 15), "sha", "hi");
        assert!(range.contains(&"start_line=10".to_string()));
        assert!(range.contains(&"line=15".to_string()));
        let file = review_comment_fields(&NoteAnchor::file("src/a.rs"), "sha", "hi");
        assert!(file.contains(&"subject_type=file".to_string()));
        assert!(!file.iter().any(|f| f.starts_with("line=")));
    }
}
