use api_types::{ListMembersResponse, ListOrganizationsResponse};
use axum::{
    Router,
    extract::{Path, State},
    response::Json as ResponseJson,
    routing::get,
};
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::{DeploymentImpl, error::ApiError};

pub fn router() -> Router<DeploymentImpl> {
    Router::new()
        .route("/organizations", get(list_organizations))
        .route("/organizations/{org_id}/members", get(list_members))
}

async fn list_organizations(
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<ListOrganizationsResponse>>, ApiError> {
    let client = deployment.remote_client()?;

    let response = client.list_organizations().await?;

    Ok(ResponseJson(ApiResponse::success(response)))
}

async fn list_members(
    State(deployment): State<DeploymentImpl>,
    Path(org_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<ListMembersResponse>>, ApiError> {
    let client = deployment.remote_client()?;

    let response = client.list_members(org_id).await?;

    Ok(ResponseJson(ApiResponse::success(response)))
}
