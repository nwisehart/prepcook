//! Knowledge-store operations that span the board and the local machine.
//! K1 of docs/plans/rca-knowledge-graph-plan.md: backfilling the permanent
//! RCA store from already-approved bugs (new approvals file automatically
//! via the spec-approval hook in `routes::remote::issues`).

use api_types::{IssueExtMetadata, SearchIssuesRequest, WorkClass};
use axum::{Router, extract::State, response::Json as ResponseJson, routing::post};
use deployment::Deployment;
use serde::Serialize;
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::{DeploymentImpl, error::ApiError, routes::remote::issues};

pub fn router() -> Router<DeploymentImpl> {
    Router::new()
        .route("/knowledge/backfill-rcas", post(backfill_rcas))
        .route("/knowledge/link-jira", post(link_jira))
        .route("/knowledge/export", axum::routing::get(export_rcas))
        .route(
            "/knowledge/documents/{document_id}/html",
            axum::routing::get(get_document_html),
        )
}

#[derive(Debug, Serialize)]
pub struct LinkJiraResponse {
    /// Documents that now carry a Jira remote link (created or refreshed).
    pub linked: u32,
    /// Documents without a jira_key — nothing to link to.
    pub skipped_no_key: u32,
    /// Documents that received backfilled mechanical tags.
    pub tagged: u32,
    /// Jira rejections, listed as "KEY: error" for follow-up.
    pub failed: Vec<String>,
}

