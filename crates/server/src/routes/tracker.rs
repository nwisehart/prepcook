//! Tracker importer and agent-facing tracker actions: previews tickets
//! matching a query and imports the selected ones as board issues (recording
//! provenance in `extension_metadata`, see `api_types::issue_ext`), creates
//! sub-tasks for an epic breakdown, and backs the agent tools that read
//! attachments and post comments. Everything here goes through the
//! configured [`Tracker`].

use std::collections::{BTreeMap, HashMap, HashSet};

use api_types::{
    CreateIssueAssigneeRequest, CreateIssueRequest, CreateIssueTagRequest, IssueExtMetadata,
    IssuePriority, JiraRef, SearchIssuesRequest, WorkClass,
};
use axum::{
    Router,
    extract::{Json, State},
    response::Json as ResponseJson,
    routing::{get, post},
};
use deployment::Deployment;
use serde::{Deserialize, Serialize};
use services::services::{
    config::TrackerConfig,
    remote_client::{RemoteClient, RemoteClientError},
    tracker::{
        self, FetchedIssue, NewSubtask, Tracker, TrackerAttachment, TrackerError, users_by_id,
    },
};
use ts_rs::TS;
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::{DeploymentImpl, error::ApiError};

#[derive(Debug, Deserialize, TS)]
pub struct TrackerPreviewRequest {
    pub project_id: Uuid,
    /// A query in the tracker's own language; the configured import filter
    /// when absent.
    #[ts(optional)]
    pub query: Option<String>,
}

#[derive(Debug, Serialize, TS)]
pub struct TrackerImportCandidate {
    pub key: String,
    pub title: String,
    pub issue_type: String,
    pub work_class: WorkClass,
    pub url: String,
    pub already_imported: bool,
}

#[derive(Debug, Serialize, TS)]
pub struct TrackerPreviewResponse {
    pub candidates: Vec<TrackerImportCandidate>,
}

#[derive(Debug, Deserialize, TS)]
pub struct TrackerImportRequest {
    pub project_id: Uuid,
    pub keys: Vec<String>,
}

#[derive(Debug, Serialize, TS)]
pub struct TrackerImportResponse {
    pub imported: u32,
    pub skipped: u32,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(tag = "type", rename_all = "snake_case")]
pub enum TrackerImportError {
    NotConfigured,
    NotSupported { tracker: String },
    AuthFailed,
    RequestFailed { message: String },
    RemoteUnavailable,
}

impl From<TrackerError> for TrackerImportError {
    fn from(error: TrackerError) -> Self {
        match error {
            TrackerError::NotConfigured(_) => Self::NotConfigured,
            TrackerError::NotSupported(kind) => Self::NotSupported {
                tracker: kind.label().to_string(),
            },
            TrackerError::AuthFailed => Self::AuthFailed,
            error @ TrackerError::Unsupported { .. } => Self::RequestFailed {
                message: error.to_string(),
            },
            TrackerError::Other(error) => Self::RequestFailed {
                message: error.to_string(),
            },
        }
    }
}

/// The tracker and a snapshot of its settings, taken without holding the
/// config lock across network calls.
async fn tracker_and_settings(
    deployment: &DeploymentImpl,
) -> Result<(std::sync::Arc<dyn Tracker>, TrackerConfig), TrackerImportError> {
    let tracker = deployment.tracker().current().await;
    if !tracker.is_configured() {
        return Err(match tracker.kind() {
            kind @ services::services::config::TrackerKind::Linear => {
                TrackerImportError::NotSupported {
                    tracker: kind.label().to_string(),
                }
            }
            _ => TrackerImportError::NotConfigured,
        });
    }
    Ok((tracker, deployment.tracker().settings().await))
}

/// Maps the team's priority names to board priorities: P0 → Urgent,
/// P1 → High, P2 → Medium, P3 → Low. Unknown names import without one.
fn priority_for(priority: Option<&str>) -> Option<IssuePriority> {
    match priority?.trim().to_ascii_uppercase().as_str() {
        "P0" => Some(IssuePriority::Urgent),
        "P1" => Some(IssuePriority::High),
        "P2" => Some(IssuePriority::Medium),
        "P3" => Some(IssuePriority::Low),
        _ => None,
    }
}

