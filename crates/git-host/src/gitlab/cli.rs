//! Minimal helpers around the GitLab CLI (`glab`).
//!
//! MR creation goes through `glab mr create`; everything that needs
//! structured data goes through `glab api` (the stable GitLab REST API)
//! so we don't depend on `glab`'s per-command JSON output support.

use std::{ffi::OsStr, path::Path, process::Command};

use chrono::{DateTime, Utc};
use db::models::merge::MergeStatus;
use serde::Deserialize;
use thiserror::Error;
use url::Url;
use utils::{command_ext::NoWindowExt, shell::resolve_executable_path_blocking};

use crate::types::{
    CreatePrRequest, FailedJob, JobRun, Mergeability, NoteAnchor, NoteSide, ProviderKind,
    PullRequestDetail, RepoInfo, UnifiedPrComment,
};

#[derive(Debug, Clone)]
pub struct GitLabRepoInfo {
    /// GitLab hostname (e.g., "gitlab.com" or a self-managed host).
    pub host: String,
    /// Full project path including nested groups (e.g., "group/subgroup/project").
    pub project_path: String,
}

impl GitLabRepoInfo {
    /// Project path URL-encoded for use in REST API endpoints.
    /// GitLab project paths only contain `[A-Za-z0-9._-]` and `/`, so
    /// encoding the separators is sufficient.
    pub fn encoded_path(&self) -> String {
        self.project_path.replace('/', "%2F")
    }

    /// Full https URL of the project, accepted by `glab --repo`.
    pub fn project_url(&self) -> String {
        format!("https://{}/{}", self.host, self.project_path)
    }

    pub fn mr_web_url(&self, iid: i64) -> String {
        format!("{}/-/merge_requests/{}", self.project_url(), iid)
    }

    /// The host-neutral view: the namespace (nested groups included) as the
    /// owner, the last path segment as the name.
    pub fn to_repo_info(&self) -> RepoInfo {
        let (owner, name) = crate::remote::split_owner_name(&self.project_path)
            .unwrap_or(("", self.project_path.as_str()));
        RepoInfo::new(ProviderKind::GitLab, &self.host, owner, name)
    }
}

impl From<&RepoInfo> for GitLabRepoInfo {
    fn from(repo: &RepoInfo) -> Self {
        Self {
            host: repo.host.clone(),
            project_path: repo.path(),
        }
    }
}

/// What GitLab says about merging an MR right now; see [`GlabCli::mr_mergeability`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MrMergeability {
    pub detailed_merge_status: String,
    pub has_conflicts: bool,
    /// The MR's head commit.
    pub sha: String,
    /// `opened`, `merged`, `closed`, `locked`.
    pub state: String,
}

impl MrMergeability {
    /// The host-neutral view of this answer.
    pub fn to_mergeability(&self) -> Mergeability {
        let opened = self.state == "opened";
        Mergeability {
            mergeable: opened && self.detailed_merge_status == "mergeable",
            needs_rebase: self.needs_rebase(),
            has_conflicts: self.has_conflicts || self.detailed_merge_status == "conflict",
            checking: matches!(
                self.detailed_merge_status.as_str(),
                "checking" | "unchecked" | "preparing"
            ),
            detail: self.detailed_merge_status.clone(),
            head_sha: self.sha.clone(),
        }
    }

    /// The branch no longer applies to its target and only a rebase fixes it.
    /// `need_rebase` is the fast-forward-only setting asking for the same thing
    /// without a textual conflict.
    pub fn needs_rebase(&self) -> bool {
        self.state == "opened"
            && (self.has_conflicts
                || matches!(
                    self.detailed_merge_status.as_str(),
                    "conflict" | "need_rebase"
                ))
    }
}

/// Parse a git remote URL (https, ssh scp-like, or ssh://) into repo info.
pub fn parse_remote_url(remote_url: &str) -> Result<GitLabRepoInfo, GlabCliError> {
    let (host, path) = crate::remote::split_remote(remote_url)
        .map_err(|e| GlabCliError::UnexpectedOutput(e.to_string()))?;
    build_repo_info(&host, &path)
}

fn build_repo_info(host: &str, path: &str) -> Result<GitLabRepoInfo, GlabCliError> {
    let project_path = path.trim_matches('/').trim_end_matches(".git").to_string();
    if project_path.is_empty() || !project_path.contains('/') {
        return Err(GlabCliError::UnexpectedOutput(format!(
            "Cannot derive GitLab project path from '{path}'"
        )));
    }
    Ok(GitLabRepoInfo {
        host: host.to_string(),
        project_path,
    })
}

