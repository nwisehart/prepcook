//! Tracker watcher: polls the configured tracker for newly created
//! high-priority tickets (or, in incidents mode, open incidents) and imports
//! them straight into the **Plan** column, assigned to the current user, so
//! the stage orchestrator fires the planning playbook on its next tick.
//!
//! Tracker-agnostic: every question it asks is a [`Query`] the provider turns
//! into its own language (JQL for Jira, label terms for GitHub Issues,
//! incident filters for PagerDuty), and a provider without the notion — an
//! incident does not reopen — answers with nothing.
//!
//! Designed for the always-on teammate runner. Polling was chosen over
//! webhooks: a hosted tracker cannot POST into a VPN, and a 30-60s trigger
//! latency is noise against a 10-30 minute RCA.
//!
//! Several boards can be watched at once (see `Config::watch_targets`), each
//! with its own filter — boards sharing a tracker project separate by
//! component, label or module — and each importing into its own kanban
//! project.
//!
//! Dedup is two-layered: a per-board high-water mark on the ticket's
//! `created` timestamp (survives restarts), plus the key dedup against issues
//! already imported into that board.
//!
//! The reopen sweep (`reopened_tick`) watches for tickets whose status went
//! back to the To Do category: a board card parked in a terminal column comes
//! back to To Do (or straight to Plan when the ticket is assigned to the
//! agent's tracker account), with the stale claim marker stripped so
//! automation can fire again. The To Do category is the whole test — a ticket moving
//! forward through working statuses (In Progress → Merged) is not a reopen,
//! however finished its card looks. Cards mid-pipeline are left alone, as are
//! cards with a session still running (the claim marker this strips is what
//! keeps a second agent off the ticket). A ticket with no card at all is left
//! to assigned pickup, which imports it.
//!
//! The mention sweep (`mentions_tick`) covers the one case the orchestrator
//! cannot: a ticket that @-mentions the agent but has no card at all. Those
//! are imported to To Do so the question is visible on the board; mentions on
//! tickets already imported belong to the orchestrator, which hands them to
//! the agent working the ticket.
//!
//! Assigned pickup is a sibling poll (`assigned_tick`): tickets assigned to
//! the orchestrator's own tracker account start automatically, whatever their
//! priority — assignment is explicit human intent. It watermarks on
//! `updated`, because assigning bumps `updated` and never `created`, so it
//! cannot ride the main poll. Tickets already on the board that are still
//! parked in To Do are advanced to Plan instead of re-imported; the forward
//! move arms the same claim machinery as a manual drop.
//!
//! Restarting *finished* work stays with the reopen sweep, not with assignment:
//! launching planning both transitions the ticket to In Progress and assigns it
//! to the agent, so "assigned to the agent" and "in a working status" are the
//! resting state of everything the agent has ever shipped and neither can carry
//! a human's intent (see `assigned_action`). A post-merge follow-up therefore
//! needs the tracker status moved back to To Do as well; when only half that
//! gesture arrives, `stuck_followup_hint` says which half is missing rather
//! than leaving the ticket silently parked.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, LazyLock, Mutex},
    time::Duration,
};

use api_types::{
    CreateIssueAssigneeRequest, CreateIssueRequest, CreateIssueTagRequest, Issue, IssueExtMetadata,
    IssuePriority, JiraRef, SearchIssuesRequest, UpdateIssueRequest, WorkClass,
};
use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::services::{
    config::{Config, TrackerConfig, WatchTarget},
    daily,
    remote_client::RemoteClient,
    reopen,
    tracker::{self, FetchedIssue, Query, Tracker, TrackerSource},
};

/// The tracker and its settings as one pass of the watcher reads them.
struct Tracking {
    tracker: Arc<dyn Tracker>,
    settings: TrackerConfig,
}

/// Only issues created within this window are considered, regardless of the
/// high-water mark — a long-stopped watcher must not import week-old bugs
/// nobody expects to be picked up automatically.
fn lookback() -> chrono::Duration {
    chrono::Duration::hours(24)
}

/// Window for the reopen sweep. Wider than [`lookback`]: a reopen over a
/// weekend should still be caught on Monday, and the sweep is idempotent
/// (an already-restored card is skipped), so a wide window costs nothing.
fn reopen_lookback() -> chrono::Duration {
    chrono::Duration::days(3)
}

/// Persisted watcher state (config asset dir, survives restarts).
#[derive(Debug, Default, Serialize, Deserialize)]
struct WatcherState {
    /// Newest `created` timestamp imported before per-board marks existed.
    /// Retained as the seed for a board that has no mark yet, so adding
    /// boards (or upgrading) never re-imports the last day of tickets.
    #[serde(default)]
    last_created: Option<DateTime<FixedOffset>>,
    /// Per-board high-water marks keyed by project UUID. Boards are polled
    /// with different filters, so one board's newest ticket must not
    /// suppress an older ticket another board hasn't seen.
    #[serde(default)]
    boards: BTreeMap<String, DateTime<FixedOffset>>,
    /// Assigned-pickup marks, on the ticket's `updated` timestamp (see module
    /// docs). Separate from `boards`: the two polls advance independently.
    #[serde(default)]
    assigned: BTreeMap<String, DateTime<FixedOffset>>,
    /// Reopen-sweep marks, also on `updated`.
    #[serde(default)]
    reopened: BTreeMap<String, DateTime<FixedOffset>>,
    /// Mention-sweep marks, also on `updated`.
    #[serde(default)]
    mentioned: BTreeMap<String, DateTime<FixedOffset>>,
}

impl WatcherState {
    fn mark(&self, project_id: Uuid) -> Option<DateTime<FixedOffset>> {
        self.boards
            .get(&project_id.to_string())
            .copied()
            .or(self.last_created)
    }

    /// Records `created` as seen for this board; returns whether it advanced
    /// the mark (and therefore needs persisting).
    fn advance(&mut self, project_id: Uuid, created: DateTime<FixedOffset>) -> bool {
        let key = project_id.to_string();
        if self.boards.get(&key).is_some_and(|mark| *mark >= created) {
            return false;
        }
        self.boards.insert(key, created);
        true
    }

    fn assigned_mark(&self, project_id: Uuid) -> Option<DateTime<FixedOffset>> {
        self.assigned.get(&project_id.to_string()).copied()
    }

    fn advance_assigned(&mut self, project_id: Uuid, updated: DateTime<FixedOffset>) -> bool {
        let key = project_id.to_string();
        if self.assigned.get(&key).is_some_and(|mark| *mark >= updated) {
            return false;
        }
        self.assigned.insert(key, updated);
        true
    }

    fn reopened_mark(&self, project_id: Uuid) -> Option<DateTime<FixedOffset>> {
        self.reopened.get(&project_id.to_string()).copied()
    }

