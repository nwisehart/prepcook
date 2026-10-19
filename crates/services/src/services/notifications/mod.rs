//! The notifications seam: everything Prepcook tells a person outside the
//! app, and the services it tells them through.
//!
//! Callers describe *what happened* as an [`Event`] and hand it to
//! [`NotificationSource::send`]; each configured target renders it its own way
//! (Slack blocks, a signed JSON webhook, a Discord embed, a Teams card).
//! Delivery is best-effort throughout: a failing target is logged and never
//! fails the caller, because a notification outage must not stop automation.
//!
//! Targets live in `notifications.targets` (several at once), and a board can
//! switch them off or add its own (`ProjectNotificationsOverride`). One kind
//! of target can also hold a conversation ([`Conversational`]): the Slack bot,
//! whose escalation threads are polled for replies that unblock an agent.
//!
//! Adding a provider: a file here with a struct implementing [`Notifier`] and
//! a `descriptor()`, a variant in [`NotifierKind`] and [`NotifierProvider`],
//! and its arm in [`NotifierKind::descriptor`] and [`build_provider`]. The
//! Settings section and the configuration reference pick it up from the
//! descriptor (see `docs/notifications.md`).

pub mod discord;
pub mod slack;
pub mod teams;
pub mod webhook;

use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
    time::Duration,
};

use api_types::{CardOrigin, HarnessOutcome, TriageReport, WorkClass};
use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use enum_dispatch::enum_dispatch;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::RwLock;
use ts_rs::TS;
use uuid::Uuid;

use self::{
    discord::DiscordNotifier, slack::SlackNotifier, teams::TeamsNotifier, webhook::WebhookNotifier,
};
use crate::services::{
    config::{Config, NotificationTarget, TargetOrigin},
    integrations::{Descriptor, FieldKind, SecretPrecedence, resolve_secret, secret_name},
    secrets::{self, Secrets},
};

/// The integration area name, as the descriptors route and Settings use it.
pub const AREA: &str = "notifications";

/// Timeout on every request a provider makes.
pub(crate) const HTTP_TIMEOUT: Duration = Duration::from_secs(10);

/// The services a notification can go to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum NotifierKind {
    Slack,
    Webhook,
    Discord,
    Teams,
}

impl NotifierKind {
    pub const ALL: [NotifierKind; 4] = [Self::Slack, Self::Webhook, Self::Discord, Self::Teams];

    /// The config string (`kind` in a target).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Slack => "slack",
            Self::Webhook => "webhook",
            Self::Discord => "discord",
            Self::Teams => "teams",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == raw)
    }

    pub fn descriptor(self) -> Descriptor {
        match self {
            Self::Slack => SlackNotifier::descriptor(),
            Self::Webhook => WebhookNotifier::descriptor(),
            Self::Discord => DiscordNotifier::descriptor(),
            Self::Teams => TeamsNotifier::descriptor(),
        }
    }
}

impl std::fmt::Display for NotifierKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Every provider's descriptor, in [`NotifierKind::ALL`] order.
pub fn descriptors() -> Vec<Descriptor> {
    NotifierKind::ALL
        .into_iter()
        .map(NotifierKind::descriptor)
        .collect()
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

/// How loudly an event should be announced.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warn,
    Urgent,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Urgent => "urgent",
        }
    }
}

/// The ticket an event is about.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct IssueRef {
    /// The tracker key (`BUG-1234`), or the card's own id when the card has no
    /// ticket.
    pub key: String,
    pub title: String,
    /// The ticket's page on the tracker.
    pub url: Option<String>,
    /// The card on the board.
    pub card_url: Option<String>,
}

impl IssueRef {
    /// The title without the importer's leading `KEY: `, for surfaces that
    /// print the key beside it.
    pub fn short_title(&self) -> &str {
        self.title
            .strip_prefix(&format!("{}: ", self.key))
            .unwrap_or(&self.title)
    }
}

/// A tracker comment that @-mentioned the agent.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Mention {
    pub author: String,
    pub text: String,
}

/// What a planning agent produced.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanKind {
    Rca,
    Spec,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    Low,
    Medium,
    High,
}

/// A plan that did not clear the autonomy bar and now waits for a person.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PlanningEscalation {
    pub issue: IssueRef,
    /// The tracker's name ("Jira"), when the ticket lives on one.
    pub tracker: Option<String>,
    pub plan_kind: PlanKind,
    /// Set for an RCA.
    pub confidence: Option<Confidence>,
    /// Set for a spec.
    pub open_questions: Option<u32>,
    /// What the plan is stuck on, in a few words ("Low confidence RCA").
    pub blocked_on: String,
    /// The summary's first sentence.
    pub root_cause: String,
    /// The agent's reason for not rating the RCA higher.
    pub why_not_higher: Option<String>,
    /// Next steps the agent labelled as such; empty when it labelled none.
    pub next_steps: Vec<String>,
    /// The whole planning summary.
    pub summary: String,
}

/// What an investigation's reproduction harness showed.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct HarnessRef {
    /// Worktree-relative path of the harness, when it is still on disk.
    pub path: Option<String>,
    pub outcome: HarnessOutcome,
}

/// An investigation card was triaged: its report is in.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct InvestigationCompleted {
    pub issue: IssueRef,
    /// The six sections as fields, plus the whole comment as `markdown`.
    pub report: TriageReport,
    /// The RCA as filed in the knowledge store, when it has been.
    pub rca_url: Option<String>,
    pub rca_id: Option<Uuid>,
    pub harness: HarnessRef,
    /// The level the report's Confidence section opens with.
    pub confidence: Option<Confidence>,
    /// Who filed the card through `POST /api/cards`; `None` for a card made
    /// any other way.
    pub source: Option<CardOrigin>,
}

/// Which digest list a section is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DigestSectionId {
    /// A merge request is open and waiting on a human.
    NeedsMerge,
    /// Planning stopped on something only a person can answer.
    NeedsAnswer,
    /// Stalled; each item carries why.
    NotMoving,
}