/// Parse an MR web URL like
/// `https://gitlab.com/group/subgroup/project/-/merge_requests/123`.
pub fn parse_mr_url(mr_url: &str) -> Result<(GitLabRepoInfo, i64), GlabCliError> {
    let url = Url::parse(mr_url.trim())
        .map_err(|e| GlabCliError::UnexpectedOutput(format!("Unparseable MR URL: {e}")))?;
    let host = url
        .host_str()
        .ok_or_else(|| GlabCliError::UnexpectedOutput("MR URL has no host".to_string()))?;
    let path = url.path();
    let (project_part, mr_part) = path.split_once("/-/merge_requests/").ok_or_else(|| {
        GlabCliError::UnexpectedOutput(format!("Not a GitLab merge request URL: {mr_url}"))
    })?;
    let iid: i64 = mr_part
        .split(['/', '#', '?'])
        .next()
        .unwrap_or_default()
        .parse()
        .map_err(|_| {
            GlabCliError::UnexpectedOutput(format!("Cannot parse MR number from: {mr_url}"))
        })?;
    let info = build_repo_info(host, project_part)?;
    Ok((info, iid))
}

#[derive(Deserialize)]
struct GlUser {
    username: Option<String>,
}

#[derive(Deserialize)]
struct GlMrResponse {
    iid: i64,
    #[serde(default)]
    web_url: String,
    #[serde(default)]
    state: String,
    merged_at: Option<DateTime<Utc>>,
    merge_commit_sha: Option<String>,
    squash_commit_sha: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    source_branch: Option<String>,
    #[serde(default)]
    target_branch: Option<String>,
}

#[derive(Deserialize)]
struct GlNotePosition {
    new_path: Option<String>,
    old_path: Option<String>,
    new_line: Option<i64>,
    old_line: Option<i64>,
}

#[derive(Deserialize)]
struct GlNote {
    id: i64,
    author: Option<GlUser>,
    #[serde(default)]
    body: String,
    created_at: Option<DateTime<Utc>>,
    #[serde(default)]
    system: bool,
    position: Option<GlNotePosition>,
}

#[derive(Debug, Error)]
pub enum GlabCliError {
    #[error("GitLab CLI (`glab`) executable not found or not runnable")]
    NotAvailable,
    #[error("GitLab CLI command failed: {0}")]
    CommandFailed(String),
    #[error("GitLab CLI authentication failed: {0}")]
    AuthFailed(String),
    #[error("GitLab CLI returned unexpected output: {0}")]
    UnexpectedOutput(String),
}

#[derive(Debug, Clone, Default)]
pub struct GlabCli;

impl GlabCli {
    pub fn new() -> Self {
        Self {}
    }