/// Backfills a "RCA — Prepcook" remote link onto every kb
/// document's Jira ticket. Idempotent: links carry a stable globalId, so
/// Jira upserts instead of duplicating. New RCAs are linked automatically
/// when filed; this covers everything filed before that hook existed.
async fn link_jira(
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<LinkJiraResponse>>, ApiError> {
    let client = deployment.remote_client()?;
    let project_ids: Vec<Uuid> = {
        let config = deployment.config().read().await;
        config
            .orchestrator
            .project_defaults
            .keys()
            .filter_map(|key| Uuid::parse_str(key).ok())
            .collect()
    };
    let tracker = deployment.tracker().current().await;
    let base = services::services::stage_orchestrator::public_ui_base();

    let mut linked = 0u32;
    let mut skipped_no_key = 0u32;
    let mut tagged = 0u32;
    let mut failed = Vec::new();
    for project_id in project_ids {
        let documents = client
            .list_kb_documents(project_id, Some("rca"), false)
            .await?
            .documents;
        for doc in documents {
            // Backfill the mechanical tag set for documents filed before
            // filing-time tagging existed (tag-union endpoint: no dupes).
            if let Some(issue_id) = doc.issue_id
                && let Ok(issue) = client.get_issue(issue_id).await
            {
                let metadata = IssueExtMetadata::from_value(&issue.extension_metadata);
                let board = deployment
                    .config()
                    .read()
                    .await
                    .project_settings(project_id)
                    .label;
                let missing: Vec<String> =
                    issues::auto_rca_tags(&issue, &metadata, board.as_deref())
                        .into_iter()
                        .filter(|tag| !doc.tags.contains(tag))
                        .collect();
                if !missing.is_empty()
                    && client
                        .add_kb_document_tags(
                            doc.id,
                            &api_types::AddKbDocumentTagsRequest { tags: missing },
                        )
                        .await
                        .is_ok()
                {
                    tagged += 1;
                }
            }
            let Some(jira_key) = doc.jira_key.as_deref() else {
                skipped_no_key += 1;
                continue;
            };
            let url = format!("{base}/api/knowledge/documents/{}/html", doc.id);
            match tracker
                .set_remote_link(
                    jira_key,
                    services::services::tracker::RemoteLink {
                        id: &format!("prepcook-rca-{}", doc.id),
                        title: &format!("RCA — Prepcook ({jira_key})"),
                        url: &url,
                    },
                )
                .await
            {
                Ok(()) => linked += 1,
                Err(e) => failed.push(format!("{jira_key}: {e}")),
            }
        }
    }
    tracing::info!(
        "RCA Jira-link backfill: linked {linked}, no key {skipped_no_key}, failed {}",
        failed.len()
    );
    Ok(ResponseJson(ApiResponse::success(LinkJiraResponse {
        linked,
        skipped_no_key,
        tagged,
        failed,
    })))
}

#[derive(Debug, serde::Deserialize)]
pub struct ExportQuery {
    pub project_id: Uuid,
    /// Inclusive lower bound, YYYY-MM-DD (document created_at, UTC).
    pub since: Option<String>,
    /// Inclusive upper bound, YYYY-MM-DD.
    pub until: Option<String>,
    /// "json" (default) or "csv".
    pub format: Option<String>,
}

#[derive(Debug, Serialize)]
struct ExportEntry {
    id: Uuid,
    jira_key: Option<String>,
    title: String,
    summary: Option<String>,
    tags: Vec<String>,
    created_at: String,
    updated_at: String,
    /// RCA body with HTML stripped — the analysis-friendly form.
    body_text: String,
}

fn csv_field(value: &str) -> String {
    format!("\"{}\"", value.replace('\r', "").replace('"', "\"\""))
}

/// Date-filtered RCA export for offline/team analysis, as a JSON or CSV
/// attachment. Bounds are whole days in UTC against `created_at`.
async fn export_rcas(
    State(deployment): State<DeploymentImpl>,
    axum::extract::Query(query): axum::extract::Query<ExportQuery>,
) -> Result<axum::response::Response, ApiError> {
    use axum::response::IntoResponse;
    use chrono::{DateTime, NaiveDate, Utc};

    let parse_day = |value: &Option<String>| -> Option<NaiveDate> {
        value
            .as_deref()
            .filter(|v| !v.trim().is_empty())
            .and_then(|v| NaiveDate::parse_from_str(v.trim(), "%Y-%m-%d").ok())
    };
    let since = parse_day(&query.since);
    let until = parse_day(&query.until);

    let client = deployment.remote_client()?;
    let summaries = client
        .list_kb_documents(query.project_id, Some("rca"), false)
        .await?
        .documents;

    let in_range = |created_at: &DateTime<Utc>| -> bool {
        let day = created_at.date_naive();
        since.is_none_or(|s| day >= s) && until.is_none_or(|u| day <= u)
    };

    let mut entries = Vec::new();
    for summary in summaries {
        if !in_range(&summary.created_at) {
            continue;
        }
        let Ok(full) = client.get_kb_document(summary.id).await else {
            continue;
        };
        entries.push(ExportEntry {
            id: summary.id,
            jira_key: summary.jira_key.clone(),
            title: summary.title.clone(),
            summary: summary.summary.clone(),
            tags: summary.tags.clone(),
            created_at: summary.created_at.to_rfc3339(),
            updated_at: summary.updated_at.to_rfc3339(),
            body_text: services::services::stage_orchestrator::html_to_plaintext(
                &full.body, 60_000,
            ),
        });
    }
    entries.sort_by(|a, b| a.created_at.cmp(&b.created_at));

    let window = format!(
        "{}-to-{}",
        query.since.as_deref().unwrap_or("start"),
        query.until.as_deref().unwrap_or("now")
    );
    let csv_mode = query.format.as_deref() == Some("csv");
    let (payload, content_type, filename): (String, &str, String) = if csv_mode {
        let mut csv = String::from("jira_key,title,created_at,tags,summary,body_text\n");
        for entry in &entries {
            csv.push_str(&format!(
                "{},{},{},{},{},{}\n",
                csv_field(entry.jira_key.as_deref().unwrap_or("")),
                csv_field(&entry.title),
                csv_field(&entry.created_at),
                csv_field(&entry.tags.join("; ")),
                csv_field(entry.summary.as_deref().unwrap_or("")),
                csv_field(&entry.body_text),
            ));
        }
        (csv, "text/csv", format!("rca-export-{window}.csv"))
    } else {
        (
            serde_json::to_string_pretty(&entries)
                .map_err(|e| ApiError::BadRequest(format!("export serialization failed: {e}")))?,
            "application/json",
            format!("rca-export-{window}.json"),
        )
    };

    Ok((
        [
            (axum::http::header::CONTENT_TYPE, content_type.to_string()),
            (
                axum::http::header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{filename}\""),
            ),
            // Browser tools (Claude Cowork artifacts, dashboards) fetch this
            // cross-origin. Read-only data on a VPN-gated host — a wildcard
            // here exposes nothing the network boundary doesn't already gate.
            (
                axum::http::header::ACCESS_CONTROL_ALLOW_ORIGIN,
                "*".to_string(),
            ),
        ],
        payload,
    )
        .into_response())
}

