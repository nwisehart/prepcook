//! Cards made and changed from outside the board: `POST /api/cards`,
//! `GET /api/cards/{id}` and `POST /api/cards/{id}/convert` (see
//! `docs/api.md`).
//!
//! The rules live here as plain functions over a card's metadata and its
//! board's columns, and [`create_card`], [`view_card`] and [`convert_card`]
//! apply them through a [`CardHost`] — the orchestrator, which holds the
//! board client, the tags and the notification targets. Tests use an
//! in-memory host.

use std::sync::Arc;

use api_types::{
    ApprovalRecord, CardAttachment, CardOrigin, ConfidenceLevel, ConversionRecord, Decision,
    DecisionSource, HarnessRecord, Issue, IssueExtMetadata, ProjectStatus, TriageReport, WorkClass,
};
use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use ts_rs::TS;
use uuid::Uuid;

use crate::services::{
    approvals::decisions, notifications::Event, stage_orchestrator::public_ui_base,
};

/// Longest title a card takes; a tracker summary is capped at 255 too.
pub const MAX_TITLE_CHARS: usize = 255;

/// `POST /api/cards`.
#[derive(Debug, Clone, Deserialize, TS)]
pub struct CreateCardRequest {
    /// The board; defaults to `orchestrator.default_board`, else the first
    /// configured board.
    #[serde(default)]
    #[ts(optional)]
    pub project_id: Option<Uuid>,
    pub title: String,
    /// Markdown.
    #[serde(default)]
    #[ts(optional)]
    pub description: Option<String>,
    /// `investigation`, `bug` or `story`.
    pub work_class: WorkClass,
    pub origin: CardOrigin,
    #[serde(default)]
    pub attachments: Vec<CardAttachment>,
    /// Anything else the filer wants kept with the card; stored under
    /// `external` in its metadata.
    #[serde(default)]
    #[ts(optional)]
    pub metadata: Option<Value>,
    /// `true` places the card in Plan, where the orchestrator picks it up;
    /// `false` (the default) leaves it in To Do.
    #[serde(default)]
    pub start: bool,
}

/// What `POST /api/cards` answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct CreatedCard {
    pub id: Uuid,
    /// The card on the board.
    pub url: String,
    /// The card's board key (`PC-12`).
    pub key: String,
}

/// `POST /api/cards/{id}/convert`.
#[derive(Debug, Clone, Deserialize, TS)]
pub struct ConvertCardRequest {
    /// `bug` or `story`.
    pub work_class: WorkClass,
    /// Who is converting, as the caller names them.
    #[serde(default)]
    #[ts(optional)]
    pub by: Option<String>,
}

/// What a conversion did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct ConvertedCard {
    pub id: Uuid,
    pub from: WorkClass,
    pub to: WorkClass,
    /// The column the card is in now.
    pub column: String,
    /// True when the RCA and harness counted as the plan and the card went
    /// straight to Executing.
    pub plan_carried_over: bool,
}

/// `GET /api/cards/{id}`.
#[derive(Debug, Clone, Serialize, TS)]
pub struct CardView {
    pub id: Uuid,
    pub key: String,
    pub title: String,
    pub url: String,
    pub project_id: Uuid,
    /// Where the card is in the workflow: `backlog`, `planning`, `triaged`,
    /// `executing`, `reviewing`, `pushed_mr`, `closed`, `cleanup`, or `other`
    /// for a column Prepcook does not know. Read from the card as well as its
    /// column (see [`card_status`]): a triaged investigation is `triaged` in
    /// Plan, and a card in Shipped is `pushed_mr` until its pull request merges
    /// and `closed` after.
    pub status: String,
    /// The column's name as the board shows it.
    pub column: String,
    pub work_class: Option<WorkClass>,
    /// The investigation's triage report, once it has one.
    pub report: Option<TriageReport>,
    pub confidence: Option<ConfidenceLevel>,
    pub harness: Option<HarnessRecord>,
    pub comments: usize,
    /// The RCA in the knowledge store, when one is filed for the card.
    pub rca_id: Option<Uuid>,
    pub origin: Option<CardOrigin>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CardError {
    /// The request itself is wrong; nothing was changed.
    #[error("{0}")]
    Invalid(String),
    #[error("no card {0}")]
    UnknownCard(Uuid),
    #[error("no board {0}")]
    UnknownBoard(Uuid),
    /// The request is well-formed but the card or board cannot take it.
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    Unavailable(String),
    #[error("{0}")]
    Failed(String),
}

/// What the card API needs from the system it runs in. The orchestrator
/// implements it; tests use an in-memory board.
#[async_trait]
pub trait CardHost: Send + Sync {
    /// The board a request without `project_id` files into.
    async fn default_board(&self) -> Option<Uuid>;

    /// The board's columns; [`CardError::UnknownBoard`] for a board that does
    /// not exist.
    async fn statuses(&self, project_id: Uuid) -> Result<Vec<ProjectStatus>, CardError>;

    /// Creates the card, assigns it to the board's owner (whose orchestrator
    /// works it) and tags it with its class.
    async fn create(
        &self,
        request: api_types::CreateIssueRequest,
        work_class: WorkClass,
    ) -> Result<Issue, CardError>;

    /// `Ok(None)` for a card that does not exist.
    async fn issue(&self, issue_id: Uuid) -> Result<Option<Issue>, CardError>;

    /// The class a card's tags name, for a card that has none recorded.
    async fn class_from_tags(&self, issue: &Issue) -> Option<WorkClass>;