    fn run<I, S>(&self, args: I, dir: Option<&Path>) -> Result<String, GlabCliError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let glab = resolve_executable_path_blocking("glab").ok_or(GlabCliError::NotAvailable)?;
        let mut cmd = Command::new(&glab);
        if let Some(d) = dir {
            cmd.current_dir(d);
        }
        // Never fall into interactive prompts when run from the server.
        // GLAB_NO_PROMPT, not NO_PROMPT: the unprefixed name is deprecated and
        // makes glab print a warning on stderr that used to be mistaken for
        // the error itself (see the stdout handling below).
        cmd.env("GLAB_NO_PROMPT", "1");
        for arg in args {
            cmd.arg(arg);
        }
        let output = cmd
            .no_window()
            .output()
            .map_err(|err| GlabCliError::CommandFailed(err.to_string()))?;

        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).to_string());
        }

        // `glab api` prints GitLab's error body on STDOUT and keeps stderr for
        // its own notices, so an error built from stderr alone can carry
        // nothing but a deprecation warning. That hid a real HTTP 400 across
        // five merge requests: always include whatever stdout holds.
        let stderr_raw = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout_raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr = match (stderr_raw.is_empty(), stdout_raw.is_empty()) {
            (_, true) => stderr_raw.clone(),
            (true, false) => stdout_raw.clone(),
            (false, false) => format!("{stderr_raw}\n{stdout_raw}"),
        };
        let lower = stderr.to_ascii_lowercase();
        if lower.contains("glab auth login")
            || lower.contains("not authenticated")
            || lower.contains("authentication")
            || lower.contains("unauthorized")
            || lower.contains("invalid token")
            || lower.contains("401")
        {
            return Err(GlabCliError::AuthFailed(stderr));
        }

        Err(GlabCliError::CommandFailed(stderr))
    }

    /// `glab api` against a specific hostname; returns raw JSON.
    fn api(&self, host: &str, endpoint: &str, dir: Option<&Path>) -> Result<String, GlabCliError> {
        self.run(["api", "--hostname", host, endpoint], dir)
    }

    /// Requests review from the given GitLab usernames.
    ///
    /// Uses `glab mr update --reviewer` rather than the REST API: `glab api -f`
    /// builds a JSON body with the field name taken literally, so
    /// `reviewer_ids[]=123` arrives as a key named `reviewer_ids[]` and GitLab
    /// answers `HTTP 400 … reviewer_ids … are missing, at least one parameter
    /// must be provided`. The subcommand resolves usernames itself, which also
    /// removes the need to look ids up.
    pub fn set_mr_reviewers(
        &self,
        repo_info: &GitLabRepoInfo,
        iid: i64,
        usernames: &[String],
    ) -> Result<(), GlabCliError> {
        if usernames.is_empty() {
            return Ok(());
        }
        self.run(
            [
                "mr".to_string(),
                "update".to_string(),
                iid.to_string(),
                "--repo".to_string(),
                repo_info.project_url(),
                "--reviewer".to_string(),
                usernames.join(","),
            ],
            None,
        )
        .map(|_| ())
    }

    /// The commit and finish time of the newest successful run of `job_name`
    /// on `ref_name`.
    ///
    /// Reads jobs rather than deployments because the staging deploy declares no
    /// `environment:`, so GitLab's deployments API knows nothing about it. One
    /// page of successful jobs is enough: the caller wants the newest, and a
    /// deploy that has fallen off a hundred-job page is far older than anything
    /// still waiting to be seen on staging.
    ///
    /// Picks by `finished_at` rather than trusting the page order, because the
    /// ordering of this endpoint is not documented as guaranteed and being wrong
    /// here would mean reporting a change as deployed before it was.
    pub fn latest_successful_job(
        &self,
        repo_info: &GitLabRepoInfo,
        job_name: &str,
        ref_name: &str,
    ) -> Result<Option<JobRun>, GlabCliError> {
        let raw = self.api(
            &repo_info.host,
            &format!(
                "projects/{}/jobs?scope[]=success&per_page=100",
                repo_info.encoded_path()
            ),
            None,
        )?;
        let jobs: serde_json::Value = serde_json::from_str(&raw)
            .map_err(|e| GlabCliError::UnexpectedOutput(format!("Bad jobs JSON: {e}")))?;

        let mut best: Option<JobRun> = None;
        for job in jobs.as_array().into_iter().flatten() {
            if job.get("name").and_then(|v| v.as_str()) != Some(job_name)
                || job.get("ref").and_then(|v| v.as_str()) != Some(ref_name)
            {
                continue;
            }
            let Some(sha) = job
                .get("commit")
                .and_then(|c| c.get("id"))
                .and_then(|v| v.as_str())
            else {
                continue;
            };
            let Some(finished) = job
                .get("finished_at")
                .and_then(|v| v.as_str())
                .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
                .map(|dt| dt.with_timezone(&Utc))
            else {
                continue;
            };
            if best.as_ref().is_none_or(|run| finished > run.finished_at) {
                best = Some(JobRun {
                    sha: sha.to_string(),
                    finished_at: finished,
                    url: job
                        .get("web_url")
                        .and_then(|v| v.as_str())
                        .map(str::to_string),
                });
            }
        }
        Ok(best)
    }

    /// Whether `ancestor` is contained in `descendant`'s history.
    ///
    /// Asks the host for the merge base rather than comparing timestamps. A
    /// pipeline that started before a merge landed carries an earlier commit, so
    /// "the deploy ran after the merge" is not the same claim as "the deploy
    /// contains the merge" — and the difference is a change reported as verified
    /// on staging while staging is still running the code before it.
    ///
    /// Identical shas are their own merge base, so a commit deployed directly
    /// answers true without a special case.
    pub fn is_ancestor(
        &self,
        repo_info: &GitLabRepoInfo,
        ancestor: &str,
        descendant: &str,
    ) -> Result<bool, GlabCliError> {
        let raw = self.api(
            &repo_info.host,
            &format!(
                "projects/{}/repository/merge_base?refs[]={ancestor}&refs[]={descendant}",
                repo_info.encoded_path()
            ),
            None,
        )?;
        let base: serde_json::Value = serde_json::from_str(&raw)
            .map_err(|e| GlabCliError::UnexpectedOutput(format!("Bad merge_base JSON: {e}")))?;
        Ok(base.get("id").and_then(|v| v.as_str()) == Some(ancestor))
    }

    /// The merge commit of a merge request, or `None` while it is still open or
    /// was closed unmerged.
    ///
    /// Read from the host rather than the local `pull_requests` row: that row
    /// goes away with its workspace, and the changes most worth following onto
    /// staging are the ones that merged long enough ago for the workspace to
    /// have been reclaimed.
    pub fn mr_merge_commit(
        &self,
        repo_info: &GitLabRepoInfo,
        iid: i64,
    ) -> Result<Option<String>, GlabCliError> {
        let raw = self.api(
            &repo_info.host,
            &format!("projects/{}/merge_requests/{iid}", repo_info.encoded_path()),
            None,
        )?;
        merged_commit_from_response(&raw, repo_info)
    }

    /// A user's display name from their login, or `None` when the host has no
    /// such user.
    ///
    /// The comment APIs return logins, and a login is jargon to the person a
    /// ticket update is written for.
    pub fn user_display_name(
        &self,
        host: &str,
        username: &str,
    ) -> Result<Option<String>, GlabCliError> {
        let raw = self.api(host, &format!("users?username={username}"), None)?;
        let users: serde_json::Value = serde_json::from_str(&raw)
            .map_err(|e| GlabCliError::UnexpectedOutput(format!("Bad users JSON: {e}")))?;
        Ok(users
            .get(0)
            .and_then(|user| user["name"].as_str())
            .map(str::to_string))
    }

    /// Applies labels to an existing merge request, replacing whatever is set.
    ///
    /// Uses `glab mr update --label` for the same reason as
    /// [`Self::set_mr_reviewers`]: `glab api -f` takes field names literally,
    /// so an array parameter never reaches GitLab in the shape it expects.
    pub fn set_mr_labels(
        &self,
        repo_info: &GitLabRepoInfo,
        iid: i64,
        labels: &[String],
    ) -> Result<(), GlabCliError> {
        if labels.is_empty() {
            return Ok(());
        }
        self.run(
            [
                "mr".to_string(),
                "update".to_string(),
                iid.to_string(),
                "--repo".to_string(),
                repo_info.project_url(),
                "--label".to_string(),
                labels.join(","),
            ],
            None,
        )
        .map(|_| ())
    }

    /// Whether GitLab can merge the MR as it stands, and the head commit it
    /// judged. `detailed_merge_status` is GitLab's own word — `mergeable`,
    /// `conflict`, `need_rebase`, `ci_must_pass`, `discussions_not_resolved`,
    /// `checking`, and so on — and `has_conflicts` is its separate flag for the
    /// one case a person cannot fix from the MR page.
    pub fn mr_mergeability(
        &self,
        repo_info: &GitLabRepoInfo,
        iid: i64,
    ) -> Result<MrMergeability, GlabCliError> {
        let raw = self.api(
            &repo_info.host,
            &format!("projects/{}/merge_requests/{iid}", repo_info.encoded_path()),
            None,
        )?;
        let mr: serde_json::Value = serde_json::from_str(&raw)
            .map_err(|e| GlabCliError::UnexpectedOutput(format!("Bad MR JSON: {e}")))?;
        let text = |key: &str| {
            mr.get(key)
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        Ok(MrMergeability {
            detailed_merge_status: text("detailed_merge_status"),
            has_conflicts: mr
                .get("has_conflicts")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            sha: text("sha"),
            state: text("state"),
        })
    }

    /// The MR's head pipeline as `(id, status, sha)`, or `None` when the MR has
    /// no pipeline yet. Statuses are GitLab's own: `success`, `failed`,
    /// `running`, `pending`, `canceled`, `skipped`.
    pub fn mr_head_pipeline(
        &self,
        repo_info: &GitLabRepoInfo,
        iid: i64,
    ) -> Result<Option<(i64, String, String)>, GlabCliError> {
        let raw = self.api(
            &repo_info.host,
            &format!(
                "projects/{}/merge_requests/{}",
                repo_info.encoded_path(),
                iid
            ),
            None,
        )?;
        let mr: serde_json::Value = serde_json::from_str(&raw)
            .map_err(|e| GlabCliError::UnexpectedOutput(format!("Bad MR JSON: {e}")))?;
        let Some(pipeline) = mr.get("head_pipeline").filter(|p| !p.is_null()) else {
            return Ok(None);
        };
        let id = pipeline.get("id").and_then(serde_json::Value::as_i64);
        let status = pipeline
            .get("status")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        let sha = pipeline
            .get("sha")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string();
        match (id, status) {
            (Some(id), Some(status)) => Ok(Some((id, status, sha))),
            _ => Ok(None),
        }
    }

    /// Failed jobs in a pipeline, each with the tail of its trace. The tail is
    /// bounded because a full job log can run to megabytes and only its end
    /// explains the failure.
    pub fn failed_jobs(
        &self,
        repo_info: &GitLabRepoInfo,
        pipeline_id: i64,
        trace_tail_bytes: usize,
    ) -> Result<Vec<FailedJob>, GlabCliError> {
        let raw = self.api(
            &repo_info.host,
            &format!(
                "projects/{}/pipelines/{}/jobs?scope[]=failed",
                repo_info.encoded_path(),
                pipeline_id
            ),
            None,
        )?;
        let jobs: serde_json::Value = serde_json::from_str(&raw)
            .map_err(|e| GlabCliError::UnexpectedOutput(format!("Bad jobs JSON: {e}")))?;
        let mut out = Vec::new();
        for job in jobs.as_array().into_iter().flatten() {
            let Some(id) = job.get("id").and_then(serde_json::Value::as_i64) else {
                continue;
            };
            let name = job
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("job")
                .to_string();
            // A trace is plain text, not JSON, so it is taken as-is.
            let trace = self
                .api(
                    &repo_info.host,
                    &format!("projects/{}/jobs/{}/trace", repo_info.encoded_path(), id),
                    None,
                )
                .unwrap_or_default();
            out.push(FailedJob {
                name,
                url: job
                    .get("web_url")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string),
                log_tail: Some(crate::types::tail_bytes(trace, trace_tail_bytes)),
            });
        }
        Ok(out)
    }

    /// Posts a plain discussion note on an MR.
    pub fn create_mr_note(
        &self,
        repo_info: &GitLabRepoInfo,
        iid: i64,
        body: &str,
    ) -> Result<(), GlabCliError> {
        self.run(
            [
                "api",
                "--hostname",
                &repo_info.host,
                "-X",
                "POST",
                &format!(
                    "projects/{}/merge_requests/{}/notes",
                    repo_info.encoded_path(),
                    iid
                ),
                "-f",
                &format!("body={body}"),
            ],
            None,
        )
        .map(|_| ())
    }

    /// Posts a note anchored to a file (and optionally a line) in the MR's
    /// latest diff version.
    ///
    /// Uses `glab mr note create --file` rather than the REST API: an anchored
    /// note needs a nested `position` object, and `glab api --field` sends field
    /// names literally in a JSON body (see [`Self::set_mr_reviewers`]), so
    /// `position[base_sha]=…` would arrive as a key of that name. The subcommand
    /// also resolves the three diff SHAs itself, which the API requires and
    /// which would otherwise cost an extra round trip per comment.
    ///
    /// Two of glab's own constraints shape the caller's options: `--file` cannot
    /// be combined with `--unique` (so deduplication is the caller's job) or
    /// with `--resolvable=false` (so a non-resolvable note must be MR-level).
    pub fn create_mr_note_at(
        &self,
        repo_info: &GitLabRepoInfo,
        iid: i64,
        body: &str,
        anchor: &NoteAnchor,
    ) -> Result<(), GlabCliError> {
        let mut args = vec![
            "mr".to_string(),
            "note".to_string(),
            "create".to_string(),
            iid.to_string(),
            "--repo".to_string(),
            repo_info.project_url(),
            "--file".to_string(),
            anchor.path.clone(),
            "--message".to_string(),
            body.to_string(),
        ];
        match (anchor.line, anchor.end_line, anchor.side) {
            (None, _, _) => {}
            (Some(line), _, NoteSide::Old) => {
                args.push("--old-line".to_string());
                args.push(line.to_string());
            }
            // A range renders as glab's `10:15`.
            (Some(start), Some(end), NoteSide::New) => {
                args.push("--line".to_string());
                args.push(format!("{start}:{end}"));
            }
            (Some(line), None, NoteSide::New) => {
                args.push("--line".to_string());
                args.push(line.to_string());
            }
        }
        self.run(args, None).map(|_| ())
    }

    /// Posts a note nobody has to resolve — praise, status, anything that should
    /// not block a merge on a project requiring every thread resolved.
    ///
    /// MR-level only, because glab refuses `--resolvable=false` alongside
    /// `--file`. That costs nothing: a compliment anchored to a diff line is a
    /// thread the author has to close on a compliment.
    pub fn create_mr_note_unresolvable(
        &self,
        repo_info: &GitLabRepoInfo,
        iid: i64,
        body: &str,
    ) -> Result<(), GlabCliError> {
        self.run(
            [
                "mr".to_string(),
                "note".to_string(),
                "create".to_string(),
                iid.to_string(),
                "--repo".to_string(),
                repo_info.project_url(),
                "--resolvable=false".to_string(),
                "--message".to_string(),
                body.to_string(),
            ],
            None,
        )
        .map(|_| ())
    }

    pub fn create_mr(
        &self,
        request: &CreatePrRequest,
        repo_info: &GitLabRepoInfo,
        repo_path: &Path,
    ) -> Result<PullRequestDetail, GlabCliError> {
        let mut args: Vec<String> = vec![
            "mr".into(),
            "create".into(),
            "--repo".into(),
            repo_info.project_url(),
            "--source-branch".into(),
            request.head_branch.clone(),
            "--target-branch".into(),
            request.base_branch.clone(),
            "--title".into(),
            request.title.clone(),
            "--description".into(),
            request.body.clone().unwrap_or_default(),
            "--yes".into(),
        ];
        if request.draft.unwrap_or(false) {
            args.push("--draft".into());
        }

        let stdout = self.run(args, Some(repo_path))?;
        parse_mr_create_text(&stdout, request)
    }

    pub fn view_mr(&self, mr_url: &str) -> Result<PullRequestDetail, GlabCliError> {
        let (repo_info, iid) = parse_mr_url(mr_url)?;
        let raw = self.api(
            &repo_info.host,
            &format!(
                "projects/{}/merge_requests/{}",
                repo_info.encoded_path(),
                iid
            ),
            None,
        )?;
        let mr: GlMrResponse = serde_json::from_str(&raw)
            .map_err(|e| GlabCliError::UnexpectedOutput(format!("Bad MR JSON: {e}")))?;
        Ok(mr_response_to_detail(mr, &repo_info))
    }

    pub fn list_mrs_for_branch(
        &self,
        repo_info: &GitLabRepoInfo,
        branch_name: &str,
        repo_path: &Path,
    ) -> Result<Vec<PullRequestDetail>, GlabCliError> {
        let raw = self.api(
            &repo_info.host,
            &format!(
                "projects/{}/merge_requests?source_branch={}&state=all&per_page=50",
                repo_info.encoded_path(),
                branch_name
            ),
            Some(repo_path),
        )?;
        parse_mr_list(&raw, repo_info)
    }

    pub fn list_open_mrs(
        &self,
        repo_info: &GitLabRepoInfo,
        repo_path: &Path,
    ) -> Result<Vec<PullRequestDetail>, GlabCliError> {
        let raw = self.api(
            &repo_info.host,
            &format!(
                "projects/{}/merge_requests?state=opened&order_by=updated_at&sort=desc&per_page=50",
                repo_info.encoded_path()
            ),
            Some(repo_path),
        )?;
        parse_mr_list(&raw, repo_info)
    }

    /// Fetch the MR's source branch and check it out in `repo_path`.
    pub fn checkout_mr(
        &self,
        repo_info: &GitLabRepoInfo,
        mr_iid: i64,
        repo_path: &Path,
    ) -> Result<(), GlabCliError> {
        self.run(
            [
                "mr",
                "checkout",
                &mr_iid.to_string(),
                "--repo",
                &repo_info.project_url(),
            ],
            Some(repo_path),
        )?;
        Ok(())
    }

    /// Fetch MR notes (GitLab's unified comment model) and convert them into
    /// the app's unified comment timeline. Positioned notes become Review
    /// comments; other non-system notes become General comments.
    pub fn get_mr_comments(
        &self,
        repo_info: &GitLabRepoInfo,
        mr_iid: i64,
        repo_path: &Path,
    ) -> Result<Vec<UnifiedPrComment>, GlabCliError> {
        let raw = self.api(
            &repo_info.host,
            &format!(
                "projects/{}/merge_requests/{}/notes?sort=asc&order_by=created_at&per_page=100",
                repo_info.encoded_path(),
                mr_iid
            ),
            Some(repo_path),
        )?;
        let notes: Vec<GlNote> = serde_json::from_str(&raw)
            .map_err(|e| GlabCliError::UnexpectedOutput(format!("Bad notes JSON: {e}")))?;

        let mut unified: Vec<UnifiedPrComment> = Vec::new();
        for note in notes {
            if note.system {
                continue;
            }
            let author = note
                .author
                .and_then(|u| u.username)
                .unwrap_or_else(|| "unknown".to_string());
            let created_at = note.created_at.unwrap_or_else(Utc::now);
            let url = Some(format!("{}#note_{}", repo_info.mr_web_url(mr_iid), note.id));

            match note.position {
                Some(pos) => {
                    let path = pos.new_path.or(pos.old_path).unwrap_or_default();
                    let (line, side) = match (pos.new_line, pos.old_line) {
                        (Some(l), _) => (Some(l), Some("RIGHT".to_string())),
                        (None, Some(l)) => (Some(l), Some("LEFT".to_string())),
                        (None, None) => (None, None),
                    };
                    unified.push(UnifiedPrComment::Review {
                        id: note.id,
                        author,
                        author_association: None,
                        body: note.body,
                        created_at,
                        url,
                        path,
                        line,
                        side,
                        diff_hunk: None,
                    });
                }
                None => {
                    unified.push(UnifiedPrComment::General {
                        id: note.id.to_string(),
                        author,
                        author_association: None,
                        body: note.body,
                        created_at,
                        url,
                    });
                }
            }
        }
        unified.sort_by_key(|c| c.created_at());
        Ok(unified)
    }
}

