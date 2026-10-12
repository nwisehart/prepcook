//! Typed view over the opaque `issues.extension_metadata` JSONB blob.
//!
//! The column stays schemaless in the database and syncs to clients
//! untouched; this module is the single place that defines what the Prep Cook
//! workflow stores in it. Parse leniently: unknown fields are preserved by
//! never round-tripping through this struct (read a typed copy, write only
//! the keys you own).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

/// Work classification driving stage playbooks (bug → root-cause/fix,
/// story → spec/execute). Mirrored as a visible tag on the issue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum WorkClass {
    Bug,
    Story,
    Epic,
    Subtask,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct JiraRef {
    /// e.g. "BUG-1234"
    pub key: String,
    /// Jira issue type name, e.g. "Bug", "Small Enhancement Request"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issue_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// How an approval reached the card.
///
/// The distinction is not decoration. `approved_at` is written by three
/// different paths and two of them verified a person first, but the field
/// records only that *something* approved. So "this ticket has a human
/// approval" has never been answerable from the card — which is what
/// BUG-10550 turned out to be asking for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalSource {
    /// Reported by the agent's own `update_issue` call, which is how a click
    /// in the Lavish review gate reaches the card. Nothing distinguishes a
    /// person having clicked from the agent saying one did, so this is
    /// recorded as what it is rather than counted as a human.
    Agent,
    /// An `APPROVE` (or `PLAN: APPROVE`) comment on the board, whose author
    /// was checked against the organization's non-agent members.
    BoardComment,
    /// An `RCA: APPROVED` comment on the tracker ticket, whose author was
    /// checked against the configured approver list.
    #[serde(alias = "jira_comment")]
    TrackerComment,
    /// The Approve button in a Lavish review session, relayed by Prep Cook itself
    /// rather than reported by the agent.
    Lavish,
    /// `POST /api/approvals/{issue_id}`, from the signed-in session or a
    /// caller holding the `PREPCOOK_API_TOKEN` secret.
    Api,
}

/// A decision a person makes about a plan (spec or RCA) under review.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    /// The plan is right; the work may go ahead.
    Approve,
    /// The plan needs revising by the agent still working on it.
    RequestChanges,
    /// Start planning over, for a plan that has already been acted on.
    Replan,
}

impl Decision {
    pub fn label(self) -> &'static str {
        match self {
            Self::Approve => "approve",
            Self::RequestChanges => "request changes",
            Self::Replan => "re-plan",
        }
    }
}

/// Where a decision was made. Each surface verifies its author its own way
/// before a decision is applied; see `docs/approvals.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum DecisionSource {
    /// The Lavish review gate.
    Lavish,
    /// A comment on the tracker ticket (Jira).
    Tracker,
    /// A comment on the Prep Cook card.
    Card,
    /// The approvals endpoint.
    Api,
}

impl DecisionSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::Lavish => "Lavish",
            Self::Tracker => "the tracker",
            Self::Card => "a card comment",
            Self::Api => "the API",
        }
    }

    /// The `approved_via` an approval from this source records on the spec.
    pub fn approval_source(self) -> ApprovalSource {
        match self {
            Self::Lavish => ApprovalSource::Lavish,
            Self::Tracker => ApprovalSource::TrackerComment,
            Self::Card => ApprovalSource::BoardComment,
            Self::Api => ApprovalSource::Api,
        }
    }
}

/// Splits a matched first line and the lines under it into the decision's
/// reasons: the rest of the first line after the verb, then every non-blank
/// line below.
fn reasons_after(body: &str, first: &str, at: usize) -> String {
    let mut reasons: Vec<&str> = Vec::new();
    // `to_ascii_uppercase` never changes a byte's width, so an offset found
    // in the upper-cased line is the same offset — and the same char
    // boundary — in `first`.
    let tail = first
        .get(at..)
        .unwrap_or("")
        .trim_start_matches([':', ' ', '\u{2014}', '-'])
        .trim();
    if !tail.is_empty() {
        reasons.push(tail);
    }
    reasons.extend(
        body.trim_start()
            .lines()
            .skip(1)
            .map(str::trim)
            .filter(|line| !line.is_empty()),
    );
    reasons.join("\n")
}