/// Serves a stored knowledge document (RCAs are self-contained HTML) for
/// viewing in the browser — the Knowledge view's "open document" target.
async fn get_document_html(
    State(deployment): State<DeploymentImpl>,
    axum::extract::Path(document_id): axum::extract::Path<Uuid>,
) -> Result<axum::response::Response, ApiError> {
    use axum::response::IntoResponse;
    let client = deployment.remote_client()?;
    let document = client.get_kb_document(document_id).await?;
    Ok(axum::response::Html(document.body).into_response())
}

#[derive(Debug, Serialize)]
pub struct BackfillRcasResponse {
    /// Approved bug RCAs found across the configured projects.
    pub scanned: u32,
    /// Documents filed (or refreshed) in the knowledge store.
    pub filed: u32,
    /// Issues whose artifact could not be located (no snapshot, worktree
    /// already cleaned up) — listed by simple id for manual follow-up.
    pub missing_artifact: Vec<String>,
}

/// Walks every orchestrator-configured project for approved bug RCAs and
/// files each into `kb_documents`. Idempotent: re-runs refresh instead of
/// duplicating (the store upserts per issue).
async fn backfill_rcas(
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<BackfillRcasResponse>>, ApiError> {
    const PAGE: i32 = 200;

    let client = deployment.remote_client()?;
    let project_ids: Vec<Uuid> = {
        let config = deployment.config().read().await;
        config
            .orchestrator
            .project_defaults
            .keys()
            .filter_map(|key| Uuid::parse_str(key).ok())
            .collect()
    };
    if project_ids.is_empty() {
        return Err(ApiError::BadRequest(
            "No projects configured in Settings > Orchestrator".to_string(),
        ));
    }

    let mut scanned = 0u32;
    let mut filed = 0u32;
    let mut missing_artifact = Vec::new();

    for project_id in project_ids {
        let mut offset: i32 = 0;
        loop {
            let response = client
                .search_issues(&SearchIssuesRequest {
                    project_id,
                    status_id: None,
                    status_ids: None,
                    priority: None,
                    parent_issue_id: None,
                    search: None,
                    simple_id: None,
                    assignee_user_id: None,
                    tag_id: None,
                    tag_ids: None,
                    sort_field: None,
                    sort_direction: None,
                    limit: Some(PAGE),
                    offset: Some(offset),
                })
                .await?;

            for issue in &response.issues {
                let metadata = IssueExtMetadata::from_value(&issue.extension_metadata);
                let approved = metadata
                    .spec
                    .as_ref()
                    .is_some_and(|spec| spec.approved_at.is_some());
                if metadata.work_class != Some(WorkClass::Bug) || !approved {
                    continue;
                }
                scanned += 1;

                // Prefer the persistent snapshot; fall back to a live
                // worktree for RCAs approved before snapshotting existed.
                let snapshot = issues::approved_specs_dir().join(format!("{}.html", issue.id));
                let artifact = if snapshot.is_file() {
                    Some(snapshot)
                } else {
                    match metadata.spec.as_ref().and_then(|spec| spec.path.clone()) {
                        Some(spec_path) => {
                            issues::find_spec_in_worktrees(&deployment, issue.id, &spec_path).await
                        }
                        None => None,
                    }
                };
                let Some(artifact) = artifact else {
                    missing_artifact.push(issue.simple_id.clone());
                    continue;
                };

                // Never post the plain-language comment from a backfill: it
                // would fire on every historical ticket at once.
                match issues::file_rca_document(&deployment, issue, &artifact, false).await {
                    Ok(()) => filed += 1,
                    Err(e) => {
                        tracing::warn!("Backfill: failed to file RCA for {}: {e}", issue.simple_id);
                        missing_artifact.push(issue.simple_id.clone());
                    }
                }
            }

            if (response.issues.len() as i32) < PAGE {
                break;
            }
            offset += PAGE;
        }
    }

    tracing::info!(
        "RCA backfill: scanned {scanned}, filed {filed}, missing {}",
        missing_artifact.len()
    );
    Ok(ResponseJson(ApiResponse::success(BackfillRcasResponse {
        scanned,
        filed,
        missing_artifact,
    })))
}
