use std::collections::BTreeMap;

use anyhow::Error;
use api_types::issue_ext::WorkClass;
use executors::{executors::BaseCodingAgent, profile::ExecutorProfileId};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;
pub use v9::{
    AutonomyMode, EditorConfig, EditorType, EffectiveMrSettings, GitHubConfig, OrchestratorMode,
    ProjectMrOverride, ProjectSlackOverride, RetentionOverride, SendMessageShortcut, ShowcaseState,
    SoundFile, ThemeMode, UiLanguage, WatchTarget, WorktreePoolConfig,
};
use v9::{
    default_autonomy_review_iterations, default_investigation_threshold, default_jira_type_mapping,
    default_max_launches_per_tick, default_poll_interval_secs, default_workspace_retention_days,
    integrations_default_true,
};

pub use crate::services::config::playbooks::{
    COMMENT_REPLY_PLAYBOOK_KEY, INVESTIGATION_PLAYBOOK_KEY, PATTERN_PLAYBOOK_KEY,
    RCA_ANALYSIS_PLAYBOOK_KEY, REVIEWING_PLAYBOOK_KEY, default_playbooks,
};
use crate::services::{
    config::versions::{superseded_playbooks, v9},
    notifications::NotifierKind,
};

/// Shipped playbook texts that have since been replaced, byte-for-byte, as
/// `(key, old text)`. Saved configs persist the whole playbook map, so a user
/// who never customized a template would otherwise keep an old default for
/// ever; `upgrade_stale_playbooks` swaps any exact match for the current
/// default and leaves customized texts alone. When changing a default in
/// `default_playbooks`, append the previous text here.
pub(super) const SUPERSEDED_PLAYBOOKS: &[(&str, &str)] = &[
    ("bug:planning", superseded_playbooks::BUG_PLANNING_1),
    ("epic:executing", superseded_playbooks::EPIC_EXECUTING_1),
    ("pattern:planning", superseded_playbooks::PATTERN_PLANNING_1),
    ("rca_analysis", superseded_playbooks::RCA_ANALYSIS_1),
];

/// The playbook key names a config was saved with, in `playbook_keys`. A
/// config without the field predates the renames below.
pub const CURRENT_PLAYBOOK_KEYS: u32 = 1;

/// `(old key, new key)`, applied once to a config saved before
/// [`CURRENT_PLAYBOOK_KEYS`]. `investigation:planning` used to be the pattern
/// review's text; the key now belongs to investigation cards, so a saved one is
/// moved rather than reinterpreted. `triage` was the comment-reply agent.
const RENAMED_PLAYBOOK_KEYS: &[(&str, &str)] = &[
    ("investigation:planning", PATTERN_PLAYBOOK_KEY),
    ("triage", COMMENT_REPLY_PLAYBOOK_KEY),
];

/// Renames the playbook keys of one map. A map that already holds the new key
/// keeps it and drops the old one.
fn rename_playbook_keys(playbooks: &mut BTreeMap<String, String>) {
    for (old, new) in RENAMED_PLAYBOOK_KEYS {
        if let Some(text) = playbooks.remove(*old) {
            playbooks.entry((*new).to_string()).or_insert(text);
        }
    }
}

/// Replaces playbook values that exactly match a superseded shipped default
/// with the current default, and fills in any missing keys. Runs on every
/// config load so new stages and template improvements reach existing configs
/// without clobbering user customizations.
pub(super) fn upgrade_stale_playbooks(playbooks: &mut BTreeMap<String, String>) {
    let defaults = default_playbooks();
    for (key, old_text) in SUPERSEDED_PLAYBOOKS {
        if playbooks.get(*key).map(String::as_str) == Some(*old_text)
            && let Some(current) = defaults.get(*key)
        {
            playbooks.insert((*key).to_string(), current.clone());
        }
    }
    for (key, current) in defaults {
        playbooks.entry(key).or_insert(current);
    }
}

fn default_git_branch_prefix() -> String {
    "prepcook".to_string()
}

fn default_pr_auto_description_enabled() -> bool {
    true
}

fn default_commit_reminder_enabled() -> bool {
    true
}

fn default_port_range() -> String {
    "4200-4299".to_string()
}

fn default_max_parallel() -> u32 {
    2
}

/// Desktop notifications, and the services events are sent to.
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct NotificationConfig {
    pub sound_enabled: bool,
    pub push_enabled: bool,
    pub sound_file: SoundFile,
    /// Where orchestrator events go (escalations, the digest, seat alerts):
    /// any number of Slack, webhook, Discord and Teams targets, each sent
    /// every event unless its `events` names some. See
    /// `docs/notifications.md`.
    #[serde(default)]
    pub targets: Vec<NotificationTarget>,
}

impl Default for NotificationConfig {
    fn default() -> Self {
        let old = v9::NotificationConfig::default();
        Self {
            sound_enabled: old.sound_enabled,
            push_enabled: old.push_enabled,
            sound_file: old.sound_file,
            targets: Vec::new(),
        }
    }
}

impl From<v9::NotificationConfig> for NotificationConfig {
    fn from(old: v9::NotificationConfig) -> Self {
        Self {
            sound_enabled: old.sound_enabled,
            push_enabled: old.push_enabled,
            sound_file: old.sound_file,
            targets: Vec::new(),
        }
    }
}

/// One place notifications are sent to.
#[derive(Clone, Debug, Serialize, Deserialize, TS, PartialEq)]
pub struct NotificationTarget {
    pub kind: NotifierKind,
    /// Event kinds this target receives. Unset, or a list holding `*`, is
    /// every event.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub events: Option<Vec<String>>,
    /// The provider's own settings, keyed as its descriptor names them
    /// (`webhook_url`, `bot_token`, …), alongside `kind` in the same object.
    #[serde(flatten)]
    pub fields: BTreeMap<String, serde_json::Value>,
}

/// Where a resolved target came from: secret fields of a board's own target
/// prefer its configured value over the shared secrets-store entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetOrigin {
    Global,
    Board,
}

/// A board's notification routing. Unset keeps the global targets.
#[derive(Clone, Debug, Default, Serialize, Deserialize, TS, PartialEq)]
pub struct ProjectNotificationsOverride {
    /// `Some(false)` silences every target for this board, its own included.
    #[serde(default)]
    pub enabled: Option<bool>,
    /// Kinds of global target this board does not send to; its own targets
    /// of that kind still apply.
    #[serde(default)]
    pub skip_global: Vec<NotifierKind>,
    /// Targets this board sends to on top of the global ones it keeps.
    #[serde(default)]
    pub targets: Vec<NotificationTarget>,
}

impl ProjectNotificationsOverride {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

fn slack_override_is_empty(slack: &ProjectSlackOverride) -> bool {
    slack.webhook_url.is_none()
        && slack.bot_token.is_none()
        && slack.channel.is_none()
        && slack.enabled.is_none()
}

/// How the script runner stands target apps up (see `docs/scripts.md`).
#[derive(Clone, Debug, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct AppRunsConfig {
    /// Ports handed to app runs, `<first>-<last>` inclusive. Each run gets one
    /// as `$PREPCOOK_PORT`.
    #[serde(default = "default_port_range")]
    pub port_range: String,
    /// App runs allowed at once. A stand-up past this is refused as busy and
    /// retried on the orchestrator's next tick; each run is a full app stack,
    /// so this is bounded by the machine's memory.
    #[serde(default = "default_max_parallel")]
    pub max_parallel: u32,
}

impl Default for AppRunsConfig {
    fn default() -> Self {
        Self {
            port_range: default_port_range(),
            max_parallel: default_max_parallel(),
        }
    }
}

/// How a planning agent puts its document in front of a person
/// (`{{gate_instructions}}` in the planning playbooks).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum InAppGate {
    /// The document carries Approve / Request changes buttons and is opened in
    /// a Lavish review session; the reviewer's answer comes back through it.
    #[default]
    Lavish,
    /// No in-document buttons: the decision comes from a card comment, a
    /// tracker comment or the approvals API (see `docs/approvals.md`).
    None,
}

/// Plan approval settings.
#[derive(Clone, Debug, Default, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct ApprovalsConfig {
    #[serde(default)]
    pub in_app_gate: InAppGate,
}

/// Where cards come from: a ticket tracker, or an incident tool whose open
/// incidents become investigation cards (see `docs/incidents.md`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum TicketingMode {
    #[default]
    Tickets,
    Incidents,
}

impl TicketingMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tickets => "tickets",
            Self::Incidents => "incidents",
        }
    }
}

/// Which service tickets (or incidents) come from. Linear is named so a
/// config can say what it wants and get a clear "not yet supported" from
/// every operation rather than a silent fallback to Jira.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum TrackerKind {
    #[default]
    Jira,
    GithubIssues,
    Linear,
    #[serde(rename = "pagerduty")]
    PagerDuty,
}

impl TrackerKind {
    pub const ALL: [TrackerKind; 4] = [
        Self::Jira,
        Self::GithubIssues,
        Self::Linear,
        Self::PagerDuty,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Jira => "Jira",
            Self::GithubIssues => "GitHub Issues",
            Self::Linear => "Linear",
            Self::PagerDuty => "PagerDuty",
        }
    }

    /// The config string (`ticketing.kind`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Jira => "jira",
            Self::GithubIssues => "github_issues",
            Self::Linear => "linear",
            Self::PagerDuty => "pagerduty",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == raw)
    }

    /// The mode this kind serves: PagerDuty is an incident tool, the rest are
    /// ticket trackers.
    pub fn mode(self) -> TicketingMode {
        match self {
            Self::PagerDuty => TicketingMode::Incidents,
            Self::Jira | Self::GithubIssues | Self::Linear => TicketingMode::Tickets,
        }
    }
}

impl std::fmt::Display for TrackerKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// What Prep Cook may do to an incident on its own, beyond the notes it
/// posts (see `docs/incidents.md`).
#[derive(Clone, Debug, Default, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct IncidentsConfig {
    /// Resolve the incident when its investigation card reaches Triaged.
    /// Off, the incident stays with the responders and only the triage
    /// report is added to it.
    #[serde(default)]
    pub resolve_on_triage: bool,
}

/// Tracker status names the board's stages correspond to. A stage with a
/// status set moves the ticket there when the orchestrator starts that
/// stage's work; unset leaves the ticket where it is. `planning` falls back
/// to [`StatusMapping::DEFAULT_PLANNING`], so starting work always shows on
/// the ticket.
#[derive(Clone, Debug, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct StatusMapping {
    #[serde(default)]
    pub planning: Option<String>,
    #[serde(default)]
    pub executing: Option<String>,
    #[serde(default)]
    pub reviewing: Option<String>,
    #[serde(default)]
    pub pushed_mr: Option<String>,
    /// Where an investigation's ticket goes when its card reaches Triaged.
    #[serde(default)]
    pub triaged: Option<String>,
    /// Where the ticket goes when the card reaches Clean-up.
    #[serde(default)]
    pub done: Option<String>,
    /// Statuses that mean the work shipped. Matched by name, never by status
    /// category: Jira projects commonly file Merged and Deployed to Production
    /// under the same category as In Progress, so the category cannot answer
    /// whether a ticket shipped. Drives reopen detection and the clean-up
    /// sweeps.
    #[serde(default = "default_shipped_statuses")]
    pub shipped: Vec<String>,
}

fn default_shipped_statuses() -> Vec<String> {
    vec![
        "Merged".to_string(),
        "Deployed to Production".to_string(),
        "Done".to_string(),
    ]
}

impl Default for StatusMapping {
    fn default() -> Self {
        Self {
            planning: None,
            executing: None,
            reviewing: None,
            pushed_mr: None,
            triaged: None,
            done: None,
            shipped: default_shipped_statuses(),
        }
    }
}

