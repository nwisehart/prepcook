use axum::{
    Router,
    extract::{Query, State},
    response::Json as ResponseJson,
    routing::get,
};
use db::models::{
    requests::ContainerQuery,
    workspace::{ContainerInfo, Workspace, WorkspaceContext},
};
use deployment::Deployment;
use serde::Serialize;
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::{DeploymentImpl, error::ApiError};

#[derive(Debug, Serialize)]
struct ContainerInfoResponse {
    pub attempt_id: Uuid,
}

/// The workspace whose checkout contains `container_ref`.
///
/// A path in no workspace is an ordinary answer, not a failure: every session
/// with the `prepcook` MCP server asks from its working directory, and an
/// engineer's own checkout (the toolkit's sessions) is not a workspace. So it
/// is a 404, logged at debug.
async fn resolve_workspace(
    deployment: &DeploymentImpl,
    container_ref: &str,
) -> Result<ContainerInfo, ApiError> {
    Workspace::resolve_container_ref_by_prefix(&deployment.db().pool, container_ref)
        .await
        .map_err(|error| lookup_error(error, container_ref))
}

fn lookup_error(error: sqlx::Error, container_ref: &str) -> ApiError {
    match error {
        sqlx::Error::RowNotFound => {
            tracing::debug!(container_ref, "no workspace contains this path");
            ApiError::NotFound(NOT_A_WORKSPACE.to_string())
        }
        error => ApiError::Database(error),
    }
}

const NOT_A_WORKSPACE: &str = "No Prep Cook workspace contains this path.";

async fn get_container_info(
    Query(query): Query<ContainerQuery>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<ContainerInfoResponse>>, ApiError> {
    let info = resolve_workspace(&deployment, &query.container_ref).await?;

    Ok(ResponseJson(ApiResponse::success(ContainerInfoResponse {
        attempt_id: info.workspace_id,
    })))
}

async fn get_context(
    State(deployment): State<DeploymentImpl>,
    Query(payload): Query<ContainerQuery>,
) -> Result<ResponseJson<ApiResponse<WorkspaceContext>>, ApiError> {
    let info = resolve_workspace(&deployment, &payload.container_ref).await?;

    let ctx = Workspace::load_context(&deployment.db().pool, info.workspace_id).await?;
    Ok(ResponseJson(ApiResponse::success(ctx)))
}

pub(super) fn router(_deployment: &DeploymentImpl) -> Router<DeploymentImpl> {
    Router::new()
        // NOTE: /containers/info is required by the VSCode extension (prepcook-vscode)
        // to auto-detect workspaces. It maps workspace_id to attempt_id for compatibility.
        // Do not remove this endpoint without updating the extension.
        .route("/containers/info", get(get_container_info))
        .route("/containers/attempt-context", get(get_context))
}

#[cfg(test)]
mod tests {
    use axum::{http::StatusCode, response::IntoResponse};

    use super::*;

    #[test]
    fn a_path_in_no_workspace_is_a_404() {
        let response =
            lookup_error(sqlx::Error::RowNotFound, "/Users/someone/code/elsewhere").into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn a_database_failure_is_still_a_500() {
        let response = lookup_error(sqlx::Error::PoolTimedOut, "/tmp").into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
