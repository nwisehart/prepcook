//! GitHub Issues behind [`Tracker`], driven through `gh api` exactly as the
//! git host drives pull requests: auth is whatever `gh auth login` signed in
//! on this machine, so there is no token field.
//!
//! One repository is configured (`ticketing.repo`, `owner/name` or
//! `host/owner/name` for GitHub Enterprise). Its issues get keys shaped like
//! a tracker key, `<REPO>-<number>` (`WIDGETS-42` for `acme/widgets`), so they
//! are safe in paths and URLs the way Jira keys are; any key ending in the
//! issue number is read back.
//!
//! Filters are space-separated terms ([`Filter`]): `state:open|closed|all`,
//! `label:<name>` (repeatable, all must match; quote names with spaces),
//! `-label:<name>`, `assignee:@me|<login>|none|*` and `no:assignee`.
//!
//! Workflow is labels: `status_mapping` names a label per stage; entering a
//! stage swaps the issue's status label for that one, `done` (and the
//! shipped names) also close the issue, and every other stage reopens a
//! closed one. Status categories are derived: closed is `done`, open with a
//! working-stage label is `indeterminate`, any other open issue is `new`.
//! The issue type is the first label that names a `type_mapping` entry
//! (`bug` → Bug), and the priority the first `P0`…`P9` label.
//!
//! Accounts are logins, lower-cased so a mention typed in any case matches.

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use anyhow::{Context, anyhow};
use async_trait::async_trait;
use chrono::{DateTime, FixedOffset, Utc};
use serde_json::{Value, json};
use thiserror::Error;
use tokio::io::AsyncWriteExt;

use super::{
    CreatedSubtask, FetchedIssue, IssueFields, NewIssue, NewSubtask, Query, RemoteLink,
    TicketStage, Tracker, TrackerAttachment, TrackerComment, TrackerError, TrackerIdentity,
    TrackerResult, TrackerUser, at_mentions, clip_text, is_project_key,
};
use crate::services::{
    config::{TrackerConfig, TrackerKind},
    integrations::{Descriptor, Field, FieldKind},
    reopen,
};

const KIND: TrackerKind = TrackerKind::GithubIssues;

/// GitHub rejects a comment or body over 65 536 characters.
const BODY_LIMIT: usize = 65_000;
const PAGE_SIZE: usize = 100;
const MAX_ISSUE_PAGES: usize = 5;
const MAX_COMMENT_PAGES: usize = 10;
const MAX_USER_PAGES: usize = 10;
/// Issues per GraphQL lookup in [`Tracker::fetch`].
const FETCH_BATCH: usize = 50;
/// A `gh` call that has not answered by then is not going to.
const GH_TIMEOUT: Duration = Duration::from_secs(120);

/// Marks the comment [`Tracker::set_remote_link`] keeps up to date for one
/// link id, so pinning again edits it instead of adding another.
fn link_marker(id: &str) -> String {
    format!("<!-- prepcook-link:{id} -->")
}

// ---------------------------------------------------------------------------
// Running `gh`
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum GhError {
    #[error("the GitHub CLI (`gh`) is not installed; GitHub Issues is driven through it")]
    NotAvailable,
    #[error("`gh` is not signed in to GitHub (run `gh auth login`): {0}")]
    AuthFailed(String),
    #[error("{message}")]
    Failed {
        /// The HTTP status `gh api` reported, when it reported one.
        status: Option<u16>,
        message: String,
    },
}

impl From<GhError> for TrackerError {
    fn from(error: GhError) -> Self {
        match error {
            GhError::AuthFailed(_)
            | GhError::Failed {
                status: Some(401), ..
            } => TrackerError::AuthFailed,
            GhError::NotAvailable => TrackerError::NotConfigured(error.to_string()),
            GhError::Failed { message, .. } => TrackerError::Other(anyhow!("GitHub: {message}")),
        }
    }
}

/// Runs the `gh` CLI. A seam so tests can answer with recorded responses.
#[async_trait]
pub trait GhRunner: Send + Sync {
    /// Runs `gh` with `args`, feeding `stdin` when given, and returns stdout.
    async fn run(&self, args: &[String], stdin: Option<&[u8]>) -> Result<Vec<u8>, GhError>;
}

/// The real `gh` on this machine.
pub struct CliRunner;

/// "gh: Not Found (HTTP 404)" → 404.
fn http_status(text: &str) -> Option<u16> {
    let start = text.rfind("(HTTP ")? + "(HTTP ".len();
    text[start..].get(..3)?.parse().ok()
}

#[async_trait]
impl GhRunner for CliRunner {
    async fn run(&self, args: &[String], stdin: Option<&[u8]>) -> Result<Vec<u8>, GhError> {
        use std::process::Stdio;

        use utils::command_ext::NoWindowExt;

        let gh = utils::shell::resolve_executable_path("gh")
            .await
            .ok_or(GhError::NotAvailable)?;
        let mut command = tokio::process::Command::new(gh);
        command
            .args(args)
            .stdin(if stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .no_window();
        let mut child = command.spawn().map_err(|error| GhError::Failed {
            status: None,
            message: format!("could not start gh: {error}"),
        })?;
        if let (Some(input), Some(mut pipe)) = (stdin, child.stdin.take()) {
            pipe.write_all(input)
                .await
                .map_err(|error| GhError::Failed {
                    status: None,
                    message: format!("could not write to gh: {error}"),
                })?;
        }
        let output = tokio::time::timeout(GH_TIMEOUT, child.wait_with_output())
            .await
            .map_err(|_| GhError::Failed {
                status: None,
                message: "gh did not answer in time".to_string(),
            })?
            .map_err(|error| GhError::Failed {
                status: None,
                message: format!("gh failed: {error}"),
            })?;
        if output.status.success() {
            return Ok(output.stdout);
        }
        // `gh api` prints the host's error body on stdout and its own summary
        // ("Not Found (HTTP 404)") on stderr; both say something.
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let message = if stdout.is_empty() {
            stderr.clone()
        } else {
            format!("{stderr}\n{stdout}")
        };
        let lower = message.to_ascii_lowercase();
        // gh exits 4 when it has no credentials for the host.
        if output.status.code() == Some(4)
            || lower.contains("gh auth login")
            || lower.contains("bad credentials")
        {
            return Err(GhError::AuthFailed(message));
        }
        Err(GhError::Failed {
            status: http_status(&stderr),
            message,
        })
    }
}

// ---------------------------------------------------------------------------
// The repository and its keys
// ---------------------------------------------------------------------------

/// The configured repository.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RepoRef {
    /// `None` for github.com.
    host: Option<String>,
    owner: String,
    name: String,
}

impl RepoRef {
    /// `owner/name`, `host/owner/name`, or a repository URL.
    pub(crate) fn parse(raw: &str) -> Option<Self> {
        let trimmed = raw
            .trim()
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .trim_end_matches('/')
            .trim_end_matches(".git");
        let parts: Vec<&str> = trimmed.split('/').filter(|part| !part.is_empty()).collect();
        let (host, owner, name) = match parts[..] {
            [owner, name] => (None, owner, name),
            [host, owner, name] => (Some(host), owner, name),
            _ => return None,
        };
        let valid = |part: &str| {
            part.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        };
        if !valid(owner) || !valid(name) {
            return None;
        }
        Some(Self {
            host: host
                .filter(|host| !host.eq_ignore_ascii_case("github.com"))
                .map(str::to_string),
            owner: owner.to_string(),
            name: name.to_string(),
        })
    }

    fn api_path(&self) -> String {
        format!("repos/{}/{}", self.owner, self.name)
    }

    fn web(&self) -> String {
        format!(
            "https://{}/{}/{}",
            self.host.as_deref().unwrap_or("github.com"),
            self.owner,
            self.name
        )
    }

    fn full_name(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }

    /// The key prefix: the repository name upper-cased, anything but letters
    /// and digits made `_`, led by a letter.
    pub(crate) fn key_prefix(&self) -> String {
        let prefix: String = self
            .name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_uppercase()
                } else {
                    '_'
                }
            })
            .collect();
        if prefix.starts_with(|c: char| c.is_ascii_alphabetic()) {
            prefix
        } else {
            format!("GH{prefix}")
        }
    }
}

/// The issue number a key ends in: `WIDGETS-42`, `#42` and `42` are all 42.
pub(crate) fn number_of(key: &str) -> Option<u64> {
    key.trim()
        .rsplit(['-', '#'])
        .next()
        .filter(|digits| !digits.is_empty())?
        .parse()
        .ok()
}

// ---------------------------------------------------------------------------
// Filters
// ---------------------------------------------------------------------------

/// A parsed filter: the terms the list endpoint takes, and the ones applied
/// to its answer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Filter {
    /// `open`, `closed` or `all`; open when unset.
    state: Option<String>,
    labels: Vec<String>,
    without: Vec<String>,
    assignee: Option<String>,
    created_since: Option<DateTime<Utc>>,
    updated_since: Option<DateTime<Utc>>,
    /// Oldest first on this field.
    sort: Option<&'static str>,
    reopened: bool,
}

