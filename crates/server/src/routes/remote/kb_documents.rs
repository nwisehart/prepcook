use api_types::{
    AddKbDocumentTagsRequest, CreateKbDocumentRequest, FileKbDocumentRequest,
    FileKbDocumentResponse, KbDocument, KbDocumentSummary, ListKbDocumentsQuery,
    ListKbDocumentsResponse,
};
use axum::{
    Router,
    extract::{Json, Path, Query, State},
    response::Json as ResponseJson,
    routing::get,
};
use deployment::Deployment;
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::{DeploymentImpl, error::ApiError};

pub(super) fn router() -> Router<DeploymentImpl> {
    Router::new()
        .route(
            "/kb_documents",
            get(list_kb_documents).post(create_kb_document),
        )
        .route("/kb_documents/file", axum::routing::post(file_kb_document))
        .route("/kb_documents/{kb_document_id}", get(get_kb_document))
        .route(
            "/kb_documents/{kb_document_id}/tags",
            axum::routing::post(add_kb_document_tags),
        )
}

async fn add_kb_document_tags(
    State(deployment): State<DeploymentImpl>,
    Path(kb_document_id): Path<Uuid>,
    Json(request): Json<AddKbDocumentTagsRequest>,
) -> Result<ResponseJson<ApiResponse<KbDocument>>, ApiError> {
    let client = deployment.remote_client()?;
    let response = client
        .add_kb_document_tags(kb_document_id, &request)
        .await?;
    Ok(ResponseJson(ApiResponse::success(response)))
}

async fn list_kb_documents(
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<ListKbDocumentsQuery>,
) -> Result<ResponseJson<ApiResponse<ListKbDocumentsResponse>>, ApiError> {
    let client = deployment.remote_client()?;
    let response = client
        .list_kb_documents(
            query.project_id,
            query.kind.as_deref(),
            query.unlinked.unwrap_or(false),
        )
        .await?;
    Ok(ResponseJson(ApiResponse::success(response)))
}

