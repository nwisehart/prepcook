use std::collections::BTreeMap;

use anyhow::Error;
use api_types::issue_ext::WorkClass;
use executors::{executors::BaseCodingAgent, profile::ExecutorProfileId};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;
pub use v8::{
    EditorConfig, EditorType, GitHubConfig, NotificationConfig, SendMessageShortcut, ShowcaseState,
    SoundFile, ThemeMode, UiLanguage,
};

use crate::services::config::versions::v8;

fn default_git_branch_prefix() -> String {
    "prepcook".to_string()
}

fn default_pr_auto_description_enabled() -> bool {
    true
}

fn default_commit_reminder_enabled() -> bool {
    true
}

pub(super) fn default_jira_type_mapping() -> BTreeMap<String, WorkClass> {
    BTreeMap::from([
        ("Bug".to_string(), WorkClass::Bug),
        ("Story".to_string(), WorkClass::Story),
        ("Task".to_string(), WorkClass::Story),
        ("Small Enhancement Request".to_string(), WorkClass::Story),
        ("Epic".to_string(), WorkClass::Epic),
        ("Sub-task".to_string(), WorkClass::Subtask),
    ])
}

/// Jira Cloud connection settings for the issue importer. Auth is Basic
/// (base64 of `email:api_token`) against `https://{site}`.
///
/// The string fields are nullable on the wire (the settings UI persists
/// cleared inputs as `null`); team defaults are applied at use time via the
/// accessor methods.
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct JiraConfig {
    /// Jira Cloud host, e.g. "example.atlassian.net".
    #[serde(default)]
    pub site: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub api_token: Option<String>,
    #[serde(default)]
    pub project_key: Option<String>,
    /// JQL used when the caller doesn't supply one. `{project_key}` is
    /// substituted at use time.
    #[serde(default)]
    pub default_jql: Option<String>,
    /// Maps Jira issue-type names to the internal work classification.
    /// Unknown types fall back to `WorkClass::Story` at use time.
    #[serde(default = "default_jira_type_mapping")]
    pub type_mapping: BTreeMap<String, WorkClass>,
    /// Atlassian account id → display name for the people who may approve an
    /// RCA from a Jira comment.
    ///
    /// Empty means nobody, and that is the safe default rather than an
    /// oversight: treating "unconfigured" as "anyone" would hand the decision
    /// to whoever happens to be on the ticket, and on a BUG ticket that
    /// includes the person who reported the bug.
    ///
    /// Chosen by hand from the project's assignable users because Jira cannot
    /// answer "is this person an engineer" for this site. Project roles would
    /// say so and are unreadable — the app's account is not a project admin,
    /// and `GET /project/{key}/role` answers 401. Groups are readable and say
    /// nothing: every human here is in `jira-software-users`, engineers
    /// and account managers alike, and one engineer is not in it at all.
    ///
    /// The account id is what authority is checked against. The display name
    /// is a convenience for the settings screen and the audit comment — it is
    /// not unique and its owner can change it.
    #[serde(default)]
    pub rca_approvers: BTreeMap<String, String>,
}

impl Default for JiraConfig {
    fn default() -> Self {
        Self {
            site: None,
            email: None,
            api_token: None,
            project_key: None,
            default_jql: None,
            type_mapping: default_jira_type_mapping(),
            rca_approvers: BTreeMap::new(),
        }
    }
}

/// A v9 config carries no playbook defaults of its own: the texts it shipped
/// with are not kept, and the v10 migration fills every missing key from the
/// current defaults.
pub(super) fn default_playbooks() -> BTreeMap<String, String> {
    BTreeMap::new()
}

/// Orchestrator mode: `Off` disables background automation entirely; `Auto`
/// polls remote projects and launches stage playbooks for eligible issues.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum OrchestratorMode {
    #[default]
    Off,
    Auto,
}

pub(super) fn default_poll_interval_secs() -> u64 {
    15
}

/// Autonomy mode for the teammate runner: `Full` lets the orchestrator
/// drive high-confidence tickets through the human gates itself
/// (self-approve passing plans, advance idle Executing tickets, act on
/// reviewer verdicts). `Off` keeps every gate human-operated.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum AutonomyMode {
    #[default]
    Off,
    Full,
}

pub(super) fn default_autonomy_review_iterations() -> u32 {
    3
}

/// Per-project defaults the orchestrator uses when auto-launching playbook
/// workspaces, plus that board's overrides for otherwise-global settings.
///
/// One runner drives several boards (the app, extensions, …), each with its
/// own repo, Jira filter, Slack channel and reviewers. Everything
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
    /// Short board name (e.g. "extensions"), used for the RCA `board:` tag
    /// and Slack context lines. Unset leaves both unlabelled.
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub jira: ProjectJiraOverride,
    #[serde(default)]
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
    /// RCA-graph investigation threshold for this board; smaller boards need
    /// a lower bar than the global default to ever fire one.
    #[serde(default)]
    pub investigation_threshold: Option<u32>,
}