impl StatusMapping {
    pub const DEFAULT_PLANNING: &'static str = "In Progress";

    /// The status work-in-progress lands in.
    pub fn planning_status(&self) -> &str {
        non_blank(&self.planning).unwrap_or(Self::DEFAULT_PLANNING)
    }

    /// Whether a tracker status name is one of [`StatusMapping::shipped`].
    /// Case- and whitespace-insensitive, because the list is typed by hand.
    pub fn is_shipped(&self, status_name: &str) -> bool {
        let name = status_name.trim();
        self.shipped
            .iter()
            .any(|shipped| shipped.trim().eq_ignore_ascii_case(name))
    }
}

fn non_blank(value: &Option<String>) -> Option<&str> {
    value.as_deref().map(str::trim).filter(|v| !v.is_empty())
}

fn default_tracker_poll_secs() -> u64 {
    45
}

fn default_blocker_priorities() -> Vec<String> {
    vec!["P0".to_string(), "P1".to_string()]
}

/// The `ticketing` section: where tickets (or incidents) come from, how the
/// watcher picks them up, and who may decide on a plan from the tracker.
///
/// One section serves every kind. Each kind reads the fields its descriptor
/// names (`services::tracker`), and those that are not shared carry the
/// kind in their name (`pagerduty_api_token`), so switching kinds never
/// hands one service another's credentials.
///
/// The string fields are nullable on the wire (the settings UI persists
/// cleared inputs as `null`); defaults are applied at use time through the
/// accessor methods.
#[derive(Clone, Debug, Serialize, Deserialize, TS, PartialEq)]
pub struct TrackerConfig {
    /// `tickets` (Jira, GitHub Issues, Linear) or `incidents` (PagerDuty).
    #[serde(default)]
    pub mode: TicketingMode,
    #[serde(default)]
    pub kind: TrackerKind,
    /// Jira Cloud host, e.g. "example.atlassian.net".
    #[serde(default)]
    pub site: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub api_token: Option<String>,
    #[serde(default)]
    pub project_key: Option<String>,
    /// GitHub repository, `owner/name` (or `host/owner/name` on GitHub
    /// Enterprise).
    #[serde(default)]
    pub repo: Option<String>,
    /// PagerDuty REST API key.
    #[serde(default)]
    pub pagerduty_api_token: Option<String>,
    /// PagerDuty service ids whose incidents are picked up, comma-separated,
    /// when the pickup filter names none.
    #[serde(default)]
    pub service_ids: Option<String>,
    /// PagerDuty user Prep Cook acts as: notes are written as this user, and
    /// it is the identity tracker comments are recognised by.
    #[serde(default)]
    pub user_id: Option<String>,
    /// Linear API key.
    #[serde(default)]
    pub linear_api_key: Option<String>,
    /// Linear team key.
    #[serde(default)]
    pub team_key: Option<String>,
    /// What Prep Cook may do to an incident in `incidents` mode.
    #[serde(default)]
    pub incidents: IncidentsConfig,
    /// Query the import dialog runs when the caller does not supply one.
    /// `{project_key}` is substituted at use time.
    #[serde(default)]
    pub import_filter: Option<String>,
    /// Query fragment selecting new tickets the watcher picks up on its own.
    /// Combined at use time with a created-recently window; `{project_key}`
    /// is substituted.
    #[serde(default)]
    pub pickup_filter: Option<String>,
    /// Query for tickets assigned to the orchestrator's tracker account:
    /// imported and started regardless of priority, because assignment is a
    /// stronger intent signal than any filter. Polled on `updated` (an
    /// assignment bumps it, never `created`). `None` turns it off.
    #[serde(default)]
    pub assigned_filter: Option<String>,
    #[serde(default = "default_tracker_poll_secs")]
    pub poll_secs: u64,
    /// Whether the watcher polls for the board in `project_id`.
    #[serde(default)]
    pub enabled: bool,
    /// Board (remote project UUID) the watcher imports into.
    #[serde(default)]
    pub project_id: Option<Uuid>,
    /// Maps tracker issue-type names to the internal work classification.
    /// Unknown types fall back to `WorkClass::Story` at use time.
    #[serde(default = "default_jira_type_mapping")]
    pub type_mapping: BTreeMap<String, WorkClass>,
    /// Tracker priorities whose merge requests are flagged as blockers.
    #[serde(default = "default_blocker_priorities")]
    pub blocker_priorities: Vec<String>,
    #[serde(default)]
    pub status_mapping: StatusMapping,
    /// Tracker user id → display name for the people who may approve,
    /// request changes on or re-plan a plan from a tracker comment.
    ///
    /// Empty means nobody, and that is the safe default rather than an
    /// oversight: treating "unconfigured" as "anyone" would hand the decision
    /// to whoever happens to be on the ticket, and on a bug that includes the
    /// person who reported it.
    ///
    /// Chosen by hand from the project's assignable users because Jira cannot
    /// answer "is this person an engineer": project roles need an admin the
    /// app's account is not, and groups record licensing, not role.
    ///
    /// The id is what authority is checked against. The display name is a
    /// convenience for the settings screen and the audit comment — it is not
    /// unique and its owner can change it.
    #[serde(default)]
    pub approvers: BTreeMap<String, String>,
    /// Tracker account id the agent's own comments are recognised by. Unset
    /// uses the account the API token belongs to.
    #[serde(default)]
    pub bot_identity: Option<String>,
}

impl TrackerConfig {
    pub const DEFAULT_SITE: &'static str = "example.atlassian.net";
    pub const DEFAULT_PROJECT_KEY: &'static str = "BUG";
    /// `statusCategory = "To Do"` keeps in-progress, blocked and finished
    /// work out of the import; the route also filters by status category as a
    /// backstop for custom queries.
    pub const DEFAULT_IMPORT_FILTER: &'static str = "project = {project_key} AND sprint in openSprints() AND assignee = currentUser() AND statusCategory = \"To Do\" ORDER BY priority DESC";
    pub const DEFAULT_PICKUP_FILTER: &'static str =
        "project = {project_key} AND priority in (P0, P1) AND issuetype = Bug";
    /// GitHub Issues: open issues labelled `bug`.
    pub const DEFAULT_GITHUB_PICKUP_FILTER: &'static str = "state:open label:bug";
    /// GitHub Issues: open issues assigned to the `gh` account.
    pub const DEFAULT_GITHUB_IMPORT_FILTER: &'static str = "state:open assignee:@me";
    /// PagerDuty: every open incident on the configured services.
    pub const DEFAULT_INCIDENT_FILTER: &'static str = "status:triggered,acknowledged";

