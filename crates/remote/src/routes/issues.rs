use api_types::{
    CreateIssueRequest, DeleteResponse, Issue, ListIssuesQuery, ListIssuesResponse,
    MutationResponse, SearchIssuesRequest, UpdateIssueRequest,
};
use axum::{
    Json,
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    routing::post,
};
use serde::{Deserialize, Serialize};
use tracing::instrument;
use uuid::Uuid;

use super::{
    error::{ErrorResponse, db_error},
    organization_members::ensure_project_access,
};
use crate::{
    AppState,
    auth::RequestContext,
    db::{
        get_txid, issues::IssueRepository, project_statuses::ProjectStatusRepository,
        pull_requests::PullRequestRepository,
    },
    mutation_definition::MutationBuilder,
    workflow,
};

/// Mutation definition for Issue - provides both router and TypeScript metadata.
pub fn mutation() -> MutationBuilder<Issue, CreateIssueRequest, UpdateIssueRequest> {
    MutationBuilder::new("issues")
        .list(list_issues)
        .get(get_issue)
        .create(create_issue)
        .update(update_issue)
        .delete(delete_issue)
}

/// Router for issue endpoints including bulk update
pub fn router() -> axum::Router<AppState> {
    mutation()
        .router()
        .route("/issues/search", post(search_issues))
        .route("/issues/bulk", post(bulk_update_issues))
}

/// Rejects status moves that violate the Prepcook workflow stage order (see
/// `crate::workflow`), including the Planning → Executing gate that requires
/// a passing planning verdict in the issue's `extension_metadata` and the
/// Clean up gate that reads the issue's pull requests. Only route handlers
/// call this; internal automation that updates issues through the repository
/// directly stays exempt.
/// Validates the move and returns the (from, to) status names when both
/// exist and actually differ — callers use them to decide whether the
/// orchestrator claim marker should be cleared. `extension_metadata` is the
/// blob the same request writes, when it writes one: a Clean up move carries
/// its discard confirmation that way.
async fn ensure_valid_status_transition(
    state: &AppState,
    issue: &Issue,
    to_status_id: Uuid,
    extension_metadata: Option<&serde_json::Value>,
) -> Result<Option<(String, String)>, ErrorResponse> {
    let from_status_id = issue.status_id;
    if from_status_id == to_status_id {
        return Ok(None);
    }

    let from_status = ProjectStatusRepository::find_by_id(state.pool(), from_status_id)
        .await
        .map_err(|error| {
            tracing::error!(?error, status_id = %from_status_id, "failed to load project status");
            ErrorResponse::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to load project status",
            )
        })?;
    let to_status = ProjectStatusRepository::find_by_id(state.pool(), to_status_id)
        .await
        .map_err(|error| {
            tracing::error!(?error, status_id = %to_status_id, "failed to load project status");
            ErrorResponse::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to load project status",
            )
        })?;

    // Unknown status ids fall through to the update's own FK handling.
    let (Some(from_status), Some(to_status)) = (from_status, to_status) else {
        return Ok(None);
    };

    if !workflow::validate_transition(&from_status.name, &to_status.name) {
        return Err(ErrorResponse::new(
            StatusCode::BAD_REQUEST,
            workflow::transition_error_message(&from_status.name, &to_status.name),
        ));
    }

    if let Some(message) = workflow::planning_gate_error(
        &from_status.name,
        &to_status.name,
        &issue.extension_metadata,
    ) {
        return Err(ErrorResponse::new(StatusCode::BAD_REQUEST, message));
    }

    if workflow::is_cleanup_stage(&to_status.name) {
        let pull_requests = PullRequestRepository::list_by_issue(state.pool(), issue.id)
            .await
            .map_err(|error| {
                tracing::error!(?error, issue_id = %issue.id, "failed to load pull requests");
                ErrorResponse::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "failed to load pull requests",
                )
            })?;
        let statuses: Vec<_> = pull_requests.iter().map(|pr| pr.status).collect();
        if let Some(message) = workflow::cleanup_gate_error(
            &to_status.name,
            &statuses,
            extension_metadata.unwrap_or(&issue.extension_metadata),
        ) {
            return Err(ErrorResponse::new(StatusCode::BAD_REQUEST, message));
        }
    }

    Ok(Some((from_status.name, to_status.name)))
}