/// Board-specific Jira watcher settings. Site, credentials and issue-type
/// mapping stay global — one Atlassian account serves every board — so only
/// the selection of issues differs here.
#[derive(Clone, Debug, Default, Serialize, Deserialize, TS)]
pub struct ProjectJiraOverride {
    /// Whether the watcher polls Jira for this board. `None` inherits the
    /// legacy single-target behaviour (`jira_watcher.enabled` applied to
    /// `jira_watcher.project_id`).
    #[serde(default)]
    pub watcher_enabled: Option<bool>,
    /// JQL fragment selecting this board's issues, with `{project_key}`
    /// substituted at use time. `None` inherits `jira_watcher.jql_filter`.
    /// Boards sharing a Jira project distinguish themselves here (by
    /// component, label, or module).
    #[serde(default)]
    pub jql_filter: Option<String>,
    /// Jira project key when this board's tickets live in a different
    /// project; `None` uses the global key.
    #[serde(default)]
    pub project_key: Option<String>,
    /// JQL selecting tickets assigned to the orchestrator's Jira account for
    /// this board (assignment = "start this ticket"). Unset or blank inherits
    /// `jira_watcher.assigned_jql` — the Settings inputs cannot tell a
    /// cleared field from an untouched one, so blank must not carry meaning.
    /// Boards sharing a Jira project add their routing here (component,
    /// label) exactly as in `jql_filter`; only watcher-enabled boards are
    /// polled at all.
    #[serde(default)]
    pub assigned_jql: Option<String>,
}

/// Board-specific Slack routing. Unset fields inherit the globals, so one
/// board can post to its own channel while the rest share the default.
#[derive(Clone, Debug, Default, Serialize, Deserialize, TS)]
pub struct ProjectSlackOverride {
    #[serde(default)]
    pub webhook_url: Option<String>,
    #[serde(default)]
    pub bot_token: Option<String>,
    #[serde(default)]
    pub channel: Option<String>,
    /// `Some(false)` silences Slack for this board without clearing the
    /// global webhook every other board relies on.
    #[serde(default)]
    pub enabled: Option<bool>,
}

/// Board-specific merge-request behaviour. Reviewer maps merge over the
/// global one (board entries win) and the exclusion/bot lists append to it,
/// so a board records only what is different about its repo's contributors.
#[derive(Clone, Debug, Default, Serialize, Deserialize, TS)]
pub struct ProjectMrOverride {
    #[serde(default)]
    pub reviewer_map: BTreeMap<String, String>,
    #[serde(default)]
    pub reviewer_exclude_emails: Vec<String>,
    #[serde(default)]
    pub auto_assign_reviewers: Option<bool>,
    #[serde(default)]
    pub bot_logins: Vec<String>,
}

/// Board override for the retention sweeps (Deployed → Clean-up, and
/// Clean-up deletion). A plain `Option<u32>` cannot express "off here but
/// on globally", hence the explicit third state.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RetentionOverride {
    /// Use `orchestrator.deployed_retention_days`.
    #[default]
    Inherit,
    /// No sweep for this board.
    Off,
    /// Sweep after this many days in Deployed.
    Days(u32),
}

/// Resolved merge-request settings for one board.
#[derive(Clone, Debug)]
pub struct EffectiveMrSettings {
    pub reviewer_map: BTreeMap<String, String>,
    pub reviewer_exclude_emails: Vec<String>,
    pub auto_assign_reviewers: bool,
    pub bot_logins: Vec<String>,
}

/// A board the Jira watcher polls, with the filter that selects its issues.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WatchTarget {
    pub project_id: Uuid,
    pub jira_project_key: String,
    /// Ready-to-use JQL fragment (no `{project_key}` placeholder left).
    pub jql: String,
    /// Ready-to-use assigned-pickup JQL; `None` = feature off for the board.
    pub assigned_jql: Option<String>,
}