/// Splits on whitespace outside double quotes, dropping the quotes.
fn terms(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for c in text.chars() {
        match c {
            '"' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => {
                if !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

fn timestamp(raw: &str) -> Result<DateTime<Utc>, String> {
    DateTime::parse_from_rfc3339(raw)
        .map(|at| at.with_timezone(&Utc))
        .map_err(|_| format!("`{raw}` is not an RFC 3339 timestamp"))
}

/// Parses a GitHub Issues filter (module docs list the terms).
pub(crate) fn parse_filter(text: &str) -> Result<Filter, String> {
    let mut filter = Filter::default();
    for term in terms(text) {
        let (key, value) = term
            .split_once(':')
            .map(|(key, value)| (key.to_ascii_lowercase(), value.to_string()))
            .unwrap_or_else(|| (term.to_ascii_lowercase(), String::new()));
        let list = |value: &str| -> Vec<String> {
            value
                .split(',')
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(str::to_string)
                .collect()
        };
        match (key.as_str(), value.to_ascii_lowercase().as_str()) {
            ("state", state @ ("open" | "closed" | "all"))
            | ("is", state @ ("open" | "closed")) => {
                filter.state = Some(state.to_string());
            }
            ("is", "issue") => {}
            ("is", "reopened") | ("reopened", "") => filter.reopened = true,
            ("label", _) => filter.labels.extend(list(&value)),
            ("-label", _) => filter.without.extend(list(&value)),
            ("no", "assignee") => filter.assignee = Some("none".to_string()),
            ("assignee", _) if !value.trim().is_empty() => {
                filter.assignee = Some(value.trim().to_string());
            }
            ("sort", "created") => filter.sort = Some("created"),
            ("sort", "updated") => filter.sort = Some("updated"),
            _ => {
                if let Some(raw) = term.strip_prefix("created>=") {
                    filter.created_since = Some(timestamp(raw)?);
                } else if let Some(raw) = term.strip_prefix("updated>=") {
                    filter.updated_since = Some(timestamp(raw)?);
                } else {
                    return Err(format!(
                        "`{term}` is not a GitHub Issues filter term (use state:, label:, \
                         -label:, assignee: or no:assignee)"
                    ));
                }
            }
        }
    }
    Ok(filter)
}

// ---------------------------------------------------------------------------
// Issues as GitHub returns them
// ---------------------------------------------------------------------------

/// The fields both the REST and GraphQL shapes give, normalised.
#[derive(Clone, Debug)]
struct RawIssue {
    number: u64,
    title: String,
    body: String,
    /// `open` or `closed`.
    state: String,
    reopened: bool,
    created: Option<DateTime<FixedOffset>>,
    updated: Option<DateTime<FixedOffset>>,
    /// As written.
    labels: Vec<String>,
    /// Lower-cased login.
    assignee: Option<String>,
    /// Lower-cased login.
    author: Option<String>,
    node_id: Option<String>,
}

fn rfc3339(value: &Value) -> Option<DateTime<FixedOffset>> {
    value
        .as_str()
        .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
}

fn login(value: &Value) -> Option<String> {
    value["login"].as_str().map(str::to_ascii_lowercase)
}

impl RawIssue {
    /// A REST issue; `None` for a pull request (the issues endpoints list
    /// both) or a malformed entry.
    fn from_rest(value: &Value) -> Option<Self> {
        if value.get("pull_request").is_some_and(|pr| !pr.is_null()) {
            return None;
        }
        Some(Self {
            number: value["number"].as_u64()?,
            title: value["title"].as_str().unwrap_or_default().to_string(),
            body: value["body"].as_str().unwrap_or_default().to_string(),
            state: value["state"]
                .as_str()
                .unwrap_or("open")
                .to_ascii_lowercase(),
            reopened: value["state_reason"].as_str() == Some("reopened"),
            created: rfc3339(&value["created_at"]),
            updated: rfc3339(&value["updated_at"]),
            labels: value["labels"]
                .as_array()
                .map(|labels| {
                    labels
                        .iter()
                        .filter_map(|label| label["name"].as_str().or(label.as_str()))
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
            assignee: value["assignees"]
                .as_array()
                .and_then(|assignees| assignees.first())
                .and_then(login)
                .or_else(|| login(&value["assignee"])),
            author: login(&value["user"]),
            node_id: value["node_id"].as_str().map(str::to_string),
        })
    }

    fn from_graphql(value: &Value) -> Option<Self> {
        let nodes = |field: &str, key: &str| -> Vec<String> {
            value[field]["nodes"]
                .as_array()
                .map(|nodes| {
                    nodes
                        .iter()
                        .filter_map(|node| node[key].as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default()
        };
        Some(Self {
            number: value["number"].as_u64()?,
            title: value["title"].as_str().unwrap_or_default().to_string(),
            body: value["body"].as_str().unwrap_or_default().to_string(),
            state: value["state"]
                .as_str()
                .unwrap_or("OPEN")
                .to_ascii_lowercase(),
            reopened: value["stateReason"].as_str() == Some("REOPENED"),
            created: rfc3339(&value["createdAt"]),
            updated: rfc3339(&value["updatedAt"]),
            labels: nodes("labels", "name"),
            assignee: nodes("assignees", "login")
                .first()
                .map(|login| login.to_ascii_lowercase()),
            author: value["author"]["login"]
                .as_str()
                .map(str::to_ascii_lowercase),
            node_id: value["id"].as_str().map(str::to_string),
        })
    }
}

/// The issue fields a GraphQL lookup reads, matching [`RawIssue::from_graphql`].
const ISSUE_FIELDS: &str = "id number title body state stateReason createdAt updatedAt \
     author { login } labels(first: 50) { nodes { name } } \
     assignees(first: 10) { nodes { login } }";

/// Logins @-mentioned in `body`, lower-cased, each once, in order. An `@`
/// inside a word (an email address) is not a mention.
pub(crate) fn mentions_in(body: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let chars: Vec<char> = body.chars().collect();
    for (index, c) in chars.iter().enumerate() {
        if *c != '@' {
            continue;
        }
        if index > 0 {
            let before = chars[index - 1];
            if before.is_ascii_alphanumeric() || matches!(before, '.' | '_' | '-' | '/' | '`') {
                continue;
            }
        }
        let login: String = chars[index + 1..]
            .iter()
            .take_while(|c| c.is_ascii_alphanumeric() || **c == '-')
            .collect();
        let login = login.trim_end_matches('-').to_ascii_lowercase();
        if !login.is_empty() && login.len() <= 39 && !out.contains(&login) {
            out.push(login);
        }
    }
    out
}

/// Attachment URLs in a body, with the name the markdown gives each. GitHub
/// keeps uploads under `/user-attachments/` (and older ones on the
/// `user-images` hosts, or under the repository's `/assets/`).
fn attachment_links(body: &str, repo: &RepoRef) -> Vec<(String, Option<String>)> {
    let repo_assets = format!("{}/assets/", repo.web());
    let web_host = repo.web();
    let web_host = web_host
        .splitn(4, '/')
        .take(3)
        .collect::<Vec<_>>()
        .join("/");
    let mut out: Vec<(String, Option<String>)> = Vec::new();
    let mut rest = body;
    let mut consumed = 0usize;
    while let Some(start) = rest.find("https://") {
        let tail = &rest[start..];
        let end = tail
            .find(|c: char| c.is_whitespace() || matches!(c, ')' | '"' | '\'' | '>' | '<' | ']'))
            .unwrap_or(tail.len());
        let url = &tail[..end];
        let is_attachment = url.starts_with("https://github.com/user-attachments/")
            || url.starts_with(&format!("{web_host}/user-attachments/"))
            || url.starts_with("https://user-images.githubusercontent.com/")
            || url.starts_with("https://private-user-images.githubusercontent.com/")
            || url.starts_with(&repo_assets);
        if is_attachment && !out.iter().any(|(seen, _)| seen == url) {
            // `[name](url)` or `![name](url)`: the text before `(` names it.
            let absolute = consumed + start;
            let before = &body[..absolute];
            let name = before
                .strip_suffix('(')
                .and_then(|before| before.strip_suffix(']'))
                .and_then(|before| before.rfind('[').map(|open| &before[open + 1..]))
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_string);
            out.push((url.to_string(), name));
        }
        consumed += start + end.max(1);
        rest = &body[consumed..];
    }
    out
}

/// A safe file name: the basename of `raw`, with anything odd made `_`.
fn safe_name(raw: &str) -> String {
    let base = Path::new(raw.split('?').next().unwrap_or(raw))
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    base.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ' ') {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim()
        .trim_start_matches('.')
        .to_string()
}

// ---------------------------------------------------------------------------
// The tracker
// ---------------------------------------------------------------------------

pub struct GithubIssuesTracker {
    config: TrackerConfig,
    repo: Option<RepoRef>,
    runner: Arc<dyn GhRunner>,
    /// The `gh` account, resolved once per tracker; the tracker is rebuilt
    /// when its config changes.
    identity: tokio::sync::OnceCell<TrackerIdentity>,
}

impl GithubIssuesTracker {
    pub fn new(config: TrackerConfig) -> Self {
        Self::with_runner(config, Arc::new(CliRunner))
    }

    pub(crate) fn with_runner(config: TrackerConfig, runner: Arc<dyn GhRunner>) -> Self {
        let repo = config.repo().and_then(RepoRef::parse);
        Self {
            config,
            repo,
            runner,
            identity: tokio::sync::OnceCell::new(),
        }
    }

    pub fn descriptor() -> Descriptor {
        Descriptor::new(
            KIND.as_str(),
            "GitHub Issues",
            "Issues in one repository, read and written through the gh CLI: it uses the \
             account `gh auth login` signed in on this machine, so there is no token to \
             enter. Stages are labels; done closes the issue.",
            vec![
                Field::new(
                    "repo",
                    "Repository",
                    FieldKind::Text,
                    true,
                    "owner/name, or host/owner/name on GitHub Enterprise. Issue keys are the \
                     repository name and number, e.g. WIDGETS-42.",
                ),
                Field::new(
                    "bot_identity",
                    "Agent login",
                    FieldKind::Text,
                    false,
                    "The login the agent's own comments are recognised by. Blank uses the \
                     account gh is signed in as.",
                ),
                Field::new(
                    "import_filter",
                    "Import filter",
                    FieldKind::Text,
                    false,
                    "Terms the import dialog lists: state:open|closed|all, label:bug \
                     (repeatable; quote names with spaces), -label:wontfix, \
                     assignee:@me|login|none. Default: state:open assignee:@me.",
                ),
                Field::new(
                    "pickup_filter",
                    "Pickup filter",
                    FieldKind::Text,
                    false,
                    "Which new issues are picked up, in the same terms; a created-recently \
                     window is added. Default: state:open label:bug.",
                ),
                Field::new(
                    "assigned_filter",
                    "Assigned pickup filter",
                    FieldKind::Text,
                    false,
                    "Issues assigned to the agent, started whatever their labels, e.g. \
                     state:open assignee:@me. Blank turns it off.",
                ),
            ],
            "https://cli.github.com/manual/gh_auth_login",
        )
    }

    fn repo(&self) -> TrackerResult<&RepoRef> {
        self.repo.as_ref().ok_or_else(|| {
            TrackerError::NotConfigured(
                "ticketing.repo is not an owner/name GitHub repository".to_string(),
            )
        })
    }

    fn key_for(&self, number: u64) -> String {
        match &self.repo {
            Some(repo) => format!("{}-{number}", repo.key_prefix()),
            None => format!("#{number}"),
        }
    }

    fn number(&self, key: &str) -> TrackerResult<u64> {
        number_of(key)
            .ok_or_else(|| TrackerError::Other(anyhow!("`{key}` is not a GitHub issue key")))
    }

    fn issue_path(&self, key: &str) -> TrackerResult<String> {
        Ok(format!(
            "{}/issues/{}",
            self.repo()?.api_path(),
            self.number(key)?
        ))
    }

    /// `gh api <endpoint> --method <method>`, with a JSON body on stdin.
    async fn api(
        &self,
        method: &str,
        endpoint: &str,
        body: Option<&Value>,
    ) -> TrackerResult<Value> {
        let mut args = vec![
            "api".to_string(),
            endpoint.to_string(),
            "--method".to_string(),
            method.to_string(),
        ];
        if let Some(host) = self.repo.as_ref().and_then(|repo| repo.host.as_deref()) {
            args.push("--hostname".to_string());
            args.push(host.to_string());
        }
        let input = match body {
            Some(body) => {
                args.push("--input".to_string());
                args.push("-".to_string());
                Some(serde_json::to_vec(body).context("failed to encode a GitHub request")?)
            }
            None => None,
        };
        let output = self.runner.run(&args, input.as_deref()).await?;
        if output.iter().all(u8::is_ascii_whitespace) {
            return Ok(Value::Null);
        }
        serde_json::from_slice(&output)
            .with_context(|| format!("GitHub answered {endpoint} with something other than JSON"))
            .map_err(TrackerError::from)
    }

    /// A GraphQL call. `data` is returned even beside errors that only say an
    /// aliased issue does not exist; any other error fails the call.
    async fn graphql(&self, query: &str, variables: Value) -> TrackerResult<Value> {
        let mut response = self
            .api(
                "POST",
                "graphql",
                Some(&json!({ "query": query, "variables": variables })),
            )
            .await?;
        let blocking: Vec<String> = response["errors"]
            .as_array()
            .map(|errors| {
                errors
                    .iter()
                    .filter(|error| error["type"].as_str() != Some("NOT_FOUND"))
                    .map(|error| {
                        error["message"]
                            .as_str()
                            .unwrap_or("unknown GraphQL error")
                            .to_string()
                    })
                    .collect()
            })
            .unwrap_or_default();
        if !blocking.is_empty() || response["data"].is_null() {
            return Err(TrackerError::Other(anyhow!(
                "GitHub GraphQL: {}",
                if blocking.is_empty() {
                    "no data".to_string()
                } else {
                    blocking.join("; ")
                }
            )));
        }
        Ok(response["data"].take())
    }

    async fn raw_issue(&self, key: &str) -> TrackerResult<RawIssue> {
        let value = self.api("GET", &self.issue_path(key)?, None).await?;
        RawIssue::from_rest(&value)
            .ok_or_else(|| TrackerError::Other(anyhow!("{key} is not an issue")))
    }

    /// Status labels as configured, in stage order: every stage's mapped
    /// label, and the label a reopened original is parked under.
    fn status_labels(&self) -> Vec<String> {
        let mut labels: Vec<String> = [
            TicketStage::Planning,
            TicketStage::Executing,
            TicketStage::Reviewing,
            TicketStage::PushedMr,
            TicketStage::Triaged,
            TicketStage::Done,
        ]
        .into_iter()
        .filter_map(|stage| stage.mapped(&self.config.status_mapping))
        .collect();
        labels.push(reopen::BLOCKED_STATUS.to_string());
        labels
    }

    /// Labels that mean the work is under way.
    fn working_labels(&self) -> Vec<String> {
        [
            TicketStage::Planning,
            TicketStage::Executing,
            TicketStage::Reviewing,
            TicketStage::PushedMr,
            TicketStage::Triaged,
        ]
        .into_iter()
        .filter_map(|stage| stage.mapped(&self.config.status_mapping))
        .collect()
    }

    /// Category and name: closed is `done` (named after its done label when
    /// it has one), open with a working label `indeterminate` (named after
    /// it), any other open issue `new`.
    fn status_of(&self, raw: &RawIssue) -> (&'static str, String) {
        let has = |label: &str| raw.labels.iter().any(|l| l.eq_ignore_ascii_case(label));
        if raw.state == "closed" {
            let name = TicketStage::Done
                .mapped(&self.config.status_mapping)
                .filter(|done| has(done))
                .unwrap_or_else(|| "Closed".to_string());
            return ("done", name);
        }
        match self.working_labels().into_iter().find(|label| has(label)) {
            Some(label) => ("indeterminate", label),
            None => ("new", "Open".to_string()),
        }
    }

    /// The first label naming a `type_mapping` entry, as the mapping spells
    /// it; "Issue" (unmapped) without one.
    fn issue_type_of(&self, raw: &RawIssue) -> String {
        raw.labels
            .iter()
            .find_map(|label| {
                self.config
                    .type_mapping
                    .keys()
                    .find(|name| name.eq_ignore_ascii_case(label))
                    .cloned()
            })
            .unwrap_or_else(|| "Issue".to_string())
    }

    fn priority_of(raw: &RawIssue) -> Option<String> {
        raw.labels.iter().find_map(|label| {
            let label = label.trim();
            let mut chars = label.chars();
            (matches!(chars.next(), Some('p' | 'P'))
                && label.len() == 2
                && chars.all(|c| c.is_ascii_digit()))
            .then(|| label.to_ascii_uppercase())
        })
    }

    fn to_fetched(&self, raw: RawIssue) -> FetchedIssue {
        let (category, status) = self.status_of(&raw);
        FetchedIssue {
            key: self.key_for(raw.number),
            issue_type: self.issue_type_of(&raw),
            priority_name: Self::priority_of(&raw),
            description_text: raw.body.trim().to_string(),
            parent_key: None,
            parent_type: None,
            created: raw.created,
            updated: raw.updated,
            assignee_account_id: raw.assignee.clone(),
            status_category: Some(category.to_string()),
            status_name: Some(status),
            labels: raw.labels.iter().map(|l| l.to_ascii_lowercase()).collect(),
            summary: raw.title,
            area: None,
        }
    }

    /// Lists the repository's issues a filter selects, oldest first when it
    /// has a window.
    async fn list(&self, filter: &Filter) -> TrackerResult<Vec<FetchedIssue>> {
        let repo = self.repo()?;
        let mut params: Vec<(String, String)> = vec![
            (
                "state".to_string(),
                filter.state.clone().unwrap_or_else(|| "open".to_string()),
            ),
            ("per_page".to_string(), PAGE_SIZE.to_string()),
        ];
        if !filter.labels.is_empty() {
            params.push(("labels".to_string(), filter.labels.join(",")));
        }
        if let Some(assignee) = &filter.assignee {
            let assignee = if assignee.eq_ignore_ascii_case("@me") {
                self.identity().await?.account_id
            } else {
                assignee.clone()
            };
            params.push(("assignee".to_string(), assignee));
        }
        // `since` is on `updated`, which is never earlier than `created`, so
        // it narrows a created window too; the exact bound is applied below.
        if let Some(since) = filter.updated_since.or(filter.created_since) {
            params.push((
                "since".to_string(),
                since.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            ));
        }
        let (sort, direction) = match filter.sort {
            Some(field) => (field, "asc"),
            None => ("created", "desc"),
        };
        params.push(("sort".to_string(), sort.to_string()));
        params.push(("direction".to_string(), direction.to_string()));

        let mut out = Vec::new();
        for page in 1..=MAX_ISSUE_PAGES {
            let query = params
                .iter()
                .chain(std::iter::once(&("page".to_string(), page.to_string())))
                .map(|(key, value)| format!("{key}={}", urlencode(value)))
                .collect::<Vec<_>>()
                .join("&");
            let value = self
                .api("GET", &format!("{}/issues?{query}", repo.api_path()), None)
                .await?;
            let entries = value.as_array().cloned().unwrap_or_default();
            let count = entries.len();
            for raw in entries.iter().filter_map(RawIssue::from_rest) {
                let created_ok = match (filter.created_since, raw.created) {
                    (Some(since), Some(created)) => created >= since,
                    _ => true,
                };
                let excluded = raw.labels.iter().any(|label| {
                    filter
                        .without
                        .iter()
                        .any(|without| without.eq_ignore_ascii_case(label))
                });
                if created_ok && !excluded && (!filter.reopened || raw.reopened) {
                    out.push(self.to_fetched(raw));
                }
            }
            if count < PAGE_SIZE {
                break;
            }
        }
        Ok(out)
    }

    async fn set_state(&self, key: &str, open: bool) -> TrackerResult<()> {
        let body = if open {
            json!({ "state": "open" })
        } else {
            json!({ "state": "closed", "state_reason": "completed" })
        };
        self.api("PATCH", &self.issue_path(key)?, Some(&body))
            .await?;
        Ok(())
    }

    /// Comments as GitHub returns them, oldest first.
    async fn raw_comments(&self, key: &str) -> TrackerResult<Vec<Value>> {
        let path = self.issue_path(key)?;
        let mut out = Vec::new();
        for page in 1..=MAX_COMMENT_PAGES {
            let value = self
                .api(
                    "GET",
                    &format!("{path}/comments?per_page={PAGE_SIZE}&page={page}"),
                    None,
                )
                .await?;
            let entries = value.as_array().cloned().unwrap_or_default();
            let count = entries.len();
            out.extend(entries);
            if count < PAGE_SIZE {
                break;
            }
        }
        Ok(out)
    }

    async fn create_raw(&self, body: &Value) -> TrackerResult<RawIssue> {
        let value = self
            .api(
                "POST",
                &format!("{}/issues", self.repo()?.api_path()),
                Some(body),
            )
            .await?;
        RawIssue::from_rest(&value).ok_or_else(|| {
            TrackerError::Other(anyhow!("GitHub answered a create without an issue"))
        })
    }

    /// Links `child` under `parent` as a sub-issue.
    async fn add_sub_issue(&self, parent: &str, child: &str) -> TrackerResult<()> {
        self.graphql(
            "mutation($issueId: ID!, $subIssueId: ID!) { \
             addSubIssue(input: {issueId: $issueId, subIssueId: $subIssueId}) { issue { number } } }",
            json!({ "issueId": parent, "subIssueId": child }),
        )
        .await?;
        Ok(())
    }
}

/// Percent-encodes a query value.
fn urlencode(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

#[async_trait]
impl Tracker for GithubIssuesTracker {
    fn kind(&self) -> TrackerKind {
        KIND
    }

    fn is_configured(&self) -> bool {
        self.repo.is_some() && self.config.mode_mismatch().is_none()
    }

    fn issue_url(&self, key: &str) -> String {
        match (&self.repo, number_of(key)) {
            (Some(repo), Some(number)) => format!("{}/issues/{number}", repo.web()),
            (Some(repo), None) => format!("{}/issues", repo.web()),
            _ => key.to_string(),
        }
    }

    fn is_valid_key(&self, key: &str) -> bool {
        let Some(repo) = &self.repo else {
            return false;
        };
        is_project_key(key)
            && key
                .rsplit_once('-')
                .is_some_and(|(prefix, _)| prefix.eq_ignore_ascii_case(&repo.key_prefix()))
    }

    /// `repos/{owner}/{repo}` and `user`: the repository is reachable with
    /// issues on, and who `gh` is signed in as.
    async fn test(&self) -> TrackerResult<String> {
        let repo = self.repo()?;
        let found = self.api("GET", &repo.api_path(), None).await?;
        if found["has_issues"].as_bool() == Some(false) {
            return Err(TrackerError::NotConfigured(format!(
                "issues are turned off for {}",
                repo.full_name()
            )));
        }
        let user = self.api("GET", "user", None).await?;
        Ok(format!(
            "gh is signed in as {}; {} has {} open issue(s).",
            user["login"].as_str().unwrap_or("an unnamed account"),
            found["full_name"]
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| repo.full_name()),
            found["open_issues_count"].as_u64().unwrap_or(0)
        ))
    }

    async fn search(&self, query: &str) -> TrackerResult<Vec<FetchedIssue>> {
        let filter = parse_filter(query).map_err(|e| TrackerError::Other(anyhow!(e)))?;
        self.list(&filter).await
    }

    async fn find(&self, query: Query<'_>) -> TrackerResult<Vec<FetchedIssue>> {
        let since = |within: chrono::Duration| Some(Utc::now() - within);
        let parse =
            |filter: &str| parse_filter(filter).map_err(|e| TrackerError::Other(anyhow!(e)));
        match query {
            Query::Created { filter, within } => {
                let mut parsed = parse(filter)?;
                parsed.created_since = since(within);
                parsed.sort = Some("created");
                self.list(&parsed).await
            }
            Query::Updated { filter, within } => {
                let mut parsed = parse(filter)?;
                parsed.updated_since = since(within);
                parsed.sort = Some("updated");
                self.list(&parsed).await
            }
            Query::Reopened { within, .. } => {
                self.list(&Filter {
                    state: Some("open".to_string()),
                    reopened: true,
                    updated_since: since(within),
                    sort: Some("updated"),
                    ..Default::default()
                })
                .await
            }
            Query::Active { within, .. } => {
                self.list(&Filter {
                    state: Some("all".to_string()),
                    updated_since: since(within),
                    sort: Some("updated"),
                    ..Default::default()
                })
                .await
            }
            Query::InStatuses {
                statuses, within, ..
            } => {
                if statuses.is_empty() {
                    return Ok(Vec::new());
                }
                let all = self
                    .list(&Filter {
                        state: Some("all".to_string()),
                        updated_since: since(within),
                        sort: Some("updated"),
                        ..Default::default()
                    })
                    .await?;
                Ok(all
                    .into_iter()
                    .filter(|issue| {
                        issue.status_name.as_deref().is_some_and(|status| {
                            statuses
                                .iter()
                                .any(|s| s.trim().eq_ignore_ascii_case(status))
                        })
                    })
                    .collect())
            }
        }
    }

    /// One GraphQL lookup per [`FETCH_BATCH`] keys, each issue an alias.
    async fn fetch(&self, keys: &[String]) -> TrackerResult<Vec<FetchedIssue>> {
        let repo = self.repo()?;
        let mut numbers: Vec<u64> = keys.iter().filter_map(|key| number_of(key)).collect();
        numbers.dedup();
        let mut out = Vec::new();
        for batch in numbers.chunks(FETCH_BATCH) {
            let aliases = batch
                .iter()
                .map(|number| format!("i{number}: issue(number: {number}) {{ {ISSUE_FIELDS} }}"))
                .collect::<Vec<_>>()
                .join(" ");
            let data = self
                .graphql(
                    &format!(
                        "query($owner: String!, $name: String!) {{ \
                         repository(owner: $owner, name: $name) {{ {aliases} }} }}"
                    ),
                    json!({ "owner": repo.owner, "name": repo.name }),
                )
                .await?;
            for number in batch {
                if let Some(raw) = RawIssue::from_graphql(&data["repository"][format!("i{number}")])
                {
                    out.push(self.to_fetched(raw));
                }
            }
        }
        Ok(out)
    }

    async fn get_issue(&self, key: &str) -> TrackerResult<IssueFields> {
        let raw = self.raw_issue(key).await?;
        let (category, status) = self.status_of(&raw);
        Ok(IssueFields {
            summary: raw.title.clone(),
            issue_type: Some(self.issue_type_of(&raw)),
            priority: Self::priority_of(&raw),
            status_category: Some(category.to_string()),
            assignee_account_id: raw.assignee.clone(),
            reporter_account_id: raw.author.clone(),
            status_name: Some(status),
            labels: raw.labels.iter().map(|l| l.to_ascii_lowercase()).collect(),
        })
    }

    async fn comments(&self, key: &str) -> TrackerResult<Vec<TrackerComment>> {
        Ok(self
            .raw_comments(key)
            .await?
            .iter()
            .map(|comment| {
                let body = comment["body"].as_str().unwrap_or_default();
                TrackerComment {
                    author: comment["user"]["login"]
                        .as_str()
                        .unwrap_or("Unknown")
                        .to_string(),
                    author_account_id: login(&comment["user"]),
                    mentions: mentions_in(body),
                    created: rfc3339(&comment["created_at"]),
                    body_text: body.trim().to_string(),
                }
            })
            .collect())
    }

    async fn post_comment(&self, key: &str, text: &str) -> TrackerResult<()> {
        let body = clip_text(&at_mentions(text), BODY_LIMIT, "GitHub");
        self.api(
            "POST",
            &format!("{}/comments", self.issue_path(key)?),
            Some(&json!({ "body": body })),
        )
        .await?;
        Ok(())
    }

    /// `Open` and `Closed` set the state. Any other name is a status label:
    /// the issue's other status labels are removed and this one added; a
    /// done or shipped label also closes the issue, any other reopens it.
    async fn transition(&self, key: &str, status_name: &str) -> TrackerResult<()> {
        let target = status_name.trim();
        let raw = self.raw_issue(key).await?;
        let path = self.issue_path(key)?;
        let open_or_closed = match target.to_ascii_lowercase().as_str() {
            "open" | "opened" | "reopened" => Some(true),
            "closed" | "close" => Some(false),
            _ => None,
        };
        for label in self.status_labels() {
            let present = raw.labels.iter().find(|l| l.eq_ignore_ascii_case(&label));
            if let Some(present) = present
                && !present.eq_ignore_ascii_case(target)
            {
                self.api(
                    "DELETE",
                    &format!("{path}/labels/{}", urlencode(present).replace('+', "%20")),
                    None,
                )
                .await?;
            }
        }
        let open = match open_or_closed {
            Some(open) => open,
            None => {
                if !raw.labels.iter().any(|l| l.eq_ignore_ascii_case(target)) {
                    self.api(
                        "POST",
                        &format!("{path}/labels"),
                        Some(&json!({ "labels": [target] })),
                    )
                    .await?;
                }
                let finishes = TicketStage::Done
                    .mapped(&self.config.status_mapping)
                    .is_some_and(|done| done.eq_ignore_ascii_case(target))
                    || self.config.status_mapping.is_shipped(target);
                !finishes
            }
        };
        if open != (raw.state == "open") {
            self.set_state(key, open).await?;
        }
        Ok(())
    }

    async fn transition_to_in_progress(&self, key: &str) -> TrackerResult<()> {
        let label = self.config.status_mapping.planning_status().to_string();
        self.transition(key, &label).await
    }

    /// Planning labels the issue and assigns it to the agent, like Jira;
    /// other stages move the label only when the mapping names one.
    async fn enter_stage(&self, key: &str, stage: TicketStage) -> TrackerResult<()> {
        if stage == TicketStage::Planning {
            let moved = self.transition_to_in_progress(key).await;
            let assigned = self.assign_to_self(key).await;
            return moved.and(assigned);
        }
        match stage.mapped(&self.config.status_mapping) {
            Some(label) => self.transition(key, &label).await,
            None => Ok(()),
        }
    }

    async fn assignee(&self, key: &str) -> TrackerResult<Option<TrackerUser>> {
        let value = self.api("GET", &self.issue_path(key)?, None).await?;
        let user = value["assignees"]
            .as_array()
            .and_then(|assignees| assignees.first().cloned())
            .unwrap_or_else(|| value["assignee"].clone());
        Ok(user["login"].as_str().map(|name| TrackerUser {
            account_id: name.to_ascii_lowercase(),
            display_name: name.to_string(),
        }))
    }

    async fn assign_to_self(&self, key: &str) -> TrackerResult<()> {
        let me = self.identity().await?;
        self.api(
            "POST",
            &format!("{}/assignees", self.issue_path(key)?),
            Some(&json!({ "assignees": [me.account_id] })),
        )
        .await?;
        Ok(())
    }

    async fn set_summary(&self, key: &str, summary: &str) -> TrackerResult<()> {
        self.api(
            "PATCH",
            &self.issue_path(key)?,
            Some(&json!({ "title": summary })),
        )
        .await?;
        Ok(())
    }

    /// A "Blocks #n" comment on the blocker, which GitHub cross-links into
    /// the blocked issue's timeline; then GraphQL `addBlockedBy`, which
    /// records the dependency where the repository has issue dependencies.
    /// Without them the comment is the whole link.
    async fn link_blocks(&self, blocker: &str, blocked: &str) -> TrackerResult<()> {
        let blocked_number = self.number(blocked)?;
        self.post_comment(blocker, &format!("Blocks #{blocked_number}"))
            .await?;
        let repo = self.repo()?;
        let ids = self
            .graphql(
                &format!(
                    "query($owner: String!, $name: String!) {{ repository(owner: $owner, name: $name) \
                     {{ blocker: issue(number: {}) {{ id }} blocked: issue(number: {blocked_number}) \
                     {{ id }} }} }}",
                    self.number(blocker)?
                ),
                json!({ "owner": repo.owner, "name": repo.name }),
            )
            .await;
        let linked = match ids {
            Ok(ids) => match (
                ids["repository"]["blocked"]["id"].as_str(),
                ids["repository"]["blocker"]["id"].as_str(),
            ) {
                (Some(issue), Some(blocking)) => self
                    .graphql(
                        "mutation($issueId: ID!, $blockingIssueId: ID!) { \
                         addBlockedBy(input: {issueId: $issueId, blockingIssueId: $blockingIssueId}) \
                         { issue { number } } }",
                        json!({ "issueId": issue, "blockingIssueId": blocking }),
                    )
                    .await
                    .map(|_| ()),
                _ => Err(TrackerError::Other(anyhow!("an issue was not found"))),
            },
            Err(error) => Err(error),
        };
        if let Err(error) = linked {
            tracing::debug!(
                "GitHub Issues: {blocker} blocks {blocked} is recorded as a comment only: {error}"
            );
        }
        Ok(())
    }

    /// One comment per link id, edited in place when pinned again.
    async fn set_remote_link(&self, key: &str, link: RemoteLink<'_>) -> TrackerResult<()> {
        let marker = link_marker(link.id);
        let body = format!("{marker}\n**{}**: {}", link.title, link.url);
        let existing = self.raw_comments(key).await?.into_iter().find(|comment| {
            comment["body"]
                .as_str()
                .is_some_and(|text| text.contains(&marker))
        });
        match existing.and_then(|comment| comment["id"].as_u64()) {
            Some(id) => {
                self.api(
                    "PATCH",
                    &format!("{}/issues/comments/{id}", self.repo()?.api_path()),
                    Some(&json!({ "body": body })),
                )
                .await?;
            }
            None => {
                self.api(
                    "POST",
                    &format!("{}/comments", self.issue_path(key)?),
                    Some(&json!({ "body": body })),
                )
                .await?;
            }
        }
        Ok(())
    }

    /// GitHub has no API for subscribing someone else to an issue.
    async fn add_watcher(&self, _key: &str, _account_id: &str) -> TrackerResult<()> {
        Err(TrackerError::Unsupported {
            kind: KIND,
            operation: "subscribe another person to an issue",
        })
    }

    /// The issue type and priority become labels, so the issue reads back
    /// as what it was filed as.
    async fn create_issue(&self, issue: &NewIssue) -> TrackerResult<String> {
        let mut labels = issue.labels.clone();
        for extra in [&issue.issue_type, &issue.priority] {
            let extra = extra.trim();
            if !extra.is_empty() && !labels.iter().any(|l| l.eq_ignore_ascii_case(extra)) {
                labels.push(extra.to_string());
            }
        }
        let raw = self
            .create_raw(&json!({
                "title": issue.summary,
                "body": clip_text(&issue.description, BODY_LIMIT, "GitHub"),
                "labels": labels,
            }))
            .await?;
        Ok(self.key_for(raw.number))
    }

    /// Each item becomes an issue labelled with the sub-task type, linked
    /// under the parent as a sub-issue (GraphQL `addSubIssue`). Where the
    /// repository has no sub-issues, the first link fails and the parent's
    /// body gets a task list of the new issues instead. A credentials failure
    /// stops the batch; any other failure skips that item.
    async fn create_subtasks(
        &self,
        parent: &str,
        items: &[NewSubtask],
    ) -> TrackerResult<Vec<CreatedSubtask>> {
        let parent_raw = self.raw_issue(parent).await?;
        let parent_number = parent_raw.number;
        let subtask_label = self
            .config
            .type_mapping
            .iter()
            .find(|(_, class)| **class == api_types::WorkClass::Subtask)
            .map(|(name, _)| name.clone())
            .unwrap_or_else(|| "Sub-task".to_string());

        let mut out = Vec::with_capacity(items.len());
        let mut created: Vec<(u64, String, Option<String>)> = Vec::new();
        for item in items {
            let mut body = item
                .description
                .as_deref()
                .filter(|text| !text.trim().is_empty())
                .map(|text| format!("{text}\n\n"))
                .unwrap_or_default();
            body.push_str(&format!("Part of #{parent_number}"));
            match self
                .create_raw(&json!({
                    "title": item.title,
                    "body": clip_text(&body, BODY_LIMIT, "GitHub"),
                    "labels": [subtask_label],
                }))
                .await
            {
                Ok(raw) => {
                    created.push((raw.number, item.title.clone(), raw.node_id.clone()));
                    out.push(CreatedSubtask {
                        title: item.title.clone(),
                        key: Some(self.key_for(raw.number)),
                    });
                }
                Err(TrackerError::AuthFailed) => return Err(TrackerError::AuthFailed),
                Err(error) => {
                    tracing::warn!(
                        "GitHub Issues: sub-task create failed for '{}': {error}",
                        item.title
                    );
                    out.push(CreatedSubtask {
                        title: item.title.clone(),
                        key: None,
                    });
                }
            }
        }

        let mut checklist: Vec<(u64, String)> = Vec::new();
        let mut sub_issues = parent_raw.node_id.is_some();
        for (number, title, node_id) in &created {
            if sub_issues && let (Some(parent_id), Some(child_id)) = (&parent_raw.node_id, node_id)
            {
                match self.add_sub_issue(parent_id, child_id).await {
                    Ok(()) => continue,
                    Err(error) => {
                        tracing::info!(
                            "GitHub Issues: sub-issues are not available on {parent} ({error}); \
                             listing the sub-tasks in its body instead"
                        );
                        sub_issues = false;
                    }
                }
            }
            checklist.push((*number, title.clone()));
        }
        if !checklist.is_empty() {
            let mut body = parent_raw.body.trim_end().to_string();
            body.push_str("\n\n### Sub-tasks\n");
            for (number, title) in &checklist {
                body.push_str(&format!("- [ ] #{number} {title}\n"));
            }
            self.api(
                "PATCH",
                &self.issue_path(parent)?,
                Some(&json!({ "body": clip_text(&body, BODY_LIMIT, "GitHub") })),
            )
            .await?;
        }
        Ok(out)
    }

    /// Files linked from the body and comments as GitHub uploads, fetched
    /// with `gh api` so private repositories' uploads come with the CLI's
    /// credentials. Names are reduced to a safe basename; collisions get a
    /// numeric prefix; a file that will not download is skipped.
    async fn download_attachments(
        &self,
        key: &str,
        dir: &Path,
    ) -> TrackerResult<Vec<TrackerAttachment>> {
        let repo = self.repo()?.clone();
        let raw = self.raw_issue(key).await?;
        let mut texts = vec![raw.body];
        texts.extend(
            self.raw_comments(key)
                .await?
                .iter()
                .filter_map(|comment| comment["body"].as_str().map(str::to_string)),
        );
        let mut links: Vec<(String, Option<String>)> = Vec::new();
        for text in &texts {
            for link in attachment_links(text, &repo) {
                if !links.iter().any(|(url, _)| *url == link.0) {
                    links.push(link);
                }
            }
        }
        if links.is_empty() {
            return Ok(Vec::new());
        }
        tokio::fs::create_dir_all(dir)
            .await
            .context("failed to create attachment directory")?;

        let mut used: HashSet<String> = HashSet::new();
        let mut downloaded = Vec::new();
        for (index, (url, name)) in links.into_iter().enumerate() {
            let mut base = safe_name(name.as_deref().unwrap_or(&url));
            if base.is_empty() {
                base = format!("attachment-{index}");
            }
            // Uploads are addressed by an opaque id; keep the extension the
            // markdown name or the URL carries.
            if Path::new(&base).extension().is_none()
                && let Some(ext) = Path::new(url.split('?').next().unwrap_or(&url)).extension()
            {
                base = format!("{base}.{}", ext.to_string_lossy());
            }
            let mut target: PathBuf = dir.join(&base);
            if used.contains(&base) || target.exists() {
                target = dir.join(format!("{index}-{base}"));
            }
            let mut args = vec!["api".to_string(), url.clone()];
            if let Some(host) = &repo.host {
                args.push("--hostname".to_string());
                args.push(host.clone());
            }
            let bytes = match self.runner.run(&args, None).await {
                Ok(bytes) => bytes,
                Err(GhError::AuthFailed(message)) => {
                    return Err(GhError::AuthFailed(message).into());
                }
                Err(error) => {
                    tracing::warn!("GitHub attachment download failed ({base}): {error}");
                    continue;
                }
            };
            tokio::fs::write(&target, &bytes)
                .await
                .context("failed to write attachment")?;
            used.insert(base.clone());
            downloaded.push(TrackerAttachment {
                mime_type: mime_guess::from_path(&target)
                    .first()
                    .map(|mime| mime.essence_str().to_string()),
                filename: target
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or(base),
                path: target.to_string_lossy().to_string(),
                size_bytes: bytes.len() as u64,
            });
        }
        Ok(downloaded)
    }

    /// `gh api user`, with `bot_identity` standing in for the login when
    /// configured. Resolved once per tracker.
    async fn identity(&self) -> TrackerResult<TrackerIdentity> {
        self.identity
            .get_or_try_init(|| async {
                if let Some(configured) = self.config.bot_identity() {
                    return Ok(TrackerIdentity {
                        account_id: configured.to_ascii_lowercase(),
                        display_name: configured.to_string(),
                    });
                }
                let user = self.api("GET", "user", None).await?;
                let login = user["login"]
                    .as_str()
                    .ok_or_else(|| anyhow!("gh api user returned no login"))?;
                Ok::<_, TrackerError>(TrackerIdentity {
                    account_id: login.to_ascii_lowercase(),
                    display_name: user["name"].as_str().unwrap_or(login).to_string(),
                })
            })
            .await
            .cloned()
    }

    /// The repository's collaborators: people with access to it, apps and
    /// the agent excluded.
    async fn assignable_users(&self, _project: &str) -> TrackerResult<Vec<TrackerUser>> {
        let repo = self.repo()?;
        let agent = self.identity().await.ok().map(|me| me.account_id);
        let mut users: Vec<TrackerUser> = Vec::new();
        for page in 1..=MAX_USER_PAGES {
            let value = self
                .api(
                    "GET",
                    &format!(
                        "{}/collaborators?per_page={PAGE_SIZE}&page={page}",
                        repo.api_path()
                    ),
                    None,
                )
                .await?;
            let entries = value.as_array().cloned().unwrap_or_default();
            let count = entries.len();
            for user in &entries {
                if user["type"].as_str() == Some("Bot") {
                    continue;
                }
                let Some(name) = user["login"].as_str() else {
                    continue;
                };
                let id = name.to_ascii_lowercase();
                if agent.as_deref() == Some(id.as_str())
                    || users.iter().any(|known| known.account_id == id)
                {
                    continue;
                }
                users.push(TrackerUser {
                    account_id: id,
                    display_name: name.to_string(),
                });
            }
            if count < PAGE_SIZE {
                break;
            }
        }
        Ok(users)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    /// One recorded `gh` answer: the call it answers (method and an endpoint
    /// prefix, plus a body substring for GraphQL), and stdout or an error.
    struct Answer {
        method: String,
        endpoint: String,
        body_contains: Option<String>,
        reply: Result<String, (Option<u16>, String)>,
    }

    /// A [`GhRunner`] that answers from recorded responses and records every
    /// call. The longest matching endpoint wins, then the newest answer, so a
    /// test can override one.
    #[derive(Default)]
    struct Recorded {
        answers: Mutex<Vec<Answer>>,
        calls: Mutex<Vec<(String, String, Option<Value>)>>,
    }

    impl Recorded {
        fn on(&self, method: &str, endpoint: &str, reply: Value) -> &Self {
            self.answers.lock().unwrap().push(Answer {
                method: method.to_string(),
                endpoint: endpoint.to_string(),
                body_contains: None,
                reply: Ok(reply.to_string()),
            });
            self
        }

        fn on_graphql(&self, body_contains: &str, reply: Value) -> &Self {
            self.answers.lock().unwrap().push(Answer {
                method: "POST".to_string(),
                endpoint: "graphql".to_string(),
                body_contains: Some(body_contains.to_string()),
                reply: Ok(reply.to_string()),
            });
            self
        }

        fn fail(&self, method: &str, endpoint: &str, status: u16, message: &str) -> &Self {
            self.answers.lock().unwrap().push(Answer {
                method: method.to_string(),
                endpoint: endpoint.to_string(),
                body_contains: None,
                reply: Err((Some(status), message.to_string())),
            });
            self
        }

        fn calls(&self) -> Vec<(String, String, Option<Value>)> {
            self.calls.lock().unwrap().clone()
        }

        /// Calls other than GETs, as "METHOD endpoint".
        fn writes(&self) -> Vec<String> {
            self.calls()
                .into_iter()
                .filter(|(method, endpoint, _)| method != "GET" && endpoint != "graphql")
                .map(|(method, endpoint, _)| format!("{method} {endpoint}"))
                .collect()
        }
    }

    #[async_trait]
    impl GhRunner for Recorded {
        async fn run(&self, args: &[String], stdin: Option<&[u8]>) -> Result<Vec<u8>, GhError> {
            assert_eq!(args[0], "api");
            let endpoint = args[1].clone();
            let method = args
                .iter()
                .position(|arg| arg == "--method")
                .map(|at| args[at + 1].clone())
                .unwrap_or_else(|| "GET".to_string());
            let body = stdin.map(|bytes| serde_json::from_slice::<Value>(bytes).unwrap());
            self.calls
                .lock()
                .unwrap()
                .push((method.clone(), endpoint.clone(), body.clone()));
            let body_text = body.as_ref().map(Value::to_string).unwrap_or_default();
            let answers = self.answers.lock().unwrap();
            // The most specific endpoint wins; among equals, the newest.
            let answer = answers
                .iter()
                .enumerate()
                .filter(|(_, answer)| {
                    answer.method == method
                        && endpoint.starts_with(&answer.endpoint)
                        && answer
                            .body_contains
                            .as_deref()
                            .is_none_or(|needle| body_text.contains(needle))
                })
                .max_by_key(|(index, answer)| (answer.endpoint.len(), *index))
                .map(|(_, answer)| answer)
                .unwrap_or_else(|| {
                    panic!("no recorded answer for {method} {endpoint} {body_text}")
                });
            match &answer.reply {
                Ok(text) => Ok(text.clone().into_bytes()),
                Err((status, message)) => Err(GhError::Failed {
                    status: *status,
                    message: message.clone(),
                }),
            }
        }
    }

    fn config() -> TrackerConfig {
        let mut config = TrackerConfig {
            kind: TrackerKind::GithubIssues,
            repo: Some("acme/widgets".to_string()),
            ..Default::default()
        };
        config.status_mapping.executing = Some("in fix".to_string());
        config.status_mapping.triaged = Some("triaged".to_string());
        config.status_mapping.done = Some("shipped".to_string());
        config
    }

    fn tracker(runner: &Arc<Recorded>) -> GithubIssuesTracker {
        GithubIssuesTracker::with_runner(config(), runner.clone())
    }

    fn issue(number: u64, labels: &[&str], state: &str) -> Value {
        json!({
            "number": number,
            "node_id": format!("I_{number}"),
            "title": format!("Issue {number}"),
            "body": "It breaks.",
            "state": state,
            "state_reason": null,
            "created_at": "2026-10-17T10:00:00Z",
            "updated_at": "2026-10-17T11:00:00Z",
            "labels": labels.iter().map(|name| json!({"name": name})).collect::<Vec<_>>(),
            "assignees": [],
            "assignee": null,
            "user": {"login": "Reporter"}
        })
    }

    #[test]
    fn repositories_keys_and_numbers() {
        let repo = RepoRef::parse("acme/widgets").unwrap();
        assert_eq!(repo.key_prefix(), "WIDGETS");
        assert_eq!(repo.web(), "https://github.com/acme/widgets");
        let enterprise = RepoRef::parse("https://github.example.com/team/my-app.git").unwrap();
        assert_eq!(enterprise.host.as_deref(), Some("github.example.com"));
        assert_eq!(enterprise.key_prefix(), "MY_APP");
        assert_eq!(RepoRef::parse("github.com/a/b").unwrap().host, None);
        assert_eq!(RepoRef::parse("9lives/x").unwrap().key_prefix(), "X");
        assert_eq!(RepoRef::parse("a/9x").unwrap().key_prefix(), "GH9X");
        assert!(RepoRef::parse("just-a-name").is_none());
        assert!(RepoRef::parse("a/b c").is_none());
        for key in ["WIDGETS-42", "#42", "42", "acme/widgets#42"] {
            assert_eq!(number_of(key), Some(42), "{key}");
        }
        assert_eq!(number_of("WIDGETS-"), None);

        let gh = GithubIssuesTracker::with_runner(config(), Arc::new(Recorded::default()));
        assert!(gh.is_configured());
        assert!(gh.is_valid_key("WIDGETS-42"));
        assert!(gh.is_valid_key("widgets-42"));
        assert!(!gh.is_valid_key("BUG-42"));
        assert!(!gh.is_valid_key("#42"));
        assert_eq!(
            gh.issue_url("WIDGETS-42"),
            "https://github.com/acme/widgets/issues/42"
        );
        let unconfigured = GithubIssuesTracker::with_runner(
            TrackerConfig {
                kind: TrackerKind::GithubIssues,
                ..Default::default()
            },
            Arc::new(Recorded::default()),
        );
        assert!(!unconfigured.is_configured());
    }

    #[test]
    fn filters_parse_into_list_terms_and_reject_jql() {
        let filter = parse_filter(
            r#"state:all label:bug,customer label:"needs triage" -label:wontfix assignee:@me"#,
        )
        .unwrap();
        assert_eq!(filter.state.as_deref(), Some("all"));
        assert_eq!(filter.labels, ["bug", "customer", "needs triage"]);
        assert_eq!(filter.without, ["wontfix"]);
        assert_eq!(filter.assignee.as_deref(), Some("@me"));
        assert_eq!(
            parse_filter("is:open no:assignee")
                .unwrap()
                .assignee
                .as_deref(),
            Some("none")
        );
        let windowed = parse_filter("created>=2026-10-17T00:00:00Z").unwrap();
        assert!(windowed.created_since.is_some());
        let error = parse_filter("project = BUG AND priority in (P0, P1)").unwrap_err();
        assert!(error.contains("not a GitHub Issues filter term"), "{error}");
    }

    #[test]
    fn mentions_markers_and_attachment_links() {
        assert_eq!(
            mentions_in("@PrepCook-bot can you look? cc @ada, mail bob@example.com, `@code`"),
            ["prepcook-bot", "ada"]
        );
        assert_eq!(
            at_mentions("[~accountid:ada] please check, [~accountid:] stays"),
            "@ada please check, [~accountid:] stays"
        );
        let repo = RepoRef::parse("acme/widgets").unwrap();
        let body = "Steps:\n![Screenshot 1](https://github.com/user-attachments/assets/abc-123)\n\
                    log: https://github.com/user-attachments/files/9/server.log and \
                    [elsewhere](https://example.com/x.png)";
        let links = attachment_links(body, &repo);
        assert_eq!(
            links,
            [
                (
                    "https://github.com/user-attachments/assets/abc-123".to_string(),
                    Some("Screenshot 1".to_string())
                ),
                (
                    "https://github.com/user-attachments/files/9/server.log".to_string(),
                    None
                ),
            ]
        );
        assert_eq!(safe_name("../../etc/passwd"), "passwd");
        assert_eq!(safe_name("a:b*.png"), "a_b_.png");
        assert_eq!(safe_name("https://x.test/f/shot.png?token=1"), "shot.png");
    }

    #[tokio::test]
    async fn search_lists_issues_with_the_filter_and_derives_status() {
        let runner = Arc::new(Recorded::default());
        let mut pull_request = issue(4, &["bug"], "open");
        pull_request["pull_request"] = json!({"url": "x"});
        runner.on(
            "GET",
            "repos/acme/widgets/issues?",
            json!([
                issue(1, &["Bug", "P1"], "open"),
                issue(2, &["bug", "in fix"], "open"),
                issue(3, &["bug", "wontfix"], "open"),
                pull_request,
                issue(5, &["bug", "shipped"], "closed"),
            ]),
        );
        let found = tracker(&runner)
            .search("state:all label:bug -label:wontfix")
            .await
            .unwrap();
        let keys: Vec<&str> = found.iter().map(|issue| issue.key.as_str()).collect();
        assert_eq!(keys, ["WIDGETS-1", "WIDGETS-2", "WIDGETS-5"]);
        assert_eq!(found[0].issue_type, "Bug");
        assert_eq!(found[0].priority_name.as_deref(), Some("P1"));
        assert_eq!(found[0].status_category.as_deref(), Some("new"));
        assert_eq!(found[0].labels, ["bug", "p1"]);
        assert_eq!(found[1].status_category.as_deref(), Some("indeterminate"));
        assert_eq!(found[1].status_name.as_deref(), Some("in fix"));
        assert_eq!(found[2].status_category.as_deref(), Some("done"));
        assert_eq!(found[2].status_name.as_deref(), Some("shipped"));

        let (_, endpoint, _) = &runner.calls()[0];
        assert!(endpoint.contains("state=all"), "{endpoint}");
        assert!(endpoint.contains("labels=bug"), "{endpoint}");
        assert!(endpoint.contains("per_page=100"), "{endpoint}");
    }

    #[tokio::test]
    async fn pickup_adds_a_created_window_oldest_first() {
        let runner = Arc::new(Recorded::default());
        let mut old = issue(7, &["bug"], "open");
        old["created_at"] = json!("2020-01-01T00:00:00Z");
        let fresh_at = (Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
        let mut fresh = issue(8, &["bug"], "open");
        fresh["created_at"] = json!(fresh_at);
        runner.on("GET", "repos/acme/widgets/issues?", json!([old, fresh]));
        runner.on("GET", "user", json!({"login": "PrepCook-Bot"}));
        let found = tracker(&runner)
            .find(Query::Created {
                filter: "state:open label:bug assignee:@me",
                within: chrono::Duration::hours(24),
            })
            .await
            .unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].key, "WIDGETS-8");
        let list = runner
            .calls()
            .into_iter()
            .find(|(_, endpoint, _)| endpoint.contains("/issues?"))
            .unwrap()
            .1;
        assert!(list.contains("sort=created&direction=asc"), "{list}");
        assert!(list.contains("since="), "{list}");
        // `@me` is the gh account, lower-cased.
        assert!(list.contains("assignee=prepcook-bot"), "{list}");
    }

    #[tokio::test]
    async fn reopened_issues_are_the_ones_github_says_were_reopened() {
        let runner = Arc::new(Recorded::default());
        let mut reopened = issue(9, &["bug"], "open");
        reopened["state_reason"] = json!("reopened");
        runner.on(
            "GET",
            "repos/acme/widgets/issues?",
            json!([issue(8, &["bug"], "open"), reopened]),
        );
        let found = tracker(&runner)
            .find(Query::Reopened {
                project: "ignored",
                within: chrono::Duration::days(3),
            })
            .await
            .unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].key, "WIDGETS-9");
        assert_eq!(found[0].status_category.as_deref(), Some("new"));
    }

    #[tokio::test]
    async fn fetch_reads_many_issues_in_one_graphql_call() {
        let runner = Arc::new(Recorded::default());
        runner.on_graphql(
            "i12: issue(number: 12)",
            json!({
                "data": {"repository": {
                    "i12": {"id": "I_12", "number": 12, "title": "Twelve", "body": "",
                            "state": "CLOSED", "stateReason": "COMPLETED",
                            "createdAt": "2026-10-01T00:00:00Z", "updatedAt": "2026-10-02T00:00:00Z",
                            "author": {"login": "Ada"},
                            "labels": {"nodes": [{"name": "bug"}]},
                            "assignees": {"nodes": [{"login": "Someone"}]}},
                    "i13": null
                }},
                "errors": [{"type": "NOT_FOUND", "message": "Could not resolve to an Issue"}]
            }),
        );
        let found = tracker(&runner)
            .fetch(&["WIDGETS-12".to_string(), "WIDGETS-13".to_string()])
            .await
            .unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].key, "WIDGETS-12");
        assert_eq!(found[0].status_category.as_deref(), Some("done"));
        assert_eq!(found[0].assignee_account_id.as_deref(), Some("someone"));
        let (_, _, body) = &runner.calls()[0];
        let body = body.as_ref().unwrap();
        assert_eq!(body["variables"]["owner"], "acme");
        assert_eq!(body["variables"]["name"], "widgets");
    }

    #[tokio::test]
    async fn comments_carry_lowercased_authors_and_mentions() {
        let runner = Arc::new(Recorded::default());
        runner.on(
            "GET",
            "repos/acme/widgets/issues/3/comments",
            json!([
                {"id": 1, "user": {"login": "Ada"}, "created_at": "2026-10-17T10:00:00Z",
                 "body": "@PrepCook-Bot RCA: APPROVED"},
                {"id": 2, "user": {"login": "prepcook-bot"}, "created_at": "2026-10-17T10:05:00Z",
                 "body": "Thanks @Ada"}
            ]),
        );
        let comments = tracker(&runner).comments("WIDGETS-3").await.unwrap();
        assert_eq!(comments.len(), 2);
        assert_eq!(comments[0].author, "Ada");
        assert_eq!(comments[0].author_account_id.as_deref(), Some("ada"));
        assert_eq!(comments[0].mentions, ["prepcook-bot"]);
        // Approval commands read the body exactly as Jira's do.
        assert_eq!(comments[0].body_text, "@PrepCook-Bot RCA: APPROVED");
        assert_eq!(comments[1].mentions, ["ada"]);
    }

    #[tokio::test]
    async fn posting_turns_mention_markers_into_at_mentions() {
        let runner = Arc::new(Recorded::default());
        runner.on(
            "POST",
            "repos/acme/widgets/issues/3/comments",
            json!({"id": 9}),
        );
        tracker(&runner)
            .post_comment("WIDGETS-3", "[~accountid:ada] the fix is merged.")
            .await
            .unwrap();
        let (method, endpoint, body) = runner.calls().remove(0);
        assert_eq!(
            (method.as_str(), endpoint.as_str()),
            ("POST", "repos/acme/widgets/issues/3/comments")
        );
        assert_eq!(body.unwrap()["body"], "@ada the fix is merged.");
    }

    #[tokio::test]
    async fn stages_move_labels_close_on_done_and_reopen_otherwise() {
        let runner = Arc::new(Recorded::default());
        runner
            .on(
                "GET",
                "repos/acme/widgets/issues/5",
                issue(5, &["bug", "In Progress"], "open"),
            )
            .on("DELETE", "repos/acme/widgets/issues/5/labels/", json!([]))
            .on("POST", "repos/acme/widgets/issues/5/labels", json!([]))
            .on("PATCH", "repos/acme/widgets/issues/5", json!({}));
        let gh = tracker(&runner);
        gh.enter_stage("WIDGETS-5", TicketStage::Executing)
            .await
            .unwrap();
        assert_eq!(
            runner.writes(),
            [
                "DELETE repos/acme/widgets/issues/5/labels/In%20Progress",
                "POST repos/acme/widgets/issues/5/labels",
            ]
        );

        // Done swaps the label and closes.
        runner.calls.lock().unwrap().clear();
        runner.on(
            "GET",
            "repos/acme/widgets/issues/5",
            issue(5, &["bug", "in fix"], "open"),
        );
        gh.enter_stage("WIDGETS-5", TicketStage::Done)
            .await
            .unwrap();
        assert_eq!(
            runner.writes(),
            [
                "DELETE repos/acme/widgets/issues/5/labels/in%20fix",
                "POST repos/acme/widgets/issues/5/labels",
                "PATCH repos/acme/widgets/issues/5",
            ]
        );
        let close = runner
            .calls()
            .into_iter()
            .find(|(method, _, _)| method == "PATCH")
            .unwrap()
            .2
            .unwrap();
        assert_eq!(close["state"], "closed");

        // A closed issue moved to a working stage is reopened.
        runner.calls.lock().unwrap().clear();
        runner.on(
            "GET",
            "repos/acme/widgets/issues/5",
            issue(5, &["bug", "shipped"], "closed"),
        );
        gh.transition("WIDGETS-5", "triaged").await.unwrap();
        let writes = runner.writes();
        assert_eq!(writes.last().unwrap(), "PATCH repos/acme/widgets/issues/5");
        let reopen = runner
            .calls()
            .into_iter()
            .find(|(method, _, _)| method == "PATCH")
            .unwrap()
            .2
            .unwrap();
        assert_eq!(reopen["state"], "open");

        // Unmapped stages leave the issue alone.
        runner.calls.lock().unwrap().clear();
        gh.enter_stage("WIDGETS-5", TicketStage::Reviewing)
            .await
            .unwrap();
        assert!(runner.calls().is_empty());
    }

    #[tokio::test]
    async fn planning_labels_and_assigns_the_gh_account() {
        let runner = Arc::new(Recorded::default());
        runner
            .on(
                "GET",
                "repos/acme/widgets/issues/6",
                issue(6, &["bug"], "open"),
            )
            .on("POST", "repos/acme/widgets/issues/6/labels", json!([]))
            .on(
                "GET",
                "user",
                json!({"login": "PrepCook-Bot", "name": "Prepcook"}),
            )
            .on("POST", "repos/acme/widgets/issues/6/assignees", json!({}));
        tracker(&runner)
            .enter_stage("WIDGETS-6", TicketStage::Planning)
            .await
            .unwrap();
        let calls = runner.calls();
        let label = calls
            .iter()
            .find(|(_, endpoint, _)| endpoint.ends_with("/labels"))
            .unwrap();
        assert_eq!(label.2.as_ref().unwrap()["labels"], json!(["In Progress"]));
        let assign = calls
            .iter()
            .find(|(_, endpoint, _)| endpoint.ends_with("/assignees"))
            .unwrap();
        assert_eq!(
            assign.2.as_ref().unwrap()["assignees"],
            json!(["prepcook-bot"])
        );
    }

    #[tokio::test]
    async fn blocks_is_a_comment_plus_a_dependency_where_available() {
        let runner = Arc::new(Recorded::default());
        runner
            .on("POST", "repos/acme/widgets/issues/10/comments", json!({"id": 1}))
            .on_graphql(
                "blocker: issue(number: 10)",
                json!({"data": {"repository": {"blocker": {"id": "I_10"}, "blocked": {"id": "I_4"}}}}),
            )
            .on_graphql(
                "addBlockedBy",
                json!({"data": {"addBlockedBy": {"issue": {"number": 4}}}}),
            );
        let gh = tracker(&runner);
        gh.link_blocks("WIDGETS-10", "WIDGETS-4").await.unwrap();
        let calls = runner.calls();
        assert_eq!(calls[0].2.as_ref().unwrap()["body"], "Blocks #4");
        let mutation = calls.last().unwrap().2.as_ref().unwrap();
        assert_eq!(mutation["variables"]["issueId"], "I_4");
        assert_eq!(mutation["variables"]["blockingIssueId"], "I_10");

        // Without issue dependencies the comment stands alone.
        runner.on_graphql(
            "addBlockedBy",
            json!({"data": null, "errors": [{"type": "UNDEFINED", "message": "no such field"}]}),
        );
        gh.link_blocks("WIDGETS-10", "WIDGETS-4").await.unwrap();
    }

    #[tokio::test]
    async fn remote_links_are_one_comment_edited_in_place() {
        let runner = Arc::new(Recorded::default());
        runner
            .on("GET", "repos/acme/widgets/issues/3/comments", json!([]))
            .on(
                "POST",
                "repos/acme/widgets/issues/3/comments",
                json!({"id": 77}),
            );
        let gh = tracker(&runner);
        let link = RemoteLink {
            id: "rca-BUG-3",
            title: "RCA",
            url: "https://prepcook.example.com/rca/1",
        };
        gh.set_remote_link("WIDGETS-3", link.clone()).await.unwrap();
        assert_eq!(
            runner.writes(),
            ["POST repos/acme/widgets/issues/3/comments"]
        );

        runner.calls.lock().unwrap().clear();
        runner
            .on(
                "GET",
                "repos/acme/widgets/issues/3/comments",
                json!([{"id": 77, "user": {"login": "bot"},
                        "body": format!("{}\n**RCA**: old", link_marker("rca-BUG-3"))}]),
            )
            .on("PATCH", "repos/acme/widgets/issues/comments/77", json!({}));
        gh.set_remote_link("WIDGETS-3", link).await.unwrap();
        assert_eq!(
            runner.writes(),
            ["PATCH repos/acme/widgets/issues/comments/77"]
        );
    }

    #[tokio::test]
    async fn watchers_are_not_supported() {
        let gh = tracker(&Arc::new(Recorded::default()));
        assert!(matches!(
            gh.add_watcher("WIDGETS-1", "ada").await,
            Err(TrackerError::Unsupported {
                kind: TrackerKind::GithubIssues,
                ..
            })
        ));
    }

    #[tokio::test]
    async fn created_issues_carry_type_and_priority_labels() {
        let runner = Arc::new(Recorded::default());
        runner.on("POST", "repos/acme/widgets/issues", issue(40, &[], "open"));
        let key = tracker(&runner)
            .create_issue(&NewIssue {
                project_key: None,
                issue_type: "Bug".to_string(),
                summary: "Follow-up".to_string(),
                description: "Again.".to_string(),
                priority: "P1".to_string(),
                labels: vec!["prepcook-followup".to_string()],
            })
            .await
            .unwrap();
        assert_eq!(key, "WIDGETS-40");
        let body = runner.calls()[0].2.clone().unwrap();
        assert_eq!(body["labels"], json!(["prepcook-followup", "Bug", "P1"]));
        assert_eq!(body["title"], "Follow-up");
    }

    #[tokio::test]
    async fn subtasks_are_sub_issues_or_a_checklist_in_the_parent() {
        let runner = Arc::new(Recorded::default());
        runner
            .on(
                "GET",
                "repos/acme/widgets/issues/20",
                issue(20, &["Epic"], "open"),
            )
            .on(
                "POST",
                "repos/acme/widgets/issues",
                issue(21, &["Sub-task"], "open"),
            )
            .on_graphql(
                "addSubIssue",
                json!({"data": {"addSubIssue": {"issue": {"number": 20}}}}),
            );
        let items = [NewSubtask {
            title: "Part one".to_string(),
            description: Some("Do the first part.".to_string()),
        }];
        let gh = tracker(&runner);
        let created = gh.create_subtasks("WIDGETS-20", &items).await.unwrap();
        assert_eq!(created[0].key.as_deref(), Some("WIDGETS-21"));
        let body = runner
            .calls()
            .into_iter()
            .find(|(method, endpoint, _)| {
                method == "POST" && endpoint == "repos/acme/widgets/issues"
            })
            .unwrap()
            .2
            .unwrap();
        assert_eq!(body["labels"], json!(["Sub-task"]));
        assert!(body["body"].as_str().unwrap().ends_with("Part of #20"));
        // Linked as a sub-issue: the parent body is untouched.
        assert!(
            !runner
                .writes()
                .iter()
                .any(|w| w == "PATCH repos/acme/widgets/issues/20")
        );

        // Without sub-issues the parent gets a task list.
        runner.calls.lock().unwrap().clear();
        runner
            .on_graphql(
                "addSubIssue",
                json!({"data": null, "errors": [{"type": "UNDEFINED", "message": "addSubIssue"}]}),
            )
            .on("PATCH", "repos/acme/widgets/issues/20", json!({}));
        gh.create_subtasks("WIDGETS-20", &items).await.unwrap();
        let patch = runner
            .calls()
            .into_iter()
            .find(|(method, _, _)| method == "PATCH")
            .unwrap()
            .2
            .unwrap();
        assert!(
            patch["body"]
                .as_str()
                .unwrap()
                .contains("### Sub-tasks\n- [ ] #21 Part one"),
            "{patch}"
        );
    }

    #[tokio::test]
    async fn attachments_download_through_gh_with_safe_names() {
        let runner = Arc::new(Recorded::default());
        let mut with_upload = issue(3, &[], "open");
        with_upload["body"] =
            json!("![shot.png](https://github.com/user-attachments/assets/aaa-111)");
        runner
            .on(
                "GET",
                "repos/acme/widgets/issues/3/comments",
                json!([
                    {"id": 1, "user": {"login": "ada"},
                     "body": "log at https://github.com/user-attachments/files/5/../../server.log"}
                ]),
            )
            .on("GET", "repos/acme/widgets/issues/3", with_upload)
            .on(
                "GET",
                "https://github.com/user-attachments/",
                json!("bytes"),
            );
        let dir = tempfile::tempdir().unwrap();
        let files = tracker(&runner)
            .download_attachments("WIDGETS-3", dir.path())
            .await
            .unwrap();
        let names: Vec<&str> = files.iter().map(|f| f.filename.as_str()).collect();
        assert_eq!(names, ["shot.png", "server.log"]);
        assert_eq!(files[0].mime_type.as_deref(), Some("image/png"));
        for file in &files {
            assert!(Path::new(&file.path).starts_with(dir.path()));
        }
    }

    #[tokio::test]
    async fn identity_users_and_test() {
        let runner = Arc::new(Recorded::default());
        runner
            .on(
                "GET",
                "user",
                json!({"login": "PrepCook-Bot", "name": "Prepcook"}),
            )
            .on(
                "GET",
                "repos/acme/widgets/collaborators",
                json!([
                    {"login": "Ada", "type": "User"},
                    {"login": "prepcook-bot", "type": "User"},
                    {"login": "dependabot", "type": "Bot"}
                ]),
            )
            .on(
                "GET",
                "repos/acme/widgets",
                json!({"full_name": "acme/widgets", "has_issues": true, "open_issues_count": 3}),
            );
        let gh = tracker(&runner);
        let me = gh.identity().await.unwrap();
        assert_eq!(me.account_id, "prepcook-bot");
        assert_eq!(me.display_name, "Prepcook");
        let users = gh.assignable_users("ignored").await.unwrap();
        assert_eq!(
            users,
            [TrackerUser {
                account_id: "ada".to_string(),
                display_name: "Ada".to_string()
            }]
        );
        assert_eq!(
            gh.test().await.unwrap(),
            "gh is signed in as PrepCook-Bot; acme/widgets has 3 open issue(s)."
        );

        let off = Arc::new(Recorded::default());
        off.on("GET", "repos/acme/widgets", json!({"has_issues": false}));
        assert!(matches!(
            tracker(&off).test().await,
            Err(TrackerError::NotConfigured(why)) if why.contains("turned off")
        ));
    }

    #[tokio::test]
    async fn gh_errors_map_to_tracker_errors() {
        let runner = Arc::new(Recorded::default());
        runner.fail("GET", "repos/acme/widgets/issues/1", 401, "Bad credentials");
        assert!(matches!(
            tracker(&runner).get_issue("WIDGETS-1").await,
            Err(TrackerError::AuthFailed)
        ));
        assert_eq!(http_status("gh: Not Found (HTTP 404)"), Some(404));
        assert_eq!(http_status("no status here"), None);
    }
}