/// Review decisions a person leaves as a tracker comment, matched on the
/// first line: `RCA: APPROVED`, `RCA: CHANGES …`, `REPLAN …`. Chosen to read
/// as something somebody would type into a ticket.
///
/// Case-insensitive because this is typed into a comment box, not submitted
/// through a form. `REPLAN` must be the whole word: without that check
/// "Replanning the sprint after this lands" would restart somebody's work.
pub fn parse_tracker_decision(body: &str) -> Option<(Decision, String)> {
    const APPROVE: &str = "RCA: APPROVED";
    const CHANGES: &str = "RCA: CHANGES";
    const REPLAN: &str = "REPLAN";

    let first = body.trim_start().lines().next()?.trim();
    let upper = first.to_ascii_uppercase();
    let (decision, at) = if upper.starts_with(APPROVE) {
        (Decision::Approve, APPROVE.len())
    } else if upper.starts_with(CHANGES) {
        (Decision::RequestChanges, CHANGES.len())
    } else if upper == REPLAN
        || upper
            .strip_prefix(REPLAN)
            .is_some_and(|rest| rest.starts_with(':') || rest.starts_with(' '))
    {
        (Decision::Replan, REPLAN.len())
    } else {
        return None;
    };
    Some((decision, reasons_after(body, first, at)))
}

/// How a tracker comment's decision is named back to the people on the
/// ticket.
pub fn tracker_label(decision: Decision) -> &'static str {
    match decision {
        Decision::Approve => "RCA: APPROVED",
        Decision::RequestChanges => "RCA: CHANGES",
        Decision::Replan => "REPLAN",
    }
}

/// Decisions a person leaves as a card comment, matched on the first line:
/// `APPROVE` (or the older `PLAN: APPROVE`), `REQUEST CHANGES: …`,
/// `REPLAN: …`. Case-insensitive.
///
/// Stricter than the tracker grammar about what may follow the verb — the end
/// of the line, a colon, or a dash — so ordinary prose that happens to open
/// with one of these words ("Approve once CI is green?", "Request changes to
/// the header copy") is not a decision.
pub fn parse_card_decision(body: &str) -> Option<(Decision, String)> {
    const VERBS: &[(&str, Decision)] = &[
        ("PLAN: APPROVE", Decision::Approve),
        ("APPROVE", Decision::Approve),
        ("REQUEST CHANGES", Decision::RequestChanges),
        ("REPLAN", Decision::Replan),
    ];
    let first = body.trim_start().lines().next()?.trim();
    let upper = first.to_ascii_uppercase();
    VERBS.iter().find_map(|(verb, decision)| {
        let rest = upper.strip_prefix(verb)?;
        let ends_the_verb = rest.is_empty()
            || rest.starts_with(':')
            || rest.starts_with(" -")
            || rest.starts_with(" \u{2014}")
            || rest.starts_with('\u{2014}');
        ends_the_verb.then(|| (*decision, reasons_after(body, first, verb.len())))
    })
}

/// One decision as recorded on the card, oldest first in
/// [`IssueExtMetadata::approvals`]. The audit trail of who decided what,
/// where; `spec.approved_at` remains what the gates read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ApprovalRecord {
    pub decision: Decision,
    /// Who decided, as the surface named them. For reading, not for
    /// deciding: each surface checked its author before this was written.
    pub by: String,
    pub source: DecisionSource,
    #[serde(default)]
    pub comment: String,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
pub struct SpecRef {
    /// Path of the spec/RCA HTML artifact relative to the workspace root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Set when the spec/RCA is approved, by whichever channel approved it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approved_at: Option<DateTime<Utc>>,
    /// Who approved, where the channel knew. For reading, not for deciding: a
    /// display name is neither unique nor stable, and the channel that
    /// verified the person is recorded separately.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approved_by: Option<String>,
    /// Which channel the approval came through; see [`ApprovalSource`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approved_via: Option<ApprovalSource>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ConfidenceLevel {
    Low,
    Medium,
    High,
}