/// One card as the digest lists it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DigestItem {
    pub key: String,
    /// The ticket's page on the tracker.
    pub url: Option<String>,
    pub title: String,
    pub mr_url: Option<String>,
    /// Urgent priority (P0).
    pub urgent: bool,
    /// Days since the card last moved; `None` for an item that did not come
    /// from the board.
    pub days: Option<i64>,
    /// Why the card is not moving (Not moving only).
    pub note: Option<String>,
}

impl DigestItem {
    /// The title without the importer's leading `KEY: `.
    pub fn short_title(&self) -> &str {
        self.title
            .strip_prefix(&format!("{}: ", self.key))
            .unwrap_or(&self.title)
    }

    /// The merge request's number, from the last path segment of its URL.
    pub fn mr_number(&self) -> Option<i64> {
        self.mr_url
            .as_deref()?
            .trim_end_matches('/')
            .rsplit('/')
            .next()?
            .parse()
            .ok()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DigestSection {
    pub id: DigestSectionId,
    /// "Needs a merge".
    pub title: String,
    /// Who the items wait on: "an engineer to read a diff".
    pub waiting_on: String,
    /// Most urgent first, then longest waiting.
    pub items: Vec<DigestItem>,
}

/// The engineers' daily to-do list for one board, as data; each provider
/// renders it its own way.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Digest {
    /// The local date the digest is for.
    pub date: NaiveDate,
    pub generated_at: DateTime<Utc>,
    /// IANA zone `date` is in.
    pub timezone: String,
    /// The local date of the next digest.
    pub next_run: NaiveDate,
    /// Merged changes not yet on staging, counted rather than listed.
    pub awaiting_staging: usize,
    pub sections: Vec<DigestSection>,
}

impl Digest {
    pub fn section(&self, id: DigestSectionId) -> Option<&DigestSection> {
        self.sections.iter().find(|section| section.id == id)
    }

    pub fn count(&self, id: DigestSectionId) -> usize {
        self.section(id).map_or(0, |section| section.items.len())
    }

    pub fn total(&self) -> usize {
        self.sections
            .iter()
            .map(|section| section.items.len())
            .sum()
    }

    /// "Orchestrator — Friday 4 September".
    pub fn heading(&self) -> String {
        format!("Orchestrator — {}", self.date.format("%A %-d %B"))
    }

    /// The counts in one line, for notifications and plain-text surfaces.
    pub fn counts_line(&self) -> String {
        format!(
            "{} to merge, {} to answer, {} not moving, {} awaiting staging",
            self.count(DigestSectionId::NeedsMerge),
            self.count(DigestSectionId::NeedsAnswer),
            self.count(DigestSectionId::NotMoving),
            self.awaiting_staging
        )
    }
}

/// Something a person should hear about. One variant per situation; the
/// fields are the contract webhook consumers read (`docs/notifications.md`).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum Event {
    /// The Claude seat hit a usage window; launches pause until it reopens.
    SeatLimitHit { window: String, minutes: u64 },
    /// Several windows in a row ran out with nothing getting through.
    SeatExhausted { rounds: u32, report: String },
    /// The seat's login expired; no agent can run.
    SeatExpired {},
    /// The seat answers again after [`Event::SeatExpired`].
    SeatRecovered {},
    /// The ticket was closed while merge requests from its card were open.
    TicketClosedWithOpenMrs {
        issue: IssueRef,
        mr_numbers: Vec<i64>,
    },
    /// The daily digest.
    DailyDigest(Digest),
    /// A plan below the autonomy bar waits for a person.
    PlanningEscalated(PlanningEscalation),
    /// The script runner cannot do its job for a card.
    RunnerFault {
        issue: IssueRef,
        /// What could not run ("Pre-merge verification").
        step: String,
        /// The runner's outcome label.
        outcome: String,
        detail: String,
    },
    /// Review kept requesting changes until the round cap.
    ReviewLoopCapped { issue: IssueRef, rounds: u32 },
    /// Review findings could not be handed to a fixing turn.
    ReviewRoundAbandoned { issue: IssueRef, reason: String },
    /// A merge request's pipeline needs a person.
    PipelineEscalated {
        issue: IssueRef,
        mr_url: String,
        reason: String,
    },
    /// Merge request review rejects the approach; re-planning is a person's
    /// call.
    ReplanRequested {
        issue: IssueRef,
        reason: String,
        mr_url: Option<String>,
    },
    /// A merge request for a blocker-priority ticket was opened.
    BlockerMrOpened {
        issue: IssueRef,
        mr_url: String,
        summary: String,
    },
    /// The agent was mentioned on the tracker and no session is running.
    MentionNeedsHuman {
        issue: IssueRef,
        mentions: Vec<Mention>,
    },
    /// An investigation card finished: reproduced, explained, triaged.
    InvestigationCompleted(InvestigationCompleted),
    /// A card's work class was changed (an investigation converted to a bug).
    CardConverted {
        issue: IssueRef,
        from: WorkClass,
        to: WorkClass,
        /// Who converted it, as the caller named them.
        by: String,
        /// The column the card moved to: `Executing` when the plan carried
        /// over, `Planning` when it is planned again.
        column: String,
    },
}

impl Event {
    /// Every kind string, in declaration order. A stable contract: routing
    /// (`events` on a target) and the webhook `type` field use these.
    pub const KINDS: [&'static str; 16] = [
        "seat_limit_hit",
        "seat_exhausted",
        "seat_expired",
        "seat_recovered",
        "ticket_closed_with_open_mrs",
        "daily_digest",
        "planning_escalated",
        "runner_fault",
        "review_loop_capped",
        "review_round_abandoned",
        "pipeline_escalated",
        "replan_requested",
        "blocker_mr_opened",
        "mention_needs_human",
        "investigation_completed",
        "card_converted",
    ];