fn parse_mr_list(
    raw: &str,
    repo_info: &GitLabRepoInfo,
) -> Result<Vec<PullRequestDetail>, GlabCliError> {
    let mrs: Vec<GlMrResponse> = serde_json::from_str(raw)
        .map_err(|e| GlabCliError::UnexpectedOutput(format!("Bad MR list JSON: {e}")))?;
    Ok(mrs
        .into_iter()
        .map(|mr| mr_response_to_detail(mr, repo_info))
        .collect())
}

fn mr_response_to_detail(mr: GlMrResponse, repo_info: &GitLabRepoInfo) -> PullRequestDetail {
    let status = match mr.state.as_str() {
        "opened" | "locked" => MergeStatus::Open,
        "merged" => MergeStatus::Merged,
        "closed" => MergeStatus::Closed,
        _ => MergeStatus::Unknown,
    };
    let url = if mr.web_url.is_empty() {
        repo_info.mr_web_url(mr.iid)
    } else {
        mr.web_url
    };
    PullRequestDetail {
        number: mr.iid,
        url,
        status,
        merged_at: mr.merged_at,
        // GitLab leaves merge_commit_sha null for squash merges; fall back.
        merge_commit_sha: mr.merge_commit_sha.or(mr.squash_commit_sha),
        title: mr.title.unwrap_or_default(),
        base_branch: mr.target_branch.unwrap_or_default(),
        head_branch: mr.source_branch.unwrap_or_default(),
    }
}