    fn advance_reopened(&mut self, project_id: Uuid, updated: DateTime<FixedOffset>) -> bool {
        let key = project_id.to_string();
        if self.reopened.get(&key).is_some_and(|mark| *mark >= updated) {
            return false;
        }
        self.reopened.insert(key, updated);
        true
    }

    fn mentioned_mark(&self, project_id: Uuid) -> Option<DateTime<FixedOffset>> {
        self.mentioned.get(&project_id.to_string()).copied()
    }

    fn advance_mentioned(&mut self, project_id: Uuid, updated: DateTime<FixedOffset>) -> bool {
        let key = project_id.to_string();
        if self
            .mentioned
            .get(&key)
            .is_some_and(|mark| *mark >= updated)
        {
            return false;
        }
        self.mentioned.insert(key, updated);
        true
    }
}

fn state_path() -> PathBuf {
    utils::assets::asset_dir().join("tracker_watcher_state.json")
}

/// The name older installs keep the marks under; read while the current
/// file does not exist yet, so an upgrade keeps them.
fn legacy_state_path() -> PathBuf {
    utils::assets::asset_dir().join("jira_watcher_state.json")
}

fn load_state() -> WatcherState {
    std::fs::read_to_string(state_path())
        .or_else(|_| std::fs::read_to_string(legacy_state_path()))
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn save_state(state: &WatcherState) {
    if let Ok(raw) = serde_json::to_string_pretty(state)
        && let Err(error) = std::fs::write(state_path(), raw)
    {
        warn!(?error, "tracker watcher: failed to persist state");
    }
}

/// Maps the team's Jira priority names to board priorities (same convention as
/// the server importer): P0 → Urgent, P1 → High, P2 → Medium, P3 → Low.
fn priority_for(name: Option<&str>) -> Option<IssuePriority> {
    match name?.trim().to_ascii_uppercase().as_str() {
        "P0" => Some(IssuePriority::Urgent),
        "P1" => Some(IssuePriority::High),
        "P2" => Some(IssuePriority::Medium),
        "P3" => Some(IssuePriority::Low),
        _ => None,
    }
}

/// Board priority for an imported ticket: [`priority_for`], except that a
/// follow-up dispatches Urgent whatever its Jira priority says.
///
/// The two scales are deliberately decoupled here. A follow-up caps at P1 in
/// Jira so one incident is counted once in the blocker rate, but the work
/// itself is a shipped fix that did not hold and should not queue behind
/// tickets nobody has been promised yet. Board priority orders the launch
/// queue (`stage_orchestrator::dispatch_rank`) and is invisible to Jira
/// reporting, so it is the right place to say "go first".
fn board_priority_for(priority_name: Option<&str>, labels: &[String]) -> Option<IssuePriority> {
    if labels
        .iter()
        .any(|label| label.eq_ignore_ascii_case(reopen::FOLLOWUP_LABEL))
    {
        return Some(IssuePriority::Urgent);
    }
    priority_for(priority_name)
}

/// Whether a board column is the To Do stage (alias-aware, mirroring
/// `workflow.rs` stage 1: the names this fork has shipped plus upstream's).
fn is_todo_status(name: &str) -> bool {
    let name = name.trim();
    ["to do", "todo", "unassigned", "backlog"]
        .iter()
        .any(|alias| name.eq_ignore_ascii_case(alias))
}

/// Whether a board column is terminal (done work): Close/Deployed aliases
/// plus the Clean-up aliases.
///
/// This is the *board's* vocabulary and answers only where a card got to.
/// Whether the team recorded the work as shipped is a separate question with a
/// separate answer (`tracker.status_mapping.shipped`), and the two disagree in both
/// directions: a card waits in Push MR while a human merges, and reaches
/// Deployed whether or not anyone moved the Jira ticket past In Progress.
pub(crate) fn is_terminal_status(name: &str) -> bool {
    let name = name.trim();
    [
        "close", "closed", "deployed", "clean up", "clean-up", "cleanup",
    ]
    .iter()
    .any(|alias| name.eq_ignore_ascii_case(alias))
}

/// Whether a Jira status category means "back at the start", which is what a
/// reopen looks like: the ticket left a done status and landed in To Do or
/// Reopened.
///
/// The `indeterminate` category is deliberately excluded. It covers every
/// working status — In Progress, In Review, Merged — so accepting it makes a
/// forward transition indistinguishable from a reopen. The cost is that a
/// ticket reopened directly into a working status is not picked up at all —
/// assigned pickup skips working statuses too, and for the same reason (see
/// [`assigned_action`]). The status has to come back to To Do;
/// [`needs_followup_hint`] spots the tickets waiting on exactly that.
///
/// `None` (Jira omitted the category) is treated as not-a-reopen: restarting
/// finished work is the expensive mistake, and a missed reopen is one
/// assignment away from being fixed.
fn is_reopened_category(category: Option<&str>) -> bool {
    category == Some("new")
}

/// Where a board card sits, as far as assigned pickup is concerned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CardColumn {
    /// Waiting to be started.
    Todo,
    /// Finished: a Deployed/Clean-up alias (see [`is_terminal_status`]).
    Terminal,
    /// Anywhere between the two — planning, executing, reviewing, pushing.
    Active,
}

/// What assigning a ticket to the agent should do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AssignedAction {
    /// Not on the board: import it straight into Plan.
    Import,
    /// Parked in To Do: advance it to Plan.
    Start,
    /// Leave it alone.
    Skip,
}

/// Decides assigned pickup for one ticket: only an unstarted ticket whose card
/// is idle acts.
///
/// Neither the assignee nor a working status can widen this, which is worth
/// spelling out because both look like they should. Launching planning
/// transitions the ticket to In Progress **and** assigns it to the agent
/// (`stage_orchestrator`), and nothing ever undoes either — so every ticket the
/// agent has shipped comes to rest as "In Progress, assigned to the agent, card
/// in Deployed/Clean-up". Treating that as "do it again" would re-plan merged
/// work on the next comment. Restarting finished work therefore stays with the
/// reopen sweep, which requires a Jira status change into the To Do category
/// inside its window — a gesture only a human can make.
///
/// The `done` category belongs to the reopen sweep for the same reason. A
/// missing category acts, matching what this poll has always done: Jira omits
/// it rarely, and an unimported ticket assigned to the agent is worth having on
/// the board either way.
fn assigned_action(category: Option<&str>, card: Option<CardColumn>) -> AssignedAction {
    match (category, card) {
        (Some("new") | None, None) => AssignedAction::Import,
        (Some("new") | None, Some(CardColumn::Todo)) => AssignedAction::Start,
        _ => AssignedAction::Skip,
    }
}