/// Outcome the planning agent records when its spec/RCA is done. Gates the
/// Planning → Executing transition: specs need `open_questions == 0`, RCAs
/// need `confidence == High`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
pub struct PlanningVerdict {
    /// "spec" (story) or "rca" (bug).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open_questions: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<ConfidenceLevel>,
    /// One sentence naming the missing confirmation when confidence is
    /// below high (e.g. "no runtime repro — needs the CloudWatch trace
    /// for generation 3775956"). Rendered on the Slack escalation card
    /// and mined by the RCA-confidence audit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// Plain-language version of `summary` for the people fielding customer
    /// questions: what broke, who felt it, what the fix does — no jargon, no
    /// file paths. Posted as a Jira comment when the RCA is filed, so account
    /// managers get the story without reading the RCA.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recorded_at: Option<DateTime<Utc>>,
    /// What the ticket is waiting for and who can supply it, when planning
    /// stopped on something; see [`PlanningAsk`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask: Option<PlanningAsk>,
    /// The external system the failure path runs through, when it runs
    /// through one — "Salesforce", "the customer's SSO provider".
    ///
    /// Recorded whatever the analysis concludes the cause is, because it is a
    /// fact about how far the reproduction could reach rather than a
    /// conclusion: the development tenant cannot hold the customer's
    /// credentials, permissions or data, so anything that went wrong on the
    /// far side of that boundary looks from here like nothing went wrong at
    /// all.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub integration: Option<String>,
}

/// What a parked ticket needs, and who can give it.
///
/// Deliberately not `confidence_reason`. That field explains to an engineer
/// why an analysis is not certain — a statement about the work. This is a
/// request to a person. They were the same field until a daily update told an
/// account manager his ticket was blocked on him and then quoted him an
/// unidentified root cause, which he could neither answer nor ignore.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PlanningAsk {
    pub audience: AskAudience,
    /// The request itself: what to do or supply, phrased as a request and
    /// addressed to that audience. Not a description of what is unknown — the
    /// reader has to be able to act on the sentence.
    pub request: String,
}

/// Who can answer a parked ticket's question.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum AskAudience {
    /// Whoever reported it, or is fielding the customer: a detail about the
    /// customer's data, an account, a screenshot, a decision only they can
    /// make.
    Reporter,
    /// Someone on the engineering team: a root cause, a scope decision, a
    /// question about the code. A reporter handed one of these cannot act on
    /// it, and the ticket sits still while they work out it was never theirs.
    Engineer,
}

impl AskAudience {
    /// Reads the word an agent passed, or `None` when it is neither. Unknown
    /// is not the same as `Engineer`: guessing here would route a real
    /// customer question away from the customer.
    pub fn parse(word: &str) -> Option<Self> {
        match word.trim().to_ascii_lowercase().as_str() {
            "reporter" => Some(Self::Reporter),
            "engineer" => Some(Self::Engineer),
            _ => None,
        }
    }
}

/// Who a review finding is for, which is what decides where it goes.
///
/// The distinction the orchestrator acts on: `Systematic` and `Correctness` are
/// answerable from the codebase, so a fix round can absorb them; `Product` is a
/// question only a human can settle, so spending a round on one wastes the
/// round and comes back unanswered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum FindingClass {
    /// A rule the codebase already answers: a duplicated helper, a diverged
    /// pattern, a cited convention broken, an unreadable name, a comment that
    /// narrates the change, a missing test.
    Systematic,
    /// A real bug with a stated failure scenario, or a security/data-loss risk.
    Correctness,
    /// A judgment call: is this the intended behaviour, is this trade-off
    /// acceptable, is this scope creep.
    Product,
}

/// How much a finding matters. Only `Blocker` and `Major` send work back — see
/// [`FindingSeverity::blocks`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum FindingSeverity {
    Nit,
    Minor,
    Major,
    Blocker,
}

impl FindingSeverity {
    /// Whether this severity is worth another round of agent time.
    ///
    /// Nits and minors are worth writing down and not worth sending work back
    /// for: a review that spends its round budget on naming never gets to the
    /// bug in round three.
    pub fn blocks(self) -> bool {
        matches!(self, Self::Major | Self::Blocker)
    }
}

/// One thing the independent reviewer found.
///
/// `line` is text, not a number, because it doubles as a merge-request anchor:
/// GitLab accepts `118`, a range `10:15`, and `old:7` for a line the branch
/// deleted.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ReviewFinding {
    pub class: FindingClass,
    pub severity: FindingSeverity,
    /// Repo-relative path; absent for a finding about the change as a whole.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<String>,
    /// One-line claim, for manifests and log lines.
    pub title: String,
    /// The argument: what it is, why it matters, what to do instead. Written to
    /// be read alone, because on a merge request it will be.
    pub body: String,
}

