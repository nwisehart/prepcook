//! The ticketing seam: where cards come from and where the orchestrator
//! writes back to.
//!
//! Everything outside this module reaches the tracker through [`Tracker`],
//! obtained from a [`TrackerSource`]. Service-specific shapes (Jira's
//! document format and JQL, GitHub's labels and `gh api`, PagerDuty's notes
//! and statuses) stay in the provider's module; the types here are what every
//! provider answers in.
//!
//! The section is `ticketing` in the config. In `tickets` mode the source is
//! a ticket tracker (Jira, GitHub Issues; Linear is named but not
//! implemented). In `incidents` mode it is an incident tool (PagerDuty): open
//! incidents become investigation cards and the triage report goes back to
//! the incident as a note (`docs/incidents.md`).
//!
//! Adding a provider: a file here with a struct implementing [`Tracker`] and
//! a `descriptor()`, a variant in `TrackerKind` (config) and in
//! [`TrackerProvider`], and its arms in [`descriptor`] and [`build_provider`].
//! Settings and the configuration reference pick it up from the descriptor.

use std::{collections::BTreeMap, path::Path, sync::Arc};

use api_types::{CardOrigin, OriginKind};
use async_trait::async_trait;
use enum_dispatch::enum_dispatch;
use serde::Serialize;
use thiserror::Error;
use tokio::sync::RwLock;

use self::{
    github_issues::GithubIssuesTracker, jira::JiraTracker, linear::LinearTracker,
    pagerduty::PagerDutyTracker,
};
use crate::services::{
    config::{Config, StatusMapping, TicketingMode, TrackerConfig, TrackerKind},
    integrations::{Descriptor, FieldKind, SecretPrecedence, resolve_secret, secret_name},
    secrets::{self, Secrets},
};

pub mod github_issues;
pub mod jira;
pub mod linear;
pub mod pagerduty;

/// The integration area name, as the descriptors route and Settings use it.
pub const AREA: &str = "ticketing";

#[derive(Debug, Error)]
pub enum TrackerError {
    #[error("{0} is not yet supported as a tracker")]
    NotSupported(TrackerKind),
    /// The service has no API for this operation.
    #[error("{kind} cannot {operation}")]
    Unsupported {
        kind: TrackerKind,
        operation: &'static str,
    },
    #[error("the tracker is not configured: {0}")]
    NotConfigured(String),
    #[error("the tracker rejected the credentials")]
    AuthFailed,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type TrackerResult<T> = Result<T, TrackerError>;

/// One ticket as returned by [`Tracker::search`] and its siblings.
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
    /// The part of the product the ticket is about, when the tracker says
    /// (a PagerDuty incident's service). An imported card is tagged with it
    /// when the board has a tag of that name.
    pub area: Option<String>,
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

/// The watcher's and the orchestrator's questions, which each provider turns
/// into its own query language (or several calls). Windows are measured back
/// from now; results come oldest first.
#[derive(Debug, Clone, Copy)]
pub enum Query<'a> {
    /// Tickets the filter selects, created within the window: new-ticket
    /// pickup.
    Created {
        filter: &'a str,
        within: chrono::Duration,
    },
    /// Tickets the filter selects, updated within the window: assigned
    /// pickup, which an assignment reaches through `updated`.
    Updated {
        filter: &'a str,
        within: chrono::Duration,
    },
    /// Tickets in `project` whose status went back to the start within the
    /// window: the reopen sweep.
    Reopened {
        project: &'a str,
        within: chrono::Duration,
    },
    /// Every ticket in `project` updated within the window: the mention
    /// sweep.
    Active {
        project: &'a str,
        within: chrono::Duration,
    },
    /// Tickets in `project` whose status is one of `statuses`, updated within
    /// the window: shipped work that may still collect comments.
    InStatuses {
        project: &'a str,
        statuses: &'a [String],
        within: chrono::Duration,
    },
}

/// A board stage as the ticket sees it: [`Tracker::enter_stage`] mirrors the
/// card's move onto the ticket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TicketStage {
    Planning,
    Executing,
    Reviewing,
    /// The card's pull request opened.
    PushedMr,
    /// An investigation's report is on the card.
    Triaged,
    /// The card's pull request merged, or a card without one was cleaned up.
    Done,
}