    /// The pickup filter used when neither the board nor `pickup_filter`
    /// names one, in the configured kind's own syntax.
    pub fn default_pickup_filter(&self) -> &'static str {
        match self.kind {
            TrackerKind::Jira | TrackerKind::Linear => Self::DEFAULT_PICKUP_FILTER,
            TrackerKind::GithubIssues => Self::DEFAULT_GITHUB_PICKUP_FILTER,
            TrackerKind::PagerDuty => Self::DEFAULT_INCIDENT_FILTER,
        }
    }

    /// The import filter used when `import_filter` is unset.
    pub fn default_import_filter(&self) -> &'static str {
        match self.kind {
            TrackerKind::Jira | TrackerKind::Linear => Self::DEFAULT_IMPORT_FILTER,
            TrackerKind::GithubIssues => Self::DEFAULT_GITHUB_IMPORT_FILTER,
            TrackerKind::PagerDuty => Self::DEFAULT_INCIDENT_FILTER,
        }
    }

    pub fn repo(&self) -> Option<&str> {
        non_blank(&self.repo)
    }

    pub fn pagerduty_api_token(&self) -> Option<&str> {
        non_blank(&self.pagerduty_api_token)
    }

    pub fn user_id(&self) -> Option<&str> {
        non_blank(&self.user_id)
    }

    /// The configured PagerDuty service ids, in order, blanks dropped.
    pub fn service_ids(&self) -> Vec<String> {
        non_blank(&self.service_ids)
            .map(|ids| {
                ids.split(',')
                    .map(str::trim)
                    .filter(|id| !id.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Why the configured kind cannot serve the configured mode, if it
    /// cannot: a ticket tracker in `incidents` mode, or the reverse.
    pub fn mode_mismatch(&self) -> Option<String> {
        (self.kind.mode() != self.mode).then(|| {
            format!(
                "{} serves ticketing.mode = \"{}\", but the mode is \"{}\"",
                self.kind.label(),
                self.kind.mode().as_str(),
                self.mode.as_str()
            )
        })
    }

    pub fn site(&self) -> &str {
        non_blank(&self.site).unwrap_or(Self::DEFAULT_SITE)
    }

    pub fn project_key(&self) -> &str {
        non_blank(&self.project_key).unwrap_or(Self::DEFAULT_PROJECT_KEY)
    }

    pub fn email(&self) -> Option<&str> {
        non_blank(&self.email)
    }

    pub fn api_token(&self) -> Option<&str> {
        non_blank(&self.api_token)
    }

    pub fn bot_identity(&self) -> Option<&str> {
        non_blank(&self.bot_identity)
    }

    /// Whether the credentials the tracker needs are present.
    pub fn has_credentials(&self) -> bool {
        self.email().is_some() && self.api_token().is_some()
    }

    /// The import query with `{project_key}` substituted.
    pub fn resolved_import_filter(&self) -> String {
        non_blank(&self.import_filter)
            .unwrap_or(self.default_import_filter())
            .replace("{project_key}", self.project_key())
    }

    /// Whether a tracker priority name is one of [`Self::blocker_priorities`].
    pub fn is_blocker_priority(&self, priority: &str) -> bool {
        let priority = priority.trim();
        !priority.is_empty()
            && self
                .blocker_priorities
                .iter()
                .any(|blocker| blocker.trim().eq_ignore_ascii_case(priority))
    }

    /// The work class a tracker issue type maps to (exact name first, then
    /// case-insensitively). Unmapped types are stories in `tickets` mode and
    /// investigations in `incidents` mode: an incident is reproduced and
    /// explained, and fixing it is a person's call.
    pub fn work_class_for(&self, issue_type: &str) -> WorkClass {
        self.type_mapping
            .get(issue_type)
            .copied()
            .or_else(|| {
                self.type_mapping
                    .iter()
                    .find(|(name, _)| name.eq_ignore_ascii_case(issue_type))
                    .map(|(_, work_class)| *work_class)
            })
            .unwrap_or(match self.mode {
                TicketingMode::Tickets => WorkClass::Story,
                TicketingMode::Incidents => WorkClass::Investigation,
            })
    }

    fn from_v9(jira: v9::JiraConfig, watcher: v9::JiraWatcherConfig) -> Self {
        Self {
            mode: TicketingMode::Tickets,
            kind: TrackerKind::Jira,
            site: jira.site,
            email: jira.email,
            api_token: jira.api_token,
            project_key: jira.project_key,
            repo: None,
            pagerduty_api_token: None,
            service_ids: None,
            user_id: None,
            linear_api_key: None,
            team_key: None,
            incidents: IncidentsConfig::default(),
            import_filter: jira.default_jql,
            pickup_filter: watcher.jql_filter,
            assigned_filter: watcher.assigned_jql,
            poll_secs: watcher.poll_secs,
            enabled: watcher.enabled,
            project_id: watcher.project_id,
            type_mapping: jira.type_mapping,
            blocker_priorities: default_blocker_priorities(),
            status_mapping: StatusMapping::default(),
            approvers: jira.rca_approvers,
            bot_identity: None,
        }
    }
}

impl Default for TrackerConfig {
    fn default() -> Self {
        Self {
            mode: TicketingMode::default(),
            kind: TrackerKind::default(),
            site: None,
            email: None,
            api_token: None,
            project_key: None,
            repo: None,
            pagerduty_api_token: None,
            service_ids: None,
            user_id: None,
            linear_api_key: None,
            team_key: None,
            incidents: IncidentsConfig::default(),
            import_filter: None,
            pickup_filter: None,
            assigned_filter: None,
            poll_secs: default_tracker_poll_secs(),
            enabled: false,
            project_id: None,
            type_mapping: default_jira_type_mapping(),
            blocker_priorities: default_blocker_priorities(),
            status_mapping: StatusMapping::default(),
            approvers: BTreeMap::new(),
            bot_identity: None,
        }
    }
}

/// Board-specific watcher settings. Site, credentials and issue-type mapping
/// stay global — one tracker account serves every board — so only the
/// selection of issues differs here.
#[derive(Clone, Debug, Default, Serialize, Deserialize, TS)]
pub struct ProjectTrackerOverride {
    /// Whether the watcher polls for this board. `None` inherits the
    /// single-target behaviour (`ticketing.enabled` applied to
    /// `ticketing.project_id`).
    #[serde(default)]
    pub watcher_enabled: Option<bool>,
    /// Query fragment selecting this board's issues, with `{project_key}`
    /// substituted at use time. `None` inherits `ticketing.pickup_filter`.
    /// Boards sharing a tracker project distinguish themselves here (by
    /// component, label, or module).
    #[serde(default)]
    pub jql_filter: Option<String>,
    /// Tracker project key when this board's tickets live in a different
    /// project; `None` uses the global key.
    #[serde(default)]
    pub project_key: Option<String>,
    /// Assigned-pickup query for this board. Unset or blank inherits
    /// `ticketing.assigned_filter` — the Settings inputs cannot tell a cleared
    /// field from an untouched one, so blank must not carry meaning.
    #[serde(default)]
    pub assigned_jql: Option<String>,
}

impl From<v9::ProjectJiraOverride> for ProjectTrackerOverride {
    fn from(old: v9::ProjectJiraOverride) -> Self {
        Self {
            watcher_enabled: old.watcher_enabled,
            jql_filter: old.jql_filter,
            project_key: old.project_key,
            assigned_jql: old.assigned_jql,
        }
    }
}

/// The optional Sentry connector for the coding agent (its stdio MCP server),
/// and the merge-request reviewer and
/// bot settings that apply on every git host. Tokens set here override the
/// same-named runner environment variables; leave them `None` to rely on the
/// environment.
#[derive(Clone, Debug, Default, Serialize, Deserialize, TS)]
pub struct IntegrationsConfig {
    /// git author email -> git-host login, for blame-based reviewer
    /// assignment. Emails found in blame but absent here are named in a
    /// comment instead of assigned (neither GitHub nor GitLab resolves a user
    /// by email for a token without admin scope).
    #[serde(default)]
    pub reviewer_map: BTreeMap<String, String>,
    /// Extra author emails to exclude from reviewer picks, on top of the
    /// built-in exclusions (the agent's own bot email pattern).
    #[serde(default)]
    pub reviewer_exclude_emails: Vec<String>,
    /// Assign blame-picked reviewers on orchestrator-created merge requests.
    #[serde(default = "integrations_default_true")]
    pub auto_assign_reviewers: bool,
    /// Logins whose merge-request comments the Push MR watcher ignores, on
    /// top of the built-in bot patterns (GitLab project tokens comment as
    /// `project_<id>_bot_<hash>`, GitHub apps as `<app>[bot]`).
    #[serde(default)]
    pub bot_logins: Vec<String>,
    /// Sentry user auth token for the `@sentry/mcp-server` stdio server.
    /// Overrides `SENTRY_ACCESS_TOKEN` from the runner env when set.
    #[serde(default)]
    pub sentry_access_token: Option<String>,
    /// Hostname of a self-hosted Sentry (e.g. `sentry.example.com`). `None`
    /// targets Sentry SaaS (sentry.io).
    #[serde(default)]
    pub sentry_host: Option<String>,
}

impl From<v9::IntegrationsConfig> for IntegrationsConfig {
    fn from(old: v9::IntegrationsConfig) -> Self {
        Self {
            reviewer_map: old.gitlab_reviewer_map,
            reviewer_exclude_emails: old.gitlab_reviewer_exclude_emails,
            auto_assign_reviewers: old.gitlab_auto_assign_reviewers,
            bot_logins: old.gitlab_bot_logins,
            sentry_access_token: old.sentry_access_token,
            sentry_host: old.sentry_host,
        }
    }
}

/// The CI job whose success means a merged change is serving on staging, and
/// the branch it runs from. GitLab: a job name in the project's pipeline.
/// GitHub: an Actions workflow (its name or file name, as `gh run list
/// --workflow` accepts).
#[derive(Clone, Debug, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct DeployWatch {
    pub job_name: String,
    pub git_ref: String,
}

/// Per-project defaults the orchestrator uses when auto-launching playbook
/// workspaces, plus that board's overrides for otherwise-global settings.
///
/// One runner drives several boards (the app, extensions, …), each with its
/// own repo, Jira filter, notification targets and reviewers. Everything
/// board-specific therefore lives here as an *override*: `None` and empty
/// collections mean "inherit the global value", so a single-board setup needs
/// nothing beyond `repo_ids`. Read these through
/// [`Config::project_settings`] rather than field-by-field, so the fallback
/// rules stay in one place.
#[derive(Clone, Debug, Default, Serialize, Deserialize, TS)]
pub struct ProjectAutomationDefaults {
    /// Local repos checked out for auto-launched workspaces.
    #[serde(default)]
    pub repo_ids: Vec<Uuid>,
    /// Overrides the global executor profile for this project's playbooks.
    #[serde(default)]
    pub executor_profile: Option<ExecutorProfileId>,
    /// Short board name (e.g. "extensions"), used for the RCA `board:` tag.
    /// Unset leaves it unlabelled.
    #[serde(default)]
    pub label: Option<String>,
    /// Also read from the `tracker` and `jira` keys older configs wrote, and
    /// saved as `ticketing`.
    #[serde(default, alias = "tracker", alias = "jira")]
    pub ticketing: ProjectTrackerOverride,
    #[serde(default)]
    pub notifications: ProjectNotificationsOverride,
    /// Board Slack routing as older configs wrote it; read once on load and
    /// converted into `notifications` (see [`Config::adopt_legacy_slack`]).
    #[serde(default, skip_serializing_if = "slack_override_is_empty")]
    #[ts(skip)]
    pub slack: ProjectSlackOverride,
    #[serde(default)]
    pub merge_requests: ProjectMrOverride,
    /// Playbook texts used for this board only, keyed like
    /// [`Config::playbooks`]. Absent keys inherit the global text, so a board
    /// records only the stages whose process actually differs.
    #[serde(default)]
    pub playbooks: BTreeMap<String, String>,
    /// Board autonomy; `None` inherits `orchestrator.autonomy`. Lets a new
    /// board run supervised while an established one runs unattended.
    #[serde(default)]
    pub autonomy: Option<AutonomyMode>,
    #[serde(default)]
    pub autonomy_review_iterations: Option<u32>,
    /// Deployed → Clean-up sweep for this board.
    #[serde(default)]
    pub deployed_retention: RetentionOverride,
    /// Clean-up deletion sweep for this board (inherits
    /// `orchestrator.cleanup_retention_days`).
    #[serde(default)]
    pub cleanup_retention: RetentionOverride,
    /// RCA-graph pattern-review threshold for this board; smaller boards need
    /// a lower bar than the global default to ever fire one. Also read from
    /// `investigation_threshold`, the name older configs saved it under.
    #[serde(default, alias = "investigation_threshold")]
    pub pattern_threshold: Option<u32>,
    /// The staging-deploy watch: records when each merged change reaches the
    /// environment this job deploys. `None` turns the watch off.
    #[serde(default)]
    pub deploy_watch: Option<DeployWatch>,
}

impl From<v9::ProjectAutomationDefaults> for ProjectAutomationDefaults {
    fn from(old: v9::ProjectAutomationDefaults) -> Self {
        Self {
            repo_ids: old.repo_ids,
            executor_profile: old.executor_profile,
            label: old.label,
            ticketing: old.jira.into(),
            notifications: ProjectNotificationsOverride::default(),
            slack: old.slack,
            merge_requests: old.merge_requests,
            playbooks: old.playbooks,
            autonomy: old.autonomy,
            autonomy_review_iterations: old.autonomy_review_iterations,
            deployed_retention: old.deployed_retention,
            cleanup_retention: old.cleanup_retention,
            pattern_threshold: old.investigation_threshold,
            deploy_watch: None,
        }
    }
}

/// Background playbook orchestrator settings. Disabled (`Off`) by default.
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct OrchestratorConfig {
    #[serde(default)]
    pub mode: OrchestratorMode,
    #[serde(default = "default_poll_interval_secs")]
    pub poll_interval_secs: u64,
    /// Auto-move tickets that have sat in Deployed for this many days to
    /// Clean-up (which triggers cleanup automation). `None` disables the
    /// sweep; it also only runs while the orchestrator mode is Auto.
    #[serde(default)]
    pub deployed_retention_days: Option<u32>,
    /// Delete tickets that have sat in Clean-up — the terminal column —
    /// longer than this many days. Clean-up already archived their
    /// workspaces; the card is the only thing left, and the durable record
    /// (MR, Jira, RCA in the kb) survives deletion. `None` keeps cards for
    /// ever. Uses `updated_at` as the clock, like the Deployed sweep.
    #[serde(default)]
    pub cleanup_retention_days: Option<u32>,
    /// RCA-graph pattern-review threshold: a node with this many linked RCA
    /// documents (and no pattern review filed yet) files a P2 `[Pattern]`
    /// story, imported into To Do. Re-arms after this many NEW edges
    /// accumulate past the fire point. Also read from
    /// `investigation_threshold`, the name older configs saved it under.
    #[serde(
        default = "default_investigation_threshold",
        alias = "investigation_threshold"
    )]
    pub pattern_threshold: u32,
    /// The board `POST /api/cards` files into when the request names none.
    /// Unset: the first board under `project_defaults`.
    #[serde(default)]
    pub default_board: Option<Uuid>,
    /// Delete workspace records whose worktree is already gone and which have
    /// been idle this many days. Nothing else prunes them: cleanup removes the
    /// worktree and marks the row, but the row (with its sessions, execution
    /// processes, logs and turns) would otherwise live for ever and slow every
    /// board query. `None` disables the sweep; pinned workspaces are never
    /// touched.
    #[serde(default = "default_workspace_retention_days")]
    pub workspace_retention_days: Option<u32>,
    /// New agent launches allowed per scan tick. Drains backlogs steadily
    /// instead of the post-pause thundering herd that re-trips seat session
    /// limits; the queue is priority-ordered, so P0s reach the budget first.
    #[serde(default = "default_max_launches_per_tick")]
    pub max_launches_per_tick: u32,
    /// See [`AutonomyMode`]. Default `Off`; enable only on the runner.
    #[serde(default)]
    pub autonomy: AutonomyMode,
    /// REQUEST CHANGES → fix → re-review cycles autonomy runs before
    /// escalating to the notification targets and leaving the ticket for a
    /// human.
    #[serde(default = "default_autonomy_review_iterations")]
    pub autonomy_review_iterations: u32,
    /// Local time of day, `HH:MM`, at which the engineers' digest is sent to
    /// each board's notification targets. `None` — and any value that does not
    /// parse — disables it, so a typo is silence rather than a post at
    /// midnight.
    #[serde(default)]
    pub digest_at_local: Option<String>,
    /// Local time of day, `HH:MM`, at which each in-flight ticket gets its
    /// `EOB update:` comment in Jira. Same parsing rules as the digest.
    #[serde(default)]
    pub eob_at_local: Option<String>,
    /// IANA timezone both daily times are read in (e.g.
    /// `America/Los_Angeles`). A named zone rather than an offset: Pacific
    /// shifts by an hour twice a year, so a stored offset is wrong for eight
    /// months of it.
    #[serde(default)]
    pub daily_timezone: Option<String>,
    /// Keyed by remote project UUID string.
    #[serde(default)]
    pub project_defaults: BTreeMap<String, ProjectAutomationDefaults>,
    /// Probe the Claude subscription seat with a headless `claude -p` every
    /// six hours and notify when its login expires. Off by default: it
    /// is for a shared runner, and on a laptop it would spawn `claude` on its
    /// own. Settings' "Re-check now" probes on demand either way.
    #[serde(default)]
    pub seat_monitor: bool,
}