/// What the independent reviewer decided, typed so the orchestrator routes on
/// structure instead of parsing the prose it also posts as a comment.
///
/// Absent on tickets reviewed before typed findings existed. Overwritten, never
/// cleared, so a reader must check `recorded_at` against the round it is
/// handling: a reviewer that skips the call leaves the previous round's findings
/// in place, and routing on those repeats work already done. Callers fall back
/// to the verdict comment's first line.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
pub struct ReviewVerdict {
    /// True when the reviewer approved. The findings still matter: an approval
    /// can carry product questions and nits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approved: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub findings: Vec<ReviewFinding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recorded_at: Option<DateTime<Utc>>,
}

impl ReviewVerdict {
    /// Findings that earn a fix round: answerable from the codebase, and severe
    /// enough to be worth one.
    pub fn fixable(&self) -> impl Iterator<Item = &ReviewFinding> {
        self.findings.iter().filter(|f| {
            f.severity.blocks()
                && matches!(
                    f.class,
                    FindingClass::Systematic | FindingClass::Correctness
                )
        })
    }

    /// Questions for a human, in the order the reviewer wrote them.
    pub fn product_questions(&self) -> impl Iterator<Item = &ReviewFinding> {
        self.findings
            .iter()
            .filter(|f| matches!(f.class, FindingClass::Product))
    }
}

/// Strips agent tool-call scaffolding out of a free-text field.
///
/// Agents occasionally close their own tool syntax inside a value, so the
/// stored text ends up as `...real summary</planning_summary><parameter
/// name="planning_client_summary">...`. On 2026-08-13 that raw XML was
/// rendered as a bullet on a Slack escalation card in front of the whole team
/// (BUG-9994). Everything from the first scaffolding marker onwards is dropped;
/// if that would empty the field, the original is kept so a mangled value is
/// still visible rather than silently blank.
pub fn strip_agent_scaffolding(text: &str) -> String {
    const MARKERS: [&str; 4] = ["</", "<parameter", "<function_calls", "<invoke"];
    let cut = MARKERS
        .iter()
        .filter_map(|marker| text.find(marker))
        .min()
        .unwrap_or(text.len());
    let trimmed = text[..cut].trim();
    if trimmed.is_empty() {
        text.trim().to_string()
    } else {
        trimmed.to_string()
    }
}

/// Present while a human owns the ticket in Jira (assignee moved off the
/// agent account). Automation finishes the in-flight stage, posts the RCA
/// to Jira, and otherwise goes hands-off: no new launches, no stage
/// advances, no feedback injection. Cleared (all fields None) when the
/// assignee returns to the agent; resuming still requires the standard
/// backward-then-forward re-arm gesture because the claim marker survives.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
pub struct HumanTakeover {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    /// Present = takeover active. The whole struct with every field None is
    /// the "cleared" state (merge semantics can't remove a key).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detected_at: Option<DateTime<Utc>>,
    /// Set once the finished RCA has been posted to the Jira ticket.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rca_posted_at: Option<DateTime<Utc>>,
}

/// Written on a ticket that shipped, when the reporter says the fix did not
/// hold. The follow-up is a fresh linked ticket rather than a re-arm of this
/// one, so each attempt keeps its own MR, its own review and its own history,
/// and the chain is walkable from either end in Jira.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
pub struct ReopenState {
    /// Key of the most recent follow-up filed from this ticket. Retained after
    /// that follow-up closes: a later report chains a new one rather than
    /// reusing it, so `followup_key` names the newest link, not the only one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub followup_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filed_at: Option<DateTime<Utc>>,
    /// Newest Jira comment the reopen scan has considered. Advanced whether or
    /// not it filed anything, so one report cannot fire twice — and so a
    /// comment that arrived before the feature existed is never treated as new.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_seen_comment_at: Option<DateTime<Utc>>,
    /// Jira status the original held before it was parked in Blocked, so it can
    /// be put back exactly where it was once the follow-up ships. Set while the
    /// original is parked and cleared when it is restored, so its presence is
    /// what marks a ticket as waiting on its follow-up.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prior_status: Option<String>,
}

