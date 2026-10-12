use api_types::{
    AddKbDocumentTagsRequest, CreateKbDocumentRequest, KbDocument, ListKbDocumentsQuery,
    ListKbDocumentsResponse,
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
use crate::{AppState, auth::RequestContext, db::kb_documents::KbDocumentRepository};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/kb_documents",
            get(list_kb_documents).post(create_kb_document),
        )
        .route("/kb_documents/{kb_document_id}", get(get_kb_document))
        .route(
            "/kb_documents/{kb_document_id}/tags",
            axum::routing::post(add_kb_document_tags),
        )
}

#[instrument(
    name = "kb_documents.add_tags",
    skip(state, ctx, request),
    fields(kb_document_id = %kb_document_id, user_id = %ctx.user.id)
)]
async fn add_kb_document_tags(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Path(kb_document_id): Path<Uuid>,
    Json(request): Json<AddKbDocumentTagsRequest>,
) -> Result<Json<KbDocument>, ErrorResponse> {
    // Load first for the project-access check.
    let existing = KbDocumentRepository::find_by_id(state.pool(), kb_document_id)
        .await
        .map_err(|error| {
            tracing::error!(?error, %kb_document_id, "failed to load kb document");
            ErrorResponse::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to load kb document",
            )
        })?
        .ok_or_else(|| ErrorResponse::new(StatusCode::NOT_FOUND, "kb document not found"))?;
    ensure_project_access(state.pool(), ctx.user.id, existing.project_id).await?;

    let tags: Vec<String> = request
        .tags
        .into_iter()
        .map(|tag| tag.trim().to_string())
        .filter(|tag| !tag.is_empty())
        .collect();
    if tags.is_empty() {
        return Err(ErrorResponse::new(
            StatusCode::BAD_REQUEST,
            "tags must not be empty",
        ));
    }

    let document = KbDocumentRepository::add_tags(state.pool(), kb_document_id, &tags)
        .await
        .map_err(|error| {
            tracing::error!(?error, %kb_document_id, "failed to tag kb document");
            ErrorResponse::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to tag kb document",
            )
        })?
        .ok_or_else(|| ErrorResponse::new(StatusCode::NOT_FOUND, "kb document not found"))?;

    Ok(Json(document))
}

#[instrument(
    name = "kb_documents.list",
    skip(state, ctx),
    fields(project_id = %query.project_id, user_id = %ctx.user.id)
)]
async fn list_kb_documents(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Query(query): Query<ListKbDocumentsQuery>,
) -> Result<Json<ListKbDocumentsResponse>, ErrorResponse> {
    ensure_project_access(state.pool(), ctx.user.id, query.project_id).await?;

    let documents = KbDocumentRepository::list_by_project(
        state.pool(),
        query.project_id,
        query.kind.as_deref(),
        query.unlinked.unwrap_or(false),
    )
    .await
    .map_err(|error| {
        tracing::error!(?error, project_id = %query.project_id, "failed to list kb documents");
        ErrorResponse::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "failed to list kb documents",
        )
    })?;

    Ok(Json(ListKbDocumentsResponse { documents }))
}

#[instrument(
    name = "kb_documents.get",
    skip(state, ctx),
    fields(kb_document_id = %kb_document_id, user_id = %ctx.user.id)
)]
async fn get_kb_document(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Path(kb_document_id): Path<Uuid>,
) -> Result<Json<KbDocument>, ErrorResponse> {
    let document = KbDocumentRepository::find_by_id(state.pool(), kb_document_id)
        .await
        .map_err(|error| {
            tracing::error!(?error, %kb_document_id, "failed to load kb document");
            ErrorResponse::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to load kb document",
            )
        })?
        .ok_or_else(|| ErrorResponse::new(StatusCode::NOT_FOUND, "kb document not found"))?;

    ensure_project_access(state.pool(), ctx.user.id, document.project_id).await?;

    Ok(Json(document))
}

#[instrument(
    name = "kb_documents.create",
    skip(state, ctx, request),
    fields(project_id = %request.project_id, kind = %request.kind, user_id = %ctx.user.id)
)]
async fn create_kb_document(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Json(request): Json<CreateKbDocumentRequest>,
) -> Result<Json<KbDocument>, ErrorResponse> {
    ensure_project_access(state.pool(), ctx.user.id, request.project_id).await?;

    if request.kind.trim().is_empty() || request.title.trim().is_empty() {
        return Err(ErrorResponse::new(
            StatusCode::BAD_REQUEST,
            "kind and title are required",
        ));
    }

    let document = KbDocumentRepository::upsert(state.pool(), &request)
        .await
        .map_err(|error| {
            tracing::error!(?error, project_id = %request.project_id, "failed to create kb document");
            ErrorResponse::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to create kb document",
            )
        })?;

    Ok(Json(document))
}