impl Default for OrchestratorConfig {
    fn default() -> Self {
        Self {
            mode: OrchestratorMode::Off,
            poll_interval_secs: default_poll_interval_secs(),
            deployed_retention_days: None,
            cleanup_retention_days: None,
            pattern_threshold: default_investigation_threshold(),
            default_board: None,
            max_launches_per_tick: default_max_launches_per_tick(),
            workspace_retention_days: default_workspace_retention_days(),
            autonomy: AutonomyMode::Off,
            autonomy_review_iterations: default_autonomy_review_iterations(),
            digest_at_local: None,
            eob_at_local: None,
            daily_timezone: None,
            project_defaults: BTreeMap::new(),
            seat_monitor: false,
        }
    }
}

impl From<v9::OrchestratorConfig> for OrchestratorConfig {
    fn from(old: v9::OrchestratorConfig) -> Self {
        Self {
            mode: old.mode,
            poll_interval_secs: old.poll_interval_secs,
            deployed_retention_days: old.deployed_retention_days,
            cleanup_retention_days: old.cleanup_retention_days,
            pattern_threshold: old.investigation_threshold,
            default_board: None,
            workspace_retention_days: old.workspace_retention_days,
            max_launches_per_tick: old.max_launches_per_tick,
            autonomy: old.autonomy,
            autonomy_review_iterations: old.autonomy_review_iterations,
            digest_at_local: old.digest_at_local,
            eob_at_local: old.eob_at_local,
            daily_timezone: old.daily_timezone,
            project_defaults: old
                .project_defaults
                .into_iter()
                .map(|(project, defaults)| (project, defaults.into()))
                .collect(),
            seat_monitor: false,
        }
    }
}

/// One board's settings with its overrides already applied over the global
/// config — the only shape the orchestrator and watcher should work from, so
/// no code path can accidentally use a global where a board differs.
#[derive(Clone, Debug)]
pub struct EffectiveProjectSettings {
    pub project_id: Uuid,
    /// Board label, when one is configured (drives the RCA `board:` tag).
    pub label: Option<String>,
    pub repo_ids: Vec<Uuid>,
    pub executor_profile: ExecutorProfileId,
    pub jira_project_key: String,
    /// Whether the Jira watcher polls for this board.
    pub watcher_enabled: bool,
    /// This board's JQL fragment, `{project_key}` already substituted.
    pub watcher_jql: String,
    /// Assigned-pickup JQL, `{project_key}` substituted; `None` = off.
    pub assigned_jql: Option<String>,
    pub playbooks: BTreeMap<String, String>,
    pub autonomy: AutonomyMode,
    pub autonomy_review_iterations: u32,
    pub deployed_retention_days: Option<u32>,
    pub cleanup_retention_days: Option<u32>,
    pub pattern_threshold: u32,
    pub merge_requests: EffectiveMrSettings,
    /// The staging-deploy watch, with a blank job name or ref read as off.
    pub deploy_watch: Option<DeployWatch>,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct Config {
    pub config_version: String,
    pub theme: ThemeMode,
    pub executor_profile: ExecutorProfileId,
    pub disclaimer_acknowledged: bool,
    pub onboarding_acknowledged: bool,
    #[serde(default)]
    pub remote_onboarding_acknowledged: bool,
    pub notifications: NotificationConfig,
    pub editor: EditorConfig,
    pub github: GitHubConfig,
    pub analytics_enabled: bool,
    pub workspace_dir: Option<String>,
    pub last_app_version: Option<String>,
    pub show_release_notes: bool,
    #[serde(default)]
    pub language: UiLanguage,
    #[serde(default = "default_git_branch_prefix")]
    pub git_branch_prefix: String,
    #[serde(default)]
    pub showcases: ShowcaseState,
    #[serde(default = "default_pr_auto_description_enabled")]
    pub pr_auto_description_enabled: bool,
    #[serde(default)]
    pub pr_auto_description_prompt: Option<String>,
    #[serde(default = "default_commit_reminder_enabled")]
    pub commit_reminder_enabled: bool,
    #[serde(default)]
    pub commit_reminder_prompt: Option<String>,
    #[serde(default)]
    pub send_message_shortcut: SendMessageShortcut,
    /// Where cards come from; see [`TrackerConfig`]. Also read from the
    /// `tracker` key older configs wrote, and saved as `ticketing`.
    #[serde(default, alias = "tracker")]
    pub ticketing: TrackerConfig,
    /// Slack settings as older configs wrote them. Read once on load and
    /// converted into a Slack entry in `notifications.targets` (see
    /// [`Config::adopt_legacy_slack`]); nothing else reads them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(skip)]
    pub slack_webhook_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(skip)]
    pub slack_bot_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(skip)]
    pub slack_channel: Option<String>,
    /// Stage playbooks keyed by "{work_class}:{stage}"; see default_playbooks.
    #[serde(default = "default_playbooks")]
    pub playbooks: BTreeMap<String, String>,
    /// Which playbook key names `playbooks` (and every board's overrides) are
    /// in; see [`CURRENT_PLAYBOOK_KEYS`]. Absent in configs saved before the
    /// keys were renamed, which is how those are recognised.
    #[serde(default)]
    pub playbook_keys: u32,
    #[serde(default)]
    pub worktree_pool: WorktreePoolConfig,
    #[serde(default)]
    pub orchestrator: OrchestratorConfig,
    /// The Sentry connector and merge-request reviewer settings.
    #[serde(default)]
    pub integrations: IntegrationsConfig,
    /// The script runner's ports and concurrency.
    #[serde(default)]
    pub app_runs: AppRunsConfig,
    /// How plans reach a person for approval.
    #[serde(default)]
    pub approvals: ApprovalsConfig,
}

impl Config {
    fn from_v9_config(old_config: v9::Config) -> Self {
        // A v9 config holds whatever playbooks it saved, and nothing for a
        // stage it never had; every missing key takes the current default.
        let mut playbooks = old_config.playbooks;
        rename_playbook_keys(&mut playbooks);
        upgrade_stale_playbooks(&mut playbooks);
        let mut orchestrator: OrchestratorConfig = old_config.orchestrator.into();
        for board in orchestrator.project_defaults.values_mut() {
            rename_playbook_keys(&mut board.playbooks);
        }
        Self {
            config_version: "v10".to_string(),
            theme: old_config.theme,
            executor_profile: old_config.executor_profile,
            disclaimer_acknowledged: old_config.disclaimer_acknowledged,
            onboarding_acknowledged: old_config.onboarding_acknowledged,
            remote_onboarding_acknowledged: old_config.remote_onboarding_acknowledged,
            notifications: old_config.notifications.into(),
            // EditorConfig is shared across versions; keys it no longer
            // defines (such as `auto_install_extension`) are ignored when read
            // and dropped on the next save.
            editor: old_config.editor,
            github: old_config.github,
            analytics_enabled: old_config.analytics_enabled,
            workspace_dir: old_config.workspace_dir,
            last_app_version: old_config.last_app_version,
            show_release_notes: old_config.show_release_notes,
            language: old_config.language,
            git_branch_prefix: old_config.git_branch_prefix,
            showcases: old_config.showcases,
            pr_auto_description_enabled: old_config.pr_auto_description_enabled,
            pr_auto_description_prompt: old_config.pr_auto_description_prompt,
            commit_reminder_enabled: old_config.commit_reminder_enabled,
            commit_reminder_prompt: old_config.commit_reminder_prompt,
            send_message_shortcut: old_config.send_message_shortcut,
            ticketing: TrackerConfig::from_v9(old_config.jira, old_config.jira_watcher),
            slack_webhook_url: old_config.slack_webhook_url,
            slack_bot_token: old_config.slack_bot_token,
            slack_channel: old_config.slack_channel,
            playbooks,
            playbook_keys: CURRENT_PLAYBOOK_KEYS,
            worktree_pool: old_config.worktree_pool,
            orchestrator,
            integrations: old_config.integrations.into(),
            app_runs: AppRunsConfig::default(),
            approvals: ApprovalsConfig::default(),
        }
    }

    pub fn from_previous_version(raw_config: &str) -> Result<Self, Error> {
        let old_config = v9::Config::from(raw_config.to_string());
        Ok(Self::from_v9_config(old_config))
    }
}

impl From<String> for Config {
    fn from(raw_config: String) -> Self {
        if let Ok(mut config) = serde_json::from_str::<Config>(&raw_config)
            && config.config_version == "v10"
        {
            config.migrate_playbook_keys();
            upgrade_stale_playbooks(&mut config.playbooks);
            adopt_legacy_tracker_fields(&mut config, &raw_config);
            config.adopt_legacy_slack();
            return config;
        }

        match Self::from_previous_version(&raw_config) {
            Ok(mut config) => {
                tracing::info!("Config upgraded to v10");
                config.adopt_legacy_slack();
                config
            }
            Err(e) => {
                tracing::warn!("Config migration failed: {}, using default", e);
                Self::default()
            }
        }
    }
}

/// A v10 config that carries its tracker settings as the `jira` and
/// `jira_watcher` pair rather than a `ticketing` (or older `tracker`)
/// section: moves them into `ticketing` so they are not dropped as unknown
/// keys. A config that has either section is left alone.
fn adopt_legacy_tracker_fields(config: &mut Config, raw_config: &str) {
    let Ok(serde_json::Value::Object(raw)) = serde_json::from_str::<serde_json::Value>(raw_config)
    else {
        return;
    };
    if raw.contains_key("ticketing")
        || raw.contains_key("tracker")
        || !(raw.contains_key("jira") || raw.contains_key("jira_watcher"))
    {
        return;
    }
    let jira = raw
        .get("jira")
        .and_then(|value| serde_json::from_value::<v9::JiraConfig>(value.clone()).ok())
        .unwrap_or_default();
    let watcher = raw
        .get("jira_watcher")
        .and_then(|value| serde_json::from_value::<v9::JiraWatcherConfig>(value.clone()).ok())
        .unwrap_or_default();
    config.ticketing = TrackerConfig::from_v9(jira, watcher);
}

