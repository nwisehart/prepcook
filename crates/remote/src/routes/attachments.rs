//! Attachment endpoints. This server has no blob store, so every route
//! answers 501 with the same message; the request and response types are
//! kept so clients compile against a stable contract and the `attachments`
//! and `blobs` tables are left in place for a future store.

use api_types::AttachmentWithBlob;
use axum::{
    Router,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, post},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use super::error::ErrorResponse;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/attachments/init", post(not_supported))
        .route("/attachments/confirm", post(not_supported))
        .route("/attachments/{id}/file", get(not_supported))
        .route("/attachments/{id}/thumbnail", get(not_supported))
        .route("/attachments/{id}", delete(not_supported))
        .route("/issues/{issue_id}/attachments", get(not_supported))
        .route("/issues/{issue_id}/attachments/commit", post(not_supported))
        .route("/comments/{comment_id}/attachments", get(not_supported))
        .route(
            "/comments/{comment_id}/attachments/commit",
            post(not_supported),
        )
}

#[derive(Debug, Serialize, Deserialize, TS)]
pub struct InitUploadRequest {
    pub project_id: Uuid,
    pub filename: String,
    #[ts(type = "number")]
    pub size_bytes: i64,
    pub hash: String,
}

#[derive(Debug, Serialize, Deserialize, TS)]
pub struct InitUploadResponse {
    pub upload_url: String,
    pub upload_id: Uuid,
    pub expires_at: DateTime<Utc>,
    pub skip_upload: bool,
    pub existing_blob_id: Option<Uuid>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
pub struct ConfirmUploadRequest {
    pub project_id: Uuid,
    pub upload_id: Uuid,
    pub filename: String,
    #[ts(optional)]
    pub content_type: Option<String>,
    #[ts(type = "number")]
    pub size_bytes: i64,
    pub hash: String,
    #[ts(optional)]
    pub issue_id: Option<Uuid>,
    #[ts(optional)]
    pub comment_id: Option<Uuid>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
pub struct CommitAttachmentsRequest {
    pub attachment_ids: Vec<Uuid>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
pub struct CommitAttachmentsResponse {
    pub attachments: Vec<AttachmentWithBlob>,
}

async fn not_supported() -> Response {
    ErrorResponse::new(
        StatusCode::NOT_IMPLEMENTED,
        "Attachments are not supported on this server",
    )
    .into_response()
}
