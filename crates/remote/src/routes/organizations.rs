use api_types::ListOrganizationsResponse;
use axum::{Json, Router, extract::State, http::StatusCode, response::IntoResponse, routing::get};

use super::error::ErrorResponse;
use crate::{AppState, auth::RequestContext, db::organizations::OrganizationRepository};

/// The organization is implicit (one per server, every user a member), so
/// the only thing a client can ask is which one it is.
pub(super) fn router() -> Router<AppState> {
    Router::new().route("/organizations", get(list_organizations))
}

async fn list_organizations(
    State(state): State<AppState>,
    axum::extract::Extension(ctx): axum::extract::Extension<RequestContext>,
) -> Result<impl IntoResponse, ErrorResponse> {
    let org_repo = OrganizationRepository::new(&state.pool);

    let organizations = org_repo
        .list_user_organizations(ctx.user.id)
        .await
        .map_err(|_| ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "Database error"))?;

    Ok(Json(ListOrganizationsResponse { organizations }))
}