async fn get_kb_document(
    State(deployment): State<DeploymentImpl>,
    Path(kb_document_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<KbDocument>>, ApiError> {
    let client = deployment.remote_client()?;
    let response = client.get_kb_document(kb_document_id).await?;
    Ok(ResponseJson(ApiResponse::success(response)))
}

async fn create_kb_document(
    State(deployment): State<DeploymentImpl>,
    Json(request): Json<CreateKbDocumentRequest>,
) -> Result<ResponseJson<ApiResponse<KbDocument>>, ApiError> {
    let client = deployment.remote_client()?;
    let response = client.create_kb_document(&request).await?;
    Ok(ResponseJson(ApiResponse::success(response)))
}

/// The most a filed document may weigh. An RCA with its harness inline and
/// a few screenshots described in words is well under a megabyte; the cap
/// stops a pasted binary from landing in the store.
const MAX_BODY_BYTES: usize = 4 * 1024 * 1024;

/// Files a document authored outside the board and pins it to its Jira
/// ticket — the same two steps `issues::file_rca_document` takes for the
/// board's own RCAs, for a document a person wrote on their machine.
///
/// The store upserts on the source issue, and a document from outside the
/// board has none, so a second filing of the same ticket would be a second
/// row. Instead the ticket is looked up first: a document already filed for
/// it (same project, same kind, same key, no source issue) is returned as it
/// is, with `filed: false`, and re-pinned. Replacing its body needs the
/// store to upsert on the key, which it does not do yet.
async fn file_kb_document(
    State(deployment): State<DeploymentImpl>,
    Json(request): Json<FileKbDocumentRequest>,
) -> Result<ResponseJson<ApiResponse<FileKbDocumentResponse>>, ApiError> {
    if let Err(reason) = check_filing(&request) {
        return Err(ApiError::BadRequest(reason));
    }
    let client = deployment.remote_client()?;

    let existing = client
        .list_kb_documents(request.project_id, Some(request.kind.as_str()), false)
        .await?;
    let (document, filed) = match already_filed(&existing.documents, &request.jira_key) {
        Some(summary) => (client.get_kb_document(summary.id).await?, false),
        None => (
            client
                .create_kb_document(&CreateKbDocumentRequest {
                    project_id: request.project_id,
                    kind: request.kind.clone(),
                    title: request.title.clone(),
                    body: request.body.clone(),
                    issue_id: None,
                    jira_key: Some(request.jira_key.clone()),
                    work_class: request.work_class.clone(),
                    summary: request.summary.clone(),
                    tags: request.tags.clone(),
                })
                .await?,
            true,
        ),
    };
    if filed {
        tracing::info!(
            "Filed {} document {} for {} from outside the board",
            request.kind,
            document.id,
            request.jira_key
        );
    }

    let tracker = deployment.tracker().current().await;
    let jira_linked = if !tracker.is_configured() {
        tracing::debug!(
            "Document {} not linked to {}: no tracker is configured",
            document.id,
            request.jira_key
        );
        false
    } else {
        match super::issues::pin_document_to_jira(tracker.as_ref(), &request.jira_key, document.id)
            .await
        {
            Ok(()) => true,
            Err(e) => {
                tracing::warn!(
                    "Linking document {} to Jira {} failed: {e}",
                    document.id,
                    request.jira_key
                );
                false
            }
        }
    };

    Ok(ResponseJson(ApiResponse::success(FileKbDocumentResponse {
        url: super::issues::document_url(document.id),
        document,
        filed,
        jira_linked,
    })))
}

/// What a filing must carry before the store is asked anything.
fn check_filing(request: &FileKbDocumentRequest) -> Result<(), String> {
    if request.kind.trim().is_empty() {
        return Err("kind is required (\"rca\" for a root-cause analysis)".to_string());
    }
    if request.title.trim().is_empty() {
        return Err("title is required".to_string());
    }
    if request.body.trim().is_empty() {
        return Err("body is required".to_string());
    }
    if request.body.len() > MAX_BODY_BYTES {
        return Err(format!(
            "body is {} bytes; the store takes at most {MAX_BODY_BYTES}",
            request.body.len()
        ));
    }
    if !is_jira_key(&request.jira_key) {
        return Err(format!(
            "jira_key {:?} is not a ticket key (expected PROJECT-123)",
            request.jira_key
        ));
    }
    Ok(())
}

/// `BUG-1234`: an upper-case project key, a dash, a number.
fn is_jira_key(value: &str) -> bool {
    let Some((project, number)) = value.split_once('-') else {
        return false;
    };
    !project.is_empty()
        && project
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        && project.starts_with(|c: char| c.is_ascii_uppercase())
        && !number.is_empty()
        && number.chars().all(|c| c.is_ascii_digit())
}

/// The document already filed for this ticket from outside the board, if
/// any: same key, no source issue. A document the board filed for the same
/// ticket has a source issue and is a different document — the board's
/// pipeline owns that one, and a local filing sits beside it.
fn already_filed<'a>(
    documents: &'a [KbDocumentSummary],
    jira_key: &str,
) -> Option<&'a KbDocumentSummary> {
    documents
        .iter()
        .find(|doc| doc.issue_id.is_none() && doc.jira_key.as_deref() == Some(jira_key))
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;

    fn summary(jira_key: Option<&str>, issue_id: Option<Uuid>) -> KbDocumentSummary {
        KbDocumentSummary {
            id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            kind: "rca".to_string(),
            title: "t".to_string(),
            issue_id,
            jira_key: jira_key.map(str::to_string),
            work_class: Some("bug".to_string()),
            summary: None,
            tags: Vec::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn filing(jira_key: &str) -> FileKbDocumentRequest {
        FileKbDocumentRequest {
            project_id: Uuid::new_v4(),
            kind: "rca".to_string(),
            title: "BUG-1234 — Org Name stays blank".to_string(),
            body: "<html></html>".to_string(),
            jira_key: jira_key.to_string(),
            work_class: Some("bug".to_string()),
            summary: None,
            tags: Vec::new(),
        }
    }

    #[test]
    fn a_ticket_filed_from_outside_the_board_is_found_again() {
        let ours = summary(Some("BUG-1234"), None);
        let ours_id = ours.id;
        let documents = vec![
            // The board's own RCA for the same ticket: a different document.
            summary(Some("BUG-1234"), Some(Uuid::new_v4())),
            // Another ticket.
            summary(Some("BUG-1235"), None),
            ours,
            // No ticket at all.
            summary(None, None),
        ];
        assert_eq!(
            already_filed(&documents, "BUG-1234").map(|d| d.id),
            Some(ours_id)
        );
        assert!(already_filed(&documents, "BUG-9999").is_none());
        assert!(already_filed(&[], "BUG-1234").is_none());
    }

    #[test]
    fn a_filing_is_checked_before_the_store_is_asked() {
        assert!(check_filing(&filing("BUG-1234")).is_ok());
        assert!(check_filing(&filing("bug-1234")).is_err());
        assert!(check_filing(&filing("BUG1234")).is_err());
        assert!(check_filing(&filing("BUG-")).is_err());
        assert!(check_filing(&filing("")).is_err());

        let mut blank = filing("BUG-1234");
        blank.body = "  ".to_string();
        assert!(check_filing(&blank).is_err());

        let mut heavy = filing("BUG-1234");
        heavy.body = "x".repeat(MAX_BODY_BYTES + 1);
        assert!(check_filing(&heavy).unwrap_err().contains("at most"));

        let mut no_kind = filing("BUG-1234");
        no_kind.kind = String::new();
        assert!(check_filing(&no_kind).is_err());
    }

    #[test]
    fn jira_keys_are_project_dash_number() {
        for good in ["BUG-1", "BUG-10723", "AB1-42"] {
            assert!(is_jira_key(good), "{good}");
        }
        for bad in [
            "", "BUG", "BUG-", "-1", "bug-1", "1BUG-1", "BUG-1a", "BUG 1", "BUG-1-2",
        ] {
            assert!(!is_jira_key(bad), "{bad}");
        }
    }
}