impl TicketStage {
    /// The status (or label) `mapping` names for this stage, when it names
    /// one. Planning always has one: it falls back to the default.
    pub fn mapped(self, mapping: &StatusMapping) -> Option<String> {
        let configured = match self {
            Self::Planning => return Some(mapping.planning_status().to_string()),
            Self::Executing => &mapping.executing,
            Self::Reviewing => &mapping.reviewing,
            Self::PushedMr => &mapping.pushed_mr,
            Self::Triaged => &mapping.triaged,
            Self::Done => &mapping.done,
        };
        configured
            .as_deref()
            .map(str::trim)
            .filter(|status| !status.is_empty())
            .map(str::to_string)
    }
}

/// Everything the orchestrator, watcher, routes and agent tools ask of a
/// ticket source. Object-safe: callers hold an `Arc<dyn Tracker>`; the
/// providers are dispatched through [`TrackerProvider`].
#[async_trait]
#[enum_dispatch(TrackerProvider)]
pub trait Tracker: Send + Sync {
    fn kind(&self) -> TrackerKind;

    /// Whether the tracker has what it needs to make calls. Pollers check
    /// this before spending a request finding out.
    fn is_configured(&self) -> bool;

    /// The ticket's web address.
    fn issue_url(&self, key: &str) -> String;

    /// Whether `key` has this tracker's key shape. Caller-supplied keys are
    /// checked before they reach a query or a URL.
    fn is_valid_key(&self, key: &str) -> bool;

    /// One cheap read proving the settings work, describing what it found.
    async fn test(&self) -> TrackerResult<String>;

    /// Runs a query in the tracker's own query language (JQL for Jira; see
    /// each provider for the others).
    async fn search(&self, query: &str) -> TrackerResult<Vec<FetchedIssue>>;