impl Default for Config {
    fn default() -> Self {
        Self {
            config_version: "v10".to_string(),
            theme: ThemeMode::System,
            executor_profile: ExecutorProfileId::new(BaseCodingAgent::ClaudeCode),
            disclaimer_acknowledged: false,
            onboarding_acknowledged: false,
            remote_onboarding_acknowledged: false,
            notifications: NotificationConfig::default(),
            editor: EditorConfig::default(),
            github: GitHubConfig::default(),
            analytics_enabled: true,
            workspace_dir: None,
            last_app_version: None,
            show_release_notes: false,
            language: UiLanguage::default(),
            git_branch_prefix: default_git_branch_prefix(),
            showcases: ShowcaseState::default(),
            pr_auto_description_enabled: true,
            pr_auto_description_prompt: None,
            commit_reminder_enabled: true,
            commit_reminder_prompt: None,
            send_message_shortcut: SendMessageShortcut::default(),
            ticketing: TrackerConfig::default(),
            slack_webhook_url: None,
            slack_bot_token: None,
            slack_channel: None,
            playbooks: default_playbooks(),
            playbook_keys: CURRENT_PLAYBOOK_KEYS,
            worktree_pool: WorktreePoolConfig::default(),
            orchestrator: OrchestratorConfig::default(),
            integrations: IntegrationsConfig::default(),
            app_runs: AppRunsConfig::default(),
            approvals: ApprovalsConfig::default(),
        }
    }
}

impl Config {
    /// Renames the playbook keys of a config saved before
    /// [`CURRENT_PLAYBOOK_KEYS`], in the global map and every board's
    /// overrides, once.
    pub fn migrate_playbook_keys(&mut self) {
        if self.playbook_keys >= CURRENT_PLAYBOOK_KEYS {
            return;
        }
        rename_playbook_keys(&mut self.playbooks);
        for board in self.orchestrator.project_defaults.values_mut() {
            rename_playbook_keys(&mut board.playbooks);
        }
        self.playbook_keys = CURRENT_PLAYBOOK_KEYS;
    }

    /// The board `POST /api/cards` files into when the request names none:
    /// `orchestrator.default_board`, else the first configured board.
    pub fn default_board(&self) -> Option<Uuid> {
        self.orchestrator.default_board.or_else(|| {
            self.orchestrator
                .project_defaults
                .keys()
                .find_map(|key| Uuid::parse_str(key).ok())
        })
    }

    /// Resolves one board's settings: board overrides win, anything unset
    /// falls back to the global value. Unknown project ids resolve to the
    /// pure globals, so callers never need to special-case them.
    pub fn project_settings(&self, project_id: Uuid) -> EffectiveProjectSettings {
        let empty = ProjectAutomationDefaults::default();
        let board = self
            .orchestrator
            .project_defaults
            .get(&project_id.to_string())
            .unwrap_or(&empty);

        fn non_empty(value: &Option<String>) -> Option<String> {
            value
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        }

        let jira_project_key = non_empty(&board.ticketing.project_key)
            .unwrap_or_else(|| self.ticketing.project_key().to_string());

        // Board filter, else the global pickup filter, else the shipped
        // default — then substitute this board's key.
        let watcher_jql = non_empty(&board.ticketing.jql_filter)
            .or_else(|| non_empty(&self.ticketing.pickup_filter))
            .unwrap_or_else(|| self.ticketing.default_pickup_filter().to_string())
            .replace("{project_key}", &jira_project_key);

        // Unset means "inherit", which for the watcher is the single-target
        // behaviour: on only for the globally configured board.
        let watcher_enabled = board.ticketing.watcher_enabled.unwrap_or_else(|| {
            self.ticketing.enabled && self.ticketing.project_id == Some(project_id)
        });

        // Assigned pickup: a non-blank board override wins, else the global
        // value. Blank inherits — the UI cannot distinguish cleared from
        // untouched, so blank must never mean "off".
        let assigned_jql = non_empty(&board.ticketing.assigned_jql)
            .or_else(|| non_empty(&self.ticketing.assigned_filter))
            .map(|jql| jql.replace("{project_key}", &jira_project_key));

        let mut playbooks = self.playbooks.clone();
        for (key, text) in &board.playbooks {
            if !text.trim().is_empty() {
                playbooks.insert(key.clone(), text.clone());
            }
        }

        let mut reviewer_map = self.integrations.reviewer_map.clone();
        reviewer_map.extend(
            board
                .merge_requests
                .reviewer_map
                .iter()
                .map(|(email, login)| (email.clone(), login.clone())),
        );
        let mut reviewer_exclude_emails = self.integrations.reviewer_exclude_emails.clone();
        reviewer_exclude_emails
            .extend(board.merge_requests.reviewer_exclude_emails.iter().cloned());
        let mut bot_logins = self.integrations.bot_logins.clone();
        bot_logins.extend(board.merge_requests.bot_logins.iter().cloned());

        EffectiveProjectSettings {
            project_id,
            label: non_empty(&board.label),
            repo_ids: board.repo_ids.clone(),
            executor_profile: board
                .executor_profile
                .clone()
                .unwrap_or_else(|| self.executor_profile.clone()),
            jira_project_key,
            watcher_enabled,
            watcher_jql,
            assigned_jql,
            playbooks,
            autonomy: board.autonomy.unwrap_or(self.orchestrator.autonomy),
            autonomy_review_iterations: board
                .autonomy_review_iterations
                .unwrap_or(self.orchestrator.autonomy_review_iterations),
            deployed_retention_days: match board.deployed_retention {
                RetentionOverride::Inherit => self.orchestrator.deployed_retention_days,
                RetentionOverride::Off => None,
                RetentionOverride::Days(days) => Some(days),
            },
            cleanup_retention_days: match board.cleanup_retention {
                RetentionOverride::Inherit => self.orchestrator.cleanup_retention_days,
                RetentionOverride::Off => None,
                RetentionOverride::Days(days) => Some(days),
            },
            pattern_threshold: board
                .pattern_threshold
                .unwrap_or(self.orchestrator.pattern_threshold),
            merge_requests: EffectiveMrSettings {
                reviewer_map,
                reviewer_exclude_emails,
                auto_assign_reviewers: board
                    .merge_requests
                    .auto_assign_reviewers
                    .unwrap_or(self.integrations.auto_assign_reviewers),
                bot_logins,
            },
            deploy_watch: board
                .deploy_watch
                .as_ref()
                .map(|watch| DeployWatch {
                    job_name: watch.job_name.trim().to_string(),
                    git_ref: watch.git_ref.trim().to_string(),
                })
                .filter(|watch| !watch.job_name.is_empty() && !watch.git_ref.is_empty()),
        }
    }

    /// The notification targets `board` sends to (the global ones for
    /// `None`): the global targets, less the kinds the board skips, plus the
    /// board's own — or nothing when the board is silenced.
    pub fn notification_targets(
        &self,
        board: Option<Uuid>,
    ) -> Vec<(TargetOrigin, NotificationTarget)> {
        let routing = board.and_then(|id| {
            self.orchestrator
                .project_defaults
                .get(&id.to_string())
                .map(|defaults| &defaults.notifications)
        });
        let Some(routing) = routing else {
            return self
                .notifications
                .targets
                .iter()
                .map(|target| (TargetOrigin::Global, target.clone()))
                .collect();
        };
        if routing.enabled == Some(false) {
            return Vec::new();
        }
        self.notifications
            .targets
            .iter()
            .filter(|target| !routing.skip_global.contains(&target.kind))
            .map(|target| (TargetOrigin::Global, target.clone()))
            .chain(
                routing
                    .targets
                    .iter()
                    .map(|target| (TargetOrigin::Board, target.clone())),
            )
            .collect()
    }

    /// Converts Slack settings written by older configs into notification
    /// targets, once: the global `slack_*` fields become one Slack target
    /// when `notifications.targets` is empty, and a board's `slack` override
    /// becomes that board's routing (its own Slack target, merged over the
    /// global Slack settings, in place of the global one). The converted
    /// fields are cleared, so the next save drops them.
    pub fn adopt_legacy_slack(&mut self) {
        fn non_empty(value: &Option<String>) -> Option<String> {
            value
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        }
        fn slack_target(
            webhook_url: Option<String>,
            bot_token: Option<String>,
            channel: Option<String>,
        ) -> NotificationTarget {
            let fields = [
                ("webhook_url", webhook_url),
                ("bot_token", bot_token),
                ("channel", channel),
            ]
            .into_iter()
            .filter_map(|(key, value)| Some((key.to_string(), serde_json::Value::String(value?))))
            .collect();
            NotificationTarget {
                kind: NotifierKind::Slack,
                events: None,
                fields,
            }
        }

        let legacy = (
            non_empty(&self.slack_webhook_url),
            non_empty(&self.slack_bot_token),
            non_empty(&self.slack_channel),
        );
        // The global Slack settings a board override is merged over: the
        // legacy fields, or the Slack target they were already converted to.
        let global_slack = match &legacy {
            (None, None, _) => self
                .notifications
                .targets
                .iter()
                .find(|target| target.kind == NotifierKind::Slack)
                .map(|target| {
                    let field = |key: &str| {
                        target
                            .fields
                            .get(key)
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_string)
                    };
                    (field("webhook_url"), field("bot_token"), field("channel"))
                })
                .unwrap_or((None, None, None)),
            (webhook, token, channel) => (webhook.clone(), token.clone(), channel.clone()),
        };

        if (legacy.0.is_some() || legacy.1.is_some()) && self.notifications.targets.is_empty() {
            let (webhook, token, channel) = legacy;
            self.notifications
                .targets
                .push(slack_target(webhook, token, channel));
            self.slack_webhook_url = None;
            self.slack_bot_token = None;
            self.slack_channel = None;
            tracing::info!("Config: Slack settings moved into notifications.targets");
        }

        for defaults in self.orchestrator.project_defaults.values_mut() {
            if slack_override_is_empty(&defaults.slack) || !defaults.notifications.is_empty() {
                continue;
            }
            let slack = std::mem::take(&mut defaults.slack);
            if slack.enabled == Some(false) {
                defaults.notifications.enabled = Some(false);
            }
            let own = (
                non_empty(&slack.webhook_url),
                non_empty(&slack.bot_token),
                non_empty(&slack.channel),
            );
            if own.0.is_some() || own.1.is_some() || own.2.is_some() {
                defaults.notifications.skip_global = vec![NotifierKind::Slack];
                defaults.notifications.targets.push(slack_target(
                    own.0.or_else(|| global_slack.0.clone()),
                    own.1.or_else(|| global_slack.1.clone()),
                    own.2.or_else(|| global_slack.2.clone()),
                ));
            }
        }
    }

    /// Boards the Jira watcher should poll, each with its own filter.
    ///
    /// Boards opt in through `ticketing.watcher_enabled`; those that leave it
    /// unset inherit the legacy single-target behaviour, so a config written
    /// before multi-board support keeps working unchanged. The globally
    /// configured board is included even when it has no `project_defaults`
    /// entry — importing tickets needs no repo.
    pub fn watch_targets(&self) -> Vec<WatchTarget> {
        let mut targets: Vec<WatchTarget> = Vec::new();
        let mut push = |settings: EffectiveProjectSettings| {
            if !settings.watcher_enabled
                || targets
                    .iter()
                    .any(|target| target.project_id == settings.project_id)
            {
                return;
            }
            targets.push(WatchTarget {
                project_id: settings.project_id,
                jira_project_key: settings.jira_project_key,
                jql: settings.watcher_jql,
                assigned_jql: settings.assigned_jql,
            });
        };

        for key in self.orchestrator.project_defaults.keys() {
            if let Ok(project_id) = Uuid::parse_str(key) {
                push(self.project_settings(project_id));
            }
        }
        if let Some(project_id) = self.ticketing.project_id {
            push(self.project_settings(project_id));
        }
        targets
    }
}

#[cfg(test)]
mod project_settings_tests {
    use super::*;

    fn slack(fields: &[(&str, &str)]) -> NotificationTarget {
        NotificationTarget {
            kind: NotifierKind::Slack,
            events: None,
            fields: fields
                .iter()
                .map(|(key, value)| (key.to_string(), serde_json::json!(value)))
                .collect(),
        }
    }