/// The commit a merge request put on its target branch, or `None` when it has
/// not merged.
///
/// Split out of [`GlabCli::mr_merge_commit`] so the decision can be tested
/// against real payloads without a host. It had no test, and was wrong for
/// every merge in the target repository from the day it shipped.
///
/// Goes through `mr_response_to_detail` rather than reading the payload
/// directly. That is where the knowledge lives: GitLab only fills
/// `merge_commit_sha` when the merge creates a merge commit, so on a project
/// with `merge_method: ff` it is always null and the commit that reached the
/// target branch is the squash instead.
fn merged_commit_from_response(
    raw: &str,
    repo_info: &GitLabRepoInfo,
) -> Result<Option<String>, GlabCliError> {
    let mr: GlMrResponse = serde_json::from_str(raw)
        .map_err(|e| GlabCliError::UnexpectedOutput(format!("Bad MR JSON: {e}")))?;
    let detail = mr_response_to_detail(mr, repo_info);
    // Only a merged request has put a commit on the target branch. The SHA
    // fields are populated while a request is open and after it is abandoned
    // too, so without this an unmerged request would report a commit that is
    // not on the target branch and never will be — and every ancestry check
    // against it would answer "no" for ever, which is the same silence this
    // fix is removing wearing different clothes.
    if !matches!(detail.status, MergeStatus::Merged) {
        return Ok(None);
    }
    Ok(detail.merge_commit_sha)
}