    pub fn kind(&self) -> &'static str {
        match self {
            Self::SeatLimitHit { .. } => "seat_limit_hit",
            Self::SeatExhausted { .. } => "seat_exhausted",
            Self::SeatExpired {} => "seat_expired",
            Self::SeatRecovered {} => "seat_recovered",
            Self::TicketClosedWithOpenMrs { .. } => "ticket_closed_with_open_mrs",
            Self::DailyDigest(_) => "daily_digest",
            Self::PlanningEscalated(_) => "planning_escalated",
            Self::RunnerFault { .. } => "runner_fault",
            Self::ReviewLoopCapped { .. } => "review_loop_capped",
            Self::ReviewRoundAbandoned { .. } => "review_round_abandoned",
            Self::PipelineEscalated { .. } => "pipeline_escalated",
            Self::ReplanRequested { .. } => "replan_requested",
            Self::BlockerMrOpened { .. } => "blocker_mr_opened",
            Self::MentionNeedsHuman { .. } => "mention_needs_human",
            Self::InvestigationCompleted(_) => "investigation_completed",
            Self::CardConverted { .. } => "card_converted",
        }
    }

    pub fn severity(&self) -> Severity {
        match self {
            Self::SeatRecovered {}
            | Self::DailyDigest(_)
            | Self::InvestigationCompleted(_)
            | Self::CardConverted { .. } => Severity::Info,
            Self::SeatLimitHit { .. }
            | Self::TicketClosedWithOpenMrs { .. }
            | Self::ReplanRequested { .. }
            | Self::BlockerMrOpened { .. }
            | Self::MentionNeedsHuman { .. } => Severity::Warn,
            Self::SeatExhausted { .. }
            | Self::SeatExpired {}
            | Self::PlanningEscalated(_)
            | Self::RunnerFault { .. }
            | Self::ReviewLoopCapped { .. }
            | Self::ReviewRoundAbandoned { .. }
            | Self::PipelineEscalated { .. } => Severity::Urgent,
        }
    }

    /// The event's fields as JSON: the webhook payload's `data`.
    pub fn data(&self) -> serde_json::Value {
        serde_json::to_value(self)
            .ok()
            .and_then(|mut value| value.get_mut("data").map(serde_json::Value::take))
            .unwrap_or_else(|| serde_json::json!({}))
    }

    /// The ticket the event is about, when it is about one.
    pub fn issue(&self) -> Option<&IssueRef> {
        match self {
            Self::TicketClosedWithOpenMrs { issue, .. }
            | Self::RunnerFault { issue, .. }
            | Self::ReviewLoopCapped { issue, .. }
            | Self::ReviewRoundAbandoned { issue, .. }
            | Self::PipelineEscalated { issue, .. }
            | Self::ReplanRequested { issue, .. }
            | Self::BlockerMrOpened { issue, .. }
            | Self::MentionNeedsHuman { issue, .. }
            | Self::CardConverted { issue, .. } => Some(issue),
            Self::PlanningEscalated(escalation) => Some(&escalation.issue),
            Self::InvestigationCompleted(completed) => Some(&completed.issue),
            Self::SeatLimitHit { .. }
            | Self::SeatExhausted { .. }
            | Self::SeatExpired {}
            | Self::SeatRecovered {}
            | Self::DailyDigest(_) => None,
        }
    }

    /// One line saying what happened, for providers that render a title.
    pub fn headline(&self) -> String {
        match self {
            Self::SeatLimitHit { window, .. } => format!("Claude seat {window} limit hit"),
            Self::SeatExhausted { .. } => "The Claude seat keeps running out".to_string(),
            Self::SeatExpired {} => "Claude seat expired".to_string(),
            Self::SeatRecovered {} => "Claude seat is healthy again".to_string(),
            Self::TicketClosedWithOpenMrs { issue, mr_numbers } => format!(
                "{} was closed with {} still open",
                issue.key,
                mr_list(mr_numbers)
            ),
            Self::DailyDigest(digest) => digest.heading(),
            Self::PlanningEscalated(escalation) => format!(
                "{}: {}",
                escalation.issue.key,
                escalation.issue.short_title()
            ),
            Self::RunnerFault {
                issue,
                step,
                outcome,
                ..
            } => format!(
                "{}: {step} cannot run — script runner reported {outcome}",
                issue.key
            ),
            Self::ReviewLoopCapped { issue, rounds } => {
                format!("{}: review loop hit the {rounds}-round cap", issue.key)
            }
            Self::ReviewRoundAbandoned { issue, .. } => {
                format!("{}: review findings could not be applied", issue.key)
            }
            Self::PipelineEscalated { issue, .. } => {
                format!("{}: MR pipeline needs a human", issue.key)
            }
            Self::ReplanRequested { issue, .. } => {
                format!("{}: MR review feedback rejects the approach", issue.key)
            }
            Self::BlockerMrOpened { issue, .. } => {
                format!("Blocker merge request needs review — {}", issue.key)
            }
            Self::MentionNeedsHuman { issue, .. } => format!(
                "{}: the agent was mentioned but no session is running",
                issue.key
            ),
            Self::InvestigationCompleted(completed) => format!(
                "{}: investigation triaged — {}",
                completed.issue.key,
                completed.issue.short_title()
            ),
            Self::CardConverted {
                issue, from, to, ..
            } => format!(
                "{}: converted from {} to {}",
                issue.key,
                from.as_str(),
                to.as_str()
            ),
        }
    }

    /// The detail under the headline, as plain text with light markdown
    /// (`**bold**`, `` `code` ``) that Discord and Teams both render.
    pub fn body(&self) -> String {
        match self {
            Self::SeatLimitHit { minutes, .. } => {
                format!("Agent launches are paused for ~{minutes}m, until the window reopens.")
            }
            Self::SeatExhausted { rounds, report } => format!(
                "{rounds} windows in a row exhausted with nothing getting through between \
                 them, so no board is advancing — that needs less burn or more budget, not \
                 more waiting.\nThe seat reported: `{report}`"
            ),
            Self::SeatExpired {} => SEAT_EXPIRED_BODY.to_string(),
            Self::SeatRecovered {} => "Agents can run again.".to_string(),
            Self::TicketClosedWithOpenMrs { .. } => {
                "The card has been retired; that merge request needs closing or merging by hand."
                    .to_string()
            }
            Self::DailyDigest(digest) => plain_digest(digest),
            Self::PlanningEscalated(escalation) => {
                let mut out = format!(
                    "**Blocked on:** {} — waiting in Plan for a human\n**Root cause:** {}",
                    escalation.blocked_on, escalation.root_cause
                );
                if let Some(reason) = &escalation.why_not_higher {
                    out.push_str(&format!("\n**Why not higher:** {reason}"));
                }
                if !escalation.next_steps.is_empty() {
                    out.push_str("\n**Next steps:**");
                    for step in &escalation.next_steps {
                        out.push_str(&format!("\n• {step}"));
                    }
                }
                out
            }
            Self::RunnerFault { detail, .. } => format!(
                "This is the setup's problem, not the branch's; the card holds until it is \
                 fixed.\n{detail}"
            ),
            Self::ReviewLoopCapped { .. } => "Waiting in Review for a human.".to_string(),
            Self::ReviewRoundAbandoned { reason, .. } => {
                format!("{reason}. Waiting in Review for a human.")
            }
            Self::PipelineEscalated { mr_url, reason, .. } => format!("{reason}\nMR: {mr_url}"),
            Self::ReplanRequested { reason, mr_url, .. } => format!(
                "The agent stopped instead of rewriting it.\n{reason}\n\n{REPLAN_DECISION}{}",
                mr_url
                    .as_deref()
                    .map(|url| format!("\nMR: {url}"))
                    .unwrap_or_default()
            ),
            Self::BlockerMrOpened {
                mr_url, summary, ..
            } => format!("{summary}\n{mr_url}"),
            Self::MentionNeedsHuman { mentions, .. } => mention_listing(mentions),
            Self::InvestigationCompleted(completed) => investigation_summary(completed),
            Self::CardConverted { by, column, .. } => {
                format!("Converted by {by}; the card is in {column}.")
            }
        }
    }

    /// Where a reader should go next.
    pub fn link(&self) -> Option<&str> {
        match self {
            Self::PipelineEscalated { mr_url, .. } | Self::BlockerMrOpened { mr_url, .. } => {
                Some(mr_url)
            }
            Self::ReplanRequested { issue, mr_url, .. } => mr_url
                .as_deref()
                .or(issue.card_url.as_deref())
                .or(issue.url.as_deref()),
            _ => self
                .issue()
                .and_then(|issue| issue.card_url.as_deref().or(issue.url.as_deref())),
        }
    }
}