/// Clears the orchestrator claim marker (`playbook_started_at`) when a
/// status move enters a column going FORWARD: each forward entry is a fresh
/// claim opportunity. Unless it enters Clean up, the same move clears
/// `merged_at` (the next Shipped entry watches a new pull request) and a discard
/// confirmation, which answers for one closed pull request only; Clean up
/// reads both. Backward (or same-stage) moves keep the marker —
/// parking a ticket back a column must not re-trigger that column's
/// automation; re-running a stage on purpose means moving the ticket out and
/// forward back in. Returns the `extension_metadata` to write (the
/// caller-provided blob, or the issue's current blob when the caller sent
/// none).
fn metadata_for_status_change(
    issue: &Issue,
    transition: Option<&(String, String)>,
    extension_metadata: Option<serde_json::Value>,
) -> Option<serde_json::Value> {
    let Some((from_name, to_name)) = transition else {
        return extension_metadata;
    };
    if workflow::is_backward_move(from_name, to_name) {
        return extension_metadata;
    }

    let mut metadata = extension_metadata.unwrap_or_else(|| issue.extension_metadata.clone());
    if let serde_json::Value::Object(map) = &mut metadata {
        map.remove("playbook_started_at");
        if !workflow::is_cleanup_stage(to_name) {
            map.remove("merged_at");
            map.remove("discard_confirmed_at");
        }
    }
    Some(metadata)
}

#[instrument(
    name = "issues.list_issues",
    skip(state, ctx),
    fields(project_id = %query.project_id, user_id = %ctx.user.id)
)]
async fn list_issues(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Query(query): Query<ListIssuesQuery>,
) -> Result<Json<ListIssuesResponse>, ErrorResponse> {
    let project_id = query.project_id;
    ensure_project_access(state.pool(), ctx.user.id, project_id).await?;
    let request = SearchIssuesRequest {
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
        limit: None,
        offset: None,
    };

    let response = IssueRepository::search(state.pool(), &request)
        .await
        .map_err(|error| {
            tracing::error!(?error, project_id = %project_id, "failed to list issues");
            ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "failed to list issues")
        })?;

    Ok(Json(response))
}

#[instrument(
    name = "issues.search_issues",
    skip(state, ctx, payload),
    fields(project_id = %payload.project_id, user_id = %ctx.user.id)
)]
async fn search_issues(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Json(payload): Json<SearchIssuesRequest>,
) -> Result<Json<ListIssuesResponse>, ErrorResponse> {
    ensure_project_access(state.pool(), ctx.user.id, payload.project_id).await?;

    let response = IssueRepository::search(state.pool(), &payload)
        .await
        .map_err(|error| {
            tracing::error!(?error, project_id = %payload.project_id, "failed to search issues");
            ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "failed to search issues")
        })?;

    Ok(Json(response))
}

#[instrument(
    name = "issues.get_issue",
    skip(state, ctx),
    fields(issue_id = %issue_id, user_id = %ctx.user.id)
)]
async fn get_issue(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Path(issue_id): Path<Uuid>,
) -> Result<Json<Issue>, ErrorResponse> {
    let issue = IssueRepository::find_by_id(state.pool(), issue_id)
        .await
        .map_err(|error| {
            tracing::error!(?error, %issue_id, "failed to load issue");
            ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "failed to load issue")
        })?
        .ok_or_else(|| ErrorResponse::new(StatusCode::NOT_FOUND, "issue not found"))?;

    ensure_project_access(state.pool(), ctx.user.id, issue.project_id).await?;

    Ok(Json(issue))
}