/// Whether a ticket is one gesture short of a follow-up restart: finished card,
/// assigned to the agent, but the status never came back to To Do so no sweep
/// will act (see [`assigned_action`]).
///
/// Only tickets with a finished card qualify. A ticket with no card cannot be
/// told apart from one the board never imported — the reopen sweep learned the
/// same lesson — and telling someone their retired ticket needs moving when the
/// agent never touched it is worse than saying nothing. `done` is excluded
/// because the reopen sweep handles closed tickets, and the To Do category
/// because those already act.
fn needs_followup_hint(
    category: Option<&str>,
    column: Option<CardColumn>,
    assignee: Option<&str>,
    agent: Option<&str>,
) -> bool {
    category == Some("indeterminate")
        && column == Some(CardColumn::Terminal)
        && agent.is_some()
        && assignee == agent
}

/// Which of the three column groups a board status name falls into.
fn classify_column(name: &str) -> CardColumn {
    if is_todo_status(name) {
        CardColumn::Todo
    } else if is_terminal_status(name) {
        CardColumn::Terminal
    } else {
        CardColumn::Active
    }
}

/// A status-only issue PATCH (all other fields untouched).
fn status_update(status_id: Uuid) -> UpdateIssueRequest {
    UpdateIssueRequest {
        status_id: Some(status_id),
        title: None,
        description: None,
        priority: None,
        start_date: None,
        target_date: None,
        completed_at: None,
        sort_order: None,
        parent_issue_id: None,
        parent_issue_sort_order: None,
        extension_metadata: None,
    }
}

/// Why the watcher has nothing to poll, or `None` when it polls. No tracker
/// comes first: without one, every card is made on the board and there is
/// nothing anywhere to pick up, whatever the boards are set to.
fn watcher_idle_reason(
    tracker: &dyn Tracker,
    no_board_enabled: bool,
    remote_connected: bool,
) -> Option<String> {
    if !tracker.is_configured() {
        Some(format!(
            "{} is not configured (Settings > Ticketing); cards are made on the board",
            tracker.kind()
        ))
    } else if no_board_enabled {
        Some("no board enabled (Settings > Orchestrator > Ticket pickup)".to_string())
    } else if !remote_connected {
        Some("remote client not configured".to_string())
    } else {
        None
    }
}

/// Tracker keys already recorded in `extension_metadata` across the
/// project's issues (same dedup the importer route uses). `None` when the search
/// failed part-way: a partial answer here is what turns one hiccup into a
/// duplicate import, so callers skip the cycle instead.
async fn existing_jira_keys(client: &RemoteClient, project_id: Uuid) -> Option<HashSet<String>> {
    existing_issues_by_key(client, project_id)
        .await
        .map(|issues| issues.into_keys().collect())
}

/// Like [`existing_jira_keys`], but keeps the issues: assigned pickup needs
/// the board card (status, metadata) to decide between import and advance.
pub(crate) async fn existing_issues_by_key(
    client: &RemoteClient,
    project_id: Uuid,
) -> Option<HashMap<String, Issue>> {
    const PAGE_SIZE: i32 = 500;
    let mut keys = HashMap::new();
    let mut offset: i32 = 0;

    loop {
        let response = match client
            .search_issues(&SearchIssuesRequest {
                project_id,
                status_id: None,
                status_ids: None,
                priority: None,
                parent_issue_id: None,
                search: None,
                simple_id: None,
                assignee_user_id: None,
                tag_id: None,
                tag_ids: None,
                sort_field: None,
                sort_direction: None,
                limit: Some(PAGE_SIZE),
                offset: Some(offset),
            })
            .await
        {
            Ok(response) => response,
            Err(error) => {
                warn!(?error, "tracker watcher: issue search failed during dedup");
                return None;
            }
        };

        let fetched = response.issues.len();
        for issue in response.issues {
            let metadata = IssueExtMetadata::from_value(&issue.extension_metadata);
            if let Some(jira) = metadata.jira {
                keys.insert(jira.key, issue);
            }
        }

        offset += fetched as i32;
        if fetched == 0 || (offset as usize) >= response.total_count {
            break;
        }
    }

    Some(keys)
}

/// True the first time this process sees a `(kind, key)` pair. Lets a condition
/// that repeats every poll be reported once at a level people read, without
/// turning one stuck ticket into a Slack or log flood.
fn first_time(kind: &str, key: &str) -> bool {
    static SEEN: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(Default::default);
    SEEN.lock().unwrap().insert(format!("{kind}:{key}"))
}

/// Records, for the morning digest, that a ticket assigned to the agent will
/// never start: its card has finished and the tracker status is still a
/// working one, so nothing picks it up.
///
/// Recorded only when a human spoke last on the thread. The trigger is an
/// `updated` bump, and the triage agent's own answers bump it too — reporting
/// "nothing will start" right after the agent answered a "thanks" would train
/// people to ignore the line. Once per ticket per process for the same reason,
/// counted *after* that decision: spending the budget on a bump the agent
/// caused would swallow the human reply that follows, which is the whole case
/// this exists for.
async fn stuck_followup_hint(tracking: &Tracking, issue: &FetchedIssue, agent_id: Option<&str>) {
    let comments = tracking.tracker.comments(&issue.key).await;
    let human_spoke_last = match &comments {
        Ok(comments) => comments
            .iter()
            .max_by_key(|comment| comment.created)
            .is_some_and(|newest| newest.author_account_id.as_deref() != agent_id),
        // Unread thread: say it rather than swallow it. A missing hint is the
        // failure this exists to prevent.
        Err(error) => {
            debug!(?error, "tracker watcher: hint could not read {}", issue.key);
            true
        }
    };
    if !human_spoke_last || !first_time("followup-hint", &issue.key) {
        return;
    }
    info!(
        "Tracker watcher: {} is assigned to the agent and its card has finished, but the ticket \
         is still in a working status — move it to To Do to restart it",
        issue.key
    );
    daily::update(|state| state.record_stuck_hint(&issue.key, &issue.summary));
}

/// Service that watches the tracker for new work (see module docs).
pub struct TrackerWatcherService {
    config: Arc<RwLock<Config>>,
    remote_client: Option<RemoteClient>,
    tracker: TrackerSource,
}

impl TrackerWatcherService {
    pub async fn spawn(
        config: Arc<RwLock<Config>>,
        remote_client: Option<RemoteClient>,
        tracker: TrackerSource,
    ) -> tokio::task::JoinHandle<()> {
        let service = Self {
            config,
            remote_client,
            tracker,
        };
        tokio::spawn(async move {
            service.start().await;
        })
    }