/// Observability integrations exposed to the coding agent (Sentry via its
/// stdio MCP server, Scalyr via its CLI). Tokens set here override the
/// same-named runner environment variables (populated from Secrets Manager);
/// leave them `None` to rely on the environment. Non-secret region/host
/// fields let a single build target either SaaS or self-hosted endpoints.
pub(super) fn integrations_default_true() -> bool {
    true
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, TS)]
pub struct IntegrationsConfig {
    /// git author email -> GitLab username, for blame-based MR reviewer
    /// auto-assignment. Emails found in blame but absent here are suggested
    /// in an MR note instead of assigned (GitLab's API cannot resolve users
    /// by email without admin scope).
    #[serde(default)]
    pub gitlab_reviewer_map: std::collections::BTreeMap<String, String>,
    /// Extra author emails to exclude from reviewer picks, on top of the
    /// built-in exclusions (the agent's own bot email pattern).
    #[serde(default)]
    pub gitlab_reviewer_exclude_emails: Vec<String>,
    /// Assign blame-picked reviewers on orchestrator-created MRs.
    #[serde(default = "integrations_default_true")]
    pub gitlab_auto_assign_reviewers: bool,
    /// GitLab logins whose MR comments the Push MR watcher ignores, on top
    /// of the built-in `_bot` filter (project access tokens comment as
    /// `project_<id>_bot_<hash>`).
    #[serde(default)]
    pub gitlab_bot_logins: Vec<String>,
    /// Sentry user auth token for the `@sentry/mcp-server` stdio server.
    /// Overrides `SENTRY_ACCESS_TOKEN` from the runner env when set.
    #[serde(default)]
    pub sentry_access_token: Option<String>,
    /// Hostname of a self-hosted Sentry (e.g. `sentry.example.com`). `None`
    /// targets Sentry SaaS (sentry.io).
    #[serde(default)]
    pub sentry_host: Option<String>,
    /// Scalyr "Read Logs" API token for the `scalyr` CLI. The CLI reads it
    /// from the `scalyr_readlog_token` env var on the runner; setting it here
    /// records it for reference and the settings UI.
    #[serde(default)]
    pub scalyr_read_token: Option<String>,
    /// Scalyr server/region base URL (e.g. `https://app.scalyr.com`, or the
    /// EU endpoint). `None` uses the CLI's default (US).
    #[serde(default)]
    pub scalyr_server: Option<String>,
}

/// Jira watcher: polls Jira for newly created high-priority bugs and
/// imports them straight into the Plan column, assigned to the current
/// user, so the orchestrator picks them up. Designed for the always-on
/// teammate runner; disabled by default.
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct JiraWatcherConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_watcher_poll_secs")]
    pub poll_secs: u64,
    /// JQL fragment selecting which new issues to pick up. Combined at use
    /// time with a created-recently window; `{project_key}` is substituted
    /// from the Jira config.
    #[serde(default)]
    pub jql_filter: Option<String>,
    /// Remote (VK) project UUID the watcher imports into.
    #[serde(default)]
    pub project_id: Option<Uuid>,
    /// JQL for assigned pickup: tickets assigned to the orchestrator's Jira
    /// account are imported and started regardless of priority — assignment
    /// is a stronger intent signal than any filter. Polled on `updated` (an
    /// assignment bumps `updated`, never `created`), so it cannot ride
    /// `jql_filter`'s created-based watermark. `None` disables it.
    #[serde(default)]
    pub assigned_jql: Option<String>,
}

fn default_watcher_poll_secs() -> u64 {
    45
}

impl Default for JiraWatcherConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            poll_secs: default_watcher_poll_secs(),
            jql_filter: None,
            project_id: None,
            assigned_jql: None,
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
    /// RCA-graph investigation threshold: a node with this many linked RCA
    /// documents (and no investigation story yet) fires a P2
    /// `[Investigation]` Jira story imported into To Do. Re-arms after this
    /// many NEW edges accumulate past the fire point.
    #[serde(default = "default_investigation_threshold")]
    pub investigation_threshold: u32,
    /// Delete workspace records whose worktree is already gone and which have
    /// been idle this many days. Nothing else prunes them: cleanup removes the
    /// worktree and marks the row, but the row (with its sessions, execution
    /// processes, logs and turns) used to live for ever — 87 had accumulated on
    /// the runner by 2026-08-06, slowing every board query. `None` disables the
    /// sweep; pinned workspaces are never touched.
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
    /// escalating to Slack and leaving the ticket for a human.
    #[serde(default = "default_autonomy_review_iterations")]
    pub autonomy_review_iterations: u32,
    /// Local time of day, `HH:MM`, at which the engineers' digest is posted
    /// to each board's Slack route. `None` — and any value that does not
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
}

pub(super) fn default_max_launches_per_tick() -> u32 {
    3
}

pub(super) fn default_workspace_retention_days() -> Option<u32> {
    Some(30)
}

pub(super) fn default_investigation_threshold() -> u32 {
    5
}