#[instrument(
    name = "issues.create_issue",
    skip(state, ctx, payload),
    fields(project_id = %payload.project_id, user_id = %ctx.user.id)
)]
async fn create_issue(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Json(payload): Json<CreateIssueRequest>,
) -> Result<Json<MutationResponse<Issue>>, ErrorResponse> {
    let organization_id =
        ensure_project_access(state.pool(), ctx.user.id, payload.project_id).await?;

    let has_parent = payload.parent_issue_id.is_some();
    let has_description = payload.description.is_some();
    let priority = payload.priority;
    let parent_issue_id = payload.parent_issue_id;

    let response = IssueRepository::create(
        state.pool(),
        payload.id,
        payload.project_id,
        payload.status_id,
        payload.title,
        payload.description,
        payload.priority,
        payload.start_date,
        payload.target_date,
        payload.completed_at,
        payload.sort_order,
        payload.parent_issue_id,
        payload.parent_issue_sort_order,
        payload.extension_metadata,
        ctx.user.id,
    )
    .await
    .map_err(|error| {
        tracing::error!(?error, "failed to create issue");
        db_error(error, "failed to create issue")
    })?;

    if let Some(analytics) = state.analytics() {
        analytics.track(
            ctx.user.id,
            "issue_created",
            serde_json::json!({
                "issue_id": response.data.id,
                "project_id": response.data.project_id,
                "organization_id": organization_id,
                "has_description": has_description,
                "has_parent": has_parent,
                "priority": format!("{:?}", priority),
            }),
        );

        if let Some(parent_id) = parent_issue_id {
            analytics.track(
                ctx.user.id,
                "subtask_created",
                serde_json::json!({
                    "issue_id": response.data.id,
                    "parent_issue_id": parent_id,
                    "project_id": response.data.project_id,
                    "organization_id": organization_id,
                }),
            );
        }
    }

    Ok(Json(response))
}

#[instrument(
    name = "issues.update_issue",
    skip(state, ctx, payload),
    fields(issue_id = %issue_id, user_id = %ctx.user.id)
)]
async fn update_issue(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Path(issue_id): Path<Uuid>,
    Json(payload): Json<UpdateIssueRequest>,
) -> Result<Json<MutationResponse<Issue>>, ErrorResponse> {
    let issue = IssueRepository::find_by_id(state.pool(), issue_id)
        .await
        .map_err(|error| {
            tracing::error!(?error, %issue_id, "failed to load issue");
            ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "failed to load issue")
        })?
        .ok_or_else(|| ErrorResponse::new(StatusCode::NOT_FOUND, "issue not found"))?;

    ensure_project_access(state.pool(), ctx.user.id, issue.project_id).await?;

    let transition = if let Some(new_status_id) = payload.status_id {
        ensure_valid_status_transition(
            &state,
            &issue,
            new_status_id,
            payload.extension_metadata.as_ref(),
        )
        .await?
    } else {
        None
    };

    let extension_metadata =
        metadata_for_status_change(&issue, transition.as_ref(), payload.extension_metadata);

    let mut tx = crate::db::begin_tx(state.pool()).await.map_err(|error| {
        tracing::error!(?error, "failed to begin transaction");
        ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
    })?;

    let data = IssueRepository::update(
        &mut *tx,
        issue_id,
        payload.status_id,
        payload.title,
        payload.description,
        payload.priority,
        payload.start_date,
        payload.target_date,
        payload.completed_at,
        payload.sort_order,
        payload.parent_issue_id,
        payload.parent_issue_sort_order,
        extension_metadata,
    )
    .await
    .map_err(|error| {
        tracing::error!(?error, "failed to update issue");
        ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
    })?;

    let txid = get_txid(&mut *tx).await.map_err(|error| {
        tracing::error!(?error, "failed to get txid");
        ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
    })?;

    tx.commit().await.map_err(|error| {
        tracing::error!(?error, "failed to commit transaction");
        ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
    })?;

    Ok(Json(MutationResponse { data, txid }))
}

#[instrument(
    name = "issues.delete_issue",
    skip(state, ctx),
    fields(issue_id = %issue_id, user_id = %ctx.user.id)
)]
async fn delete_issue(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Path(issue_id): Path<Uuid>,
) -> Result<Json<DeleteResponse>, ErrorResponse> {
    let issue = IssueRepository::find_by_id(state.pool(), issue_id)
        .await
        .map_err(|error| {
            tracing::error!(?error, %issue_id, "failed to load issue");
            ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "failed to load issue")
        })?
        .ok_or_else(|| ErrorResponse::new(StatusCode::NOT_FOUND, "issue not found"))?;

    ensure_project_access(state.pool(), ctx.user.id, issue.project_id).await?;

    let response = IssueRepository::delete(state.pool(), issue_id)
        .await
        .map_err(|error| {
            tracing::error!(?error, "failed to delete issue");
            ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
        })?;

    Ok(Json(response))
}