/// One importable ticket, with its epic resolved.
struct Importable {
    issue: FetchedIssue,
    /// Key of the parent epic, when the parent is an epic-type issue.
    epic_key: Option<String>,
}

/// Keeps the To Do tickets of a query's or a lookup's answer. Work already
/// in progress, blocked or done does not belong on the To Do column, even if
/// a custom query matches it.
fn importable(settings: &TrackerConfig, issues: Vec<FetchedIssue>) -> Vec<Importable> {
    issues
        .into_iter()
        .filter(|issue| issue.status_category.as_deref().is_none_or(|c| c == "new"))
        .map(|issue| {
            // Epic linkage: the parent is the epic for epic children. Classic
            // sub-tasks have a story or bug parent, which is not an epic.
            let epic_key = issue
                .parent_type
                .as_deref()
                .filter(|parent_type| settings.work_class_for(parent_type) == WorkClass::Epic)
                .and(issue.parent_key.clone());
            Importable { issue, epic_key }
        })
        .collect()
}

// ── Remote-side helpers ──────────────────────────────────────────────────

/// Collects the ticket keys already recorded in `extension_metadata` across
/// all issues of the project, for dedup.
async fn existing_keys(
    client: &RemoteClient,
    project_id: Uuid,
) -> Result<HashSet<String>, RemoteClientError> {
    const PAGE_SIZE: i32 = 500;
    let mut keys = HashSet::new();
    let mut offset: i32 = 0;

    loop {
        let response = client
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
            .await?;

        let fetched = response.issues.len();
        for issue in &response.issues {
            let metadata = IssueExtMetadata::from_value(&issue.extension_metadata);
            if let Some(jira) = metadata.jira {
                keys.insert(jira.key);
            }
        }

        offset += fetched as i32;
        if fetched == 0 || (offset as usize) >= response.total_count {
            break;
        }
    }

    Ok(keys)
}

/// Picks the status for imported issues: "To Do" (or its earlier name
/// "Unassigned") if it exists, otherwise the lowest sort_order non-hidden
/// status.
async fn default_import_status_id(
    client: &RemoteClient,
    project_id: Uuid,
) -> Result<Uuid, ApiError> {
    let response = client.list_project_statuses(project_id).await?;
    let statuses = response.project_statuses;

    for name in ["To Do", "Unassigned"] {
        if let Some(status) = statuses
            .iter()
            .find(|status| status.name.eq_ignore_ascii_case(name))
        {
            return Ok(status.id);
        }
    }

    statuses
        .iter()
        .filter(|status| !status.hidden)
        .min_by_key(|status| status.sort_order)
        .map(|status| status.id)
        .ok_or_else(|| ApiError::BadRequest("Project has no visible statuses".to_string()))
}

// ── Handlers ─────────────────────────────────────────────────────────────

pub async fn preview_tracker_issues(
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<TrackerPreviewRequest>,
) -> Result<ResponseJson<ApiResponse<TrackerPreviewResponse, TrackerImportError>>, ApiError> {
    let (tracker, settings) = match tracker_and_settings(&deployment).await {
        Ok(found) => found,
        Err(e) => return Ok(ResponseJson(ApiResponse::error_with_data(e))),
    };
    let Ok(client) = deployment.remote_client() else {
        return Ok(ResponseJson(ApiResponse::error_with_data(
            TrackerImportError::RemoteUnavailable,
        )));
    };

    let query = match payload.query.filter(|query| !query.trim().is_empty()) {
        Some(query) => query.replace("{project_key}", settings.project_key()),
        None => settings.resolved_import_filter(),
    };
    let issues = match tracker.search(&query).await {
        Ok(issues) => importable(&settings, issues),
        Err(e) => return Ok(ResponseJson(ApiResponse::error_with_data(e.into()))),
    };

    let existing = existing_keys(&client, payload.project_id).await?;

    let candidates = issues
        .into_iter()
        .map(|Importable { issue, .. }| TrackerImportCandidate {
            url: tracker.issue_url(&issue.key),
            already_imported: existing.contains(&issue.key),
            work_class: settings.work_class_for(&issue.issue_type),
            title: issue.summary,
            issue_type: issue.issue_type,
            key: issue.key,
        })
        .collect();

    Ok(ResponseJson(ApiResponse::success(TrackerPreviewResponse {
        candidates,
    })))
}

