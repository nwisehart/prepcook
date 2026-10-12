use api_types::{
    CreateKbEdgeRequest, CreateKbNodeRequest, KbEdge, KbNode, ListKbEdgesQuery,
    ListKbEdgesResponse, ListKbNodesQuery, ListKbNodesResponse, StampKbNodeInvestigationRequest,
};
use axum::{
    Json, Router,
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    routing::get,
};
use tracing::instrument;
use uuid::Uuid;

use super::{error::ErrorResponse, organization_members::ensure_project_access};
use crate::{AppState, auth::RequestContext, db::kb_graph::KbGraphRepository};

const NODE_KINDS: &[&str] = &[
    "product_area",
    "component",
    "root_cause_class",
    "data_model",
    "infra",
];

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/kb_nodes", get(list_kb_nodes).post(create_kb_node))
        .route("/kb_edges", get(list_kb_edges).post(create_kb_edge))
        .route(
            "/kb_nodes/{kb_node_id}/investigation",
            axum::routing::post(stamp_kb_node_investigation),
        )
}

#[instrument(
    name = "kb_graph.stamp_investigation",
    skip(state, ctx, request),
    fields(kb_node_id = %kb_node_id, user_id = %ctx.user.id)
)]
async fn stamp_kb_node_investigation(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Path(kb_node_id): Path<Uuid>,
    Json(request): Json<StampKbNodeInvestigationRequest>,
) -> Result<Json<KbNode>, ErrorResponse> {
    let node = KbGraphRepository::stamp_investigation(
        state.pool(),
        kb_node_id,
        &request.jira_key,
        request.edge_count,
    )
    .await
    .map_err(|error| {
        tracing::error!(?error, %kb_node_id, "failed to stamp kb node");
        ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "failed to stamp kb node")
    })?
    .ok_or_else(|| ErrorResponse::new(StatusCode::NOT_FOUND, "kb node not found"))?;

    ensure_project_access(state.pool(), ctx.user.id, node.project_id).await?;

    Ok(Json(node))
}

#[instrument(
    name = "kb_graph.list_nodes",
    skip(state, ctx),
    fields(project_id = %query.project_id, user_id = %ctx.user.id)
)]
async fn list_kb_nodes(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Query(query): Query<ListKbNodesQuery>,
) -> Result<Json<ListKbNodesResponse>, ErrorResponse> {
    ensure_project_access(state.pool(), ctx.user.id, query.project_id).await?;

    let nodes = KbGraphRepository::list_nodes(state.pool(), query.project_id)
        .await
        .map_err(|error| {
            tracing::error!(?error, project_id = %query.project_id, "failed to list kb nodes");
            ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "failed to list kb nodes")
        })?;

    Ok(Json(ListKbNodesResponse { nodes }))
}

#[instrument(
    name = "kb_graph.create_node",
    skip(state, ctx, request),
    fields(project_id = %request.project_id, kind = %request.kind, user_id = %ctx.user.id)
)]
async fn create_kb_node(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Json(request): Json<CreateKbNodeRequest>,
) -> Result<Json<KbNode>, ErrorResponse> {
    ensure_project_access(state.pool(), ctx.user.id, request.project_id).await?;

    if !NODE_KINDS.contains(&request.kind.as_str()) {
        return Err(ErrorResponse::new(
            StatusCode::BAD_REQUEST,
            format!("kind must be one of: {}", NODE_KINDS.join(", ")),
        ));
    }
    if request.name.trim().is_empty() {
        return Err(ErrorResponse::new(
            StatusCode::BAD_REQUEST,
            "name is required",
        ));
    }

    let node = KbGraphRepository::upsert_node(state.pool(), &request)
        .await
        .map_err(|error| {
            tracing::error!(?error, project_id = %request.project_id, "failed to create kb node");
            ErrorResponse::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to create kb node",
            )
        })?;

    Ok(Json(node))
}

#[instrument(
    name = "kb_graph.list_edges",
    skip(state, ctx),
    fields(project_id = %query.project_id, user_id = %ctx.user.id)
)]
async fn list_kb_edges(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Query(query): Query<ListKbEdgesQuery>,
) -> Result<Json<ListKbEdgesResponse>, ErrorResponse> {
    ensure_project_access(state.pool(), ctx.user.id, query.project_id).await?;

    let edges = KbGraphRepository::list_edges(
        state.pool(),
        query.project_id,
        query.node_id,
        query.document_id,
    )
    .await
    .map_err(|error| {
        tracing::error!(?error, project_id = %query.project_id, "failed to list kb edges");
        ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "failed to list kb edges")
    })?;

    Ok(Json(ListKbEdgesResponse { edges }))
}

#[instrument(
    name = "kb_graph.create_edge",
    skip(state, ctx, request),
    fields(project_id = %request.project_id, user_id = %ctx.user.id)
)]
async fn create_kb_edge(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Json(request): Json<CreateKbEdgeRequest>,
) -> Result<Json<KbEdge>, ErrorResponse> {
    ensure_project_access(state.pool(), ctx.user.id, request.project_id).await?;

    let edge = KbGraphRepository::upsert_edge(state.pool(), &request)
        .await
        .map_err(|error| {
            tracing::error!(?error, project_id = %request.project_id, "failed to create kb edge");
            ErrorResponse::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to create kb edge",
            )
        })?;

    Ok(Json(edge))
}