    /// Single-repository board: repos only, everything else inherited.
    fn config_with_board(project_id: Uuid, board: ProjectAutomationDefaults) -> Config {
        let mut config = Config::default();
        config.ticketing.project_key = Some("BUG".to_string());
        config
            .notifications
            .targets
            .push(slack(&[("webhook_url", "https://hooks.slack/global")]));
        config.orchestrator.autonomy = AutonomyMode::Full;
        config.orchestrator.deployed_retention_days = Some(7);
        config.orchestrator.pattern_threshold = 5;
        config
            .integrations
            .reviewer_map
            .insert("dev@example.com".to_string(), "dev".to_string());
        config
            .orchestrator
            .project_defaults
            .insert(project_id.to_string(), board);
        config
    }

    #[test]
    fn a_saved_superseded_playbook_upgrades_to_the_current_default() {
        // The point of SUPERSEDED_PLAYBOOKS is byte-exactness: a saved config
        // holding a previous default must match it character for character or
        // the user silently keeps the old text forever. Nothing else checks
        // that, and the entries are added by hand.
        let defaults = default_playbooks();
        for (key, old_text) in SUPERSEDED_PLAYBOOKS {
            let current = defaults
                .get(*key)
                .unwrap_or_else(|| panic!("superseded key {key} has no current default"));
            assert_ne!(
                old_text, current,
                "{key}: a superseded entry equal to the current default never migrates"
            );

            let mut saved = BTreeMap::from([((*key).to_string(), (*old_text).to_string())]);
            upgrade_stale_playbooks(&mut saved);
            assert_eq!(saved.get(*key), Some(current), "{key} did not upgrade");
        }
    }

    /// A v10 config saved before the renames carries `investigation:planning`
    /// (the pattern review's text) and `triage`, globally and on a board. On
    /// load both move to their new keys; the old shipped pattern text upgrades,
    /// a board's customised one survives, and investigation cards get the new
    /// default under the key the pattern review left.
    #[test]
    fn saved_playbook_keys_are_renamed_once_on_load() {
        let board_id = Uuid::new_v4();
        let mut raw = serde_json::to_value(Config::default()).unwrap();
        let map = raw.as_object_mut().unwrap();
        map.remove("playbook_keys");
        map.insert(
            "playbooks".to_string(),
            serde_json::json!({
                "investigation:planning": superseded_playbooks::PATTERN_PLANNING_1,
                "triage": "our own comment replies",
                "bug:planning": "our own RCA text",
            }),
        );
        map["orchestrator"]["project_defaults"] = serde_json::json!({
            board_id.to_string(): {
                "repo_ids": [],
                "investigation_threshold": 3,
                "playbooks": {
                    "investigation:planning": "board pattern text",
                    "triage": "board replies",
                }
            }
        });
        let orchestrator = map["orchestrator"].as_object_mut().unwrap();
        orchestrator.remove("pattern_threshold");
        orchestrator.insert("investigation_threshold".to_string(), serde_json::json!(7));

        let config = Config::from(raw.to_string());
        let defaults = default_playbooks();
        assert_eq!(config.playbook_keys, CURRENT_PLAYBOOK_KEYS);
        assert_eq!(
            config.playbooks[PATTERN_PLAYBOOK_KEY],
            defaults[PATTERN_PLAYBOOK_KEY]
        );
        assert_eq!(
            config.playbooks[INVESTIGATION_PLAYBOOK_KEY],
            defaults[INVESTIGATION_PLAYBOOK_KEY]
        );
        assert_eq!(
            config.playbooks[COMMENT_REPLY_PLAYBOOK_KEY],
            "our own comment replies"
        );
        assert!(!config.playbooks.contains_key("triage"));
        assert_eq!(config.playbooks["bug:planning"], "our own RCA text");

        let board = &config.orchestrator.project_defaults[&board_id.to_string()];
        assert_eq!(board.playbooks[PATTERN_PLAYBOOK_KEY], "board pattern text");
        assert_eq!(board.playbooks[COMMENT_REPLY_PLAYBOOK_KEY], "board replies");
        assert!(!board.playbooks.contains_key(INVESTIGATION_PLAYBOOK_KEY));
        assert_eq!(board.pattern_threshold, Some(3));
        assert_eq!(config.orchestrator.pattern_threshold, 7);

        // Saved again, the config is in current keys: a board override of
        // the investigation card's playbook is never mistaken for an old one.
        let mut saved = config.clone();
        saved
            .orchestrator
            .project_defaults
            .get_mut(&board_id.to_string())
            .unwrap()
            .playbooks
            .insert(
                INVESTIGATION_PLAYBOOK_KEY.to_string(),
                "board triage text".to_string(),
            );
        let reloaded = Config::from(serde_json::to_string(&saved).unwrap());
        let board = &reloaded.orchestrator.project_defaults[&board_id.to_string()];
        assert_eq!(
            board.playbooks[INVESTIGATION_PLAYBOOK_KEY],
            "board triage text"
        );
        assert_eq!(board.playbooks[PATTERN_PLAYBOOK_KEY], "board pattern text");
    }

    #[test]
    fn the_default_board_is_configured_or_the_first_one() {
        let first = Uuid::from_u128(1);
        let second = Uuid::from_u128(2);
        let mut config = Config::default();
        assert_eq!(config.default_board(), None);
        for id in [second, first] {
            config
                .orchestrator
                .project_defaults
                .insert(id.to_string(), ProjectAutomationDefaults::default());
        }
        assert_eq!(config.default_board(), Some(first));
        config.orchestrator.default_board = Some(second);
        assert_eq!(config.default_board(), Some(second));
    }

    #[test]
    fn a_customized_playbook_is_left_alone() {
        let mut saved = BTreeMap::from([(
            REVIEWING_PLAYBOOK_KEY.to_string(),
            "our own reviewer instructions".to_string(),
        )]);
        upgrade_stale_playbooks(&mut saved);
        assert_eq!(
            saved.get(REVIEWING_PLAYBOOK_KEY).map(String::as_str),
            Some("our own reviewer instructions")
        );
    }

    #[test]
    fn unset_overrides_inherit_the_globals() {
        let project_id = Uuid::new_v4();
        let config = config_with_board(project_id, ProjectAutomationDefaults::default());
        let settings = config.project_settings(project_id);

        assert_eq!(settings.jira_project_key, "BUG");
        assert_eq!(
            config.notification_targets(Some(project_id)),
            vec![(
                TargetOrigin::Global,
                slack(&[("webhook_url", "https://hooks.slack/global")])
            )]
        );
        assert_eq!(settings.autonomy, AutonomyMode::Full);
        assert_eq!(settings.deployed_retention_days, Some(7));
        assert_eq!(settings.pattern_threshold, 5);
        assert_eq!(
            settings.merge_requests.reviewer_map.get("dev@example.com"),
            Some(&"dev".to_string())
        );
        // Playbooks come through wholesale so a board needs no copy of them.
        assert!(settings.playbooks.contains_key("bug:planning"));
    }

    #[test]
    fn board_overrides_win_and_merge() {
        let project_id = Uuid::new_v4();
        let mut board = ProjectAutomationDefaults {
            label: Some("extensions".to_string()),
            pattern_threshold: Some(2),
            autonomy: Some(AutonomyMode::Off),
            deployed_retention: RetentionOverride::Off,
            ..Default::default()
        };
        board.ticketing.jql_filter =
            Some("project = {project_key} AND component = Extension".to_string());
        board.notifications.targets.push(NotificationTarget {
            kind: NotifierKind::Webhook,
            events: Some(vec!["daily_digest".to_string()]),
            fields: BTreeMap::from([(
                "url".to_string(),
                serde_json::json!("https://example.com/hook"),
            )]),
        });
        board
            .merge_requests
            .reviewer_map
            .insert("ext@example.com".to_string(), "ext-dev".to_string());
        board.playbooks.insert(
            "bug:planning".to_string(),
            "extension-specific RCA".to_string(),
        );

        let config = config_with_board(project_id, board);
        let settings = config.project_settings(project_id);

        assert_eq!(settings.label.as_deref(), Some("extensions"));
        assert_eq!(
            settings.watcher_jql,
            "project = BUG AND component = Extension"
        );
        // The board's own target adds to the global one.
        let targets = config.notification_targets(Some(project_id));
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].0, TargetOrigin::Global);
        assert_eq!(targets[1].0, TargetOrigin::Board);
        assert_eq!(targets[1].1.kind, NotifierKind::Webhook);
        assert_eq!(settings.autonomy, AutonomyMode::Off);
        assert_eq!(settings.deployed_retention_days, None);
        // The Clean-up sweep resolves the same three ways, independently.
        assert_eq!(settings.cleanup_retention_days, None); // global unset

        let mut config2 = Config::default();
        config2.orchestrator.cleanup_retention_days = Some(14);
        let project2 = Uuid::new_v4();
        let board2 = ProjectAutomationDefaults {
            cleanup_retention: RetentionOverride::Days(3),
            ..Default::default()
        };
        config2
            .orchestrator
            .project_defaults
            .insert(project2.to_string(), board2);
        assert_eq!(
            config2.project_settings(project2).cleanup_retention_days,
            Some(3)
        );
        let board2 = config2
            .orchestrator
            .project_defaults
            .get_mut(&project2.to_string())
            .unwrap();
        board2.cleanup_retention = RetentionOverride::Off;
        assert_eq!(
            config2.project_settings(project2).cleanup_retention_days,
            None
        );
        let board2 = config2
            .orchestrator
            .project_defaults
            .get_mut(&project2.to_string())
            .unwrap();
        board2.cleanup_retention = RetentionOverride::Inherit;
        assert_eq!(
            config2.project_settings(project2).cleanup_retention_days,
            Some(14)
        );
        assert_eq!(settings.pattern_threshold, 2);
        assert_eq!(settings.playbooks["bug:planning"], "extension-specific RCA");
        // Board reviewers add to the global map rather than replacing it.
        assert_eq!(settings.merge_requests.reviewer_map.len(), 2);
    }

    #[test]
    fn notifications_can_be_silenced_or_replaced_for_one_board() {
        let project_id = Uuid::new_v4();
        let mut board = ProjectAutomationDefaults::default();
        board.notifications.enabled = Some(false);
        let config = config_with_board(project_id, board);
        assert!(config.notification_targets(Some(project_id)).is_empty());
        // The global scope and other boards are unaffected.
        assert_eq!(config.notification_targets(None).len(), 1);
        assert_eq!(config.notification_targets(Some(Uuid::new_v4())).len(), 1);

        let mut board = ProjectAutomationDefaults::default();
        board.notifications.skip_global = vec![NotifierKind::Slack];
        board
            .notifications
            .targets
            .push(slack(&[("webhook_url", "https://hooks.slack/board")]));
        let config = config_with_board(project_id, board);
        assert_eq!(
            config.notification_targets(Some(project_id)),
            vec![(
                TargetOrigin::Board,
                slack(&[("webhook_url", "https://hooks.slack/board")])
            )]
        );
    }

    #[test]
    fn legacy_single_target_still_watched() {
        let project_id = Uuid::new_v4();
        let mut config = config_with_board(project_id, ProjectAutomationDefaults::default());
        config.ticketing.enabled = true;
        config.ticketing.project_id = Some(project_id);
        config.ticketing.pickup_filter =
            Some("project = {project_key} AND priority = P0".to_string());

        let targets = config.watch_targets();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].project_id, project_id);
        assert_eq!(targets[0].jql, "project = BUG AND priority = P0");
    }

    #[test]
    fn second_board_opts_in_with_its_own_filter() {
        let legacy = Uuid::new_v4();
        let extensions = Uuid::new_v4();
        let mut config = config_with_board(legacy, ProjectAutomationDefaults::default());
        config.ticketing.enabled = true;
        config.ticketing.project_id = Some(legacy);

        let mut board = ProjectAutomationDefaults::default();
        board.ticketing.watcher_enabled = Some(true);
        board.ticketing.jql_filter =
            Some("project = {project_key} AND labels = extension".to_string());
        config
            .orchestrator
            .project_defaults
            .insert(extensions.to_string(), board);

        let targets = config.watch_targets();
        assert_eq!(targets.len(), 2);
        let extension_target = targets
            .iter()
            .find(|target| target.project_id == extensions)
            .expect("extensions board is watched");
        assert_eq!(extension_target.jql, "project = BUG AND labels = extension");
        // The legacy board keeps the shipped default filter.
        let legacy_target = targets
            .iter()
            .find(|target| target.project_id == legacy)
            .expect("legacy board is watched");
        assert!(legacy_target.jql.contains("issuetype = Bug"));
    }

    #[test]
    fn board_can_opt_out_while_global_watcher_is_on() {
        let project_id = Uuid::new_v4();
        let mut board = ProjectAutomationDefaults::default();
        board.ticketing.watcher_enabled = Some(false);
        let mut config = config_with_board(project_id, board);
        config.ticketing.enabled = true;
        config.ticketing.project_id = Some(project_id);

        assert!(config.watch_targets().is_empty());
    }

    #[test]
    fn assigned_jql_inherits_substitutes_and_disables() {
        let project_id = Uuid::new_v4();
        let mut config = config_with_board(project_id, ProjectAutomationDefaults::default());
        // Unset everywhere: the feature is off.
        assert!(config.project_settings(project_id).assigned_jql.is_none());

        // Global value inherits, with this board's key substituted.
        config.ticketing.assigned_filter =
            Some("project = {project_key} AND assignee = currentUser()".to_string());
        assert_eq!(
            config.project_settings(project_id).assigned_jql.as_deref(),
            Some("project = BUG AND assignee = currentUser()")
        );

        // A board override wins over the global…
        let board = config
            .orchestrator
            .project_defaults
            .get_mut(&project_id.to_string())
            .unwrap();
        board.ticketing.assigned_jql =
            Some("project = {project_key} AND component = Extension".to_string());
        assert_eq!(
            config.project_settings(project_id).assigned_jql.as_deref(),
            Some("project = BUG AND component = Extension")
        );

        // …and a blank board value inherits, exactly like None: the Settings
        // inputs normalize cleared fields to blank, so blank cannot mean off.
        let board = config
            .orchestrator
            .project_defaults
            .get_mut(&project_id.to_string())
            .unwrap();
        board.ticketing.assigned_jql = Some("  ".to_string());
        assert_eq!(
            config.project_settings(project_id).assigned_jql.as_deref(),
            Some("project = BUG AND assignee = currentUser()")
        );
    }
}