pub async fn import_tracker_issues(
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<TrackerImportRequest>,
) -> Result<ResponseJson<ApiResponse<TrackerImportResponse, TrackerImportError>>, ApiError> {
    let (tracker, settings) = match tracker_and_settings(&deployment).await {
        Ok(found) => found,
        Err(e) => return Ok(ResponseJson(ApiResponse::error_with_data(e))),
    };
    let Ok(client) = deployment.remote_client() else {
        return Ok(ResponseJson(ApiResponse::error_with_data(
            TrackerImportError::RemoteUnavailable,
        )));
    };

    // Fetch exactly the requested keys instead of re-running the configured
    // query: import must work for tickets outside it too (e.g. sub-tasks
    // just created by an epic breakdown). Keys are checked for the tracker's
    // own shape before they reach a query or a URL.
    let valid_keys: Vec<String> = payload
        .keys
        .iter()
        .filter(|key| tracker.is_valid_key(key))
        .cloned()
        .collect();
    if valid_keys.is_empty() {
        return Ok(ResponseJson(ApiResponse::success(TrackerImportResponse {
            imported: 0,
            skipped: payload.keys.len() as u32,
        })));
    }
    let fetched = match tracker.fetch(&valid_keys).await {
        Ok(issues) => importable(&settings, issues),
        Err(e) => return Ok(ResponseJson(ApiResponse::error_with_data(e.into()))),
    };
    let by_key: HashMap<String, Importable> = fetched
        .into_iter()
        .map(|importable| (importable.issue.key.clone(), importable))
        .collect();

    let mut existing = existing_keys(&client, payload.project_id).await?;
    let status_id = default_import_status_id(&client, payload.project_id).await?;

    // The remote client can list tags and link them to issues but cannot
    // create new tags; missing work-class tags are skipped and
    // extension_metadata remains the source of truth.
    let project_tags = client
        .list_tags(payload.project_id)
        .await
        .map(|response| response.tags)
        .unwrap_or_default();

    // Imports come from an assigned-to-me query, so mirror that assignment on
    // the board: the orchestrator only automates tickets assigned to the
    // current user, and unassigned imports would sit invisible to it.
    let me = client.profile().await.ok().map(|profile| profile.user_id);

    let mut imported: u32 = 0;
    let mut skipped: u32 = 0;

    for key in payload.keys {
        if existing.contains(&key) {
            skipped += 1;
            continue;
        }
        let Some(Importable { issue, epic_key }) = by_key.get(&key) else {
            // Not To Do, or not found; nothing to import.
            skipped += 1;
            continue;
        };

        let work_class = settings.work_class_for(&issue.issue_type);
        let url = tracker.issue_url(&key);

        let metadata = IssueExtMetadata {
            work_class: Some(work_class),
            jira: Some(JiraRef {
                key: key.clone(),
                issue_type: Some(issue.issue_type.clone()),
                url: Some(url.clone()),
            }),
            epic_key: epic_key.clone(),
            origin: tracker::origin_for(tracker.kind(), &url),
            filed_externally: issue.filed_externally,
            ..Default::default()
        };

        // Best effort: an unreadable thread imports without its comments.
        let comments_block = tracker
            .comments(&key)
            .await
            .map(|comments| tracker::format_comments(&comments))
            .unwrap_or_default();
        let kind = tracker.kind();
        let description = if issue.description_text.is_empty() {
            format!("{kind}: {url}{comments_block}")
        } else {
            format!(
                "{}\n\n{kind}: {url}{comments_block}",
                issue.description_text
            )
        };

        let request = CreateIssueRequest {
            id: None,
            project_id: payload.project_id,
            status_id,
            title: format!("{}: {}", key, issue.summary),
            description: Some(description),
            priority: priority_for(issue.priority_name.as_deref()),
            start_date: None,
            target_date: None,
            completed_at: None,
            sort_order: 0.0,
            parent_issue_id: None,
            parent_issue_sort_order: None,
            extension_metadata: serde_json::to_value(&metadata)
                .unwrap_or_else(|_| serde_json::json!({})),
        };

        let created = client.create_issue(&request).await?;

        if let Some(user_id) = me
            && let Err(e) = client
                .create_issue_assignee(&CreateIssueAssigneeRequest {
                    id: None,
                    issue_id: created.data.id,
                    user_id,
                })
                .await
        {
            tracing::warn!("Failed to assign imported issue {key}: {e}");
        }

        // Best-effort: mirror the work class (and the ticket's area, when the
        // tracker names one) as visible tags when the project already has
        // ones with those names.
        let tag_names = std::iter::once(work_class.as_str()).chain(issue.area.as_deref());
        for tag_name in tag_names {
            if let Some(tag) = project_tags
                .iter()
                .find(|tag| tag.name.eq_ignore_ascii_case(tag_name))
                && let Err(e) = client
                    .create_issue_tag(&CreateIssueTagRequest {
                        id: None,
                        issue_id: created.data.id,
                        tag_id: tag.id,
                    })
                    .await
            {
                tracing::warn!("Failed to link '{}' tag to imported issue: {}", tag_name, e);
            }
        }

        imported += 1;
        // Guard against duplicate keys within the same request.
        existing.insert(key);
    }

    Ok(ResponseJson(ApiResponse::success(TrackerImportResponse {
        imported,
        skipped,
    })))
}