    async fn start(&self) {
        info!("Starting tracker watcher service");
        let mut state = load_state();
        // Idle reasons are logged once per state change, not every tick.
        let mut last_idle_reason: Option<String> = Some("(startup)".to_string());

        loop {
            // Each enabled board is polled with its own filter. Nothing here
            // sends a notification: what the watcher finds is either a to-do, which
            // the morning digest carries, or bookkeeping, which is a log line.
            let (targets, settings, poll_secs) = {
                let config = self.config.read().await;
                // First board per tracker project is "primary": boards sharing a
                // project would otherwise each announce the same retired
                // reopen (they cannot tell whose card it was).
                let mut seen_projects: HashSet<String> = HashSet::new();
                let targets: Vec<(WatchTarget, bool)> = config
                    .watch_targets()
                    .into_iter()
                    .map(|target| {
                        let primary = seen_projects.insert(target.jira_project_key.clone());
                        (target, primary)
                    })
                    .collect();
                (
                    targets,
                    config.ticketing.clone(),
                    config.ticketing.poll_secs,
                )
            };
            let tracking = Tracking {
                tracker: self.tracker.current().await,
                settings,
            };

            let idle_reason = watcher_idle_reason(
                tracking.tracker.as_ref(),
                targets.is_empty(),
                self.remote_client.is_some(),
            );

            match idle_reason {
                Some(reason) => {
                    if last_idle_reason.as_deref() != Some(reason.as_str()) {
                        info!("Tracker watcher idle: {reason}");
                        last_idle_reason = Some(reason);
                    }
                }
                None => {
                    if last_idle_reason.take().is_some() {
                        info!(
                            "Tracker watcher active: polling {} board(s) every {}s",
                            targets.len(),
                            poll_secs.max(1)
                        );
                        // The filter is the usual misconfiguration; log each
                        // board's resolved one so "active but importing
                        // nothing" is diagnosable from the log alone.
                        for (target, _) in &targets {
                            debug!(
                                project_id = %target.project_id,
                                tracker_project = %target.jira_project_key,
                                filter = %target.jql,
                                assigned_filter = ?target.assigned_jql,
                                "Tracker watcher board"
                            );
                        }
                    }
                    if let Some(client) = &self.remote_client {
                        for (target, primary) in &targets {
                            self.tick(client, target, &tracking, &mut state).await;
                            // Reopens run before assigned pickup: a restore that
                            // lands a card in To Do is inert until something
                            // advances it, and pickup can do that in the same
                            // pass. The other order advances pickup's mark past
                            // the ticket first, and moving a card does not bump
                            // the ticket's `updated`, so nothing would ever
                            // advance it.
                            self.reopened_tick(client, target, &tracking, &mut state)
                                .await;
                            self.assigned_tick(client, target, &tracking, &mut state)
                                .await;
                            self.mentions_tick(client, target, &tracking, *primary, &mut state)
                                .await;
                        }
                    }
                }
            }

            tokio::time::sleep(Duration::from_secs(poll_secs.max(1))).await;
        }
    }

    async fn tick(
        &self,
        client: &RemoteClient,
        target: &WatchTarget,
        tracking: &Tracking,
        state: &mut WatcherState,
    ) {
        let project_id = target.project_id;
        let query = Query::Created {
            filter: &target.jql,
            within: lookback(),
        };

        let fetched = match tracking.tracker.find(query).await {
            Ok(fetched) => fetched,
            Err(error) => {
                warn!(?error, "tracker watcher: search failed");
                return;
            }
        };

        // High-water mark: only issues created strictly after the newest one
        // already handled for this board. Issues without a parsable created
        // timestamp are handled once (key dedup catches replays).
        let mark = state.mark(project_id);
        let candidates: Vec<_> = fetched
            .into_iter()
            .filter(|issue| match (&issue.created, &mark) {
                (Some(created), Some(mark)) => created > mark,
                _ => true,
            })
            // Only fresh, not-yet-started work belongs in Plan.
            .filter(|issue| {
                issue
                    .status_category
                    .as_deref()
                    .is_none_or(|category| category == "new")
            })
            .collect();

        if candidates.is_empty() {
            return;
        }

        let Some(existing) = existing_jira_keys(client, project_id).await else {
            // Without the full dedup set, importing risks duplicates; the
            // next tick retries with the watermark unchanged.
            return;
        };

        // Plan column (alias-aware, mirroring the stage orchestrator).
        let statuses = match client.list_project_statuses(project_id).await {
            Ok(response) => response.project_statuses,
            Err(error) => {
                warn!(?error, "tracker watcher: could not list project statuses");
                return;
            }
        };
        let Some(plan_status) = statuses.iter().find(|status| {
            let name = status.name.trim();
            name.eq_ignore_ascii_case("plan") || name.eq_ignore_ascii_case("planning")
        }) else {
            warn!("tracker watcher: project {project_id} has no Plan column; skipping tick");
            return;
        };

        let me = client.profile().await.ok().map(|profile| profile.user_id);
        let project_tags = client
            .list_tags(project_id)
            .await
            .map(|response| response.tags)
            .unwrap_or_default();

        for issue in candidates {
            // Advance the mark even for skipped issues: they were seen.
            let advance_mark = |state: &mut WatcherState| {
                if let Some(created) = issue.created
                    && state.advance(project_id, created)
                {
                    save_state(state);
                }
            };

            if existing.contains(&issue.key) {
                advance_mark(state);
                continue;
            }

            let imported = self
                .import_one(
                    client,
                    project_id,
                    tracking,
                    &issue,
                    plan_status.id,
                    me,
                    &project_tags,
                )
                .await;
            if imported.is_err() {
                // Do NOT advance the mark: the next tick retries this one.
                return;
            }

            info!(
                "Tracker watcher: imported {} ({}) into Plan",
                issue.key,
                issue.priority_name.as_deref().unwrap_or("?")
            );
            advance_mark(state);
        }
    }