    async fn comment_count(&self, issue: &Issue) -> Result<usize, CardError>;

    /// The card's RCA in the knowledge store, when one is filed.
    async fn rca_id(&self, issue: &Issue) -> Option<Uuid>;

    /// Replaces the card's metadata through the path that runs approval side
    /// effects (spec snapshot, RCA filing).
    async fn write_metadata(&self, issue: &Issue, metadata: Value) -> Result<(), CardError>;

    /// Swaps the card's class tag. Best effort.
    async fn swap_class_tag(&self, issue: &Issue, from: WorkClass, to: WorkClass);

    async fn move_card(&self, issue: &Issue, status_id: Uuid) -> Result<(), CardError>;

    /// Best-effort audit comment on the card.
    async fn post_comment(&self, issue: &Issue, text: &str);

    /// Sends an event to the board's notification targets.
    async fn notify(&self, project_id: Uuid, event: Event);
}

/// The card's page on the board.
pub fn card_url(project_id: Uuid, issue_id: Uuid) -> String {
    format!(
        "{}/projects/{}/issues/{}",
        public_ui_base(),
        project_id,
        issue_id
    )
}

/// Where a column sits in the workflow, as [`CardView::status`] names it. A
/// Triaged column is one a board added; Shipped, and an older board's Push MR
/// or Done, hold a card while its pull request is open.
pub fn status_word(column: &str) -> &'static str {
    let name = column.trim().to_ascii_lowercase();
    match name.as_str() {
        "to do" | "unassigned" | "backlog" => "backlog",
        "plan" | "planning" => "planning",
        "triaged" => "triaged",
        "execute" | "executing" => "executing",
        "review" | "reviewing" => "reviewing",
        "shipped" | "done" | "push mr" | "pushed mr" => "pushed_mr",
        "close" | "closed" | "deployed" => "closed",
        "clean up" | "clean-up" | "cleanup" => "cleanup",
        _ => "other",
    }
}

/// Where the card is, from its column and its own state: triage and a merge
/// are states of the card rather than columns, so a triaged investigation in
/// Plan reads `triaged` and a merged card in Shipped reads `closed`, as they
/// did when each had a column of its own.
pub fn card_status(column: &str, metadata: &IssueExtMetadata) -> &'static str {
    match status_word(column) {
        "planning" if metadata.is_triaged() => "triaged",
        "pushed_mr" if metadata.merged_at.is_some() => "closed",
        word => word,
    }
}

/// The board's column for a status word, the first by sort order when a
/// board names two the same way.
pub fn column_for<'a>(statuses: &'a [ProjectStatus], word: &str) -> Option<&'a ProjectStatus> {
    let mut matching: Vec<&ProjectStatus> = statuses
        .iter()
        .filter(|status| status_word(&status.name) == word)
        .collect();
    matching.sort_by_key(|status| status.sort_order);
    matching.into_iter().next()
}

/// Checks a `POST /api/cards` body before anything is written.
pub fn validate(request: &CreateCardRequest) -> Result<(), CardError> {
    let title = request.title.trim();
    if title.is_empty() {
        return Err(CardError::Invalid("title is required".to_string()));
    }
    if title.chars().count() > MAX_TITLE_CHARS {
        return Err(CardError::Invalid(format!(
            "title is longer than {MAX_TITLE_CHARS} characters"
        )));
    }
    if !matches!(
        request.work_class,
        WorkClass::Investigation | WorkClass::Bug | WorkClass::Story
    ) {
        return Err(CardError::Invalid(format!(
            "work_class must be investigation, bug or story, not {}",
            request.work_class.as_str()
        )));
    }
    if request.origin.reference.trim().is_empty() {
        return Err(CardError::Invalid(
            "origin.ref is required: the thread URL or incident id the card came from".to_string(),
        ));
    }
    for attachment in &request.attachments {
        if attachment.name.trim().is_empty() {
            return Err(CardError::Invalid(
                "every attachment needs a name".to_string(),
            ));
        }
        let url = attachment.url.trim();
        if !(url.starts_with("https://") || url.starts_with("http://")) {
            return Err(CardError::Invalid(format!(
                "attachment {:?} needs an http(s) url",
                attachment.name.trim()
            )));
        }
    }
    if let Some(metadata) = &request.metadata
        && !metadata.is_object()
    {
        return Err(CardError::Invalid("metadata must be an object".to_string()));
    }
    Ok(())
}

/// One line naming the filer, linked when its reference is a URL:
/// "Filed by Tammy from <thread>".
pub fn filed_by_line(origin: &CardOrigin) -> String {
    let reference = origin.reference.trim();
    let from = if reference.starts_with("https://") || reference.starts_with("http://") {
        format!("[{reference}]({reference})")
    } else {
        format!("`{reference}`")
    };
    let reporter = origin
        .reporter
        .as_deref()
        .map(str::trim)
        .filter(|reporter| !reporter.is_empty())
        .map(|reporter| format!(", reported by {reporter}"))
        .unwrap_or_default();
    format!("Filed by {} from {from}{reporter}.", origin.kind.label())
}