pub(crate) const SEAT_EXPIRED_BODY: &str = "Agents can't run until it's re-authenticated. On \
     the runner:\n`sudo docker exec -it prepcook claude /login` (sign in as the agent's \
     Claude account, then paste the code).";

pub(crate) const REPLAN_DECISION: &str = "A human decides: move the ticket back to Plan (then \
     forward again to re-arm) to re-plan, or reply on the MR if the agent misread the review.";

/// The report's what/why/workaround and the confidence, in plain text.
pub(crate) fn investigation_summary(completed: &InvestigationCompleted) -> String {
    let report = &completed.report;
    let mut out = format!(
        "**What happens:** {}\n**Why:** {}",
        clip(report.what_happens.trim(), 400),
        clip(report.why.trim(), 700)
    );
    if !report.workaround.trim().is_empty() {
        out.push_str(&format!(
            "\n**Workaround:** {}",
            clip(report.workaround.trim(), 300)
        ));
    }
    if !report.confidence.trim().is_empty() {
        out.push_str(&format!(
            "\n**Confidence:** {}",
            clip(report.confidence.trim(), 200)
        ));
    }
    out
}

/// "!12, !13".
pub(crate) fn mr_list(numbers: &[i64]) -> String {
    numbers
        .iter()
        .map(|number| format!("!{number}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// "- [author] text" per mention.
pub(crate) fn mention_listing(mentions: &[Mention]) -> String {
    mentions
        .iter()
        .map(|mention| format!("- [{}] {}", mention.author, mention.text))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Most items listed per section on surfaces that cap the digest.
pub(crate) const DIGEST_SECTION_CAP: usize = 12;

/// The digest as plain text: counts, then each non-empty section, capped.
fn plain_digest(digest: &Digest) -> String {
    let mut out = digest.counts_line();
    for section in &digest.sections {
        if section.items.is_empty() {
            continue;
        }
        out.push_str(&format!(
            "\n\n**{}** · {} waiting on {}",
            section.title,
            section.items.len(),
            section.waiting_on
        ));
        for item in section.items.iter().take(DIGEST_SECTION_CAP) {
            let link = match (&item.mr_url, item.mr_number(), &item.url) {
                (Some(url), Some(number), _) => format!("[!{number}]({url}) {}", item.key),
                (Some(url), None, _) => format!("[MR]({url}) {}", item.key),
                (None, _, Some(url)) => format!("[{}]({url})", item.key),
                (None, _, None) => item.key.clone(),
            };
            let marker = if item.urgent { "🔴 " } else { "" };
            let age = item.days.map(|d| format!(" · {d}d")).unwrap_or_default();
            let note = item
                .note
                .as_deref()
                .map(|note| format!(" — {note}"))
                .unwrap_or_default();
            out.push_str(&format!(
                "\n• {marker}{link} {}{age}{note}",
                item.short_title()
            ));
        }
        if section.items.len() > DIGEST_SECTION_CAP {
            out.push_str(&format!(
                "\n…and {} more",
                section.items.len() - DIGEST_SECTION_CAP
            ));
        }
    }
    if digest.total() == 0 {
        out.push_str("\n\nNothing is waiting on a person today.");
    }
    out
}

/// `text` clipped to `max` characters, with an ellipsis when cut.
pub(crate) fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// `value` with every string in it passed through [`secrets::redact`].
pub(crate) fn redact_value(value: serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match value {
        Value::String(text) => Value::String(secrets::redact(&text)),
        Value::Array(items) => Value::Array(items.into_iter().map(redact_value).collect()),
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, value)| (key, redact_value(value)))
                .collect(),
        ),
        other => other,
    }
}

// ---------------------------------------------------------------------------
// The provider seam
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum NotifyError {
    /// The target lacks a setting it needs.
    #[error("not configured: {0}")]
    NotConfigured(String),
    /// The request did not complete (connect error, timeout).
    #[error("request failed: {0}")]
    Http(String),
    /// The service answered with a failure status.
    #[error("HTTP {status}: {body}")]
    Rejected { status: u16, body: String },
    /// The service answered 2xx but reported an error (Slack's `ok: false`).
    #[error("{0}")]
    Api(String),
    /// The provider has no such operation.
    #[error("not supported: {0}")]
    Unsupported(String),
}

impl NotifyError {
    pub(crate) fn http(error: reqwest::Error) -> Self {
        // `reqwest` errors carry the URL, and a webhook URL is a credential.
        Self::Http(secrets::redact(&error.without_url().to_string()))
    }
}

/// A conversation a sent message started, for reading replies back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadRef {
    pub channel: String,
    /// The root message's id (Slack's `ts`).
    pub ts: String,
}