#[cfg(test)]
mod migration_tests {
    use super::*;

    /// A v10 file as written before notification targets existed.
    fn legacy_slack_config() -> serde_json::Value {
        let board = "6f8c2f8e-0000-4000-8000-000000000001";
        let mut raw = serde_json::to_value(Config::default()).unwrap();
        raw["slack_webhook_url"] = serde_json::json!("https://hooks.slack.com/services/T/B/global");
        raw["slack_bot_token"] = serde_json::json!("xoxb-global");
        raw["slack_channel"] = serde_json::json!("C0GLOBAL");
        raw["notifications"]
            .as_object_mut()
            .unwrap()
            .remove("targets");
        raw["orchestrator"]["project_defaults"][board] = serde_json::json!({
            "repo_ids": [],
            "slack": { "channel": "C0BOARD" }
        });
        raw["orchestrator"]["project_defaults"]["6f8c2f8e-0000-4000-8000-000000000002"] =
            serde_json::json!({ "repo_ids": [], "slack": { "enabled": false } });
        raw
    }

    #[test]
    fn legacy_slack_settings_become_a_slack_target_once() {
        let config = Config::from(legacy_slack_config().to_string());

        assert_eq!(config.notifications.targets.len(), 1);
        let target = &config.notifications.targets[0];
        assert_eq!(target.kind, NotifierKind::Slack);
        assert_eq!(
            target.fields["webhook_url"],
            "https://hooks.slack.com/services/T/B/global"
        );
        assert_eq!(target.fields["bot_token"], "xoxb-global");
        assert_eq!(target.fields["channel"], "C0GLOBAL");
        assert!(config.slack_webhook_url.is_none());
        assert!(config.slack_bot_token.is_none());
        assert!(config.slack_channel.is_none());

        // A board that only changed the channel gets its own Slack target
        // carrying the global credentials, in place of the global one.
        let board = &config.orchestrator.project_defaults["6f8c2f8e-0000-4000-8000-000000000001"];
        assert_eq!(board.notifications.skip_global, vec![NotifierKind::Slack]);
        assert_eq!(board.notifications.targets.len(), 1);
        assert_eq!(board.notifications.targets[0].fields["channel"], "C0BOARD");
        assert_eq!(
            board.notifications.targets[0].fields["bot_token"],
            "xoxb-global"
        );
        assert!(slack_override_is_empty(&board.slack));
        // A silenced board stays silenced.
        let silenced =
            &config.orchestrator.project_defaults["6f8c2f8e-0000-4000-8000-000000000002"];
        assert_eq!(silenced.notifications.enabled, Some(false));

        // The saved file no longer carries the old keys, and loading it again
        // changes nothing.
        let saved = serde_json::to_value(&config).unwrap();
        assert!(saved.get("slack_webhook_url").is_none());
        assert!(
            saved["orchestrator"]["project_defaults"]["6f8c2f8e-0000-4000-8000-000000000001"]
                .get("slack")
                .is_none()
        );
        let reloaded = Config::from(saved.to_string());
        assert_eq!(reloaded.notifications.targets, config.notifications.targets);
        assert_eq!(
            reloaded.orchestrator.project_defaults["6f8c2f8e-0000-4000-8000-000000000001"]
                .notifications,
            board.notifications
        );
    }

    #[test]
    fn existing_targets_are_never_overwritten_by_legacy_fields() {
        let mut raw = legacy_slack_config();
        raw["notifications"]["targets"] = serde_json::json!([
            { "kind": "webhook", "url": "https://example.com/hook", "events": ["*"] }
        ]);
        let config = Config::from(raw.to_string());
        assert_eq!(config.notifications.targets.len(), 1);
        assert_eq!(config.notifications.targets[0].kind, NotifierKind::Webhook);
        assert_eq!(
            config.notifications.targets[0].fields["url"],
            "https://example.com/hook"
        );
        assert_eq!(
            config.notifications.targets[0].events,
            Some(vec!["*".to_string()])
        );
    }

    fn with_auto_install_extension(config: serde_json::Value) -> String {
        let mut config = config;
        config["editor"]["auto_install_extension"] = serde_json::Value::Bool(true);
        config.to_string()
    }

    #[test]
    fn a_v9_config_with_auto_install_extension_upgrades_and_drops_it() {
        let old = v9::Config {
            editor: EditorConfig::new(EditorType::Cursor, None, Some("devbox".to_string()), None),
            ..v9::Config::default()
        };
        let raw = with_auto_install_extension(serde_json::to_value(&old).unwrap());

        let config = Config::from(raw);
        assert_eq!(config.config_version, "v10");
        let editor = serde_json::to_value(&config.editor).unwrap();
        assert_eq!(editor["editor_type"], "CURSOR");
        assert_eq!(editor["remote_ssh_host"], "devbox");
        assert!(editor.get("auto_install_extension").is_none());
    }

    #[test]
    fn a_v10_config_with_auto_install_extension_loads_as_v10() {
        let current = Config {
            git_branch_prefix: "keep".to_string(),
            ..Config::default()
        };
        let raw = with_auto_install_extension(serde_json::to_value(&current).unwrap());

        let config = Config::from(raw);
        assert_eq!(config.config_version, "v10");
        assert_eq!(config.git_branch_prefix, "keep");
        let editor = serde_json::to_value(&config.editor).unwrap();
        assert!(editor.get("auto_install_extension").is_none());
    }

    #[test]
    fn a_v9_config_upgrades_with_its_settings_and_default_app_runs() {
        let mut old = v9::Config {
            slack_webhook_url: Some("https://hooks.slack.com/services/T/B/x".to_string()),
            slack_channel: Some("C0TEST".to_string()),
            ..v9::Config::default()
        };
        old.orchestrator.poll_interval_secs = 42;
        let raw = serde_json::to_string(&old).unwrap();

        let config = Config::from(raw);
        assert_eq!(config.config_version, "v10");
        assert_eq!(config.notifications.targets.len(), 1);
        assert_eq!(
            config.notifications.targets[0].fields["channel"],
            serde_json::json!("C0TEST")
        );
        assert!(config.slack_channel.is_none());
        assert_eq!(config.orchestrator.poll_interval_secs, 42);
        assert_eq!(config.app_runs, AppRunsConfig::default());
        assert_eq!(config.app_runs.port_range, "4200-4299");
        assert_eq!(config.app_runs.max_parallel, 2);
    }

    #[test]
    fn a_v9_config_carries_its_gitlab_settings_to_the_neutral_names() {
        let mut old = v9::Config::default();
        old.integrations
            .gitlab_reviewer_map
            .insert("dev@example.com".to_string(), "dev".to_string());
        old.integrations.gitlab_reviewer_exclude_emails = vec!["ci@example.com".to_string()];
        old.integrations.gitlab_auto_assign_reviewers = false;
        old.integrations.gitlab_bot_logins = vec!["release-bot".to_string()];
        old.integrations.sentry_host = Some("sentry.example.com".to_string());
        let board = Uuid::new_v4().to_string();
        old.orchestrator.project_defaults.insert(
            board.clone(),
            v9::ProjectAutomationDefaults {
                label: Some("app".to_string()),
                ..Default::default()
            },
        );

        let config = Config::from(serde_json::to_string(&old).unwrap());
        assert_eq!(config.config_version, "v10");
        let integrations = &config.integrations;
        assert_eq!(
            integrations.reviewer_map.get("dev@example.com"),
            Some(&"dev".to_string())
        );
        assert_eq!(integrations.reviewer_exclude_emails, ["ci@example.com"]);
        assert!(!integrations.auto_assign_reviewers);
        assert_eq!(integrations.bot_logins, ["release-bot"]);
        assert_eq!(
            integrations.sentry_host.as_deref(),
            Some("sentry.example.com")
        );
        // Of v9's observability settings only Sentry's carry over.
        let integrations = serde_json::to_value(&config.integrations).unwrap();
        let mut keys: Vec<&String> = integrations.as_object().unwrap().keys().collect();
        keys.sort();
        assert_eq!(
            keys,
            [
                "auto_assign_reviewers",
                "bot_logins",
                "reviewer_exclude_emails",
                "reviewer_map",
                "sentry_access_token",
                "sentry_host",
            ]
        );
        let migrated = &config.orchestrator.project_defaults[&board];
        assert_eq!(migrated.label.as_deref(), Some("app"));
        // The watch is opt-in: nothing migrated turns it on.
        assert_eq!(migrated.deploy_watch, None);
    }

    #[test]
    fn a_deploy_watch_is_off_unless_both_fields_are_set() {
        let project_id = Uuid::new_v4();
        let mut config = Config::default();
        let mut board = ProjectAutomationDefaults::default();
        assert!(
            config_with(&mut config, project_id, board.clone())
                .deploy_watch
                .is_none()
        );

        board.deploy_watch = Some(DeployWatch {
            job_name: " deploy_staging ".to_string(),
            git_ref: "  ".to_string(),
        });
        assert!(
            config_with(&mut config, project_id, board.clone())
                .deploy_watch
                .is_none()
        );

        board.deploy_watch = Some(DeployWatch {
            job_name: " deploy_staging ".to_string(),
            git_ref: "main".to_string(),
        });
        assert_eq!(
            config_with(&mut config, project_id, board).deploy_watch,
            Some(DeployWatch {
                job_name: "deploy_staging".to_string(),
                git_ref: "main".to_string(),
            })
        );
    }