// ── Epic breakdown: create sub-tasks + import them ───────────────────────

#[derive(Debug, Deserialize, TS)]
pub struct TrackerSubtaskSpec {
    pub title: String,
    #[ts(optional)]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize, TS)]
pub struct TrackerCreateSubtasksRequest {
    pub project_id: Uuid,
    /// Key of the parent epic, e.g. "BUG-8127".
    pub epic_key: String,
    pub subtasks: Vec<TrackerSubtaskSpec>,
}

#[derive(Debug, Serialize, TS)]
pub struct TrackerCreateSubtasksResponse {
    /// Keys created, in request order.
    pub created_keys: Vec<String>,
    /// Number of created keys also imported as board issues.
    pub imported: u32,
    /// Titles that failed to create on the tracker side.
    pub failed: Vec<String>,
}

/// Creates sub-tasks under an epic and imports them as board issues
/// (work_class subtask, epic_key set) so the tracker stays the source of
/// truth. Called by the epic executing playbook through the MCP
/// `tracker_create_subtasks` tool.
pub async fn create_tracker_subtasks(
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<TrackerCreateSubtasksRequest>,
) -> Result<ResponseJson<ApiResponse<TrackerCreateSubtasksResponse, TrackerImportError>>, ApiError>
{
    let (tracker, settings) = match tracker_and_settings(&deployment).await {
        Ok(found) => found,
        Err(e) => return Ok(ResponseJson(ApiResponse::error_with_data(e))),
    };

    if !tracker.is_valid_key(&payload.epic_key) {
        return Ok(ResponseJson(ApiResponse::error_with_data(
            TrackerImportError::RequestFailed {
                message: format!("'{}' is not a valid issue key", payload.epic_key),
            },
        )));
    }

    let Ok(client) = deployment.remote_client() else {
        return Ok(ResponseJson(ApiResponse::error_with_data(
            TrackerImportError::RemoteUnavailable,
        )));
    };

    let items: Vec<NewSubtask> = payload
        .subtasks
        .iter()
        .map(|subtask| NewSubtask {
            title: subtask.title.clone(),
            description: subtask.description.clone(),
        })
        .collect();
    let outcomes = match tracker.create_subtasks(&payload.epic_key, &items).await {
        Ok(outcomes) => outcomes,
        Err(e) => return Ok(ResponseJson(ApiResponse::error_with_data(e.into()))),
    };
    let subtask_type_name = settings
        .type_mapping
        .iter()
        .find(|(_, work_class)| **work_class == WorkClass::Subtask)
        .map(|(name, _)| name.clone())
        .unwrap_or_else(|| "Sub-task".to_string());

    let mut created: Vec<(String, &TrackerSubtaskSpec)> = Vec::new();
    let mut failed: Vec<String> = Vec::new();
    for (outcome, spec) in outcomes.into_iter().zip(&payload.subtasks) {
        match outcome.key {
            Some(key) => created.push((key, spec)),
            None => failed.push(outcome.title),
        }
    }

    // Import the created sub-tasks as board issues (dedup for retried calls).
    let mut existing = existing_keys(&client, payload.project_id).await?;
    let status_id = default_import_status_id(&client, payload.project_id).await?;
    let project_tags = client
        .list_tags(payload.project_id)
        .await
        .map(|response| response.tags)
        .unwrap_or_default();
    // Assign to the current user so the orchestrator automates the subtasks.
    let me = client.profile().await.ok().map(|profile| profile.user_id);

    let mut imported: u32 = 0;
    for (key, subtask) in &created {
        if existing.contains(key) {
            continue;
        }
        let url = tracker.issue_url(key);
        let metadata = IssueExtMetadata {
            work_class: Some(WorkClass::Subtask),
            jira: Some(JiraRef {
                key: key.clone(),
                issue_type: Some(subtask_type_name.clone()),
                url: Some(url.clone()),
            }),
            epic_key: Some(payload.epic_key.clone()),
            ..Default::default()
        };
        let description = match subtask
            .description
            .as_deref()
            .filter(|text| !text.trim().is_empty())
        {
            Some(text) => format!("{text}\n\n{}: {url}", tracker.kind()),
            None => format!("{}: {url}", tracker.kind()),
        };

        let request = CreateIssueRequest {
            id: None,
            project_id: payload.project_id,
            status_id,
            title: format!("{}: {}", key, subtask.title),
            description: Some(description),
            priority: None,
            start_date: None,
            target_date: None,
            completed_at: None,
            sort_order: 0.0,
            parent_issue_id: None,
            parent_issue_sort_order: None,
            extension_metadata: serde_json::to_value(&metadata)
                .unwrap_or_else(|_| serde_json::json!({})),
        };
        let created_issue = client.create_issue(&request).await?;

        if let Some(user_id) = me
            && let Err(e) = client
                .create_issue_assignee(&CreateIssueAssigneeRequest {
                    id: None,
                    issue_id: created_issue.data.id,
                    user_id,
                })
                .await
        {
            tracing::warn!("Failed to assign imported sub-task {key}: {e}");
        }

        if let Some(tag) = project_tags
            .iter()
            .find(|tag| tag.name.eq_ignore_ascii_case("subtask"))
            && let Err(e) = client
                .create_issue_tag(&CreateIssueTagRequest {
                    id: None,
                    issue_id: created_issue.data.id,
                    tag_id: tag.id,
                })
                .await
        {
            tracing::warn!("Failed to link 'subtask' tag to imported issue: {}", e);
        }

        imported += 1;
        existing.insert(key.clone());
    }

    Ok(ResponseJson(ApiResponse::success(
        TrackerCreateSubtasksResponse {
            created_keys: created.into_iter().map(|(key, _)| key).collect(),
            imported,
            failed,
        },
    )))
}

#[derive(Debug, Deserialize)]
pub struct TrackerAttachmentsRequest {
    pub issue_key: String,
    /// Absolute directory the files are written into (the agent's worktree —
    /// the MCP tool passes its own cwd, and server + agent share a
    /// filesystem in every deployment shape).
    pub dest_dir: String,
}

/// Downloads a ticket's attachments into `dest_dir` with the app's tracker
/// credentials. Backs the `tracker_get_attachments` MCP tool so investigating
/// agents can read screenshots and logs attached to the ticket.
async fn download_tracker_attachments(
    State(deployment): State<DeploymentImpl>,
    Json(request): Json<TrackerAttachmentsRequest>,
) -> ResponseJson<ApiResponse<Vec<TrackerAttachment>>> {
    let dest = std::path::PathBuf::from(&request.dest_dir);
    if !dest.is_absolute() {
        return ResponseJson(ApiResponse::error("dest_dir must be an absolute path"));
    }
    let tracker = deployment.tracker().current().await;
    match tracker
        .download_attachments(&request.issue_key, &dest)
        .await
    {
        Ok(files) => ResponseJson(ApiResponse::success(files)),
        Err(e) => ResponseJson(ApiResponse::error(&format!(
            "Failed to download attachments: {e}"
        ))),
    }
}

#[derive(Debug, Deserialize)]
pub struct TrackerAddCommentRequest {
    pub issue_key: String,
    pub body: String,
}

/// Posts a plain-text comment on a ticket with the app's tracker credentials.
/// Backs the `tracker_add_comment` MCP tool so agents can answer people where
/// they asked — on the ticket — instead of only on the board.
async fn add_tracker_comment(
    State(deployment): State<DeploymentImpl>,
    Json(request): Json<TrackerAddCommentRequest>,
) -> ResponseJson<ApiResponse<()>> {
    if request.body.trim().is_empty() {
        return ResponseJson(ApiResponse::error("comment body is empty"));
    }
    let tracker = deployment.tracker().current().await;
    match tracker
        .post_comment(&request.issue_key, &request.body)
        .await
    {
        Ok(()) => ResponseJson(ApiResponse::success(())),
        Err(e) => ResponseJson(ApiResponse::error(&format!("Failed to post comment: {e}"))),
    }
}

/// The people assignable in the configured tracker project, for the approver
/// picker in settings.
async fn tracker_assignable_users(
    State(deployment): State<DeploymentImpl>,
) -> ResponseJson<ApiResponse<BTreeMap<String, String>>> {
    let tracker = deployment.tracker().current().await;
    let project_key = deployment
        .tracker()
        .settings()
        .await
        .project_key()
        .to_string();
    match tracker.assignable_users(&project_key).await {
        Ok(users) => ResponseJson(ApiResponse::success(users_by_id(users))),
        Err(e) => ResponseJson(ApiResponse::error(&format!(
            "Failed to list tracker users: {e}"
        ))),
    }
}

/// Whether a tracker is configured, and which.
#[derive(Debug, Serialize)]
pub struct TrackerStatus {
    pub configured: bool,
    pub kind: String,
}

/// Backs the agent tools' "is there a tracker at all" question: without one
/// the card is the only record, and a tool asked to post on the ticket says
/// so instead of failing the session.
async fn tracker_status(
    State(deployment): State<DeploymentImpl>,
) -> ResponseJson<ApiResponse<TrackerStatus>> {
    let tracker = deployment.tracker().current().await;
    ResponseJson(ApiResponse::success(TrackerStatus {
        configured: tracker.is_configured(),
        kind: tracker.kind().label().to_string(),
    }))
}

pub fn router() -> Router<DeploymentImpl> {
    Router::new()
        .route("/tracker/status", get(tracker_status))
        .route("/tracker/assignable-users", get(tracker_assignable_users))
        .route("/tracker/preview", post(preview_tracker_issues))
        .route("/tracker/import", post(import_tracker_issues))
        .route("/tracker/create-subtasks", post(create_tracker_subtasks))
        .route("/tracker/attachments", post(download_tracker_attachments))
        .route("/tracker/comment", post(add_tracker_comment))
}