/// What a sent notification left behind.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Delivery {
    /// Set when the message opened a thread that can be read back.
    pub thread: Option<ThreadRef>,
}

/// One message in a conversation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reply {
    /// The message's id in its service (Slack's `ts`).
    pub id: String,
    pub at: DateTime<Utc>,
    pub text: String,
    /// True for the bot's own posts (the escalation itself, its detail
    /// reply); only human replies should reach an agent.
    pub is_bot: bool,
    /// The author's id in the service, for human messages.
    pub user: Option<String>,
}

/// A high-water mark for replies as stored on a card: epoch seconds with
/// microseconds, the same text as a Slack `ts`, so marks recorded by either
/// read back the same way.
pub fn reply_mark(at: DateTime<Utc>) -> String {
    format!("{}.{:06}", at.timestamp(), at.timestamp_subsec_micros())
}

/// Parses a mark written by [`reply_mark`] (or a Slack `ts`).
pub fn parse_reply_mark(mark: &str) -> Option<DateTime<Utc>> {
    let (secs, frac) = mark.trim().split_once('.').unwrap_or((mark.trim(), ""));
    let secs: i64 = secs.parse().ok()?;
    let micros: u32 = if frac.is_empty() {
        0
    } else {
        format!("{frac:0<6}").get(..6)?.parse().ok()?
    };
    DateTime::from_timestamp(secs, micros * 1_000)
}

/// A notifier that can read replies to what it posted and answer them.
#[async_trait]
pub trait Conversational: Send + Sync {
    /// Every message in the thread newer than `since` (all of them when
    /// `None`), the root included, oldest first.
    async fn replies(
        &self,
        thread: &ThreadRef,
        since: Option<DateTime<Utc>>,
    ) -> Result<Vec<Reply>, NotifyError>;

    async fn reply(&self, thread: &ThreadRef, text: &str) -> Result<(), NotifyError>;
}

/// One service notifications can be sent to.
#[async_trait]
#[enum_dispatch(NotifierProvider)]
pub trait Notifier: Send + Sync {
    /// The config string of this provider.
    fn kind(&self) -> &'static str;

    async fn send(&self, event: &Event) -> Result<Delivery, NotifyError>;

    /// `Some` when this target can hold a conversation.
    fn conversational(&self) -> Option<&dyn Conversational>;

    /// A cheap check that the settings work, returning what it found. Where
    /// the service offers nothing to read, it sends a test message instead.
    async fn test(&self) -> Result<String, NotifyError>;
}

#[enum_dispatch]
pub enum NotifierProvider {
    Slack(SlackNotifier),
    Webhook(WebhookNotifier),
    Discord(DiscordNotifier),
    Teams(TeamsNotifier),
}

// ---------------------------------------------------------------------------
// Building
// ---------------------------------------------------------------------------

/// One target's settings with its secrets resolved: what a provider is built
/// from, and what the cache compares to decide whether to rebuild.
#[derive(Clone, Debug, PartialEq)]
pub struct TargetSettings {
    pub kind: NotifierKind,
    /// Event kinds the target receives; `None` (or `*` in the list) is all.
    pub events: Option<Vec<String>>,
    pub fields: BTreeMap<String, String>,
}

impl TargetSettings {
    /// A field's value, `None` when unset or blank.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.fields
            .get(key)
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
    }

    pub fn accepts(&self, kind: &str) -> bool {
        self.events.as_ref().is_none_or(|events| {
            events
                .iter()
                .any(|event| event.trim() == "*" || event.trim() == kind)
        })
    }
}

/// Where the notifications are being sent from, for links back to the app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Context {
    pub board_id: Option<Uuid>,
    /// The board's page in the app, or the app's root without a board.
    pub board_url: String,
}

impl Context {
    pub fn for_board(board_id: Option<Uuid>) -> Self {
        let base = crate::services::stage_orchestrator::public_ui_base();
        Self {
            board_id,
            board_url: match board_id {
                Some(id) => format!("{base}/projects/{id}"),
                None => base,
            },
        }
    }
}

/// One built target.
struct Target {
    settings: TargetSettings,
    provider: NotifierProvider,
}

/// The targets for one scope (a board, or the global ones), built.
pub struct NotificationService {
    targets: Vec<Target>,
}

/// Builds one provider.
pub fn build_provider(
    settings: &TargetSettings,
    context: &Context,
) -> Result<NotifierProvider, NotifyError> {
    Ok(match settings.kind {
        NotifierKind::Slack => SlackNotifier::new(settings, context)?.into(),
        NotifierKind::Webhook => WebhookNotifier::new(settings, context)?.into(),
        NotifierKind::Discord => DiscordNotifier::new(settings)?.into(),
        NotifierKind::Teams => TeamsNotifier::new(settings)?.into(),
    })
}

/// Builds every target, skipping (and logging) one whose settings are
/// incomplete so it cannot silence the others. `None` when nothing usable is
/// configured.
pub fn build(
    targets: &[TargetSettings],
    context: &Context,
) -> Result<Option<NotificationService>, NotifyError> {
    let mut built = Vec::new();
    for settings in targets {
        match build_provider(settings, context) {
            Ok(provider) => built.push(Target {
                settings: settings.clone(),
                provider,
            }),
            Err(NotifyError::NotConfigured(what)) => {
                tracing::warn!(
                    kind = %settings.kind,
                    "notifications: skipping a {} target: {what}",
                    settings.kind
                );
            }
            Err(error) => return Err(error),
        }
    }
    Ok((!built.is_empty()).then_some(NotificationService { targets: built }))
}

impl NotificationService {
    /// Sends `event` to every target that takes it, concurrently. Failures are
    /// logged, never returned: a broken target must not fail the caller.
    pub async fn send(&self, event: &Event) {
        self.fan_out(event).await;
    }