// =============================================================================
// Bulk Update
// =============================================================================

#[derive(Debug, Deserialize)]
pub struct BulkUpdateIssueItem {
    pub id: Uuid,
    #[serde(flatten)]
    pub changes: UpdateIssueRequest,
}

#[derive(Debug, Deserialize)]
pub struct BulkUpdateIssuesRequest {
    pub updates: Vec<BulkUpdateIssueItem>,
}

#[derive(Debug, Serialize)]
pub struct BulkUpdateIssuesResponse {
    pub data: Vec<Issue>,
    pub txid: i64,
}

#[instrument(
    name = "issues.bulk_update",
    skip(state, ctx, payload),
    fields(user_id = %ctx.user.id, count = payload.updates.len())
)]
async fn bulk_update_issues(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Json(payload): Json<BulkUpdateIssuesRequest>,
) -> Result<Json<BulkUpdateIssuesResponse>, ErrorResponse> {
    if payload.updates.is_empty() {
        return Ok(Json(BulkUpdateIssuesResponse {
            data: vec![],
            txid: 0,
        }));
    }

    // Get first issue to determine project_id for access check
    let first_issue = IssueRepository::find_by_id(state.pool(), payload.updates[0].id)
        .await
        .map_err(|error| {
            tracing::error!(?error, "failed to find first issue");
            ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "failed to find issue")
        })?
        .ok_or_else(|| ErrorResponse::new(StatusCode::NOT_FOUND, "issue not found"))?;

    let project_id = first_issue.project_id;
    ensure_project_access(state.pool(), ctx.user.id, project_id).await?;

    let mut tx = crate::db::begin_tx(state.pool()).await.map_err(|error| {
        tracing::error!(?error, "failed to begin transaction");
        ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
    })?;

    let mut results = Vec::with_capacity(payload.updates.len());

    for item in payload.updates {
        // Verify issue belongs to the same project
        let issue = IssueRepository::find_by_id(&mut *tx, item.id)
            .await
            .map_err(|error| {
                tracing::error!(?error, issue_id = %item.id, "failed to find issue");
                ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "failed to find issue")
            })?
            .ok_or_else(|| ErrorResponse::new(StatusCode::NOT_FOUND, "issue not found"))?;

        if issue.project_id != project_id {
            return Err(ErrorResponse::new(
                StatusCode::BAD_REQUEST,
                "all issues must belong to the same project",
            ));
        }

        // Reject invalid workflow moves; the error drops `tx`, rolling back
        // any updates already applied in this bulk request.
        let transition = if let Some(new_status_id) = item.changes.status_id {
            ensure_valid_status_transition(
                &state,
                &issue,
                new_status_id,
                item.changes.extension_metadata.as_ref(),
            )
            .await?
        } else {
            None
        };

        let extension_metadata = metadata_for_status_change(
            &issue,
            transition.as_ref(),
            item.changes.extension_metadata,
        );

        // Update the issue
        let updated = IssueRepository::update(
            &mut *tx,
            item.id,
            item.changes.status_id,
            item.changes.title,
            item.changes.description,
            item.changes.priority,
            item.changes.start_date,
            item.changes.target_date,
            item.changes.completed_at,
            item.changes.sort_order,
            item.changes.parent_issue_id,
            item.changes.parent_issue_sort_order,
            extension_metadata,
        )
        .await
        .map_err(|error| {
            tracing::error!(?error, issue_id = %item.id, "failed to update issue");
            ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "failed to update issue")
        })?;

        results.push(updated);
    }

    let txid = get_txid(&mut *tx).await.map_err(|error| {
        tracing::error!(?error, "failed to get txid");
        ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
    })?;
    tx.commit().await.map_err(|error| {
        tracing::error!(?error, "failed to commit transaction");
        ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
    })?;

    Ok(Json(BulkUpdateIssuesResponse {
        data: results,
        txid,
    }))
}