    /// Imports one ticket into the Plan column: metadata, comment thread,
    /// create, self-assign, work-class and area tags. An incident tool's
    /// ticket also records the incident as the card's origin. Logging of the
    /// outcome and any notification stay with the caller — the two polls
    /// announce differently. `Err` means the create itself failed (already
    /// logged); assignee/tag failures only warn, matching the importer
    /// route.
    #[allow(clippy::too_many_arguments)]
    async fn import_one(
        &self,
        client: &RemoteClient,
        project_id: Uuid,
        tracking: &Tracking,
        issue: &FetchedIssue,
        plan_status_id: Uuid,
        me: Option<Uuid>,
        project_tags: &[api_types::Tag],
    ) -> Result<Uuid, ()> {
        let work_class = tracking.settings.work_class_for(&issue.issue_type);
        let url = tracking.tracker.issue_url(&issue.key);
        let epic_key = issue.parent_key.clone().filter(|_| {
            issue.parent_type.as_deref().is_some_and(|parent_type| {
                tracking.settings.work_class_for(parent_type) == WorkClass::Epic
            })
        });

        let metadata = IssueExtMetadata {
            work_class: Some(work_class),
            jira: Some(JiraRef {
                key: issue.key.clone(),
                issue_type: Some(issue.issue_type.clone()),
                url: Some(url.clone()),
            }),
            epic_key,
            origin: tracker::origin_for(tracking.tracker.kind(), &url),
            ..Default::default()
        };

        // Pull the full comment thread so the planning agent starts with
        // the context teammates left on the ticket, not just the
        // description. Best-effort: a comment-fetch failure just imports
        // without them.
        let comments = tracking
            .tracker
            .comments(&issue.key)
            .await
            .unwrap_or_default();
        let comments_block = tracker::format_comments(&comments);
        let kind = tracking.tracker.kind();
        let description = if issue.description_text.is_empty() {
            format!("{kind}: {url}{comments_block}")
        } else {
            format!(
                "{}\n\n{kind}: {url}{comments_block}",
                issue.description_text
            )
        };

        let created = match client
            .create_issue(&CreateIssueRequest {
                id: None,
                project_id,
                status_id: plan_status_id,
                title: format!("{}: {}", issue.key, issue.summary),
                description: Some(description),
                priority: board_priority_for(issue.priority_name.as_deref(), &issue.labels),
                start_date: None,
                target_date: None,
                completed_at: None,
                sort_order: 0.0,
                parent_issue_id: None,
                parent_issue_sort_order: None,
                extension_metadata: serde_json::to_value(&metadata)
                    .unwrap_or_else(|_| serde_json::json!({})),
            })
            .await
        {
            Ok(created) => created,
            Err(error) => {
                warn!(?error, "tracker watcher: failed to import {}", issue.key);
                return Err(());
            }
        };

        if let Some(user_id) = me
            && let Err(error) = client
                .create_issue_assignee(&CreateIssueAssigneeRequest {
                    id: None,
                    issue_id: created.data.id,
                    user_id,
                })
                .await
        {
            warn!(?error, "tracker watcher: failed to assign {}", issue.key);
        }

        // Best-effort work-class and area tags, find-or-skip like the
        // importer: the board's tags are its owner's to create.
        let tag_names = std::iter::once(work_class.as_str()).chain(issue.area.as_deref());
        for tag_name in tag_names {
            if let Some(tag) = project_tags
                .iter()
                .find(|tag| tag.name.eq_ignore_ascii_case(tag_name))
                && let Err(error) = client
                    .create_issue_tag(&CreateIssueTagRequest {
                        id: None,
                        issue_id: created.data.id,
                        tag_id: tag.id,
                    })
                    .await
            {
                warn!(?error, "tracker watcher: failed to tag {}", issue.key);
            }
        }

        Ok(created.data.id)
    }

    /// Assigned pickup: tickets assigned to the orchestrator's tracker
    /// account (the board's assigned filter) are started — imported into Plan when new
    /// to the board, advanced To Do → Plan when already imported. See module
    /// docs for why this polls `updated` with its own watermark.
    async fn assigned_tick(
        &self,
        client: &RemoteClient,
        target: &WatchTarget,
        tracking: &Tracking,
        state: &mut WatcherState,
    ) {
        let Some(assigned_jql) = &target.assigned_jql else {
            return;
        };
        let project_id = target.project_id;
        let query = Query::Updated {
            filter: assigned_jql,
            within: lookback(),
        };

        let fetched = match tracking.tracker.find(query).await {
            Ok(fetched) => fetched,
            Err(error) => {
                warn!(?error, "tracker watcher: assigned search failed");
                return;
            }
        };

        let mark = state.assigned_mark(project_id);
        let candidates: Vec<_> = fetched
            .into_iter()
            .filter(|issue| match (&issue.updated, &mark) {
                (Some(updated), Some(mark)) => updated > mark,
                _ => true,
            })
            // Closed tickets are the reopen sweep's business. Working
            // categories reach `assigned_action` (which skips them) so the log
            // can name the one gesture a stuck follow-up is waiting for.
            .filter(|issue| issue.status_category.as_deref() != Some("done"))
            .collect();

        if candidates.is_empty() {
            return;
        }

        let Some(existing) = existing_issues_by_key(client, project_id).await else {
            return;
        };

        let statuses = match client.list_project_statuses(project_id).await {
            Ok(response) => response.project_statuses,
            Err(error) => {
                warn!(?error, "tracker watcher: could not list project statuses");
                return;
            }
        };
        let Some(plan_status) = statuses.iter().find(|status| {
            let name = status.name.trim();
            name.eq_ignore_ascii_case("plan") || name.eq_ignore_ascii_case("planning")
        }) else {
            warn!(
                "tracker watcher: project {project_id} has no Plan column; skipping assigned tick"
            );
            return;
        };

        let me = client.profile().await.ok().map(|profile| profile.user_id);
        let project_tags = client
            .list_tags(project_id)
            .await
            .map(|response| response.tags)
            .unwrap_or_default();
        // Best-effort, and only used to decide whether the stuck-follow-up
        // hint applies: without an identity the hint is skipped.
        let agent_id = tracking
            .tracker
            .identity()
            .await
            .map(|identity| identity.account_id)
            .ok();

        for issue in candidates {
            // Advance the mark even for skipped issues: they were seen, and
            // any later edit bumps `updated` past the mark again.
            let advance_mark = |state: &mut WatcherState| {
                if let Some(updated) = issue.updated
                    && state.advance_assigned(project_id, updated)
                {
                    save_state(state);
                }
            };

            let card = existing.get(&issue.key);
            let column = card.map(|card| {
                let name = statuses
                    .iter()
                    .find(|status| status.id == card.status_id)
                    .map(|status| status.name.as_str())
                    .unwrap_or_default();
                classify_column(name)
            });
            let action = assigned_action(issue.status_category.as_deref(), column);

            // A takeover pause is a human's explicit stop sign; it outranks
            // every action here.
            let taken_over = card.is_some_and(|card| {
                IssueExtMetadata::from_value(&card.extension_metadata)
                    .human_takeover
                    .is_some()
            });
            if taken_over || action == AssignedAction::Skip {
                if !taken_over
                    && needs_followup_hint(
                        issue.status_category.as_deref(),
                        column,
                        issue.assignee_account_id.as_deref(),
                        agent_id.as_deref(),
                    )
                {
                    stuck_followup_hint(tracking, &issue, agent_id.as_deref()).await;
                } else {
                    debug!(
                        "tracker watcher: {} assigned, no action ({:?}, column {:?}, takeover {})",
                        issue.key, action, column, taken_over
                    );
                }
                advance_mark(state);
                continue;
            }

            match action {
                // Handled above; the arm keeps the match exhaustive.
                AssignedAction::Skip => continue,
                AssignedAction::Start => {
                    let Some(card) = card else {
                        continue; // only reachable with a card
                    };
                    match client
                        .update_issue(card.id, &status_update(plan_status.id))
                        .await
                    {
                        Ok(_) => {
                            info!(
                                "Tracker watcher: {} assigned to the agent — moved to Plan",
                                issue.key
                            );
                            advance_mark(state);
                        }
                        Err(error) => {
                            // No mark advance: the next tick retries the move.
                            warn!(?error, "tracker watcher: failed to advance {}", issue.key);
                            return;
                        }
                    }
                }
                AssignedAction::Import => {
                    let imported = self
                        .import_one(
                            client,
                            project_id,
                            tracking,
                            &issue,
                            plan_status.id,
                            me,
                            &project_tags,
                        )
                        .await;
                    if imported.is_err() {
                        // No mark advance: the next tick retries the import.
                        return;
                    }
                    info!(
                        "Tracker watcher: {} assigned to the agent — imported into Plan",
                        issue.key
                    );
                    advance_mark(state);
                }
            }
        }
    }