    /// [`Self::send`], returning the thread the first conversational target
    /// opened, if any did.
    pub async fn escalate(&self, event: &Event) -> Option<ThreadRef> {
        self.fan_out(event).await.into_iter().flatten().next()
    }

    /// The first target that can hold a conversation.
    pub fn conversational(&self) -> Option<&dyn Conversational> {
        self.targets
            .iter()
            .find_map(|target| target.provider.conversational())
    }

    /// Per target that took the event: the thread a conversational target
    /// opened.
    async fn fan_out(&self, event: &Event) -> Vec<Option<ThreadRef>> {
        let kind = event.kind();
        let sends = self
            .targets
            .iter()
            .filter(|target| target.settings.accepts(kind))
            .map(|target| async move {
                match target.provider.send(event).await {
                    Ok(delivery) => target.provider.conversational().and(delivery.thread),
                    Err(error) => {
                        tracing::warn!(
                            target = target.provider.kind(),
                            event = kind,
                            "notifications: delivery failed: {error}"
                        );
                        None
                    }
                }
            });
        futures::future::join_all(sends).await
    }
}

/// Resolves the targets a scope sends to: the config entries, each secret
/// field read through the store (store first for global targets, the board's
/// own value first for a board's targets).
pub fn resolve_targets(
    config: &Config,
    board: Option<Uuid>,
    store: &Secrets,
) -> Vec<TargetSettings> {
    config
        .notification_targets(board)
        .into_iter()
        .map(|(origin, target)| {
            let precedence = match origin {
                TargetOrigin::Global => SecretPrecedence::StoreFirst,
                TargetOrigin::Board => SecretPrecedence::ConfigFirst,
            };
            resolve_target(&target, store, precedence)
        })
        .collect()
}

fn field_text(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::Null => None,
        serde_json::Value::String(text) => Some(text.clone()),
        other => Some(other.to_string()),
    }
}

fn resolve_target(
    target: &NotificationTarget,
    store: &Secrets,
    precedence: SecretPrecedence,
) -> TargetSettings {
    let mut fields: BTreeMap<String, String> = target
        .fields
        .iter()
        .filter_map(|(key, value)| Some((key.clone(), field_text(value)?)))
        .collect();
    for field in target.kind.descriptor().fields {
        if field.kind != FieldKind::Secret {
            continue;
        }
        let name = secret_name(target.kind.as_str(), &field.key);
        match resolve_secret(
            fields.get(&field.key).map(String::as_str),
            &name,
            store,
            precedence,
        ) {
            Some(value) => {
                secrets::register_redaction(&value);
                fields.insert(field.key, value);
            }
            None => {
                fields.remove(&field.key);
            }
        }
    }
    TargetSettings {
        kind: target.kind,
        events: target.events.clone(),
        fields,
    }
}

/// Runs a provider's `test()`: against `fields` when given (Settings' unsaved
/// values, which win over the secrets store so what was typed is what is
/// tested), else against the first global target of that kind.
pub async fn test_target(
    config: &Config,
    kind: &str,
    fields: Option<BTreeMap<String, serde_json::Value>>,
) -> Result<String, String> {
    let kind =
        NotifierKind::parse(kind).ok_or_else(|| format!("unknown notification kind: {kind}"))?;
    let (target, precedence) = match fields {
        Some(fields) => (
            NotificationTarget {
                kind,
                events: None,
                fields,
            },
            SecretPrecedence::ConfigFirst,
        ),
        None => (
            config
                .notification_targets(None)
                .into_iter()
                .map(|(_, target)| target)
                .find(|target| target.kind == kind)
                .ok_or_else(|| format!("no {kind} target is configured"))?,
            SecretPrecedence::StoreFirst,
        ),
    };
    let settings = resolve_target(&target, &secrets::load(), precedence);
    let provider =
        build_provider(&settings, &Context::for_board(None)).map_err(|e| e.to_string())?;
    provider.test().await.map_err(|e| e.to_string())
}

/// The scope a cached service was built for, and what it was built from.
type Built = HashMap<Option<Uuid>, (Vec<TargetSettings>, Option<Arc<NotificationService>>)>;

/// Hands out the notification targets the live config describes.
///
/// Built per scope (each board, and the global targets) and reused; rebuilt
/// when that scope's resolved settings change, secrets included, so a change
/// in Settings or the secrets store applies without a restart.
#[derive(Clone)]
pub struct NotificationSource {
    config: Arc<RwLock<Config>>,
    built: Arc<std::sync::Mutex<Built>>,
}

impl NotificationSource {
    pub fn new(config: Arc<RwLock<Config>>) -> Self {
        Self {
            config,
            built: Arc::new(std::sync::Mutex::new(HashMap::new())),
        }
    }

    /// The service for `board` (`None` for process-wide events), or `None`
    /// when nothing is configured there.
    pub async fn current(&self, board: Option<Uuid>) -> Option<Arc<NotificationService>> {
        let wanted = {
            let config = self.config.read().await;
            resolve_targets(&config, board, &secrets::load())
        };
        let mut built = self
            .built
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some((settings, service)) = built.get(&board)
            && *settings == wanted
        {
            return service.clone();
        }
        let service = match build(&wanted, &Context::for_board(board)) {
            Ok(service) => service.map(Arc::new),
            Err(error) => {
                tracing::warn!("notifications: could not build the targets: {error}");
                None
            }
        };
        built.insert(board, (wanted, service.clone()));
        service
    }

    /// Sends `event` to `board`'s targets (the global ones for `None`).
    pub async fn send(&self, board: Option<Uuid>, event: Event) {
        if let Some(service) = self.current(board).await {
            service.send(&event).await;
        }
    }

    /// Sends `event` and returns the thread a conversational target opened
    /// for it. `None` when no conversational target took it: the message
    /// still went to every other target, it just cannot be read back.
    pub async fn escalate(&self, board: Option<Uuid>, event: Event) -> Option<ThreadRef> {
        self.current(board).await?.escalate(&event).await
    }

    /// Messages in `thread` newer than `since`, read from `board`'s
    /// conversational target. `None` when the scope has none.
    pub async fn replies(
        &self,
        board: Option<Uuid>,
        thread: &ThreadRef,
        since: Option<DateTime<Utc>>,
    ) -> Option<Result<Vec<Reply>, NotifyError>> {
        let service = self.current(board).await?;
        let conversational = service.conversational()?;
        Some(conversational.replies(thread, since).await)
    }