impl Default for OrchestratorConfig {
    fn default() -> Self {
        Self {
            mode: OrchestratorMode::Off,
            poll_interval_secs: default_poll_interval_secs(),
            deployed_retention_days: None,
            cleanup_retention_days: None,
            investigation_threshold: default_investigation_threshold(),
            max_launches_per_tick: default_max_launches_per_tick(),
            workspace_retention_days: default_workspace_retention_days(),
            autonomy: AutonomyMode::Off,
            autonomy_review_iterations: default_autonomy_review_iterations(),
            digest_at_local: None,
            eob_at_local: None,
            daily_timezone: None,
            project_defaults: BTreeMap::new(),
        }
    }
}

/// Reusable-worktree pool settings (treehouse-style). Disabled by default;
/// when enabled, workspace worktrees are acquired from / returned to a warm
/// pool instead of being created and destroyed per task.
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct WorktreePoolConfig {
    #[serde(default)]
    pub enabled: bool,
    /// Maximum worktrees kept per repository pool.
    #[serde(default = "default_max_trees")]
    pub max_trees: u32,
    /// Pool root directory; empty uses the default location.
    #[serde(default)]
    pub root: Option<String>,
}

fn default_max_trees() -> u32 {
    8
}

impl Default for WorktreePoolConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_trees: default_max_trees(),
            root: None,
        }
    }
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
    #[serde(default)]
    pub jira: JiraConfig,
    /// Slack incoming-webhook URL for orchestrator notifications
    /// (low-confidence escalations, autonomy failures). `None` disables
    /// Slack notifications.
    #[serde(default)]
    pub slack_webhook_url: Option<String>,
    /// Slack bot token (`xoxb-…`). When set together with `slack_channel`,
    /// planning escalations post via the Web API (chat.postMessage) so the
    /// orchestrator can poll the escalation thread for teammate replies and
    /// feed them back to the blocked agent. When unset, escalations fall
    /// back to `slack_webhook_url` (post-only, no reply polling).
    #[serde(default)]
    pub slack_bot_token: Option<String>,
    /// Slack channel id (e.g. `C0123456`) escalations post to when
    /// `slack_bot_token` is set.
    #[serde(default)]
    pub slack_channel: Option<String>,
    /// Stage playbooks keyed by "{work_class}:{stage}"; see default_playbooks.
    #[serde(default = "default_playbooks")]
    pub playbooks: BTreeMap<String, String>,
    #[serde(default)]
    pub worktree_pool: WorktreePoolConfig,
    #[serde(default)]
    pub orchestrator: OrchestratorConfig,
    #[serde(default)]
    pub jira_watcher: JiraWatcherConfig,
    /// Observability integrations (Sentry MCP, Scalyr CLI) for agent sessions.
    #[serde(default)]
    pub integrations: IntegrationsConfig,
}

impl Config {
    fn from_v8_config(old_config: v8::Config) -> Self {
        Self {
            config_version: "v9".to_string(),
            theme: old_config.theme,
            executor_profile: old_config.executor_profile,
            disclaimer_acknowledged: old_config.disclaimer_acknowledged,
            onboarding_acknowledged: old_config.onboarding_acknowledged,
            remote_onboarding_acknowledged: old_config.remote_onboarding_acknowledged,
            notifications: old_config.notifications,
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
            jira: JiraConfig::default(),
            slack_webhook_url: None,
            slack_bot_token: None,
            slack_channel: None,
            playbooks: default_playbooks(),
            worktree_pool: WorktreePoolConfig::default(),
            orchestrator: OrchestratorConfig::default(),
            jira_watcher: JiraWatcherConfig::default(),
            integrations: IntegrationsConfig::default(),
        }
    }

    pub fn from_previous_version(raw_config: &str) -> Result<Self, Error> {
        let old_config = v8::Config::from(raw_config.to_string());
        Ok(Self::from_v8_config(old_config))
    }
}

impl From<String> for Config {
    fn from(raw_config: String) -> Self {
        if let Ok(config) = serde_json::from_str::<Config>(&raw_config)
            && config.config_version == "v9"
        {
            return config;
        }

        match Self::from_previous_version(&raw_config) {
            Ok(config) => {
                tracing::info!("Config upgraded to v9");
                config
            }
            Err(e) => {
                tracing::warn!("Config migration failed: {}, using default", e);
                Self::default()
            }
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            config_version: "v9".to_string(),
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
            jira: JiraConfig::default(),
            slack_webhook_url: None,
            slack_bot_token: None,
            slack_channel: None,
            playbooks: default_playbooks(),
            worktree_pool: WorktreePoolConfig::default(),
            orchestrator: OrchestratorConfig::default(),
            jira_watcher: JiraWatcherConfig::default(),
            integrations: IntegrationsConfig::default(),
        }
    }
}