    /// Reopen sweep: tickets whose status left the done category come
    /// back onto the board (see module docs). Restores are one PATCH: the
    /// new column plus extension metadata with the stale claim marker
    /// removed — the move is backward, so the route would keep the claim and
    /// automation would never re-fire.
    async fn reopened_tick(
        &self,
        client: &RemoteClient,
        target: &WatchTarget,
        tracking: &Tracking,
        state: &mut WatcherState,
    ) {
        let project_id = target.project_id;
        // Back at the start, not merely "not done": a ticket going In
        // Progress → Merged reads as "not done and just changed", and with its
        // card already parked in a terminal column that was enough to drag
        // merged work back to Plan. Each provider asks for the start.
        let query = Query::Reopened {
            project: &target.jira_project_key,
            within: reopen_lookback(),
        };

        let fetched = match tracking.tracker.find(query).await {
            Ok(fetched) => fetched,
            Err(error) => {
                warn!(?error, "tracker watcher: reopen search failed");
                return;
            }
        };

        let mark = state.reopened_mark(project_id);
        let candidates: Vec<_> = fetched
            .into_iter()
            .filter(|issue| match (&issue.updated, &mark) {
                (Some(updated), Some(mark)) => updated > mark,
                _ => true,
            })
            // Same guard as the query, enforced on the response: the filter is
            // the difference between restarting a reopened ticket and
            // restarting one that merely moved forward, and it must not depend
            // on a query string alone.
            .filter(|issue| is_reopened_category(issue.status_category.as_deref()))
            .collect();
        if candidates.is_empty() {
            return;
        }

        let Some(existing) = existing_issues_by_key(client, project_id).await else {
            return;
        };
        // Only to tell a terminal card from one already back in play. Nothing
        // here moves a card between columns any more.
        let statuses = match client.list_project_statuses(project_id).await {
            Ok(response) => response.project_statuses,
            Err(error) => {
                warn!(?error, "tracker watcher: could not list project statuses");
                return;
            }
        };

        // Set to a deferred candidate's `updated`: the mark is one timestamp per
        // board, so advancing it for any ticket at or past that instant would
        // carry it over the deferred one and lose the reopen for good. Candidates
        // arrive oldest-first, and a tracker hands several tickets the same
        // millisecond on a bulk transition, so the bound has to be inclusive.
        // Held back, the next sweep re-sees the tail, which is harmless: the
        // recorded follow-up key is what stops a second filing for one report.
        let mut hold_from: Option<DateTime<FixedOffset>> = None;

        for issue in candidates {
            let advance_mark = |state: &mut WatcherState, hold: Option<DateTime<FixedOffset>>| {
                if let Some(updated) = issue.updated
                    && hold.is_none_or(|deferred| updated < deferred)
                    && state.advance_reopened(project_id, updated)
                {
                    save_state(state);
                }
            };

            let Some(card) = existing.get(&issue.key) else {
                // No card. This sweep matches every ticket in the tracker
                // project whose status left the done category, and the vast
                // majority were never imported at all — so "the card was
                // retired" cannot be distinguished from "the agent never
                // touched this" and announcing it was wrong far more often
                // than it was right. Assigned pickup already covers the real
                // case: assign the agent and the ticket is imported.
                debug!("tracker watcher: {} reopened, not on the board", issue.key);
                advance_mark(state, hold_from);
                continue;
            };

            let terminal = statuses
                .iter()
                .any(|status| status.id == card.status_id && is_terminal_status(&status.name));
            if !terminal {
                // Already active somewhere on the board; the reopen changes
                // nothing here.
                advance_mark(state, hold_from);
                continue;
            }

            let metadata = IssueExtMetadata::from_value(&card.extension_metadata);
            if metadata.human_takeover.is_some() {
                debug!(
                    "tracker watcher: {} reopened but taken over — a human decides",
                    issue.key
                );
                advance_mark(state, hold_from);
                continue;
            }

            // A second attempt gets its own ticket rather than this card being
            // pushed back through the board. Re-arming would hand it the first
            // attempt's branch, merge request and review history, and leave one
            // ticket whose meaning changed halfway through — the thing that
            // makes these impossible for a TAM to follow. See `reopen`.
            match reopen::file_followup(
                client,
                tracking.tracker.as_ref(),
                card,
                &issue.key,
                reopen::ReopenTrigger::StatusReopened,
                &[],
            )
            .await
            {
                // Nothing happened, so nothing is announced. The status trigger
                // matches this ticket on every sweep for as long as it sits
                // outside the done category, and its follow-up is already open —
                // there is no news here, only a mark to move past it.
                Ok(reopen::Outcome::AlreadyOpen(followup)) => {
                    debug!(
                        "tracker watcher: {} already depends on the open follow-up {followup}",
                        issue.key
                    );
                    advance_mark(state, hold_from);
                }
                // The watcher idles without a tracker, so this is the tracker
                // being removed mid-sweep: nothing was filed, and the mark
                // holds so the report is seen again once one is configured.
                Ok(reopen::Outcome::NoTracker) => {
                    debug!(
                        "tracker watcher: no tracker is configured — no follow-up for {}",
                        issue.key
                    );
                    hold_from = hold_from.or(issue.updated);
                }
                Ok(outcome) => {
                    let (followup, verb) = match &outcome {
                        reopen::Outcome::Filed(key) => (key.as_str(), "filed"),
                        reopen::Outcome::Appended(key) => (key.as_str(), "added to open"),
                        // Handled above; matched here only to keep this
                        // exhaustive without a catch-all that would silently
                        // swallow a future variant.
                        reopen::Outcome::AlreadyOpen(key) => (key.as_str(), "already on"),
                        reopen::Outcome::NoTracker => ("nothing", "no tracker for"),
                    };
                    info!(
                        "Tracker watcher: {} reopened — {verb} {followup}",
                        issue.key
                    );
                    advance_mark(state, hold_from);
                }
                Err(error) => {
                    // Hold the mark and carry on: the next sweep retries, and
                    // one unfileable ticket must not strand the ones behind it.
                    warn!(
                        ?error,
                        "tracker watcher: could not file a follow-up for {}", issue.key
                    );
                    hold_from = hold_from.or(issue.updated);
                }
            }
        }
    }