    fn config_with(
        config: &mut Config,
        project_id: Uuid,
        board: ProjectAutomationDefaults,
    ) -> EffectiveProjectSettings {
        config
            .orchestrator
            .project_defaults
            .insert(project_id.to_string(), board);
        config.project_settings(project_id)
    }

    #[test]
    fn a_v9_config_moves_its_jira_settings_into_the_tracker() {
        let mut old = v9::Config::default();
        old.jira.site = Some("acme.atlassian.net".to_string());
        old.jira.email = Some("bot@acme.test".to_string());
        old.jira.api_token = Some("token".to_string());
        old.jira.project_key = Some("OPS".to_string());
        old.jira.default_jql = Some("project = {project_key}".to_string());
        old.jira
            .rca_approvers
            .insert("acct-1".to_string(), "Ada".to_string());
        old.jira
            .type_mapping
            .insert("Incident".to_string(), WorkClass::Bug);
        let project_id = Uuid::new_v4();
        old.jira_watcher.enabled = true;
        old.jira_watcher.poll_secs = 90;
        old.jira_watcher.project_id = Some(project_id);
        old.jira_watcher.jql_filter = Some("project = {project_key} AND priority = P0".to_string());
        old.jira_watcher.assigned_jql = Some("assignee = currentUser()".to_string());
        let mut board = v9::ProjectAutomationDefaults::default();
        board.jira.jql_filter = Some("component = Extension".to_string());
        board.jira.watcher_enabled = Some(true);
        old.orchestrator
            .project_defaults
            .insert(project_id.to_string(), board);

        let config = Config::from(serde_json::to_string(&old).unwrap());
        let tracker = &config.ticketing;
        assert_eq!(tracker.kind, TrackerKind::Jira);
        assert_eq!(tracker.site(), "acme.atlassian.net");
        assert_eq!(tracker.email(), Some("bot@acme.test"));
        assert_eq!(tracker.api_token(), Some("token"));
        assert_eq!(tracker.project_key(), "OPS");
        assert_eq!(tracker.resolved_import_filter(), "project = OPS");
        assert_eq!(
            tracker.approvers.get("acct-1").map(String::as_str),
            Some("Ada")
        );
        assert_eq!(tracker.work_class_for("incident"), WorkClass::Bug);
        assert!(tracker.enabled);
        assert_eq!(tracker.poll_secs, 90);
        assert_eq!(tracker.project_id, Some(project_id));
        assert_eq!(
            tracker.pickup_filter.as_deref(),
            Some("project = {project_key} AND priority = P0")
        );
        assert_eq!(
            tracker.assigned_filter.as_deref(),
            Some("assignee = currentUser()")
        );
        // What was hard-coded before the tracker had settings of its own.
        assert_eq!(tracker.blocker_priorities, ["P0", "P1"]);
        assert_eq!(
            tracker.status_mapping.shipped,
            ["Merged", "Deployed to Production", "Done"]
        );
        assert_eq!(tracker.status_mapping.planning_status(), "In Progress");

        let board = &config.orchestrator.project_defaults[&project_id.to_string()];
        assert_eq!(
            board.ticketing.jql_filter.as_deref(),
            Some("component = Extension")
        );
        assert_eq!(board.ticketing.watcher_enabled, Some(true));
        let targets = config.watch_targets();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].jql, "component = Extension");
    }

    #[test]
    fn a_v10_config_that_names_its_tracker_jira_keeps_its_settings() {
        let project_id = Uuid::new_v4();
        let mut raw = serde_json::to_value(Config::default()).unwrap();
        let map = raw.as_object_mut().unwrap();
        map.remove("ticketing");
        map.insert(
            "jira".to_string(),
            serde_json::json!({ "email": "bot@acme.test", "api_token": "token",
                                "rca_approvers": { "acct-1": "Ada" } }),
        );
        map.insert(
            "jira_watcher".to_string(),
            serde_json::json!({ "enabled": true, "project_id": project_id }),
        );
        map["orchestrator"]["project_defaults"] = serde_json::json!({
            project_id.to_string(): { "jira": { "project_key": "EXT" } }
        });

        let config = Config::from(raw.to_string());
        assert!(config.ticketing.has_credentials());
        assert!(config.ticketing.enabled);
        assert_eq!(config.ticketing.project_id, Some(project_id));
        assert_eq!(config.ticketing.approvers.len(), 1);
        assert_eq!(
            config.orchestrator.project_defaults[&project_id.to_string()]
                .ticketing
                .project_key
                .as_deref(),
            Some("EXT")
        );

        // Once saved with a `ticketing` key, that key is what counts.
        let resaved = Config::from(serde_json::to_string(&config).unwrap());
        assert_eq!(resaved.ticketing, config.ticketing);
    }

    /// A config saved while the section was called `tracker` (and a board's
    /// override with it) loads into `ticketing`, and is saved under the new
    /// name from then on.
    #[test]
    fn the_tracker_section_is_read_as_ticketing_and_saved_under_the_new_name() {
        let project_id = Uuid::new_v4();
        let mut raw = serde_json::to_value(Config::default()).unwrap();
        let map = raw.as_object_mut().unwrap();
        map.remove("ticketing");
        map.insert(
            "tracker".to_string(),
            serde_json::json!({
                "kind": "jira",
                "email": "bot@acme.test",
                "api_token": "token",
                "project_key": "OPS",
                "enabled": true,
                "project_id": project_id,
            }),
        );
        map["orchestrator"]["project_defaults"] = serde_json::json!({
            project_id.to_string(): { "tracker": { "jql_filter": "component = Ext" } }
        });

        let config = Config::from(raw.to_string());
        assert_eq!(config.ticketing.mode, TicketingMode::Tickets);
        assert_eq!(config.ticketing.kind, TrackerKind::Jira);
        assert_eq!(config.ticketing.project_key(), "OPS");
        assert!(config.ticketing.has_credentials());
        assert_eq!(
            config.orchestrator.project_defaults[&project_id.to_string()]
                .ticketing
                .jql_filter
                .as_deref(),
            Some("component = Ext")
        );

        let saved = serde_json::to_value(&config).unwrap();
        assert!(saved.get("tracker").is_none());
        assert_eq!(saved["ticketing"]["project_key"], "OPS");
        assert_eq!(saved["ticketing"]["mode"], "tickets");
        let board = &saved["orchestrator"]["project_defaults"][project_id.to_string()];
        assert!(board.get("tracker").is_none());
        assert_eq!(board["ticketing"]["jql_filter"], "component = Ext");
        assert_eq!(
            Config::from(saved.to_string()).ticketing,
            config.ticketing,
            "the saved shape reads back unchanged"
        );
    }

    #[test]
    fn incidents_mode_reads_pagerduty_and_defaults_unmapped_types_to_investigation() {
        let raw = serde_json::json!({
            "mode": "incidents",
            "kind": "pagerduty",
            "pagerduty_api_token": "pd-token",
            "service_ids": " PSVC1, ,PSVC2 ",
            "user_id": "PUSER1",
            "incidents": { "resolve_on_triage": true },
        });
        let mut ticketing: TrackerConfig = serde_json::from_value(raw).unwrap();
        assert_eq!(ticketing.kind, TrackerKind::PagerDuty);
        assert_eq!(ticketing.mode_mismatch(), None);
        assert!(ticketing.incidents.resolve_on_triage);
        assert_eq!(ticketing.service_ids(), ["PSVC1", "PSVC2"]);
        assert_eq!(
            ticketing.default_pickup_filter(),
            "status:triggered,acknowledged"
        );
        // Incidents are investigated unless the type mapping says otherwise.
        assert_eq!(
            ticketing.work_class_for("Incident"),
            WorkClass::Investigation
        );
        ticketing
            .type_mapping
            .insert("Incident".to_string(), WorkClass::Bug);
        assert_eq!(ticketing.work_class_for("incident"), WorkClass::Bug);
        // Tickets mode keeps stories as the fallback.
        assert_eq!(
            TrackerConfig::default().work_class_for("Incident"),
            WorkClass::Story
        );

        ticketing.mode = TicketingMode::Tickets;
        assert!(
            ticketing
                .mode_mismatch()
                .is_some_and(|why| why.contains("PagerDuty"))
        );
        assert_eq!(
            TrackerKind::parse("pagerduty"),
            Some(TrackerKind::PagerDuty)
        );
        assert_eq!(
            serde_json::to_value(TrackerKind::GithubIssues).unwrap(),
            "github_issues"
        );
    }

    #[test]
    fn blocker_priorities_and_shipped_statuses_match_loosely() {
        let tracker = TrackerConfig::default();
        assert!(tracker.is_blocker_priority(" p0 "));
        assert!(tracker.is_blocker_priority("P1"));
        assert!(!tracker.is_blocker_priority("P2"));
        assert!(!tracker.is_blocker_priority(""));
        assert!(tracker.status_mapping.is_shipped("deployed to production"));
        assert!(!tracker.status_mapping.is_shipped("In Progress"));
    }

    #[test]
    fn a_v9_config_upgrades_with_every_default_playbook() {
        let mut old = v9::Config::default();
        assert!(old.playbooks.is_empty(), "v9 ships no playbook texts");
        old.playbooks.insert(
            REVIEWING_PLAYBOOK_KEY.to_string(),
            "our own reviewer instructions".to_string(),
        );

        let config = Config::from(serde_json::to_string(&old).unwrap());
        let defaults = default_playbooks();
        assert_eq!(config.playbooks.len(), defaults.len());
        for (key, text) in &defaults {
            if key != REVIEWING_PLAYBOOK_KEY {
                assert_eq!(config.playbooks.get(key), Some(text), "{key}");
            }
        }
        // A text the user wrote survives the upgrade.
        assert_eq!(
            config.playbooks[REVIEWING_PLAYBOOK_KEY],
            "our own reviewer instructions"
        );
    }

    #[test]
    fn a_v9_config_without_a_playbooks_key_gets_the_defaults() {
        let mut raw = serde_json::to_value(v9::Config::default()).unwrap();
        raw.as_object_mut().unwrap().remove("playbooks");
        let config = Config::from(raw.to_string());
        assert_eq!(config.playbooks, default_playbooks());
        assert_eq!(config.approvals.in_app_gate, InAppGate::Lavish);
    }

    #[test]
    fn the_in_app_gate_defaults_to_lavish_and_round_trips() {
        let mut raw = serde_json::to_value(Config::default()).unwrap();
        raw.as_object_mut().unwrap().remove("approvals");
        assert_eq!(
            Config::from(raw.to_string()).approvals.in_app_gate,
            InAppGate::Lavish
        );

        let mut config = Config::default();
        config.approvals.in_app_gate = InAppGate::None;
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains(r#""in_app_gate":"none""#), "{json}");
        assert_eq!(Config::from(json).approvals.in_app_gate, InAppGate::None);
    }

    #[test]
    fn a_v10_config_keeps_its_app_runs() {
        let mut config = Config::default();
        config.app_runs.port_range = "5000-5009".to_string();
        config.app_runs.max_parallel = 4;
        let reread = Config::from(serde_json::to_string(&config).unwrap());
        assert_eq!(reread.app_runs.port_range, "5000-5009");
        assert_eq!(reread.app_runs.max_parallel, 4);
    }
}