    /// Answers one of the watcher's or orchestrator's questions. A provider
    /// that has no such notion (a reopened incident) answers with nothing.
    async fn find(&self, query: Query<'_>) -> TrackerResult<Vec<FetchedIssue>>;

    /// The tickets with these keys; keys that do not exist are left out.
    async fn fetch(&self, keys: &[String]) -> TrackerResult<Vec<FetchedIssue>>;

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

    /// Mirrors a stage the orchestrator has just started onto the ticket,
    /// following `status_mapping`. A stage the mapping leaves unset is a
    /// no-op.
    async fn enter_stage(&self, key: &str, stage: TicketStage) -> TrackerResult<()>;

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

/// The built provider: one variant per kind.
#[enum_dispatch]
pub enum TrackerProvider {
    Jira(JiraTracker),
    GithubIssues(GithubIssuesTracker),
    Linear(LinearTracker),
    PagerDuty(PagerDutyTracker),
}

/// One provider's descriptor.
pub fn descriptor(kind: TrackerKind) -> Descriptor {
    match kind {
        TrackerKind::Jira => JiraTracker::descriptor(),
        TrackerKind::GithubIssues => GithubIssuesTracker::descriptor(),
        TrackerKind::Linear => LinearTracker::descriptor(),
        TrackerKind::PagerDuty => PagerDutyTracker::descriptor(),
    }
}

/// Every provider's descriptor, in `TrackerKind::ALL` order.
pub fn descriptors() -> Vec<Descriptor> {
    TrackerKind::ALL.into_iter().map(descriptor).collect()
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

/// The `origin` an imported card carries: an incident tool's tickets are
/// incidents, recorded as such so the card says where it came from. `None`
/// for a ticket tracker.
pub fn origin_for(kind: TrackerKind, url: &str) -> Option<CardOrigin> {
    (kind.mode() == TicketingMode::Incidents).then(|| CardOrigin {
        kind: OriginKind::Incident,
        reference: url.to_string(),
        reporter: None,
    })
}

/// `Key-123` shapes: a letter-led alphanumeric project, a dash, digits.
pub(crate) fn is_project_key(key: &str) -> bool {
    let Some((project, number)) = key.rsplit_once('-') else {
        return false;
    };
    project
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic())
        && project
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !number.is_empty()
        && number.chars().all(|c| c.is_ascii_digit())
}

/// `[~accountid:octocat]` → `@octocat`: the shared mention marker as a
/// plain-text service writes a mention. A malformed marker stays literal.
pub(crate) fn at_mentions(text: &str) -> String {
    const OPEN: &str = "[~accountid:";
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(OPEN) {
        let after = &rest[start + OPEN.len()..];
        let Some(end) = after.find(']') else {
            break;
        };
        let id = &after[..end];
        if id.is_empty() || id.contains(char::is_whitespace) {
            out.push_str(&rest[..start + OPEN.len()]);
            rest = after;
            continue;
        }
        out.push_str(&rest[..start]);
        out.push('@');
        out.push_str(id);
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

/// Text cut to at most `max` characters, marking the cut. Services reject an
/// over-long comment outright, which would lose the update entirely.
pub(crate) fn clip_text(text: &str, max: usize, service: &str) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let notice =
        format!("\n\n[Truncated for {service}: the full text is in the orchestrator's records.]");
    let keep = max.saturating_sub(notice.chars().count());
    let mut out: String = text.chars().take(keep).collect();
    out.push_str(&notice);
    out
}

/// Builds the provider a (secrets-resolved) config describes. Always a
/// provider: one without what it needs answers `is_configured() == false`,
/// which is how callers learn there is no tracker.
pub fn build_provider(config: &TrackerConfig) -> TrackerProvider {
    match config.kind {
        TrackerKind::Jira => JiraTracker::new(config.clone()).into(),
        TrackerKind::GithubIssues => GithubIssuesTracker::new(config.clone()).into(),
        TrackerKind::Linear => LinearTracker::new(config.clone()).into(),
        TrackerKind::PagerDuty => PagerDutyTracker::new(config.clone()).into(),
    }
}

/// Builds the configured provider, `None` when nothing usable is configured.
/// A kind that does not serve the configured mode is an error rather than a
/// silent `None`: the config names a source, and it is the wrong kind.
pub fn build(config: &TrackerConfig) -> TrackerResult<Option<TrackerProvider>> {
    if let Some(why) = config.mode_mismatch() {
        return Err(TrackerError::NotConfigured(why));
    }
    let provider = build_provider(config);
    Ok(provider.is_configured().then_some(provider))
}

/// The config slot a secret field is stored in, by its descriptor key.
fn secret_slot<'a>(config: &'a mut TrackerConfig, key: &str) -> Option<&'a mut Option<String>> {
    match key {
        "api_token" => Some(&mut config.api_token),
        "pagerduty_api_token" => Some(&mut config.pagerduty_api_token),
        "linear_api_key" => Some(&mut config.linear_api_key),
        _ => None,
    }
}

/// The config with the configured kind's secret fields read through the
/// secrets store (see [`resolve_secret`]).
pub fn resolve(
    config: &TrackerConfig,
    store: &Secrets,
    precedence: SecretPrecedence,
) -> TrackerConfig {
    let mut resolved = config.clone();
    for field in descriptor(config.kind).fields {
        if field.kind != FieldKind::Secret {
            continue;
        }
        let name = secret_name(config.kind.as_str(), &field.key);
        let Some(slot) = secret_slot(&mut resolved, &field.key) else {
            continue;
        };
        let value = resolve_secret(slot.as_deref(), &name, store, precedence);
        if let Some(value) = &value {
            secrets::register_redaction(value);
        }
        *slot = value;
    }
    resolved
}

/// Runs a provider's `test()`: against `fields` when given (Settings' unsaved
/// values, laid over the saved section, which win over the secrets store so
/// what was typed is what is tested), else against the saved section. The
/// tested kind's own mode is assumed, so a provider can be tried before the
/// mode is switched.
pub async fn test_provider(
    config: &Config,
    kind: &str,
    fields: Option<BTreeMap<String, serde_json::Value>>,
) -> Result<String, String> {
    let kind = TrackerKind::parse(kind).ok_or_else(|| format!("unknown ticketing kind: {kind}"))?;
    let (mut settings, precedence) = match fields {
        Some(fields) => {
            let mut value = serde_json::to_value(&config.ticketing).map_err(|e| e.to_string())?;
            if let Some(object) = value.as_object_mut() {
                object.extend(fields);
            }
            let settings: TrackerConfig = serde_json::from_value(value)
                .map_err(|e| format!("the settings do not read as a ticketing section: {e}"))?;
            (settings, SecretPrecedence::ConfigFirst)
        }
        None => (config.ticketing.clone(), SecretPrecedence::StoreFirst),
    };
    settings.kind = kind;
    settings.mode = kind.mode();
    let resolved = resolve(&settings, &secrets::load(), precedence);
    build_provider(&resolved)
        .test()
        .await
        .map_err(|e| secrets::redact(&e.to_string()))
}

/// The tracker last built, and the resolved settings it was built from.
type Built = Option<(TrackerConfig, Arc<dyn Tracker>)>;

/// Hands out the tracker the live config describes.
///
/// Built once and reused; rebuilt when the `ticketing` section of the config
/// (or a secret it reads) changes, the same way the orchestrator re-reads its
/// settings every tick. Anything a tracker caches for its lifetime (the
/// agent's own identity) therefore follows a change of credentials without a
/// restart.
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
        let wanted = {
            let config = self.config.read().await;
            resolve(
                &config.ticketing,
                &secrets::load(),
                SecretPrecedence::StoreFirst,
            )
        };
        let mut built = self
            .built
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some((config, tracker)) = built.as_ref()
            && *config == wanted
        {
            return tracker.clone();
        }
        if let Some(why) = wanted.mode_mismatch() {
            tracing::warn!("ticketing: {why}; no tracker is configured until they agree");
        }
        let tracker: Arc<dyn Tracker> = Arc::new(build_provider(&wanted));
        *built = Some((wanted, tracker.clone()));
        tracker
    }

    /// The ticketing section of the current config.
    pub async fn settings(&self) -> TrackerConfig {
        self.config.read().await.ticketing.clone()
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
    async fn the_source_rebuilds_only_when_the_ticketing_config_changes() {
        let config = Arc::new(RwLock::new(Config::default()));
        let source = TrackerSource::new(config.clone());

        let first = source.current().await;
        assert_eq!(first.kind(), TrackerKind::Jira);
        assert!(Arc::ptr_eq(&first, &source.current().await));

        // A change elsewhere in the config keeps the same tracker.
        config.write().await.git_branch_prefix = "elsewhere".to_string();
        assert!(Arc::ptr_eq(&first, &source.current().await));

        config.write().await.ticketing.kind = TrackerKind::Linear;
        let rebuilt = source.current().await;
        assert_eq!(rebuilt.kind(), TrackerKind::Linear);
        assert!(!Arc::ptr_eq(&first, &rebuilt));
    }

    #[test]
    fn build_answers_none_without_settings_and_refuses_a_mode_mismatch() {
        assert!(build(&TrackerConfig::default()).unwrap().is_none());
        let jira = TrackerConfig {
            email: Some("agent@example.com".to_string()),
            api_token: Some("token".to_string()),
            ..Default::default()
        };
        let built = build(&jira).unwrap().expect("credentials configure Jira");
        assert_eq!(built.kind(), TrackerKind::Jira);

        let wrong_mode = TrackerConfig {
            mode: TicketingMode::Incidents,
            ..jira
        };
        assert!(matches!(
            build(&wrong_mode),
            Err(TrackerError::NotConfigured(why)) if why.contains("incidents")
        ));
        // The always-built provider says it is not configured, so nothing
        // polls a Jira project in incidents mode.
        assert!(!build_provider(&wrong_mode).is_configured());
    }

    #[test]
    fn descriptors_cover_every_kind_with_secret_names_and_linear_unavailable() {
        let all = descriptors();
        let kinds: Vec<&str> = all.iter().map(|d| d.kind.as_str()).collect();
        assert_eq!(kinds, ["jira", "github_issues", "linear", "pagerduty"]);
        let by_kind = |kind: &str| all.iter().find(|d| d.kind == kind).unwrap();
        assert_eq!(by_kind("jira").secrets, ["JIRA_API_TOKEN"]);
        assert!(by_kind("github_issues").secrets.is_empty());
        assert_eq!(by_kind("pagerduty").secrets, ["PAGERDUTY_API_TOKEN"]);
        assert_eq!(by_kind("linear").secrets, ["LINEAR_API_KEY"]);
        assert!(!by_kind("linear").available);
        assert!(by_kind("linear").summary.contains("not implemented"));
        for descriptor in &all {
            assert!(!descriptor.summary.is_empty(), "{}", descriptor.kind);
            assert!(!descriptor.fields.is_empty(), "{}", descriptor.kind);
            // Every field binds to a key the ticketing section has.
            let section = serde_json::to_value(TrackerConfig::default()).unwrap();
            for field in &descriptor.fields {
                assert!(
                    section.get(&field.key).is_some(),
                    "{}: `{}` is not a ticketing key",
                    descriptor.kind,
                    field.key
                );
            }
        }
        assert!(by_kind("github_issues").summary.contains("gh auth login"));
    }

    #[test]
    fn secrets_resolve_from_the_store_first_for_the_configured_kind() {
        let store = Secrets::load_from(
            Path::new("/nonexistent/prepcook-test-secrets.env"),
            [(
                "PREPCOOK_SECRET_PAGERDUTY_API_TOKEN".to_string(),
                "from-store".to_string(),
            )],
        );
        let config = TrackerConfig {
            mode: TicketingMode::Incidents,
            kind: TrackerKind::PagerDuty,
            pagerduty_api_token: Some("from-config".to_string()),
            api_token: Some("jira-token".to_string()),
            ..Default::default()
        };
        let resolved = resolve(&config, &store, SecretPrecedence::StoreFirst);
        assert_eq!(resolved.pagerduty_api_token.as_deref(), Some("from-store"));
        // Another kind's secret is left as configured.
        assert_eq!(resolved.api_token.as_deref(), Some("jira-token"));
        let typed = resolve(&config, &store, SecretPrecedence::ConfigFirst);
        assert_eq!(typed.pagerduty_api_token.as_deref(), Some("from-config"));
    }

    #[test]
    fn mapped_stages_fall_back_only_for_planning() {
        let mut mapping = StatusMapping::default();
        assert_eq!(
            TicketStage::Planning.mapped(&mapping).as_deref(),
            Some("In Progress")
        );
        assert_eq!(TicketStage::Triaged.mapped(&mapping), None);
        mapping.triaged = Some(" Triaged ".to_string());
        mapping.done = Some("  ".to_string());
        assert_eq!(
            TicketStage::Triaged.mapped(&mapping).as_deref(),
            Some("Triaged")
        );
        assert_eq!(TicketStage::Done.mapped(&mapping), None);
    }

    #[test]
    fn incident_kinds_give_imported_cards_an_incident_origin() {
        let origin = origin_for(
            TrackerKind::PagerDuty,
            "https://acme.pagerduty.com/incidents/Q1",
        )
        .unwrap();
        assert_eq!(origin.kind, OriginKind::Incident);
        assert_eq!(origin.reference, "https://acme.pagerduty.com/incidents/Q1");
        assert!(origin_for(TrackerKind::Jira, "https://x/browse/BUG-1").is_none());
    }

    #[test]
    fn project_keys_and_clipping() {
        assert!(is_project_key("BUG-1234"));
        assert!(is_project_key("MY_APP-7"));
        assert!(!is_project_key("BUG-"));
        assert!(!is_project_key("-12"));
        assert!(!is_project_key("BUG-12; DROP"));
        assert!(!is_project_key("12-34"));
        assert_eq!(clip_text("short", 10, "X"), "short");
        let clipped = clip_text(&"a".repeat(500), 200, "GitHub");
        assert!(clipped.chars().count() <= 200);
        assert!(clipped.contains("Truncated for GitHub"));
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
