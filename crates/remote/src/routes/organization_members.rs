use api_types::{ListMembersResponse, MemberRole, OrganizationMemberWithProfile};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
};
use sqlx::PgPool;
use tracing::warn;
use uuid::Uuid;

use super::error::{ErrorResponse, membership_error};
use crate::{
    AppState,
    auth::RequestContext,
    db::{
        identity_errors::IdentityError, issues::IssueRepository, organization_members,
        organizations::OrganizationRepository, projects::ProjectRepository,
    },
};

/// Read-only roster of the organization: the board resolves assignee and
/// comment-author profiles through it, and the orchestrator uses it to tell
/// humans from agents when it checks plan approvals.
pub(super) fn router() -> Router<AppState> {
    Router::new().route("/organizations/{org_id}/members", get(list_members))
}

async fn list_members(
    State(state): State<AppState>,
    axum::extract::Extension(ctx): axum::extract::Extension<RequestContext>,
    Path(org_id): Path<Uuid>,
) -> Result<impl IntoResponse, ErrorResponse> {
    let user = ctx.user;
    ensure_member_access(&state.pool, org_id, user.id).await?;

    let members = sqlx::query_as!(
        OrganizationMemberWithProfile,
        r#"
        SELECT
            omm.user_id AS "user_id!: Uuid",
            omm.role AS "role!: MemberRole",
            omm.joined_at AS "joined_at!",
            u.first_name AS "first_name?",
            u.last_name AS "last_name?",
            u.username AS "username?",
            u.email AS "email?",
            oa.avatar_url AS "avatar_url?",
            u.is_agent AS "is_agent!"
        FROM organization_member_metadata omm
        INNER JOIN users u ON omm.user_id = u.id
        LEFT JOIN LATERAL (
            SELECT avatar_url
            FROM oauth_accounts
            WHERE user_id = omm.user_id
            ORDER BY created_at ASC
            LIMIT 1
        ) oa ON true
        WHERE omm.organization_id = $1
        ORDER BY omm.joined_at ASC
        "#,
        org_id
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|_| ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "Database error"))?;

    Ok(Json(ListMembersResponse { members }))
}

pub(crate) async fn ensure_member_access(
    pool: &PgPool,
    organization_id: Uuid,
    user_id: Uuid,
) -> Result<(), ErrorResponse> {
    organization_members::assert_membership(pool, organization_id, user_id)
        .await
        .map_err(|err| membership_error(err, "Not a member of organization"))
}

pub(crate) async fn ensure_admin_access(
    pool: &PgPool,
    organization_id: Uuid,
    user_id: Uuid,
) -> Result<(), ErrorResponse> {
    OrganizationRepository::new(pool)
        .assert_admin(organization_id, user_id)
        .await
        .map_err(|err| membership_error(err, "Admin access required"))
}

pub(crate) async fn ensure_project_access(
    pool: &PgPool,
    user_id: Uuid,
    project_id: Uuid,
) -> Result<Uuid, ErrorResponse> {
    let organization_id = ProjectRepository::organization_id(pool, project_id)
        .await
        .map_err(|error| {
            tracing::error!(?error, %project_id, "failed to load project");
            ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
        })?
        .ok_or_else(|| {
            warn!(
                %project_id,
                %user_id,
                "project not found for access check"
            );
            ErrorResponse::new(StatusCode::NOT_FOUND, "project not found")
        })?;

    organization_members::assert_membership(pool, organization_id, user_id)
        .await
        .map_err(|err| {
            if let IdentityError::Database(error) = &err {
                tracing::error!(
                    ?error,
                    %organization_id,
                    %project_id,
                    "failed to authorize project membership"
                );
            } else {
                warn!(
                    ?err,
                    %organization_id,
                    %project_id,
                    %user_id,
                    "project access denied"
                );
            }
            membership_error(err, "project not accessible")
        })?;

    Ok(organization_id)
}

pub(crate) async fn ensure_issue_access(
    pool: &PgPool,
    user_id: Uuid,
    issue_id: Uuid,
) -> Result<Uuid, ErrorResponse> {
    let organization_id = IssueRepository::organization_id(pool, issue_id)
        .await
        .map_err(|error| {
            tracing::error!(?error, %issue_id, "failed to load issue");
            ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
        })?
        .ok_or_else(|| {
            warn!(
                %issue_id,
                %user_id,
                "issue not found for access check"
            );
            ErrorResponse::new(StatusCode::NOT_FOUND, "issue not found")
        })?;

    organization_members::assert_membership(pool, organization_id, user_id)
        .await
        .map_err(|err| {
            if let IdentityError::Database(error) = &err {
                tracing::error!(
                    ?error,
                    %organization_id,
                    %issue_id,
                    "failed to authorize issue access"
                );
            } else {
                warn!(
                    ?err,
                    %organization_id,
                    %issue_id,
                    %user_id,
                    "issue access denied"
                );
            }
            membership_error(err, "issue not accessible")
        })?;

    Ok(organization_id)
}