/// How far a merged change has travelled toward production.
///
/// Staging is the only environment that runs the artefact production will be
/// cut from, and nothing in the orchestrator has ever known when a merged change
/// arrives there. Until it does, "merged" and "shippable" are the same word for
/// two different things.
///
/// The replay outcome joins this record when phase 03 lands. Recording the
/// journey first is what makes the outcome mean anything: a card whose change is
/// not yet on staging has not failed verification, it has not been verified, and
/// a single field cannot say both.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct VerifyState {
    /// The commit the merge produced, read from the host rather than the local
    /// pull-request row, which goes away with its workspace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merge_sha: Option<String>,
    /// The commit the newest successful staging deploy carried, at the moment
    /// this change was found to be contained in it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub staged_sha: Option<String>,
    /// When that deploy finished — not when the orchestrator noticed. The gap
    /// between merging and this is the number nobody can currently quote.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub staged_at: Option<DateTime<Utc>>,
    /// The newest pre-merge verification of this card's branch; see
    /// [`PreMrVerify`]. Overwritten each attempt: the trail lives in the
    /// issue comments, this is what the Execute gate routes on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pre_mr: Option<PreMrVerify>,
    /// The newest run somebody asked for by hand; see [`OnDemandVerify`].
    ///
    /// Kept apart from `pre_mr` because that record is the Execute gate's
    /// routing state: its attempt count and its outcome decide whether a card
    /// advances or goes back to the agent. A question a reviewer asked about
    /// a branch is not a gate decision, and writing the answer where the gate
    /// reads it would make one out of it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_demand: Option<OnDemandVerify>,
}

/// One verification of this card's branch before the merge request is
/// reviewed: the script runner stands the app up from the repository's
/// `.prepcook/scripts.toml` in the implementation worktree, runs its `test`
/// script, and tears it down.
///
/// The gate that advances a card out of Execute records the start here, holds
/// the card while the run is in flight, and records the outcome when it lands.
/// A run that never records an outcome — the orchestrator restarted under it —
/// is recognisable by `finished_at` staying empty past the run's natural
/// length, which is how the gate knows to start again rather than wait forever.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PreMrVerify {
    /// The app run's id.
    pub token: String,
    pub started_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<DateTime<Utc>>,
    /// The run's outcome: `ok`, `setup-failed`, `build-failed`,
    /// `start-failed`, `not-ready`, `seed-failed` or `test-failed` (the
    /// branch's fault), or `manifest-missing`, `manifest-invalid`,
    /// `secret-missing`, `port-unavailable`, `busy` or `error` (the setup's).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    /// The worktree's HEAD when the run started.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha: Option<String>,
    /// Runs started for this card so far, across follow-up rounds.
    #[serde(default)]
    pub attempts: u32,
}

/// One verification a person asked for, by commenting `VERIFY:` on the card.
///
/// It reports and stops there. Nothing routes on it: no card advances or is
/// held back because of what it found, and a failure starts no follow-up turn.
/// Somebody wanted to know whether the branch stands up and passes its tests,
/// and the answer is posted where they asked.
///
/// There is no attempt cap, unlike the gate's record. A gate retrying twice
/// means it cannot get past a failure; a person asking twice means they want
/// to know again, usually because something changed in between.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct OnDemandVerify {
    /// The app run's id, as in [`PreMrVerify`].
    pub token: String,
    /// The implementation workspace's branch.
    pub branch: String,
    pub requested_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<DateTime<Utc>>,
    /// The run's outcome once it has one; see [`PreMrVerify::result`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
}