/// `glab mr create` prints the MR URL on stdout; scrape it and derive the IID.
fn parse_mr_create_text(
    stdout: &str,
    request: &CreatePrRequest,
) -> Result<PullRequestDetail, GlabCliError> {
    for token in stdout.split_whitespace() {
        if token.starts_with("http") && token.contains("/-/merge_requests/") {
            let (repo_info, iid) = parse_mr_url(token)?;
            return Ok(PullRequestDetail {
                number: iid,
                url: repo_info.mr_web_url(iid),
                status: MergeStatus::Open,
                merged_at: None,
                merge_commit_sha: None,
                title: request.title.clone(),
                base_branch: request.base_branch.clone(),
                head_branch: request.head_branch.clone(),
            });
        }
    }
    Err(GlabCliError::UnexpectedOutput(format!(
        "Could not find merge request URL in glab output: {stdout}"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_https_remote() {
        let info = parse_remote_url("https://gitlab.com/example-org/example-app.git").unwrap();
        assert_eq!(info.host, "gitlab.com");
        assert_eq!(info.project_path, "example-org/example-app");
        assert_eq!(info.encoded_path(), "example-org%2Fexample-app");
    }

    #[test]
    fn parses_scp_remote_with_subgroup() {
        let info = parse_remote_url("git@gitlab.com:group/subgroup/project.git").unwrap();
        assert_eq!(info.host, "gitlab.com");
        assert_eq!(info.project_path, "group/subgroup/project");
        assert_eq!(info.encoded_path(), "group%2Fsubgroup%2Fproject");
    }

    #[test]
    fn parses_ssh_scheme_remote() {
        let info = parse_remote_url("ssh://git@gitlab.com/example-org/example-app.git").unwrap();
        assert_eq!(info.host, "gitlab.com");
        assert_eq!(info.project_path, "example-org/example-app");
    }

    #[test]
    fn parses_mr_url() {
        let (info, iid) =
            parse_mr_url("https://gitlab.com/group/sub/proj/-/merge_requests/42").unwrap();
        assert_eq!(info.project_path, "group/sub/proj");
        assert_eq!(iid, 42);
    }

    #[test]
    fn parses_mr_url_with_anchor() {
        let (_, iid) = parse_mr_url("https://gitlab.com/a/b/-/merge_requests/7#note_123").unwrap();
        assert_eq!(iid, 7);
    }

    #[test]
    fn rejects_non_mr_url() {
        assert!(parse_mr_url("https://gitlab.com/a/b/-/issues/7").is_err());
    }

    #[test]
    fn create_output_scrape() {
        let request = CreatePrRequest {
            title: "T".into(),
            body: None,
            head_branch: "feat".into(),
            base_branch: "main".into(),
            draft: None,
            head_repo_url: None,
        };
        let detail = parse_mr_create_text(
            "!42 Some title (feat)\nhttps://gitlab.com/example-org/example-app/-/merge_requests/42\n",
            &request,
        )
        .unwrap();
        assert_eq!(detail.number, 42);
        assert!(matches!(detail.status, MergeStatus::Open));
    }

    fn mergeability(status: &str, conflicts: bool, state: &str) -> MrMergeability {
        MrMergeability {
            detailed_merge_status: status.to_string(),
            has_conflicts: conflicts,
            sha: "abc123".to_string(),
            state: state.to_string(),
        }
    }

    fn target_app() -> GitLabRepoInfo {
        parse_remote_url("https://gitlab.com/example-org/example-app.git").unwrap()
    }

    #[test]
    fn a_squash_merge_reports_the_commit_that_actually_landed() {
        // The shape the target repository produces: `merge_method: ff` with squash on by
        // default, so GitLab creates no merge commit and the field the old
        // code read is null. Payload taken from a real merged request.
        let raw = r#"{
            "iid": 9998,
            "state": "merged",
            "merged_at": "2026-09-23T17:28:18.904Z",
            "merge_commit_sha": null,
            "squash_commit_sha": "524ecf7bfd4fc9a774042d6a205e30405acdf1b3"
        }"#;
        assert_eq!(
            merged_commit_from_response(raw, &target_app()).unwrap(),
            Some("524ecf7bfd4fc9a774042d6a205e30405acdf1b3".to_string())
        );
    }

    #[test]
    fn a_real_merge_commit_still_wins_where_one_exists() {
        // A project configured for merge commits rather than fast-forward.
        // Both fields can be set when a merge request is squashed *and* merged
        // with a merge commit; the merge commit is the one on the branch.
        let raw = r#"{
            "iid": 4242,
            "state": "merged",
            "merged_at": "2026-09-23T17:28:18.904Z",
            "merge_commit_sha": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "squash_commit_sha": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        }"#;
        assert_eq!(
            merged_commit_from_response(raw, &target_app()).unwrap(),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string())
        );
    }

    #[test]
    fn an_abandoned_request_reports_no_commit() {
        // MR !9902, closed unmerged. Its `sha` is populated like any other
        // request's, so a fallback chain that did not check the state would
        // record a commit for work that was thrown away.
        let raw = r#"{
            "iid": 9902,
            "state": "closed",
            "merged_at": null,
            "merge_commit_sha": null,
            "squash_commit_sha": null,
            "sha": "f115afc3c5af05855fc76b1b389adb9ad332767f"
        }"#;
        assert_eq!(
            merged_commit_from_response(raw, &target_app()).unwrap(),
            None
        );
    }

    #[test]
    fn an_open_request_reports_no_commit() {
        let raw = r#"{
            "iid": 10001,
            "state": "opened",
            "merged_at": null,
            "merge_commit_sha": null,
            "squash_commit_sha": null
        }"#;
        assert_eq!(
            merged_commit_from_response(raw, &target_app()).unwrap(),
            None
        );
    }

    #[test]
    fn a_rebase_is_needed_for_conflicts_and_for_need_rebase_only() {
        assert!(mergeability("conflict", true, "opened").needs_rebase());
        assert!(mergeability("conflict", false, "opened").needs_rebase());
        assert!(mergeability("need_rebase", false, "opened").needs_rebase());
        // GitLab's flag can lead its status word; the flag alone is enough.
        assert!(mergeability("checking", true, "opened").needs_rebase());
        for status in [
            "mergeable",
            "checking",
            "ci_must_pass",
            "discussions_not_resolved",
            "not_approved",
        ] {
            assert!(
                !mergeability(status, false, "opened").needs_rebase(),
                "{status}"
            );
        }
    }

    #[test]
    fn a_closed_or_merged_mr_never_needs_a_rebase() {
        assert!(!mergeability("conflict", true, "merged").needs_rebase());
        assert!(!mergeability("conflict", true, "closed").needs_rebase());
    }
}