/// The card's description: the filer's markdown, the attachments as links
/// (so the planning agent sees them), and who filed it.
pub fn card_description(request: &CreateCardRequest) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(description) = request
        .description
        .as_deref()
        .map(str::trim)
        .filter(|description| !description.is_empty())
    {
        parts.push(description.to_string());
    }
    if !request.attachments.is_empty() {
        let list = request
            .attachments
            .iter()
            .map(|attachment| {
                format!(
                    "- [{}]({})",
                    attachment.name.trim().replace(['[', ']'], ""),
                    attachment.url.trim()
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        parts.push(format!("Attachments:\n{list}"));
    }
    parts.push(filed_by_line(&request.origin));
    parts.join("\n\n")
}

/// The new card's metadata: its class, where it came from, its attachments,
/// and the filer's own metadata kept apart under `external`.
pub fn initial_metadata(request: &CreateCardRequest) -> Value {
    let metadata = IssueExtMetadata {
        work_class: Some(request.work_class),
        origin: Some(CardOrigin {
            kind: request.origin.kind,
            reference: request.origin.reference.trim().to_string(),
            reporter: request
                .origin
                .reporter
                .as_deref()
                .map(str::trim)
                .filter(|reporter| !reporter.is_empty())
                .map(str::to_string),
        }),
        attachments: request
            .attachments
            .iter()
            .map(|attachment| CardAttachment {
                name: attachment.name.trim().to_string(),
                url: attachment.url.trim().to_string(),
            })
            .collect(),
        external: request
            .metadata
            .clone()
            .filter(|value| value.as_object().is_some_and(|object| !object.is_empty())),
        ..Default::default()
    };
    serde_json::to_value(&metadata).unwrap_or_else(|_| serde_json::json!({}))
}

/// Files a card: validates, picks the board and column, creates it.
pub async fn create_card(
    host: &dyn CardHost,
    request: CreateCardRequest,
) -> Result<CreatedCard, CardError> {
    validate(&request)?;
    let project_id = match request.project_id {
        Some(project_id) => project_id,
        None => host.default_board().await.ok_or_else(|| {
            CardError::Invalid(
                "no project_id given and no board is configured under Settings → Orchestrator"
                    .to_string(),
            )
        })?,
    };
    let statuses = host.statuses(project_id).await?;
    let word = if request.start { "planning" } else { "backlog" };
    let status = column_for(&statuses, word).ok_or_else(|| {
        CardError::Conflict(format!(
            "board {project_id} has no {} column",
            if request.start { "Plan" } else { "To Do" }
        ))
    })?;
    let issue = host
        .create(
            api_types::CreateIssueRequest {
                id: None,
                project_id,
                status_id: status.id,
                title: request.title.trim().to_string(),
                description: Some(card_description(&request)),
                priority: None,
                start_date: None,
                target_date: None,
                completed_at: None,
                sort_order: 0.0,
                parent_issue_id: None,
                parent_issue_sort_order: None,
                extension_metadata: initial_metadata(&request),
            },
            request.work_class,
        )
        .await?;
    Ok(CreatedCard {
        id: issue.id,
        url: card_url(project_id, issue.id),
        key: issue.simple_id,
    })
}

/// A card as `GET /api/cards/{id}` shows it; `Ok(None)` for no such card.
pub async fn view_card(host: &dyn CardHost, issue_id: Uuid) -> Result<Option<CardView>, CardError> {
    let Some(issue) = host.issue(issue_id).await? else {
        return Ok(None);
    };
    let statuses = host.statuses(issue.project_id).await?;
    let column = statuses
        .iter()
        .find(|status| status.id == issue.status_id)
        .map(|status| status.name.clone())
        .unwrap_or_default();
    let metadata = IssueExtMetadata::from_value(&issue.extension_metadata);
    let work_class = match metadata.work_class {
        Some(class) => Some(class),
        None => host.class_from_tags(&issue).await,
    };
    let triage = metadata.triage.clone();
    Ok(Some(CardView {
        id: issue.id,
        key: issue.simple_id.clone(),
        title: issue.title.clone(),
        url: card_url(issue.project_id, issue.id),
        project_id: issue.project_id,
        status: card_status(&column, &metadata).to_string(),
        column,
        work_class,
        report: triage.as_ref().map(|triage| triage.report.clone()),
        confidence: triage.as_ref().and_then(|triage| triage.confidence),
        harness: triage.and_then(|triage| triage.harness),
        comments: host.comment_count(&issue).await?,
        rca_id: host.rca_id(&issue).await,
        origin: metadata.origin,
    }))
}

/// Whether an investigation's work counts as a bug's plan: a finished triage
/// report, an RCA verdict at high confidence, and a harness still on disk.
/// The Executing gate asks for exactly that verdict, so anything less is
/// planned again rather than parked at the gate.
pub fn plan_exists(metadata: &IssueExtMetadata) -> bool {
    let rca_at_high = metadata.planning.as_ref().is_some_and(|planning| {
        planning
            .kind
            .as_deref()
            .is_none_or(|kind| kind.eq_ignore_ascii_case("rca"))
            && planning.confidence == Some(ConfidenceLevel::High)
    });
    let harness = metadata
        .triage
        .as_ref()
        .and_then(|triage| triage.harness.as_ref())
        .is_some_and(|harness| harness.path.is_some());
    rca_at_high && harness
}

/// Which conversions exist: an investigation becomes a bug (the usual case,
/// with its plan carried over when there is one) or a story (always planned
/// again: an RCA is not a spec).
pub fn check_conversion(from: Option<WorkClass>, to: WorkClass) -> Result<WorkClass, CardError> {
    let Some(from) = from else {
        return Err(CardError::Conflict(
            "the card has no work class; only an investigation card converts".to_string(),
        ));
    };
    if from != WorkClass::Investigation {
        return Err(CardError::Conflict(format!(
            "only an investigation card converts; this card is a {}",
            from.as_str()
        )));
    }
    if !matches!(to, WorkClass::Bug | WorkClass::Story) {
        return Err(CardError::Invalid(format!(
            "an investigation converts to a bug or a story, not {}",
            to.as_str()
        )));
    }
    Ok(from)
}

/// The card's metadata after a conversion: the new class, the conversion on
/// record, the claim cleared so the next tick starts the right stage, and —
/// when the plan carries over — the converter's decision recorded as the
/// plan's approval, which is what the Executing gate reads. Everything else
/// (the triage report, the RCA, the harness record, the origin) stays.
pub fn converted_metadata(
    metadata: &Value,
    from: WorkClass,
    to: WorkClass,
    by: &str,
    carries_plan: bool,
) -> Value {
    let now = Utc::now();
    let mut blob = if carries_plan {
        decisions::recorded(
            metadata,
            &ApprovalRecord {
                decision: Decision::Approve,
                by: by.to_string(),
                source: DecisionSource::Api,
                comment: format!(
                    "converted from {} to {}: the RCA and harness are the plan",
                    from.as_str(),
                    to.as_str()
                ),
                at: now,
            },
        )
    } else {
        metadata.clone()
    };
    let typed = IssueExtMetadata::from_value(metadata);
    let mut conversions = typed.conversions;
    conversions.push(ConversionRecord {
        from,
        to,
        by: by.to_string(),
        at: now,
    });
    if let Value::Object(map) = &mut blob {
        map.insert(
            "work_class".to_string(),
            Value::String(to.as_str().to_string()),
        );
        map.insert(
            "conversions".to_string(),
            serde_json::to_value(&conversions).unwrap_or(Value::Array(Vec::new())),
        );
        map.remove("playbook_started_at");
        if carries_plan && let Some(Value::Object(planning)) = map.get_mut("planning") {
            planning.insert("kind".to_string(), Value::String("rca".to_string()));
        }
    }
    blob
}

/// Whether the card's last conversion was to `to` and has already been
/// written: a request repeated after its move failed finds the card in its
/// new class, and then only moves it.
fn already_converted(metadata: &IssueExtMetadata, to: WorkClass) -> bool {
    metadata.work_class == Some(to)
        && metadata
            .conversions
            .last()
            .is_some_and(|last| last.from == WorkClass::Investigation && last.to == to)
}

/// Converts an investigation card: swaps its class, keeps everything it has,
/// and moves it to Executing when its work counts as the plan
/// ([`plan_exists`]), else to Plan.
pub async fn convert_card(
    host: &dyn CardHost,
    issue_id: Uuid,
    to: WorkClass,
    by: &str,
) -> Result<ConvertedCard, CardError> {
    let issue = host
        .issue(issue_id)
        .await?
        .ok_or(CardError::UnknownCard(issue_id))?;
    let metadata = IssueExtMetadata::from_value(&issue.extension_metadata);
    let recorded = match metadata.work_class {
        Some(class) => Some(class),
        None => host.class_from_tags(&issue).await,
    };
    let resuming = already_converted(&metadata, to);
    let from = if resuming {
        WorkClass::Investigation
    } else {
        check_conversion(recorded, to)?
    };
    let carries_plan = to == WorkClass::Bug
        && if resuming {
            // The conversion recorded the plan's approval when it carried one.
            metadata
                .spec
                .as_ref()
                .is_some_and(|spec| spec.approved_at.is_some())
        } else {
            plan_exists(&metadata)
        };
    let statuses = host.statuses(issue.project_id).await?;
    let word = if carries_plan {
        "executing"
    } else {
        "planning"
    };
    let status = column_for(&statuses, word).ok_or_else(|| {
        CardError::Conflict(format!(
            "the board has no {} column",
            if carries_plan { "Execute" } else { "Plan" }
        ))
    })?;
    let by = by.trim();
    let by = if by.is_empty() { "someone" } else { by };

    // The metadata goes first: the move is checked against it (the board
    // refuses to move an investigation to Executing).
    if !resuming {
        host.write_metadata(
            &issue,
            converted_metadata(&issue.extension_metadata, from, to, by, carries_plan),
        )
        .await?;
    }
    host.swap_class_tag(&issue, from, to).await;
    if issue.status_id != status.id {
        host.move_card(&issue, status.id).await?;
    }
    let column = status.name.clone();
    host.post_comment(
        &issue,
        &if carries_plan {
            format!(
                "convert: {} → {} by {by} — the RCA and harness are the plan; moved to {column}",
                from.as_str(),
                to.as_str()
            )
        } else {
            format!(
                "convert: {} → {} by {by} — moved to {column} to be planned",
                from.as_str(),
                to.as_str()
            )
        },
    )
    .await;
    host.notify(
        issue.project_id,
        Event::CardConverted {
            issue: crate::services::notifications::IssueRef {
                key: metadata
                    .jira
                    .as_ref()
                    .map(|jira| jira.key.clone())
                    .unwrap_or_else(|| issue.simple_id.clone()),
                title: issue.title.clone(),
                url: metadata.jira.as_ref().and_then(|jira| jira.url.clone()),
                card_url: Some(card_url(issue.project_id, issue.id)),
            },
            from,
            to,
            by: by.to_string(),
            column: column.clone(),
        },
    )
    .await;
    Ok(ConvertedCard {
        id: issue.id,
        from,
        to,
        column,
        plan_carried_over: carries_plan,
    })
}

/// A handle on the card API for code that is not itself the host: the
/// `/api/cards` routes.
#[derive(Clone)]
pub struct Cards {
    host: Arc<dyn CardHost>,
}

impl Cards {
    pub fn new(host: Arc<dyn CardHost>) -> Self {
        Self { host }
    }

    pub async fn create(&self, request: CreateCardRequest) -> Result<CreatedCard, CardError> {
        create_card(self.host.as_ref(), request).await
    }

    pub async fn view(&self, issue_id: Uuid) -> Result<Option<CardView>, CardError> {
        view_card(self.host.as_ref(), issue_id).await
    }

    pub async fn convert(
        &self,
        issue_id: Uuid,
        to: WorkClass,
        by: &str,
    ) -> Result<ConvertedCard, CardError> {
        convert_card(self.host.as_ref(), issue_id, to, by).await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use api_types::{
        CreateIssueRequest, HarnessOutcome, OriginKind, PlanningVerdict, SpecRef, TriageRecord,
    };
    use chrono::DateTime;
    use serde_json::json;

    use super::*;

    const BOARD: Uuid = Uuid::from_u128(0xb0a2d);

    /// Every column name the tests use, for stable ids across both boards.
    const ALL_COLUMNS: [&str; 10] = [
        "To Do", "Plan", "Triaged", "Execute", "Review", "Shipped", "Done", "Push MR", "Close",
        "Clean up",
    ];

    fn status(name: &str, sort_order: i32) -> ProjectStatus {
        let at = ALL_COLUMNS.iter().position(|known| *known == name).unwrap();
        ProjectStatus {
            id: Uuid::from_u128(at as u128 + 1),
            project_id: BOARD,
            name: name.to_string(),
            color: "0 0% 50%".to_string(),
            sort_order,
            hidden: false,
            created_at: DateTime::from_timestamp(0, 0).unwrap(),
        }
    }

    fn board(names: &[&str]) -> Vec<ProjectStatus> {
        names
            .iter()
            .enumerate()
            .map(|(at, name)| status(name, at as i32))
            .collect()
    }

    /// A new project's columns.
    fn columns() -> Vec<ProjectStatus> {
        board(&["To Do", "Plan", "Execute", "Review", "Shipped", "Clean up"])
    }

    /// A board made before Shipped, with the Triaged column its team added.
    fn older_columns() -> Vec<ProjectStatus> {
        board(&[
            "To Do", "Plan", "Triaged", "Execute", "Review", "Push MR", "Close", "Clean up",
        ])
    }

    fn issue(metadata: Value, column: &str) -> Issue {
        Issue {
            id: Uuid::from_u128(7),
            project_id: BOARD,
            issue_number: 7,
            simple_id: "PC-7".to_string(),
            status_id: status(column, 0).id,
            title: "Exports drop a row".to_string(),
            description: None,
            priority: None,
            start_date: None,
            target_date: None,
            completed_at: None,
            sort_order: 0.0,
            parent_issue_id: None,
            parent_issue_sort_order: None,
            extension_metadata: metadata,
            creator_user_id: None,
            created_at: DateTime::from_timestamp(0, 0).unwrap(),
            updated_at: DateTime::from_timestamp(0, 0).unwrap(),
        }
    }

    #[derive(Default)]
    struct FakeBoard {
        /// Serves [`older_columns`] instead of [`columns`].
        older: bool,
        default_board: Option<Uuid>,
        card: Mutex<Option<Issue>>,
        created: Mutex<Vec<(CreateIssueRequest, WorkClass)>>,
        tags: Mutex<Vec<(WorkClass, WorkClass)>>,
        moves: Mutex<Vec<Uuid>>,
        comments: Mutex<Vec<String>>,
        events: Mutex<Vec<Event>>,
    }

    impl FakeBoard {
        fn with_card(metadata: Value, column: &str) -> Self {
            Self {
                card: Mutex::new(Some(issue(metadata, column))),
                ..Default::default()
            }
        }

        fn older_with_card(metadata: Value, column: &str) -> Self {
            Self {
                older: true,
                ..Self::with_card(metadata, column)
            }
        }

        fn metadata(&self) -> IssueExtMetadata {
            IssueExtMetadata::from_value(
                &self
                    .card
                    .lock()
                    .unwrap()
                    .as_ref()
                    .unwrap()
                    .extension_metadata,
            )
        }
    }

    #[async_trait]
    impl CardHost for FakeBoard {
        async fn default_board(&self) -> Option<Uuid> {
            self.default_board
        }

        async fn statuses(&self, project_id: Uuid) -> Result<Vec<ProjectStatus>, CardError> {
            if project_id == BOARD {
                Ok(if self.older {
                    older_columns()
                } else {
                    columns()
                })
            } else {
                Err(CardError::UnknownBoard(project_id))
            }
        }

        async fn create(
            &self,
            request: CreateIssueRequest,
            work_class: WorkClass,
        ) -> Result<Issue, CardError> {
            let mut created = issue(request.extension_metadata.clone(), "To Do");
            created.status_id = request.status_id;
            self.created.lock().unwrap().push((request, work_class));
            Ok(created)
        }

        async fn issue(&self, issue_id: Uuid) -> Result<Option<Issue>, CardError> {
            Ok(self
                .card
                .lock()
                .unwrap()
                .clone()
                .filter(|card| card.id == issue_id))
        }

        async fn class_from_tags(&self, _issue: &Issue) -> Option<WorkClass> {
            None
        }

        async fn comment_count(&self, _issue: &Issue) -> Result<usize, CardError> {
            Ok(3)
        }

        async fn rca_id(&self, _issue: &Issue) -> Option<Uuid> {
            Some(Uuid::from_u128(0xd0c))
        }

        async fn write_metadata(&self, _issue: &Issue, metadata: Value) -> Result<(), CardError> {
            self.card
                .lock()
                .unwrap()
                .as_mut()
                .unwrap()
                .extension_metadata = metadata;
            Ok(())
        }

        async fn swap_class_tag(&self, _issue: &Issue, from: WorkClass, to: WorkClass) {
            self.tags.lock().unwrap().push((from, to));
        }

        async fn move_card(&self, _issue: &Issue, status_id: Uuid) -> Result<(), CardError> {
            self.moves.lock().unwrap().push(status_id);
            Ok(())
        }

        async fn post_comment(&self, _issue: &Issue, text: &str) {
            self.comments.lock().unwrap().push(text.to_string());
        }

        async fn notify(&self, _project_id: Uuid, event: Event) {
            self.events.lock().unwrap().push(event);
        }
    }

    fn request() -> CreateCardRequest {
        serde_json::from_value(json!({
            "title": "  Exports drop the last row  ",
            "description": "Customer says the CSV is short.",
            "work_class": "investigation",
            "origin": {
                "kind": "tammy",
                "ref": "https://example.slack.com/archives/C1/p1",
                "reporter": "Grace"
            },
            "attachments": [{ "name": "export.csv", "url": "https://files.example.com/x.csv" }],
            "metadata": { "customer": "acme" },
            "start": true
        }))
        .unwrap()
    }

    #[tokio::test]
    async fn a_card_is_filed_in_plan_on_the_default_board() {
        let board = FakeBoard {
            default_board: Some(BOARD),
            ..Default::default()
        };
        let created = create_card(&board, request()).await.unwrap();
        assert_eq!(created.key, "PC-7");
        assert!(
            created
                .url
                .ends_with(&format!("/projects/{BOARD}/issues/{}", created.id))
        );

        let (sent, class) = board.created.lock().unwrap().pop().unwrap();
        assert_eq!(class, WorkClass::Investigation);
        assert_eq!(sent.project_id, BOARD);
        assert_eq!(
            sent.status_id,
            column_for(&columns(), "planning").unwrap().id
        );
        assert_eq!(sent.title, "Exports drop the last row");
        let description = sent.description.unwrap();
        assert!(description.starts_with("Customer says the CSV is short."));
        assert!(description.contains("- [export.csv](https://files.example.com/x.csv)"));
        assert!(description.ends_with(
            "Filed by Tammy from [https://example.slack.com/archives/C1/p1]\
             (https://example.slack.com/archives/C1/p1), reported by Grace."
        ));
        let metadata = IssueExtMetadata::from_value(&sent.extension_metadata);
        assert_eq!(metadata.work_class, Some(WorkClass::Investigation));
        assert_eq!(metadata.origin.unwrap().kind, OriginKind::Tammy);
        assert_eq!(metadata.attachments.len(), 1);
        assert_eq!(metadata.external, Some(json!({ "customer": "acme" })));
        // The filer's metadata can never stand in for a workflow key.
        assert!(sent.extension_metadata.get("customer").is_none());
    }

    #[tokio::test]
    async fn a_card_that_does_not_start_waits_in_to_do() {
        let board = FakeBoard::default();
        let mut request = request();
        request.start = false;
        request.project_id = Some(BOARD);
        create_card(&board, request).await.unwrap();
        let (sent, _) = board.created.lock().unwrap().pop().unwrap();
        assert_eq!(
            sent.status_id,
            column_for(&columns(), "backlog").unwrap().id
        );
    }

    #[tokio::test]
    async fn without_a_board_or_a_default_nothing_is_filed() {
        let board = FakeBoard::default();
        let error = create_card(&board, request()).await.unwrap_err();
        assert!(matches!(error, CardError::Invalid(_)), "{error:?}");
        let mut request = request();
        request.project_id = Some(Uuid::from_u128(99));
        assert_eq!(
            create_card(&board, request).await.unwrap_err(),
            CardError::UnknownBoard(Uuid::from_u128(99))
        );
        assert!(board.created.lock().unwrap().is_empty());
    }

    #[test]
    fn a_bad_body_is_refused_before_anything_is_written() {
        let check = |edit: &dyn Fn(&mut CreateCardRequest)| {
            let mut request = request();
            edit(&mut request);
            validate(&request)
        };
        assert!(check(&|_| {}).is_ok());
        for refused in [
            check(&|r| r.title = "   ".to_string()),
            check(&|r| r.title = "x".repeat(MAX_TITLE_CHARS + 1)),
            check(&|r| r.work_class = WorkClass::Epic),
            check(&|r| r.work_class = WorkClass::Subtask),
            check(&|r| r.origin.reference = " ".to_string()),
            check(&|r| r.attachments[0].url = "file:///etc/passwd".to_string()),
            check(&|r| r.attachments[0].name = String::new()),
            check(&|r| r.metadata = Some(json!(["not", "an", "object"]))),
        ] {
            assert!(matches!(refused, Err(CardError::Invalid(_))), "{refused:?}");
        }
        // The kinds are a closed set: an unknown one does not parse.
        assert!(
            serde_json::from_value::<CreateCardRequest>(json!({
                "title": "t", "work_class": "investigation",
                "origin": { "kind": "email", "ref": "x" }
            }))
            .is_err()
        );
    }

    fn triaged(confidence: ConfidenceLevel, harness: Option<&str>) -> Value {
        serde_json::to_value(IssueExtMetadata {
            work_class: Some(WorkClass::Investigation),
            playbook_started_at: Some(Utc::now()),
            planning: Some(PlanningVerdict {
                kind: Some("rca".to_string()),
                confidence: Some(confidence),
                recorded_at: Some(Utc::now()),
                ..Default::default()
            }),
            spec: Some(SpecRef {
                path: Some("docs/specs/PC-7.html".to_string()),
                ..Default::default()
            }),
            triage: Some(TriageRecord {
                report: TriageReport::default(),
                confidence: Some(confidence),
                harness: Some(HarnessRecord {
                    path: harness.map(str::to_string),
                    outcome: HarnessOutcome::Reproduced,
                }),
                comment_id: None,
                recorded_at: Utc::now(),
                notified_at: Some(Utc::now()),
                tracker_posted_at: None,
                completed_at: Some(Utc::now()),
            }),
            ..Default::default()
        })
        .unwrap()
    }

    /// The RCA at high confidence and the harness are the plan: the card
    /// goes straight to Executing with the conversion recorded as the
    /// approval, so the gate passes and Planning does not run again.
    #[tokio::test]
    async fn a_converted_investigation_with_a_plan_goes_to_executing() {
        // Triaged in Plan, as on a new project's board.
        let board = FakeBoard::with_card(
            triaged(ConfidenceLevel::High, Some(".repro/PC-7/repro.mjs")),
            "Plan",
        );
        let outcome = convert_card(&board, Uuid::from_u128(7), WorkClass::Bug, "Ada")
            .await
            .unwrap();
        assert!(outcome.plan_carried_over);
        assert_eq!(outcome.column, "Execute");
        assert_eq!(
            *board.moves.lock().unwrap(),
            [column_for(&columns(), "executing").unwrap().id]
        );
        assert_eq!(
            *board.tags.lock().unwrap(),
            [(WorkClass::Investigation, WorkClass::Bug)]
        );

        let metadata = board.metadata();
        assert_eq!(metadata.work_class, Some(WorkClass::Bug));
        assert!(metadata.playbook_started_at.is_none());
        let spec = metadata.spec.as_ref().unwrap();
        assert!(spec.approved_at.is_some());
        assert_eq!(spec.path.as_deref(), Some("docs/specs/PC-7.html"));
        assert_eq!(spec.approved_by.as_deref(), Some("Ada"));
        // The report, the harness and the verdict all survive.
        assert!(metadata.triage.is_some());
        assert_eq!(
            metadata.planning.as_ref().unwrap().confidence,
            Some(ConfidenceLevel::High)
        );
        assert_eq!(metadata.conversions.len(), 1);
        assert_eq!(metadata.approvals.len(), 1);

        let event = board.events.lock().unwrap()[0].clone();
        let Event::CardConverted {
            from,
            to,
            column,
            by,
            ..
        } = &event
        else {
            panic!("{event:?}");
        };
        assert_eq!((*from, *to), (WorkClass::Investigation, WorkClass::Bug));
        assert_eq!((column.as_str(), by.as_str()), ("Execute", "Ada"));
        assert!(board.comments.lock().unwrap()[0].contains("moved to Execute"));

        // From a Triaged column a board added, the same.
        let board = FakeBoard::older_with_card(
            triaged(ConfidenceLevel::High, Some(".repro/PC-7/repro.mjs")),
            "Triaged",
        );
        let outcome = convert_card(&board, Uuid::from_u128(7), WorkClass::Bug, "Ada")
            .await
            .unwrap();
        assert_eq!(outcome.column, "Execute");
        assert_eq!(
            *board.moves.lock().unwrap(),
            [column_for(&older_columns(), "executing").unwrap().id]
        );
    }

    #[tokio::test]
    async fn without_a_plan_a_converted_card_is_planned_again() {
        for (confidence, harness) in [
            (ConfidenceLevel::Medium, Some(".repro/PC-7/repro.mjs")),
            (ConfidenceLevel::High, None),
        ] {
            let board = FakeBoard::with_card(triaged(confidence, harness), "Plan");
            let outcome = convert_card(&board, Uuid::from_u128(7), WorkClass::Bug, "Ada")
                .await
                .unwrap();
            assert!(!outcome.plan_carried_over);
            assert_eq!(outcome.column, "Plan");
            // Already in Plan: planned again where it stands.
            assert!(board.moves.lock().unwrap().is_empty());
            let metadata = board.metadata();
            assert_eq!(metadata.work_class, Some(WorkClass::Bug));
            assert!(metadata.spec.unwrap().approved_at.is_none());
            assert!(metadata.approvals.is_empty());
            assert!(metadata.playbook_started_at.is_none());
        }
        // A story is always planned again, whatever the investigation found.
        let board = FakeBoard::older_with_card(
            triaged(ConfidenceLevel::High, Some(".repro/PC-7/repro.mjs")),
            "Triaged",
        );
        let outcome = convert_card(&board, Uuid::from_u128(7), WorkClass::Story, "Ada")
            .await
            .unwrap();
        assert_eq!(
            (outcome.to, outcome.column.as_str()),
            (WorkClass::Story, "Plan")
        );
    }

    /// A conversion whose move failed is repeated by asking again: the card is
    /// already a bug with its approval, so the request only moves it.
    #[tokio::test]
    async fn a_repeated_conversion_finishes_the_move() {
        let board = FakeBoard::with_card(
            triaged(ConfidenceLevel::High, Some(".repro/PC-7/repro.mjs")),
            "Plan",
        );
        convert_card(&board, Uuid::from_u128(7), WorkClass::Bug, "Ada")
            .await
            .unwrap();
        // The move did not happen: the card is still in Plan.
        board.moves.lock().unwrap().clear();
        let written = board.metadata();
        let again = convert_card(&board, Uuid::from_u128(7), WorkClass::Bug, "Ada")
            .await
            .unwrap();
        assert!(again.plan_carried_over);
        assert_eq!(again.column, "Execute");
        assert_eq!(board.moves.lock().unwrap().len(), 1);
        // Nothing was recorded twice.
        let metadata = board.metadata();
        assert_eq!(metadata.conversions.len(), written.conversions.len());
        assert_eq!(metadata.approvals.len(), 1);
    }

    #[tokio::test]
    async fn only_an_investigation_converts_and_only_to_a_bug_or_story() {
        let mut bug = triaged(ConfidenceLevel::High, Some("x"));
        bug["work_class"] = json!("bug");
        let board = FakeBoard::with_card(bug, "Plan");
        assert!(matches!(
            convert_card(&board, Uuid::from_u128(7), WorkClass::Story, "Ada").await,
            Err(CardError::Conflict(_))
        ));
        let board = FakeBoard::with_card(triaged(ConfidenceLevel::High, Some("x")), "Plan");
        assert!(matches!(
            convert_card(&board, Uuid::from_u128(7), WorkClass::Epic, "Ada").await,
            Err(CardError::Invalid(_))
        ));
        assert_eq!(
            convert_card(&board, Uuid::from_u128(8), WorkClass::Bug, "Ada").await,
            Err(CardError::UnknownCard(Uuid::from_u128(8)))
        );
        assert!(board.moves.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_card_view_carries_its_report_column_and_rca() {
        let mut metadata = triaged(ConfidenceLevel::Medium, Some(".repro/PC-7/repro.mjs"));
        metadata["triage"]["report"]["why"] = json!("The pager drops the last page.");
        metadata["origin"] = json!({ "kind": "incident", "ref": "INC-4" });
        // Triaged is the card's state: in Plan on a new project's board…
        let board = FakeBoard::with_card(metadata.clone(), "Plan");
        let view = view_card(&board, Uuid::from_u128(7))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(view.status, "triaged");
        assert_eq!(view.column, "Plan");
        // …and in the Triaged column of a board that keeps one.
        let older = FakeBoard::older_with_card(metadata.clone(), "Triaged");
        let older_view = view_card(&older, Uuid::from_u128(7))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            (older_view.status.as_str(), older_view.column.as_str()),
            ("triaged", "Triaged")
        );
        assert_eq!(view.work_class, Some(WorkClass::Investigation));
        assert_eq!(view.report.unwrap().why, "The pager drops the last page.");
        assert_eq!(view.confidence, Some(ConfidenceLevel::Medium));
        assert_eq!(view.comments, 3);
        assert_eq!(view.rca_id, Some(Uuid::from_u128(0xd0c)));
        assert_eq!(view.origin.unwrap().kind, OriginKind::Incident);
        let value =
            serde_json::to_value(view_card(&board, Uuid::from_u128(7)).await.unwrap()).unwrap();
        assert_eq!(value["harness"]["path"], ".repro/PC-7/repro.mjs");
        assert!(
            view_card(&board, Uuid::from_u128(8))
                .await
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn columns_are_named_the_way_boards_name_them() {
        assert_eq!(status_word(" Unassigned "), "backlog");
        assert_eq!(status_word("TRIAGED"), "triaged");
        assert_eq!(status_word("Deployed"), "closed");
        assert_eq!(status_word("Shipped"), "pushed_mr");
        assert_eq!(status_word("Done"), "pushed_mr");
        assert_eq!(status_word("Push MR"), "pushed_mr");
        assert_eq!(status_word("Icebox"), "other");
        assert!(column_for(&columns(), "triaged").is_none());
        assert!(column_for(&older_columns(), "triaged").is_some());
        assert_eq!(column_for(&columns(), "pushed_mr").unwrap().name, "Shipped");
    }

    #[test]
    fn a_card_status_reads_triage_and_merge_from_the_card() {
        let investigating = IssueExtMetadata {
            work_class: Some(WorkClass::Investigation),
            ..Default::default()
        };
        assert_eq!(card_status("Plan", &investigating), "planning");
        let triaged = IssueExtMetadata::from_value(&triaged(ConfidenceLevel::High, None));
        assert_eq!(card_status("Plan", &triaged), "triaged");
        assert_eq!(card_status("Triaged", &triaged), "triaged");
        let in_review = IssueExtMetadata {
            mr_url: Some("https://github.com/acme/pantry/pull/7".to_string()),
            ..Default::default()
        };
        assert_eq!(card_status("Shipped", &in_review), "pushed_mr");
        let merged = IssueExtMetadata {
            merged_at: Some(Utc::now()),
            ..in_review
        };
        assert_eq!(card_status("Shipped", &merged), "closed");
        assert_eq!(card_status("Done", &merged), "closed");
        assert_eq!(card_status("Close", &merged), "closed");
    }
}