/// Bookkeeping the orchestrator's autonomy mode writes so its transitions
/// are idempotent across ticks: one Slack escalation per issue, a bounded
/// review-fix loop, and no double-handling of a reviewer verdict comment.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
pub struct AutonomyState {
    /// Set when a low-confidence RCA (or review-loop cap) has been
    /// escalated to Slack, so the escalation fires once.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub escalated_at: Option<DateTime<Utc>>,
    /// High-water mark for the Push MR comment watcher: newest MR comment
    /// already injected into the impl agent (never re-delivered).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mr_last_comment_at: Option<DateTime<Utc>>,
    /// Git-host login of whoever wrote that comment. Recorded because the
    /// reporter-facing daily update names the reviewer, and the watcher is
    /// already reading the comment that says who it is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mr_last_commenter: Option<String>,
    /// Set when MR review feedback was judged to reject the approach and the
    /// re-plan escalation has fired, so it fires once per verdict.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replan_flagged_at: Option<DateTime<Utc>>,
    /// Newest `VERIFY:` comment already acted on. Its own watermark rather
    /// than the card-comment one: that sweep and this one own different
    /// prefixes, and sharing a mark would let either consume the other's work.
    /// Seeded on first sight, so shipping this replays no old comment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_verify_request_at: Option<DateTime<Utc>>,
    /// Head SHA of a failed pipeline already handled, so one red pipeline
    /// produces one fix attempt rather than one per poll.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pipeline_handled_sha: Option<String>,
    /// Set while a CI-fix follow-up is running: the commits it produces are
    /// held back until their diff is checked against what the merge
    /// request had already changed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pipeline_fix_pending_at: Option<DateTime<Utc>>,
    /// Consecutive CI-fix attempts, capped like the review loop.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pipeline_fix_rounds: Option<u32>,
    /// Head SHA of a conflicting merge request already handed back to the
    /// agent for a rebase, so one conflict produces one follow-up rather than
    /// one per poll. Cleared when the MR reports mergeable again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rebase_handled_sha: Option<String>,
    /// Newest Jira comment whose @-mention of the agent was already handled
    /// while the ticket was mid-pipeline. Separate from the triage watermark:
    /// the two sweeps own different columns and must not consume each other's
    /// work. Seeded on first sight so enabling the sweep replays nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mention_last_comment_at: Option<DateTime<Utc>>,
    /// Newest Jira comment already triaged after the ticket reached a
    /// terminal column. Seeded to the newest existing comment the first time
    /// a done ticket is scanned, so shipping the feature never triages
    /// historical threads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub triage_last_comment_at: Option<DateTime<Utc>>,
    /// REQUEST CHANGES cycles consumed (capped by
    /// `orchestrator.autonomy_review_iterations`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_rounds: Option<u32>,
    /// Set while a review-fix follow-up is running in the implementation
    /// workspace; cleared when the reviewer is re-armed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fix_started_at: Option<DateTime<Utc>>,
    /// `created_at` of the newest reviewer verdict comment already handled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_review_comment_at: Option<DateTime<Utc>>,
    /// `created_at` of the newest `PLAN:` gate comment already handled
    /// (teammate feedback/approval on a waiting plan).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_plan_comment_at: Option<DateTime<Utc>>,
    /// `created_at` of the newest card comment already read for a decision
    /// (`APPROVE`, `REQUEST CHANGES:`, `REPLAN:`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_card_decision_at: Option<DateTime<Utc>>,
    /// Reasons given with a re-plan from the card or the API, waiting to be
    /// handed to the next planning agent; cleared when that agent starts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replan_feedback: Option<String>,
    /// Slack channel id the planning escalation was posted to (bot-token
    /// path only). Present means the escalation thread is being polled for
    /// teammate replies while the ticket stays in Planning.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slack_channel: Option<String>,
    /// `ts` of the escalation message (the reply thread's root).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slack_thread_ts: Option<String>,
    /// `ts` of the newest thread reply already delivered to the agent, so
    /// each teammate reply is injected once.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slack_last_reply_ts: Option<String>,
    /// `created` of the newest Jira comment already handled while the ticket
    /// was in Planning (comments are polled and injected into the agent).
    /// Seeded on the first poll so comments already imported into the
    /// description are not re-injected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_jira_comment_at: Option<DateTime<Utc>>,
    /// Newest Jira review command (`RCA: APPROVED`, `RCA: CHANGES`, `REPLAN`)
    /// already acted on.
    ///
    /// Deliberately not the comment watermark above. That one tracks what has
    /// been *shown* to the planning agent and this one tracks what has been
    /// *done*, and a re-plan needs both: the command restarts planning, and
    /// the same comment then reaches the new agent as the reasons it restarted.
    /// One mark would let either consume the other's copy.
    ///
    /// Seeded on first sight, so shipping this replays no old comment — and
    /// nothing written before the feature existed is read as an instruction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_jira_command_at: Option<DateTime<Utc>>,
    /// Set once this card's Jira ticket has been renamed from its filing-time
    /// title to one naming the cause its own analysis found.
    ///
    /// Only follow-ups are ever renamed, and only once: a title that keeps
    /// moving is worse than one that is merely approximate, and anyone who has
    /// the ticket open watches it change under them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub followup_renamed_at: Option<DateTime<Utc>>,
    /// Newest kanban-card comment addressed to the agent (`VERIFY:`,
    /// `NUDGE:`, any `WORD:` opener that is not a gate's own) already
    /// delivered to the implementation workspace. Seeded on first sight so
    /// shipping the feature replays nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_card_comment_at: Option<DateTime<Utc>>,
}

