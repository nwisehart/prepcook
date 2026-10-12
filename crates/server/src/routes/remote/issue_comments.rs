use api_types::{
    CreateIssueCommentRequest, IssueComment, ListIssueCommentsQuery, ListIssueCommentsResponse,
    MutationResponse,
};
use axum::{
    Router,
    extract::{Json, Query, State},
    response::Json as ResponseJson,
    routing::get,
};
use utils::response::ApiResponse;

use crate::{DeploymentImpl, error::ApiError};

pub(super) fn router() -> Router<DeploymentImpl> {
    Router::new().route(
        "/issue_comments",
        get(list_issue_comments).post(create_issue_comment),
    )
}

async fn list_issue_comments(
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<ListIssueCommentsQuery>,
) -> Result<ResponseJson<ApiResponse<ListIssueCommentsResponse>>, ApiError> {
    let client = deployment.remote_client()?;
    let response = client.list_issue_comments(query.issue_id).await?;
    Ok(ResponseJson(ApiResponse::success(response)))
}

async fn create_issue_comment(
    State(deployment): State<DeploymentImpl>,
    Json(request): Json<CreateIssueCommentRequest>,
) -> Result<ResponseJson<ApiResponse<MutationResponse<IssueComment>>>, ApiError> {
    let client = deployment.remote_client()?;
    let response = client.create_issue_comment(&request).await?;
    Ok(ResponseJson(ApiResponse::success(response)))
}
