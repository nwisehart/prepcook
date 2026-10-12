use api_types::{
    CreateKbEdgeRequest, CreateKbNodeRequest, KbEdge, KbNode, ListKbEdgesQuery,
    ListKbEdgesResponse, ListKbNodesQuery, ListKbNodesResponse,
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
    Router::new()
        .route("/kb_nodes", get(list_kb_nodes).post(create_kb_node))
        .route("/kb_edges", get(list_kb_edges).post(create_kb_edge))
}

async fn list_kb_nodes(
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<ListKbNodesQuery>,
) -> Result<ResponseJson<ApiResponse<ListKbNodesResponse>>, ApiError> {
    let client = deployment.remote_client()?;
    let response = client.list_kb_nodes(query.project_id).await?;
    Ok(ResponseJson(ApiResponse::success(response)))
}

async fn create_kb_node(
    State(deployment): State<DeploymentImpl>,
    Json(request): Json<CreateKbNodeRequest>,
) -> Result<ResponseJson<ApiResponse<KbNode>>, ApiError> {
    let client = deployment.remote_client()?;
    let response = client.create_kb_node(&request).await?;
    Ok(ResponseJson(ApiResponse::success(response)))
}

async fn list_kb_edges(
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<ListKbEdgesQuery>,
) -> Result<ResponseJson<ApiResponse<ListKbEdgesResponse>>, ApiError> {
    let client = deployment.remote_client()?;
    let response = client
        .list_kb_edges(query.project_id, query.node_id, query.document_id)
        .await?;
    Ok(ResponseJson(ApiResponse::success(response)))
}

async fn create_kb_edge(
    State(deployment): State<DeploymentImpl>,
    Json(request): Json<CreateKbEdgeRequest>,
) -> Result<ResponseJson<ApiResponse<KbEdge>>, ApiError> {
    let client = deployment.remote_client()?;
    let response = client.create_kb_edge(&request).await?;
    Ok(ResponseJson(ApiResponse::success(response)))
}