/// Prep Cook workflow keys inside `extension_metadata`. All optional so blobs
/// written by other tools (or older versions) always deserialize.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
pub struct IssueExtMetadata {
    /// Planning-stage outcome; see [`PlanningVerdict`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub planning: Option<PlanningVerdict>,
    /// Orchestrator claim marker: set before auto-launching a playbook so
    /// other teammates' orchestrators skip the issue.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub playbook_started_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_class: Option<WorkClass>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jira: Option<JiraRef>,
    /// Jira key of the parent epic, for epic tabs/progress.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub epic_key: Option<String>,
    /// Name of the RCA-graph node this ticket was raised to investigate. Its
    /// presence is what marks a ticket as an investigation: the work is to
    /// explain why a class of bug recurs and recommend what would stop it, not
    /// to fix one instance, so it takes a different playbook and never reaches
    /// the implementing stages.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub investigation_node: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spec: Option<SpecRef>,
    /// Merge request URL once pushed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mr_url: Option<String>,
    /// Newest independent-review outcome; see [`ReviewVerdict`]. Overwritten
    /// each round: the prose trail lives in the issue comments, this is what
    /// the current round routes on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review: Option<ReviewVerdict>,
    /// Autonomy-mode bookkeeping; see [`AutonomyState`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autonomy: Option<AutonomyState>,
    /// Human-takeover pause state; see [`HumanTakeover`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub human_takeover: Option<HumanTakeover>,
    /// Reopen bookkeeping for a ticket that already shipped; see
    /// [`ReopenState`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reopen: Option<ReopenState>,
    /// How far a merged change has travelled toward production; see
    /// [`VerifyState`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verify: Option<VerifyState>,
    /// Every plan decision made on this card, oldest first; see
    /// [`ApprovalRecord`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub approvals: Vec<ApprovalRecord>,
}

impl IssueExtMetadata {
    /// Lenient parse from the raw blob; returns defaults on any mismatch.
    pub fn from_value(value: &Value) -> Self {
        serde_json::from_value(value.clone()).unwrap_or_default()
    }

    /// Merge this struct's keys into an existing blob, preserving any
    /// foreign keys other tools may have written.
    pub fn merge_into(&self, existing: &Value) -> Value {
        let mut base = match existing {
            Value::Object(map) => map.clone(),
            _ => serde_json::Map::new(),
        };
        if let Ok(Value::Object(ours)) = serde_json::to_value(self) {
            for (k, v) in ours {
                base.insert(k, v);
            }
        }
        Value::Object(base)
    }
}

#[cfg(test)]
mod scaffolding_tests {
    use super::strip_agent_scaffolding;

    /// The BUG-9994 shape: a summary that swallowed the agent's own closing tag
    /// and the next parameter block.
    #[test]
    fn drops_tool_syntax_and_everything_after_it() {
        let raw = "The page rebuilt the whole table on every click.</planning_summary>\
                   <parameter name=\"planning_client_summary\">The page that lists…";
        assert_eq!(
            strip_agent_scaffolding(raw),
            "The page rebuilt the whole table on every click."
        );
    }

    #[test]
    fn leaves_ordinary_prose_alone() {
        let clean = "Root cause: the LUID rotates on republish, so the stored id goes stale.";
        assert_eq!(strip_agent_scaffolding(clean), clean);
    }

    /// A value that is nothing but scaffolding stays visible: a blank field on
    /// the card hides the fact that something went wrong.
    #[test]
    fn all_scaffolding_keeps_the_original() {
        let raw = "</planning_summary>";
        assert_eq!(strip_agent_scaffolding(raw), "</planning_summary>");
    }
}