    /// Mentions of the agent on tickets that have no board card. A ticket
    /// nobody imported cannot be answered by any agent, so it is imported to
    /// To Do — the question becomes visible, and assigning the agent starts
    /// work on it as usual. Mentions on tickets already imported belong to the
    /// orchestrator (it can reach the running session).
    ///
    /// Comment bodies are fetched per candidate, so the number of candidates
    /// per sweep is capped: a bulk edit must not turn one poll into hundreds
    /// of tracker requests.
    async fn mentions_tick(
        &self,
        client: &RemoteClient,
        target: &WatchTarget,
        tracking: &Tracking,
        primary: bool,
        state: &mut WatcherState,
    ) {
        /// Comment fetches per sweep.
        const MAX_CANDIDATES: usize = 25;

        if !primary {
            // One board per tracker project owns the un-imported case; otherwise
            // boards sharing a project would each import the same ticket.
            return;
        }
        let project_id = target.project_id;
        let query = Query::Active {
            project: &target.jira_project_key,
            within: reopen_lookback(),
        };

        let fetched = match tracking.tracker.find(query).await {
            Ok(fetched) => fetched,
            Err(error) => {
                warn!(?error, "tracker watcher: mention search failed");
                return;
            }
        };

        let mark = state.mentioned_mark(project_id);
        let Some(existing) = existing_jira_keys(client, project_id).await else {
            return;
        };
        let candidates: Vec<_> = fetched
            .into_iter()
            .filter(|issue| match (&issue.updated, &mark) {
                (Some(updated), Some(mark)) => updated > mark,
                _ => true,
            })
            // Imported tickets are the orchestrator's to answer.
            .filter(|issue| !existing.contains(&issue.key))
            .take(MAX_CANDIDATES)
            .collect();
        if candidates.is_empty() {
            return;
        }

        let agent_id = match tracking
            .tracker
            .identity()
            .await
            .map(|identity| identity.account_id)
        {
            Ok(id) => id,
            Err(error) => {
                debug!(?error, "tracker watcher: no agent identity for mentions");
                return;
            }
        };
        let statuses = match client.list_project_statuses(project_id).await {
            Ok(response) => response.project_statuses,
            Err(error) => {
                warn!(?error, "tracker watcher: could not list project statuses");
                return;
            }
        };
        let Some(todo_status) = statuses.iter().find(|status| is_todo_status(&status.name)) else {
            warn!("tracker watcher: project {project_id} has no To Do column; skipping mentions");
            return;
        };
        let me = client.profile().await.ok().map(|profile| profile.user_id);
        let project_tags = client
            .list_tags(project_id)
            .await
            .map(|response| response.tags)
            .unwrap_or_default();

        for issue in candidates {
            let advance_mark = |state: &mut WatcherState| {
                if let Some(updated) = issue.updated
                    && state.advance_mentioned(project_id, updated)
                {
                    save_state(state);
                }
            };

            let comments = match tracking.tracker.comments(&issue.key).await {
                Ok(comments) => comments,
                Err(error) => {
                    debug!(
                        ?error,
                        "tracker watcher: mention fetch for {} failed", issue.key
                    );
                    continue;
                }
            };
            let mentioned = comments.iter().any(|comment| {
                comment.author_account_id.as_deref() != Some(agent_id.as_str())
                    && comment.mentions.iter().any(|id| id == &agent_id)
            });
            if !mentioned {
                advance_mark(state);
                continue;
            }

            let imported = self
                .import_one(
                    client,
                    project_id,
                    tracking,
                    &issue,
                    todo_status.id,
                    me,
                    &project_tags,
                )
                .await;
            if imported.is_err() {
                // No mark advance: the next sweep retries this one.
                return;
            }
            info!(
                "Tracker watcher: {} mentions the agent — imported into To Do",
                issue.key
            );
            advance_mark(state);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_a_tracker_the_watcher_idles_whatever_the_boards_say() {
        let unconfigured =
            tracker::build_provider(&crate::services::config::TrackerConfig::default());
        let reason =
            watcher_idle_reason(&unconfigured, false, true).expect("no tracker, no polling");
        assert!(reason.contains("not configured"), "{reason}");

        let configured = tracker::build_provider(&crate::services::config::TrackerConfig {
            email: Some("agent@example.com".to_string()),
            api_token: Some("token".to_string()),
            ..Default::default()
        });
        assert_eq!(watcher_idle_reason(&configured, false, true), None);
        assert!(
            watcher_idle_reason(&configured, true, true)
                .is_some_and(|reason| reason.contains("no board"))
        );
    }

    #[test]
    fn todo_aliases_match_and_working_columns_do_not() {
        for name in ["To Do", "  to do ", "TODO", "Unassigned", "Backlog"] {
            assert!(is_todo_status(name), "expected To Do alias: {name}");
        }
        for name in ["Plan", "Execute", "Review", "Push MR", "Close", "Clean up"] {
            assert!(!is_todo_status(name), "not a To Do alias: {name}");
        }
    }

    #[test]
    fn only_the_todo_category_counts_as_a_reopen() {
        // BUG-9934 went In Progress → Merged, both `indeterminate`, and was
        // restarted from a terminal column. Forward moves are not reopens.
        assert!(is_reopened_category(Some("new")));
        assert!(!is_reopened_category(Some("indeterminate")));
        assert!(!is_reopened_category(Some("done")));
        assert!(!is_reopened_category(None));
    }

    #[test]
    fn assigned_marks_advance_independently_of_board_marks() {
        let mut state = WatcherState::default();
        let project = Uuid::new_v4();
        let earlier = DateTime::parse_from_rfc3339("2026-08-10T10:00:00+00:00").unwrap();
        let later = DateTime::parse_from_rfc3339("2026-08-10T11:00:00+00:00").unwrap();

        assert!(state.advance_assigned(project, later));
        // Regressions never move the mark backwards.
        assert!(!state.advance_assigned(project, earlier));
        assert_eq!(state.assigned_mark(project), Some(later));
        // The created-based board mark is untouched by assigned advances —
        // and, unlike it, the assigned mark has no legacy seed to fall
        // back on.
        assert_eq!(state.mark(project), None);
        assert!(state.advance(project, earlier));
        assert_eq!(state.assigned_mark(project), Some(later));
    }

    #[test]
    fn a_follow_up_dispatches_urgent_whatever_jira_says() {
        let labelled = vec![reopen::FOLLOWUP_LABEL.to_string()];
        // The Jira cap keeps one incident counted once; the board still runs
        // the work first.
        assert_eq!(
            board_priority_for(Some("P1"), &labelled),
            Some(IssuePriority::Urgent)
        );
        assert_eq!(
            board_priority_for(Some("P3"), &labelled),
            Some(IssuePriority::Urgent)
        );
        // Jira lowercases labels on write, so the match cannot be exact.
        assert_eq!(
            board_priority_for(Some("P2"), &["PrepCook-Followup".to_string()]),
            Some(IssuePriority::Urgent)
        );
        // Everything else maps as it always did.
        assert_eq!(
            board_priority_for(Some("P1"), &[]),
            Some(IssuePriority::High)
        );
        assert_eq!(board_priority_for(None, &[]), None);
        assert_eq!(
            board_priority_for(Some("P2"), &["something-else".to_string()]),
            Some(IssuePriority::Medium)
        );
    }

    #[test]
    fn terminal_aliases_match_and_active_columns_do_not() {
        for name in [
            "Close", "Closed", "Deployed", "Clean up", "Clean-up", "CLEANUP",
        ] {
            assert!(is_terminal_status(name), "expected terminal: {name}");
        }
        for name in ["To Do", "Plan", "Execute", "Review", "Push MR"] {
            assert!(!is_terminal_status(name), "not terminal: {name}");
        }
    }

    #[test]
    fn reopened_marks_are_independent() {
        let mut state = WatcherState::default();
        let project = Uuid::new_v4();
        let at = DateTime::parse_from_rfc3339("2026-08-12T10:00:00+00:00").unwrap();
        assert!(state.advance_reopened(project, at));
        assert!(!state.advance_reopened(project, at));
        assert_eq!(state.reopened_mark(project), Some(at));
        assert_eq!(state.assigned_mark(project), None);
        assert_eq!(state.mark(project), None);
    }

    #[test]
    fn assignment_starts_waiting_cards_and_imports_unknown_tickets() {
        use AssignedAction::*;
        use CardColumn::*;

        // To Do is the plain case: the board's JQL matched, so advance it.
        assert_eq!(assigned_action(Some("new"), Some(Todo)), Start);
        assert_eq!(assigned_action(Some("new"), None), Import);
        // Jira occasionally omits the category; keep acting, as this poll
        // always has.
        assert_eq!(assigned_action(None, None), Import);
        assert_eq!(assigned_action(None, Some(Todo)), Start);
    }

    #[test]
    fn assignment_never_touches_started_or_finished_work() {
        use AssignedAction::*;
        use CardColumn::*;

        // Planning transitions the ticket to In Progress AND assigns it to the
        // agent, so a shipped ticket rests as "indeterminate + assigned + card
        // in Deployed/Clean-up". Restarting from that state would re-plan
        // merged work on the next comment (the BUG-9934 failure); a real
        // follow-up goes through the reopen sweep instead.
        for card in [Todo, Active, Terminal] {
            assert_eq!(assigned_action(Some("indeterminate"), Some(card)), Skip);
        }
        assert_eq!(assigned_action(Some("indeterminate"), None), Skip);
        // Terminal cards never act here even in the To Do category — and
        // neither does a missing category, which must not be a back door into
        // moving finished work.
        assert_eq!(assigned_action(Some("new"), Some(Terminal)), Skip);
        assert_eq!(assigned_action(None, Some(Terminal)), Skip);
        // Mid-pipeline work is in flight.
        assert_eq!(assigned_action(Some("new"), Some(Active)), Skip);
        // Closed tickets belong to the reopen sweep.
        assert_eq!(assigned_action(Some("done"), Some(Todo)), Skip);
        assert_eq!(assigned_action(Some("done"), None), Skip);
    }

    #[test]
    fn the_followup_hint_targets_only_the_half_done_gesture() {
        const AGENT: Option<&str> = Some("agent-account");
        let hint =
            |category, column, assignee| needs_followup_hint(category, column, assignee, AGENT);

        // Finished card, on the agent, status never moved back: the one case
        // where someone did half the gesture and nothing will happen.
        assert!(hint(
            Some("indeterminate"),
            Some(CardColumn::Terminal),
            AGENT
        ));
        // Someone else's ticket, or nobody's.
        assert!(!hint(
            Some("indeterminate"),
            Some(CardColumn::Terminal),
            Some("someone-else")
        ));
        assert!(!hint(
            Some("indeterminate"),
            Some(CardColumn::Terminal),
            None
        ));
        // Without an agent identity there is nothing to compare against, so a
        // ticket with no assignee must not read as "assigned to the agent".
        assert!(!needs_followup_hint(
            Some("indeterminate"),
            Some(CardColumn::Terminal),
            None,
            None
        ));
        // Categories that need no hint: To Do acts, done is the reopen sweep's.
        assert!(!hint(Some("new"), Some(CardColumn::Terminal), AGENT));
        assert!(!hint(Some("done"), Some(CardColumn::Terminal), AGENT));
        assert!(!hint(None, Some(CardColumn::Terminal), AGENT));
        // Unfinished work needs no hint either, and a ticket with no card
        // cannot be told from one the board never imported.
        assert!(!hint(
            Some("indeterminate"),
            Some(CardColumn::Active),
            AGENT
        ));
        assert!(!hint(Some("indeterminate"), Some(CardColumn::Todo), AGENT));
        assert!(!hint(Some("indeterminate"), None, AGENT));
    }

    #[test]
    fn column_classification_falls_back_to_active() {
        assert_eq!(classify_column("To Do"), CardColumn::Todo);
        assert_eq!(classify_column("Backlog"), CardColumn::Todo);
        assert_eq!(classify_column("Clean-up"), CardColumn::Terminal);
        assert_eq!(classify_column("Deployed"), CardColumn::Terminal);
        assert_eq!(classify_column("Plan"), CardColumn::Active);
        // Unknown names (including the empty name a missing status resolves to)
        // read as active, so pickup leaves them alone rather than moving a card
        // it cannot place.
        assert_eq!(classify_column("Awaiting QA"), CardColumn::Active);
        assert_eq!(classify_column(""), CardColumn::Active);
    }

    #[test]
    fn status_update_touches_nothing_but_the_status() {
        let status_id = Uuid::new_v4();
        let update = status_update(status_id);
        assert_eq!(update.status_id, Some(status_id));
        let raw = serde_json::to_value(&update).unwrap();
        // Every other field serializes as null (or is skipped): a PATCH from
        // the watcher must never clobber title, priority, or metadata.
        for (key, value) in raw.as_object().unwrap() {
            if key != "status_id" {
                assert!(value.is_null(), "{key} must stay untouched");
            }
        }
    }
}