    /// The channel `board`'s conversational target posts to, when it names
    /// one.
    pub async fn conversation_channel(&self, board: Option<Uuid>) -> Option<String> {
        let config = self.config.read().await;
        resolve_targets(&config, board, &secrets::load())
            .into_iter()
            .find(|target| {
                target.kind == NotifierKind::Slack
                    && target.get("bot_token").is_some()
                    && target.get("channel").is_some()
            })
            .and_then(|target| target.get("channel").map(str::to_string))
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;

    pub fn settings(kind: NotifierKind, fields: &[(&str, &str)]) -> TargetSettings {
        TargetSettings {
            kind,
            events: None,
            fields: fields
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect(),
        }
    }

    pub fn context() -> Context {
        Context {
            board_id: None,
            board_url: "https://prepcook.example.com/projects/board-1".to_string(),
        }
    }

    pub fn issue() -> IssueRef {
        IssueRef {
            key: "BUG-7".to_string(),
            title: "BUG-7: Export drops the last row".to_string(),
            url: Some("https://example.atlassian.net/browse/BUG-7".to_string()),
            card_url: Some("https://prepcook.example.com/projects/p/issues/i".to_string()),
        }
    }

    /// One of every event, in [`Event::KINDS`] order.
    pub fn every_event() -> Vec<Event> {
        let issue = issue();
        vec![
            Event::SeatLimitHit {
                window: "session".to_string(),
                minutes: 42,
            },
            Event::SeatExhausted {
                rounds: 3,
                report: "5-hour limit reached".to_string(),
            },
            Event::SeatExpired {},
            Event::SeatRecovered {},
            Event::TicketClosedWithOpenMrs {
                issue: issue.clone(),
                mr_numbers: vec![12, 13],
            },
            Event::DailyDigest(Digest {
                date: NaiveDate::from_ymd_opt(2026, 9, 4).unwrap(),
                generated_at: DateTime::from_timestamp(1_788_000_000, 0).unwrap(),
                timezone: "America/Los_Angeles".to_string(),
                next_run: NaiveDate::from_ymd_opt(2026, 9, 7).unwrap(),
                awaiting_staging: 1,
                sections: vec![DigestSection {
                    id: DigestSectionId::NeedsMerge,
                    title: "Needs a merge".to_string(),
                    waiting_on: "an engineer to read a diff".to_string(),
                    items: vec![DigestItem {
                        key: "BUG-7".to_string(),
                        url: issue.url.clone(),
                        title: issue.title.clone(),
                        mr_url: Some(
                            "https://gitlab.example.com/acme/app/-/merge_requests/12".to_string(),
                        ),
                        urgent: true,
                        days: Some(2),
                        note: None,
                    }],
                }],
            }),
            Event::PlanningEscalated(PlanningEscalation {
                issue: issue.clone(),
                tracker: Some("Jira".to_string()),
                plan_kind: PlanKind::Rca,
                confidence: Some(Confidence::Low),
                open_questions: None,
                blocked_on: "Low confidence RCA".to_string(),
                root_cause: "The exporter stops one row early.".to_string(),
                why_not_higher: Some("Could not reproduce on staging".to_string()),
                next_steps: vec!["Confirm the row count".to_string()],
                summary: "The exporter stops one row early. More detail.".to_string(),
            }),
            Event::RunnerFault {
                issue: issue.clone(),
                step: "Pre-merge verification".to_string(),
                outcome: "setup failed".to_string(),
                detail: "docker: not found".to_string(),
            },
            Event::ReviewLoopCapped {
                issue: issue.clone(),
                rounds: 3,
            },
            Event::ReviewRoundAbandoned {
                issue: issue.clone(),
                reason: "the workspace is gone".to_string(),
            },
            Event::PipelineEscalated {
                issue: issue.clone(),
                mr_url: "https://gitlab.example.com/acme/app/-/merge_requests/12".to_string(),
                reason: "lint failed twice".to_string(),
            },
            Event::ReplanRequested {
                issue: issue.clone(),
                reason: "Fix it in the query instead".to_string(),
                mr_url: None,
            },
            Event::BlockerMrOpened {
                issue: issue.clone(),
                mr_url: "https://gitlab.example.com/acme/app/-/merge_requests/12".to_string(),
                summary: "Export drops the last row".to_string(),
            },
            Event::MentionNeedsHuman {
                issue: issue.clone(),
                mentions: vec![Mention {
                    author: "Ada".to_string(),
                    text: "can you take a look?".to_string(),
                }],
            },
            Event::InvestigationCompleted(investigation_completed()),
            Event::CardConverted {
                issue,
                from: WorkClass::Investigation,
                to: WorkClass::Bug,
                by: "Ada".to_string(),
                column: "Executing".to_string(),
            },
        ]
    }

    pub fn investigation_completed() -> InvestigationCompleted {
        InvestigationCompleted {
            issue: issue(),
            report: TriageReport {
                what_happens: "Exports stop one row short.".to_string(),
                location: "Reports → Export, CSV only.".to_string(),
                why: "The pager drops the last page.".to_string(),
                scope: "Every CSV export over 500 rows; a defect.".to_string(),
                workaround: "Export as XLSX.".to_string(),
                confidence: "High — reproduced on main.".to_string(),
                markdown: "TRIAGE REPORT\n\n## What happens\nExports stop one row short."
                    .to_string(),
            },
            rca_url: Some(
                "https://prepcook.example.com/api/knowledge/documents/d/html".to_string(),
            ),
            rca_id: Some(Uuid::nil()),
            harness: HarnessRef {
                path: Some(".repro/BUG-7/repro.mjs".to_string()),
                outcome: HarnessOutcome::Reproduced,
            },
            confidence: Some(Confidence::High),
            source: Some(CardOrigin {
                kind: api_types::OriginKind::Tammy,
                reference: "https://example.slack.com/archives/C1/p1".to_string(),
                reporter: Some("Grace".to_string()),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    use super::{test_support::*, *};

    #[test]
    fn event_kinds_are_stable() {
        // The webhook `type` field and every target's `events` filter use
        // these strings; renaming one breaks consumers, so a rename must show
        // up as a diff here.
        let kinds: Vec<&str> = every_event().iter().map(Event::kind).collect();
        assert_eq!(kinds, Event::KINDS);
        assert_eq!(
            Event::KINDS,
            [
                "seat_limit_hit",
                "seat_exhausted",
                "seat_expired",
                "seat_recovered",
                "ticket_closed_with_open_mrs",
                "daily_digest",
                "planning_escalated",
                "runner_fault",
                "review_loop_capped",
                "review_round_abandoned",
                "pipeline_escalated",
                "replan_requested",
                "blocker_mr_opened",
                "mention_needs_human",
                "investigation_completed",
                "card_converted",
            ]
        );
    }

    /// The `investigation_completed` payload is a contract a support agent
    /// reads (`docs/notifications.md`); every field is present, null or not.
    #[test]
    fn the_investigation_payload_has_the_documented_shape() {
        let event = Event::InvestigationCompleted(investigation_completed());
        assert_eq!(event.severity(), Severity::Info);
        let data = event.data();
        let keys: Vec<&str> = data
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        let mut sorted = keys.clone();
        sorted.sort_unstable();
        assert_eq!(
            sorted,
            [
                "confidence",
                "harness",
                "issue",
                "rca_id",
                "rca_url",
                "report",
                "source"
            ]
        );
        let mut report: Vec<&str> = data["report"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        report.sort_unstable();
        assert_eq!(
            report,
            [
                "confidence",
                "markdown",
                "scope",
                "what_happens",
                "where",
                "why",
                "workaround"
            ]
        );
        assert_eq!(data["harness"]["outcome"], "reproduced");
        assert_eq!(data["harness"]["path"], ".repro/BUG-7/repro.mjs");
        assert_eq!(data["confidence"], "high");
        assert_eq!(data["source"]["kind"], "tammy");
        assert_eq!(
            data["source"]["ref"],
            "https://example.slack.com/archives/C1/p1"
        );
        assert_eq!(data["source"]["reporter"], "Grace");
        assert_eq!(data["issue"]["key"], "BUG-7");

        let converted = every_event().pop().unwrap();
        assert_eq!(converted.kind(), "card_converted");
        assert_eq!(converted.severity(), Severity::Info);
        let data = converted.data();
        assert_eq!(data["from"], "investigation");
        assert_eq!(data["to"], "bug");
        assert_eq!(data["column"], "Executing");
    }

    #[test]
    fn the_serialized_tag_is_the_kind_and_data_is_an_object() {
        for event in every_event() {
            let value = serde_json::to_value(&event).unwrap();
            assert_eq!(value["type"], event.kind());
            assert!(event.data().is_object(), "{}", event.kind());
        }
    }

    #[test]
    fn targets_filter_by_event_kind() {
        let mut target = settings(NotifierKind::Webhook, &[]);
        assert!(target.accepts("daily_digest"));
        target.events = Some(vec!["*".to_string()]);
        assert!(target.accepts("daily_digest"));
        target.events = Some(vec!["planning_escalated".to_string()]);
        assert!(target.accepts("planning_escalated"));
        assert!(!target.accepts("daily_digest"));
    }

    #[test]
    fn reply_marks_round_trip_and_read_slack_timestamps() {
        let at = parse_reply_mark("1700000000.000100").unwrap();
        assert_eq!(at.timestamp(), 1_700_000_000);
        assert_eq!(at.timestamp_subsec_micros(), 100);
        assert_eq!(reply_mark(at), "1700000000.000100");
        assert_eq!(
            parse_reply_mark("1700000000").unwrap().timestamp(),
            1_700_000_000
        );
        assert!(parse_reply_mark("not a mark").is_none());
    }

    #[tokio::test]
    async fn one_failing_target_does_not_stop_the_others() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/broken"))
            .respond_with(ResponseTemplate::new(400))
            .mount(&server)
            .await;
        let delivered = Arc::new(AtomicUsize::new(0));
        let counter = delivered.clone();
        Mock::given(method("POST"))
            .and(path("/working"))
            .respond_with(move |_: &wiremock::Request| {
                counter.fetch_add(1, Ordering::SeqCst);
                ResponseTemplate::new(204)
            })
            .mount(&server)
            .await;

        let targets = vec![
            settings(
                NotifierKind::Discord,
                &[("webhook_url", &format!("{}/broken", server.uri()))],
            ),
            settings(
                NotifierKind::Discord,
                &[("webhook_url", &format!("{}/working", server.uri()))],
            ),
            // Incomplete: skipped at build time rather than failing the rest.
            settings(NotifierKind::Teams, &[]),
        ];
        let service = build(&targets, &context()).unwrap().unwrap();
        assert_eq!(service.targets.len(), 2);
        service.send(&Event::SeatRecovered {}).await;
        assert_eq!(delivered.load(Ordering::SeqCst), 1);
        // Nothing conversational among them: an escalation opens no thread.
        assert!(service.escalate(&Event::SeatRecovered {}).await.is_none());
    }

    #[test]
    fn nothing_configured_builds_nothing() {
        assert!(build(&[], &context()).unwrap().is_none());
        assert!(
            build(&[settings(NotifierKind::Slack, &[])], &context())
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn board_targets_prefer_their_own_secret_and_global_ones_the_store() {
        let store = Secrets::load_from(
            std::path::Path::new("/nonexistent/prepcook-test-secrets.env"),
            [(
                "PREPCOOK_SECRET_DISCORD_WEBHOOK_URL".to_string(),
                "https://discord.example.com/api/webhooks/from-store".to_string(),
            )],
        );
        let target = NotificationTarget {
            kind: NotifierKind::Discord,
            events: None,
            fields: BTreeMap::from([(
                "webhook_url".to_string(),
                serde_json::json!("https://discord.example.com/api/webhooks/from-config"),
            )]),
        };
        let global = resolve_target(&target, &store, SecretPrecedence::StoreFirst);
        assert!(global.get("webhook_url").unwrap().ends_with("from-store"));
        let board = resolve_target(&target, &store, SecretPrecedence::ConfigFirst);
        assert!(board.get("webhook_url").unwrap().ends_with("from-config"));
    }
}
