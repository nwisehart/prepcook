use api_types::{
    CreateIssueRequest, Issue, ListIssuesQuery, ListIssuesResponse, MutationResponse,
    SearchIssuesRequest, UpdateIssueRequest,
};
use axum::{
    Router,
    extract::{Json, Path, Query, State},
    response::Json as ResponseJson,
    routing::{get, post},
};
use db::models::workspace::Workspace;
use deployment::Deployment;
use services::services::container::ContainerService;
use utils::response::ApiResponse;
use uuid::Uuid;
use workspace_manager::WorkspaceManager;

use crate::{DeploymentImpl, error::ApiError};

pub(super) fn router() -> Router<DeploymentImpl> {
    Router::new()
        .route("/issues", get(list_issues).post(create_issue))
        .route("/issues/search", post(search_issues))
        .route(
            "/issues/{issue_id}",
            get(get_issue).patch(update_issue).delete(delete_issue),
        )
        .route("/issues/{issue_id}/spec", get(get_issue_spec))
}

async fn list_issues(
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<ListIssuesQuery>,
) -> Result<ResponseJson<ApiResponse<ListIssuesResponse>>, ApiError> {
    let client = deployment.remote_client()?;
    let response = client.list_issues(query.project_id).await?;
    Ok(ResponseJson(ApiResponse::success(response)))
}

async fn search_issues(
    State(deployment): State<DeploymentImpl>,
    Json(request): Json<SearchIssuesRequest>,
) -> Result<ResponseJson<ApiResponse<ListIssuesResponse>>, ApiError> {
    let client = deployment.remote_client()?;
    let response = client.search_issues(&request).await?;
    Ok(ResponseJson(ApiResponse::success(response)))
}

async fn get_issue(
    State(deployment): State<DeploymentImpl>,
    Path(issue_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<Issue>>, ApiError> {
    let client = deployment.remote_client()?;
    let response = client.get_issue(issue_id).await?;
    Ok(ResponseJson(ApiResponse::success(response)))
}

async fn create_issue(
    State(deployment): State<DeploymentImpl>,
    Json(request): Json<CreateIssueRequest>,
) -> Result<ResponseJson<ApiResponse<MutationResponse<Issue>>>, ApiError> {
    let client = deployment.remote_client()?;
    let response = client.create_issue(&request).await?;
    Ok(ResponseJson(ApiResponse::success(response)))
}

async fn update_issue(
    State(deployment): State<DeploymentImpl>,
    Path(issue_id): Path<Uuid>,
    Json(request): Json<UpdateIssueRequest>,
) -> Result<ResponseJson<ApiResponse<MutationResponse<Issue>>>, ApiError> {
    let client = deployment.remote_client()?;

    // Detect the spec approval landing so the artifact can be snapshotted
    // out of the (eventually cleaned-up) worktree.
    let prior_issue = client.get_issue(issue_id).await.ok();
    let was_approved = prior_issue
        .as_ref()
        .map(|issue| spec_approved_at(&issue.extension_metadata).is_some())
        .unwrap_or(false);
    let prior_verdict_at = prior_issue
        .as_ref()
        .and_then(|issue| planning_recorded_at(&issue.extension_metadata));

    let response = client.update_issue(issue_id, &request).await?;

    if !was_approved && spec_approved_at(&response.data.extension_metadata).is_some() {
        // Persist the approved plan while the worktree still exists.
        let deployment = deployment.clone();
        let issue = response.data.clone();
        tokio::spawn(async move {
            if let Err(e) = snapshot_approved_spec(&deployment, &issue).await {
                tracing::warn!(
                    "Failed to snapshot approved spec for issue {}: {e}",
                    issue.id
                );
            }
        });
    } else if planning_recorded_at(&response.data.extension_metadata)
        .is_some_and(|at| prior_verdict_at != Some(at))
    {
        // A planning verdict (re-)landed before approval: file the DRAFT
        // RCA into the shared kb so teammates can read and react to it
        // from any machine while the ticket waits at the gate. Per-issue
        // upsert — the approval-time refiling simply updates the same doc.
        let deployment = deployment.clone();
        let issue = response.data.clone();
        tokio::spawn(async move {
            let metadata = api_types::IssueExtMetadata::from_value(&issue.extension_metadata);
            let Some(spec_path) = metadata.spec.and_then(|spec| spec.path) else {
                return;
            };
            let Some(source) = find_spec_in_worktrees(&deployment, issue.id, &spec_path).await
            else {
                return;
            };
            // Draft filing: no Jira comment. The summary is posted once, at
            // approval, by the caller below.
            if let Err(e) = file_rca_document(&deployment, &issue, &source, false).await {
                tracing::warn!("Failed to file draft RCA for issue {}: {e}", issue.id);
            }
            if let Err(e) = file_epic_spec_document(&deployment, &issue, &source).await {
                tracing::warn!("Failed to file draft epic spec for issue {}: {e}", issue.id);
            }
        });
    }

    Ok(ResponseJson(ApiResponse::success(response)))
}

fn planning_recorded_at(extension_metadata: &serde_json::Value) -> Option<String> {
    api_types::IssueExtMetadata::from_value(extension_metadata)
        .planning
        .and_then(|planning| planning.recorded_at.map(|at| at.to_rfc3339()))
}

fn spec_approved_at(extension_metadata: &serde_json::Value) -> Option<String> {
    api_types::IssueExtMetadata::from_value(extension_metadata)
        .spec
        .and_then(|spec| spec.approved_at.map(|at| at.to_rfc3339()))
}

/// Directory holding snapshots of approved spec/RCA artifacts, keyed by
/// remote issue id. Worktrees are transient (Clean-up archives them), so the
/// plan is copied out at approval time and served from here afterwards.
pub(crate) fn approved_specs_dir() -> std::path::PathBuf {
    utils::assets::approved_specs_dir()
}

/// Resolves the spec artifact inside the issue's newest linked worktree.
/// `spec_path` comes from agent-written metadata, so path traversal and
/// absolute paths are rejected.
pub(crate) async fn find_spec_in_worktrees(
    deployment: &DeploymentImpl,
    issue_id: Uuid,
    spec_path: &str,
) -> Option<std::path::PathBuf> {
    let relative = std::path::Path::new(spec_path);
    let is_safe = relative.is_relative()
        && relative
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)));
    if !is_safe {
        return None;
    }

    let client = deployment.remote_client().ok()?;
    let remote_workspaces = client.list_workspaces_by_issue(issue_id).await.ok()?;
    let pool = &deployment.db().pool;

    for remote_workspace in &remote_workspaces {
        let Some(local_id) = remote_workspace.local_workspace_id else {
            continue;
        };
        let Ok(Some(workspace)) = Workspace::find_by_id(pool, local_id).await else {
            continue;
        };
        let Some(container_ref) = workspace.container_ref else {
            continue;
        };
        let root = std::path::PathBuf::from(&container_ref);
        // spec_path is relative to a repo checkout: try the workspace root
        // and each repo directory inside it.
        let mut candidates = vec![root.join(relative)];
        if let Ok(entries) = std::fs::read_dir(&root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    candidates.push(path.join(relative));
                }
            }
        }
        if let Some(found) = candidates.into_iter().find(|candidate| candidate.is_file()) {
            return Some(found);
        }
    }
    None
}

/// Copies the approved artifact into the persistent snapshot dir.
async fn snapshot_approved_spec(
    deployment: &DeploymentImpl,
    issue: &Issue,
) -> Result<(), anyhow::Error> {
    let metadata = api_types::IssueExtMetadata::from_value(&issue.extension_metadata);
    let Some(spec_path) = metadata.spec.and_then(|spec| spec.path) else {
        return Ok(());
    };
    let Some(source) = find_spec_in_worktrees(deployment, issue.id, &spec_path).await else {
        tracing::warn!(
            "Approved spec for issue {} not found in any worktree ({spec_path})",
            issue.id
        );
        return Ok(());
    };
    let dir = approved_specs_dir();
    tokio::fs::create_dir_all(&dir).await?;
    tokio::fs::copy(&source, dir.join(format!("{}.html", issue.id))).await?;
    tracing::info!("Snapshotted approved spec for issue {}", issue.id);

    // RCAs additionally get filed into the team-shared knowledge store —
    // the raw material for the RCA graph (docs/plans/rca-knowledge-graph-plan.md).
    if let Err(e) = file_rca_document(deployment, issue, &source, true).await {
        tracing::warn!("Failed to file RCA document for issue {}: {e}", issue.id);
    }
    // Epic specs are durable team documents (retention policy: keep epic
    // specs forever; story specs are cleaned with their tickets), so they
    // get a kb copy too — readable from any machine, surviving Clean-up.
    if let Err(e) = file_epic_spec_document(deployment, issue, &source).await {
        tracing::warn!("Failed to file epic spec for issue {}: {e}", issue.id);
    }
    Ok(())
}

/// Files an approved EPIC spec into the remote `kb_documents` store (kind =
/// "spec", idempotent per issue). Stories and subtasks are deliberately not
/// filed: their plans age out with the ticket, per the retention policy.
async fn file_epic_spec_document(
    deployment: &DeploymentImpl,
    issue: &Issue,
    artifact: &std::path::Path,
) -> Result<(), anyhow::Error> {
    let metadata = api_types::IssueExtMetadata::from_value(&issue.extension_metadata);
    if metadata.work_class != Some(api_types::WorkClass::Epic) {
        return Ok(());
    }
    let body = tokio::fs::read_to_string(artifact).await?;
    let client = deployment.remote_client()?;
    client
        .create_kb_document(&api_types::CreateKbDocumentRequest {
            project_id: issue.project_id,
            kind: "spec".to_string(),
            title: issue.title.clone(),
            body,
            issue_id: Some(issue.id),
            jira_key: metadata.jira.as_ref().map(|jira| jira.key.clone()),
            work_class: Some("epic".to_string()),
            summary: metadata.planning.as_ref().and_then(|p| p.summary.clone()),
            tags: Vec::new(),
        })
        .await?;
    tracing::info!("Filed epic spec document for issue {}", issue.id);
    Ok(())
}

/// Files an approved bug RCA into the remote `kb_documents` store (kind =
/// "rca", idempotent per issue). Non-bug specs are not filed — the knowledge
/// graph is about recurring defect classes.
/// Files the RCA into the kb and pins it to its Jira ticket.
///
/// Called twice per ticket by design — once when the planning verdict lands
/// (so teammates can read the draft at the gate) and once at approval. The kb
/// write is a per-issue upsert and the Jira remote link carries a stable
/// globalId, so both are idempotent; the plain-language comment is NOT, so
/// `post_client_summary` must be true only on the approval path. Passing it on
/// both posted two identical comments on every bug (reported on BUG-10007).
pub(crate) async fn file_rca_document(
    deployment: &DeploymentImpl,
    issue: &Issue,
    artifact: &std::path::Path,
    post_client_summary: bool,
) -> Result<(), anyhow::Error> {
    let metadata = api_types::IssueExtMetadata::from_value(&issue.extension_metadata);
    if metadata.work_class != Some(api_types::WorkClass::Bug) {
        return Ok(());
    }
    let body = tokio::fs::read_to_string(artifact).await?;
    let client = deployment.remote_client()?;
    let board = deployment
        .config()
        .read()
        .await
        .project_settings(issue.project_id)
        .label;
    let document = client
        .create_kb_document(&api_types::CreateKbDocumentRequest {
            project_id: issue.project_id,
            kind: "rca".to_string(),
            title: issue.title.clone(),
            body,
            issue_id: Some(issue.id),
            jira_key: metadata.jira.as_ref().map(|jira| jira.key.clone()),
            work_class: Some("bug".to_string()),
            summary: metadata.planning.as_ref().and_then(|p| p.summary.clone()),
            // Filing-time tags only land on first insert (the upsert's
            // refresh path deliberately preserves tags), so a re-plan can't
            // duplicate or fight the analysis agent's additions.
            tags: auto_rca_tags(issue, &metadata, board.as_deref()),
        })
        .await?;
    tracing::info!("Filed RCA document for issue {}", issue.id);

    // Pin the RCA to its Jira ticket as a remote link (best effort; the
    // globalId makes re-filing idempotent). See also /knowledge/link-jira
    // for backfilling documents filed before this hook existed.
    // A board-only card, or a setup with no tracker, has no ticket to pin it
    // to: the document and the card are the record.
    let tracker = deployment.tracker().current().await;
    if let Some(jira_ref) = metadata.jira.as_ref().filter(|_| tracker.is_configured()) {
        let url = document_url(document.id);
        if let Err(e) = pin_document_to_jira(tracker.as_ref(), &jira_ref.key, document.id).await {
            tracing::warn!("Linking RCA to Jira {} failed: {e}", jira_ref.key);
        }

        // Alongside the link, the plain-language version for whoever is
        // fielding customer questions. The RCA itself is written for
        // engineers; account managers need three sentences they can relay.
        // Prefixed so the mention/triage sweeps recognise it as the agent's
        // own comment rather than a human asking for something.
        if post_client_summary
            && let Some(client_summary) = metadata
                .planning
                .as_ref()
                .and_then(|planning| planning.client_summary.as_deref())
                .map(str::trim)
                .filter(|summary| !summary.is_empty())
        {
            // Idempotent by observation rather than by stored state: a
            // re-plan clears the approval, so approving twice reaches this
            // path twice. Only the agent writes AM_SUMMARY_PREFIX, so its
            // presence in the thread means the summary is already posted.
            let already_posted = match tracker.comments(&jira_ref.key).await {
                Ok(comments) => comments.iter().any(|comment| {
                    comment
                        .body_text
                        .trim_start()
                        .starts_with(AM_SUMMARY_PREFIX)
                }),
                // Unreadable thread: skip rather than risk a duplicate.
                Err(e) => {
                    tracing::warn!(
                        "Could not read Jira comments for {} ({e}); skipping the summary",
                        jira_ref.key
                    );
                    true
                }
            };
            if !already_posted {
                let body =
                    format!("{AM_SUMMARY_PREFIX}\n\n{client_summary}\n\nFull technical RCA: {url}");
                if let Err(e) = tracker.post_comment(&jira_ref.key, &body).await {
                    tracing::warn!(
                        "Posting the plain-language RCA summary to Jira {} failed: {e}",
                        jira_ref.key
                    );
                }
            }
        }
    }
    Ok(())
}

/// Where a filed document is read: the board's HTML view of it.
pub(crate) fn document_url(document_id: Uuid) -> String {
    format!(
        "{}/api/knowledge/documents/{}/html",
        services::services::stage_orchestrator::public_ui_base(),
        document_id
    )
}

/// Pins a filed document to its Jira ticket as a remote link. The link's
/// globalId is derived from the document, so pinning the same document
/// again replaces the link instead of adding a second one. Shared by the
/// board's own RCA filing and by documents filed from outside it
/// (`POST /kb_documents/file`), so a ticket carries the same link either way.
pub(crate) async fn pin_document_to_jira(
    tracker: &dyn services::services::tracker::Tracker,
    jira_key: &str,
    document_id: Uuid,
) -> Result<(), services::services::tracker::TrackerError> {
    tracker
        .set_remote_link(
            jira_key,
            services::services::tracker::RemoteLink {
                id: &format!("prepcook-rca-{document_id}"),
                title: &format!("RCA — Prep Cook ({jira_key})"),
                url: &document_url(document_id),
            },
        )
        .await
}

/// First line of the agent's plain-language RCA comment. Stable so the
/// comment is recognisable — both to people skimming the thread and to the
/// sweeps that must not treat the agent's own words as a new question.
pub(crate) use services::services::stage_orchestrator::AM_SUMMARY_PREFIX;

/// Mechanical tags derivable at filing time — no agent judgment involved.
/// The analysis agent layers the judgment set (area/fix-type/surface/...)
/// on top later; see the rca_analysis playbook.
pub(crate) fn auto_rca_tags(
    issue: &api_types::Issue,
    metadata: &api_types::IssueExtMetadata,
    board: Option<&str>,
) -> Vec<String> {
    let mut tags = Vec::new();
    // Documents are already partitioned by project, so this is for exports
    // and cross-board queries: one CSV can be split per repo.
    if let Some(board) = board.map(tag_slug).filter(|board| !board.is_empty()) {
        tags.push(format!("board:{board}"));
    }
    if let Some(priority) = issue.priority {
        tags.push(format!(
            "severity:{}",
            match priority {
                api_types::IssuePriority::Urgent => "p0",
                api_types::IssuePriority::High => "p1",
                api_types::IssuePriority::Medium => "p2",
                api_types::IssuePriority::Low => "p3",
            }
        ));
    }
    if let Some(confidence) = metadata.planning.as_ref().and_then(|p| p.confidence) {
        tags.push(format!(
            "confidence:{}",
            match confidence {
                api_types::ConfidenceLevel::High => "high",
                api_types::ConfidenceLevel::Medium => "medium",
                api_types::ConfidenceLevel::Low => "low",
            }
        ));
    }
    if let Some(customer) = customer_from_title(&issue.title) {
        tags.push(format!("customer:{customer}"));
    }
    tags
}

/// Lowercases and dash-separates a free-text label so it reads like the rest
/// of the tag taxonomy (`board:chrome-extension`).
fn tag_slug(value: &str) -> String {
    let slug: String = value
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    slug.trim_matches('-').to_string()
}

/// Extracts the reporter-customer slug from the importer's title shape:
/// `BUG-1234: (Salesloft) Insights DC throwing ...` → `salesloft`. Only a
/// parenthesized token near the start counts — parentheses later in the
/// title are prose, not attribution.
pub(crate) fn customer_from_title(title: &str) -> Option<String> {
    let after_key = title
        .split_once(": ")
        .map(|(_, rest)| rest)
        .unwrap_or(title)
        .trim_start();
    let rest = after_key.strip_prefix('(')?;
    let inner = &rest[..rest.find(')')?];
    let slug: String = inner
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let slug = slug.trim_matches('-').to_string();
    // Reject prose-looking captures: multiword sentences, severity markers,
    // empty leftovers.
    if slug.is_empty() || slug.len() > 24 || slug.matches('-').count() > 2 {
        return None;
    }
    if matches!(
        slug.as_str(),
        "blocker" | "urgent" | "p0" | "p1" | "internal"
    ) {
        return None;
    }
    Some(slug)
}

/// Serves the approved plan/spec for an issue: the persistent snapshot when
/// present, otherwise straight from the newest linked worktree.
async fn get_issue_spec(
    State(deployment): State<DeploymentImpl>,
    Path(issue_id): Path<Uuid>,
) -> Result<axum::response::Response, ApiError> {
    use axum::response::IntoResponse;

    let snapshot = approved_specs_dir().join(format!("{issue_id}.html"));
    let mut content = if let Ok(content) = tokio::fs::read_to_string(&snapshot).await {
        Some(content)
    } else {
        let client = deployment.remote_client()?;
        let issue = client.get_issue(issue_id).await?;
        let metadata = api_types::IssueExtMetadata::from_value(&issue.extension_metadata);
        match metadata.spec.and_then(|spec| spec.path) {
            Some(spec_path) => {
                match find_spec_in_worktrees(&deployment, issue_id, &spec_path).await {
                    Some(path) => tokio::fs::read_to_string(&path).await.ok(),
                    None => None,
                }
            }
            None => None,
        }
    };

    // Neither a local snapshot nor a local worktree: the artifact may live
    // on another machine (e.g. the agent runner). RCAs and epic specs are
    // filed to the shared kb at verdict time, so serve that copy — any kind
    // linked to this issue counts.
    if content.is_none()
        && let Ok(client) = deployment.remote_client()
        && let Ok(issue) = client.get_issue(issue_id).await
        && let Ok(list) = client
            .list_kb_documents(issue.project_id, None, false)
            .await
        && let Some(doc) = list
            .documents
            .iter()
            .find(|doc| doc.issue_id == Some(issue_id))
        && let Ok(full) = client.get_kb_document(doc.id).await
    {
        content = Some(full.body);
    }

    match content {
        Some(html) => Ok(axum::response::Html(with_plan_comments_widget(html, issue_id))
            .into_response()),
        None => Ok((
            axum::http::StatusCode::NOT_FOUND,
            "No plan artifact found for this issue (the worktree may have been cleaned up before approval).",
        )
            .into_response()),
    }
}

/// Appends a self-contained comment panel to the served plan artifact so
/// teammates can discuss it right on the page — no VK login needed (the
/// page and the comment proxy share the local server's trust model, and
/// the poster's name travels inside the message text). Comments marked
/// "ask the agent" post with the `PLAN:` prefix, so the orchestrator's
/// plan gate delivers them to the planning agent as a follow-up.
///
/// Comments can be ANCHORED: selecting text in the artifact offers a
/// floating "Comment" button, and the selected excerpt is embedded in the
/// message as `[re: "<quote>"]`. On load the widget locates each quote
/// (whitespace-tolerant search), highlights it via the CSS Custom
/// Highlight API (no DOM mutation, so cross-element quotes work), and
/// clicking a highlight or its list entry jumps between the two. Quotes
/// that no longer match a revised document degrade to plain list comments.
///
/// Styling is theme-adaptive: surfaces and borders are `color-mix`
/// blends of the artifact's own `currentColor`/background, the accent is
/// read from a document link, and fonts are inherited — so the panel
/// matches light or dark specs without hardcoding. Its width and left
/// edge are measured from the artifact's widest content block at runtime
/// so the panel lines up with the spec's text column.
fn with_plan_comments_widget(html: String, issue_id: Uuid) -> String {
    let widget = format!(
        r##"
<!-- prepcook plan comments (injected by the server; not part of the artifact) -->
<style>
  #vk-plan-comments {{
    /* Theme-derived tokens: everything blends the artifact's own text
       color and background, so the panel adapts to light/dark specs. */
    --vk-fg: currentColor;
    --vk-surface: color-mix(in srgb, currentColor 4%, transparent);
    --vk-surface-raised: color-mix(in srgb, currentColor 7%, transparent);
    --vk-border: color-mix(in srgb, currentColor 18%, transparent);
    --vk-muted: color-mix(in srgb, currentColor 60%, transparent);
    --vk-accent: var(--vk-page-accent, color-mix(in srgb, currentColor 55%, transparent));
    --vk-accent-fg: var(--vk-page-accent-fg, #fff);
    --vk-quote-edge: color-mix(in srgb, var(--vk-accent) 70%, currentColor);
    --vk-quote-bg: color-mix(in srgb, var(--vk-accent) 12%, transparent);
    box-sizing: border-box;
    margin: 3rem auto 2rem;
    padding: 1rem 1.25rem;
    border: 1px solid var(--vk-border);
    border-radius: 8px;
    font: inherit;
    color: var(--vk-fg);
    background: var(--vk-surface);
  }}
  #vk-plan-comments * {{ box-sizing: border-box; }}
  #vk-plan-comments h3 {{ margin: 0 0 .25rem; font-size: 1.05em; }}
  #vk-plan-comments .vk-hint {{ margin: 0 0 .75rem; font-size: .85em; color: var(--vk-muted); }}
  #vk-plan-comments .vk-list {{ display: flex; flex-direction: column; gap: .5rem; margin-bottom: 1rem; }}
  #vk-plan-comments .vk-comment {{ padding: .5rem .65rem; border: 1px solid var(--vk-border); border-radius: 6px; background: var(--vk-surface-raised); white-space: pre-wrap; word-break: break-word; }}
  #vk-plan-comments .vk-comment-meta {{ font-size: .82em; color: var(--vk-muted); margin-bottom: .25rem; }}
  #vk-plan-comments .vk-comment-quote,
  #vk-plan-comments #vk-quote-chip {{ border-left: 3px solid var(--vk-quote-edge); background: var(--vk-quote-bg); border-radius: 4px; padding: .3rem .55rem; font-size: .9em; font-style: italic; color: var(--vk-fg); }}
  #vk-plan-comments .vk-form {{ display: flex; flex-direction: column; gap: .5rem; }}
  #vk-plan-comments #vk-quote-chip {{ display: none; align-items: flex-start; gap: .5rem; }}
  #vk-plan-comments input,
  #vk-plan-comments textarea {{ padding: .45rem .6rem; border: 1px solid var(--vk-border); border-radius: 6px; font: inherit; font-size: .95em; color: var(--vk-fg); background: var(--vk-surface-raised); }}
  #vk-plan-comments input {{ max-width: 260px; }}
  #vk-plan-comments textarea {{ resize: vertical; }}
  #vk-plan-comments .vk-check {{ display: flex; align-items: center; gap: .45rem; font-size: .9em; color: var(--vk-muted); }}
  #vk-plan-comments .vk-actions {{ display: flex; align-items: center; gap: .75rem; }}
  #vk-plan-comments #vk-post {{ padding: .45rem 1rem; border: none; border-radius: 6px; background: var(--vk-accent); color: var(--vk-accent-fg); font: inherit; font-size: .95em; cursor: pointer; }}
  #vk-plan-comments #vk-post:hover {{ filter: brightness(1.08); }}
  #vk-plan-comments #vk-status {{ font-size: .88em; color: var(--vk-muted); }}
  #vk-plan-comments .vk-quote-clear {{ border: none; background: none; cursor: pointer; color: var(--vk-muted); font-size: 1em; line-height: 1; }}
  #vk-annot-btn {{ display: none; position: absolute; z-index: 9999; padding: .3rem .7rem; border: none; border-radius: 6px; background: var(--vk-annot-accent, #3b5bdb); color: #fff; font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; font-size: 13px; cursor: pointer; box-shadow: 0 2px 8px rgba(0,0,0,.25); }}
  ::highlight(vk-annot) {{ background-color: var(--vk-hl, rgba(255, 212, 0, 0.35)); }}
  ::highlight(vk-annot-active) {{ background-color: var(--vk-hl-active, rgba(255, 150, 0, 0.6)); }}
</style>
<div id="vk-plan-comments">
  <h3>Plan discussion</h3>
  <p class="vk-hint">Tip: select any text in the plan above to attach a comment to it.</p>
  <div id="vk-comments-list" class="vk-list"></div>
  <div class="vk-form">
    <div id="vk-quote-chip">
      <span id="vk-quote-text" style="flex:1"></span>
      <button id="vk-quote-clear" class="vk-quote-clear" aria-label="Remove quote">×</button>
    </div>
    <input id="vk-name" placeholder="Your name" />
    <textarea id="vk-message" rows="3" placeholder="Comment on this plan…"></textarea>
    <label class="vk-check">
      <input type="checkbox" id="vk-to-agent" style="max-width:none" />
      Ask the agent to address this (delivers the comment to the planning agent, which updates the plan)
    </label>
    <div class="vk-actions">
      <button id="vk-post">Post</button>
      <span id="vk-status"></span>
    </div>
  </div>
</div>
<button id="vk-annot-btn">💬 Comment</button>
<script>
(function () {{
  var ISSUE_ID = "{issue_id}";
  var panel = document.getElementById("vk-plan-comments");
  var list = document.getElementById("vk-comments-list");
  var nameInput = document.getElementById("vk-name");
  var messageInput = document.getElementById("vk-message");
  var toAgent = document.getElementById("vk-to-agent");
  var status = document.getElementById("vk-status");
  var annotBtn = document.getElementById("vk-annot-btn");
  var quoteChip = document.getElementById("vk-quote-chip");
  var quoteText = document.getElementById("vk-quote-text");
  var pendingQuote = null;
  var annots = [];
  try {{ nameInput.value = localStorage.getItem("vk-plan-comment-name") || ""; }} catch (e) {{}}

  // ── Theme + layout matching ──────────────────────────────────────────
  // Pick an accent from a real document link (falls back to a neutral
  // blend), and align the panel to the artifact's widest content column.
  function styleFromPage() {{
    try {{
      var link = document.querySelector("a[href]");
      if (link) {{
        var lc = getComputedStyle(link).color;
        if (lc) {{
          panel.style.setProperty("--vk-page-accent", lc);
          annotBtn.style.setProperty("--vk-annot-accent", lc);
        }}
      }}
      // Match width/offset to the largest block-level element that holds
      // the spec body (main/article, else the widest direct child of body).
      var candidate =
        document.querySelector("main, article, .container, #content, [role=main]");
      if (!candidate) {{
        var widest = null;
        var kids = document.body.children;
        for (var i = 0; i < kids.length; i++) {{
          var el = kids[i];
          if (el === panel || el === annotBtn || el.tagName === "SCRIPT" || el.tagName === "STYLE") continue;
          var w = el.getBoundingClientRect().width;
          if (w > 200 && (!widest || w > widest.getBoundingClientRect().width)) widest = el;
        }}
        candidate = widest;
      }}
      if (candidate) {{
        var rect = candidate.getBoundingClientRect();
        var cs = getComputedStyle(candidate);
        var innerLeft = rect.left + window.scrollX + parseFloat(cs.paddingLeft || "0");
        var innerWidth = rect.width - parseFloat(cs.paddingLeft || "0") - parseFloat(cs.paddingRight || "0");
        if (innerWidth > 240) {{
          panel.style.margin = "3rem 0 2rem";
          panel.style.width = innerWidth + "px";
          panel.style.marginLeft = innerLeft + "px";
        }}
      }}
    }} catch (e) {{}}
  }}

  function escapeRegex(s) {{ return s.replace(/[.*+?^${{}}()|[\]\\]/g, "\\$&"); }}

  // ── Document text index (artifact only; our UI excluded) ─────────────
  function buildIndex() {{
    var parts = [];
    var nodes = [];
    var offset = 0;
    var walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT, {{
      acceptNode: function (node) {{
        var el = node.parentElement;
        while (el) {{
          if (el.id === "vk-plan-comments" || el.id === "vk-annot-btn") return NodeFilter.FILTER_REJECT;
          var tag = el.tagName;
          if (tag === "SCRIPT" || tag === "STYLE" || tag === "NOSCRIPT") return NodeFilter.FILTER_REJECT;
          el = el.parentElement;
        }}
        return NodeFilter.FILTER_ACCEPT;
      }}
    }});
    var n;
    while ((n = walker.nextNode())) {{
      nodes.push({{ node: n, start: offset }});
      parts.push(n.nodeValue);
      offset += n.nodeValue.length;
    }}
    return {{ text: parts.join(""), nodes: nodes }};
  }}

  function rangeFromOffsets(index, start, end) {{
    var range = document.createRange();
    var startSet = false;
    for (var i = 0; i < index.nodes.length; i++) {{
      var entry = index.nodes[i];
      var nodeEnd = entry.start + entry.node.nodeValue.length;
      if (!startSet && start >= entry.start && start < nodeEnd) {{
        range.setStart(entry.node, start - entry.start);
        startSet = true;
      }}
      if (startSet && end > entry.start && end <= nodeEnd) {{
        range.setEnd(entry.node, end - entry.start);
        return range;
      }}
    }}
    return null;
  }}

  // Whitespace-tolerant quote search: the artifact may reflow across
  // revisions but the words usually survive.
  function findQuoteRange(index, quote) {{
    var pattern = escapeRegex(quote).replace(/\s+/g, "\\s+");
    var match = index.text.match(new RegExp(pattern));
    if (!match) return null;
    return rangeFromOffsets(index, match.index, match.index + match[0].length);
  }}

  function parseAnnot(message) {{
    var m = message.match(/\[re: "([^"]+)"\]\s*/);
    if (!m) return {{ quote: null, body: message }};
    return {{ quote: m[1], body: message.replace(m[0], " ").replace(/\s{{2,}}/g, " ") }};
  }}

  function applyHighlights() {{
    if (typeof Highlight === "undefined" || !window.CSS || !CSS.highlights) return;
    var ranges = annots.map(function (a) {{ return a.range; }});
    CSS.highlights.set("vk-annot", new (window.Highlight)(...ranges));
  }}

  function flashRange(range) {{
    if (typeof Highlight === "undefined" || !window.CSS || !CSS.highlights) return;
    CSS.highlights.set("vk-annot-active", new (window.Highlight)(range));
    setTimeout(function () {{ CSS.highlights.delete("vk-annot-active"); }}, 2000);
  }}

  function scrollToRange(range) {{
    var el = range.startContainer.parentElement;
    if (el) el.scrollIntoView({{ behavior: "smooth", block: "center" }});
    flashRange(range);
  }}

  // ── Comment list ──────────────────────────────────────────────────────
  function render(comments) {{
    list.innerHTML = "";
    annots = [];
    if (!comments.length) {{
      var empty = document.createElement("div");
      empty.className = "vk-hint";
      empty.textContent = "No comments yet.";
      list.appendChild(empty);
      applyHighlights();
      return;
    }}
    var index = buildIndex();
    comments
      .slice()
      .sort(function (a, b) {{ return a.created_at < b.created_at ? -1 : 1; }})
      .forEach(function (c) {{
        var parsed = parseAnnot(c.message);
        var item = document.createElement("div");
        item.className = "vk-comment";
        var meta = document.createElement("div");
        meta.className = "vk-comment-meta";
        meta.textContent = new Date(c.created_at).toLocaleString();
        item.appendChild(meta);
        if (parsed.quote) {{
          var q = document.createElement("div");
          q.className = "vk-comment-quote";
          q.style.marginBottom = ".3rem";
          q.textContent = parsed.quote;
          item.appendChild(q);
          var range = findQuoteRange(index, parsed.quote);
          if (range) {{
            annots.push({{ range: range, item: item }});
            item.style.cursor = "pointer";
            item.title = "Jump to the highlighted passage";
            item.addEventListener("click", function () {{ scrollToRange(range); }});
          }}
        }}
        var body = document.createElement("div");
        body.textContent = parsed.body;
        item.appendChild(body);
        list.appendChild(item);
      }});
    applyHighlights();
  }}

  function load() {{
    fetch("/api/remote/issue_comments?issue_id=" + ISSUE_ID)
      .then(function (r) {{ return r.json(); }})
      .then(function (r) {{ render((r.data && r.data.issue_comments) || []); }})
      .catch(function () {{
        var err = document.createElement("div");
        err.className = "vk-hint";
        err.textContent = "Could not load comments.";
        list.innerHTML = "";
        list.appendChild(err);
      }});
  }}

  // Clicking a highlighted passage opens its comment in the list.
  document.addEventListener("click", function (ev) {{
    if (!annots.length || panel.contains(ev.target)) return;
    var pos = null;
    if (document.caretRangeFromPoint) {{
      pos = document.caretRangeFromPoint(ev.clientX, ev.clientY);
    }} else if (document.caretPositionFromPoint) {{
      var cp = document.caretPositionFromPoint(ev.clientX, ev.clientY);
      if (cp) {{
        pos = document.createRange();
        pos.setStart(cp.offsetNode, cp.offset);
        pos.setEnd(cp.offsetNode, cp.offset);
      }}
    }}
    if (!pos) return;
    for (var i = 0; i < annots.length; i++) {{
      var r = annots[i].range;
      if (r.comparePoint && r.comparePoint(pos.startContainer, pos.startOffset) === 0) {{
        annots[i].item.scrollIntoView({{ behavior: "smooth", block: "center" }});
        annots[i].item.style.borderColor = "var(--vk-quote-edge)";
        (function (item) {{
          setTimeout(function () {{ item.style.borderColor = "var(--vk-border)"; }}, 2000);
        }})(annots[i].item);
        return;
      }}
    }}
  }});

  // ── Select-to-annotate ────────────────────────────────────────────────
  function currentSelectionQuote() {{
    var sel = window.getSelection();
    if (!sel || sel.isCollapsed || sel.rangeCount === 0) return null;
    var range = sel.getRangeAt(0);
    if (panel.contains(range.commonAncestorContainer)) return null;
    var text = sel.toString().replace(/\s+/g, " ").trim().replace(/"/g, "'");
    if (text.length < 3) return null;
    if (text.length > 200) text = text.slice(0, 200);
    return {{ text: text, rect: range.getBoundingClientRect() }};
  }}

  document.addEventListener("mouseup", function () {{
    setTimeout(function () {{
      var q = currentSelectionQuote();
      if (!q) {{ annotBtn.style.display = "none"; return; }}
      annotBtn.style.display = "block";
      annotBtn.style.top = window.scrollY + q.rect.top - 38 + "px";
      annotBtn.style.left = window.scrollX + q.rect.left + Math.max(0, q.rect.width / 2 - 45) + "px";
      annotBtn.dataset.quote = q.text;
    }}, 0);
  }});

  annotBtn.addEventListener("mousedown", function (ev) {{
    ev.preventDefault();
    pendingQuote = annotBtn.dataset.quote || null;
    annotBtn.style.display = "none";
    if (pendingQuote) {{
      quoteText.textContent = pendingQuote;
      quoteChip.style.display = "flex";
      panel.scrollIntoView({{ behavior: "smooth", block: "center" }});
      messageInput.focus();
    }}
  }});

  document.getElementById("vk-quote-clear").addEventListener("click", function () {{
    pendingQuote = null;
    quoteChip.style.display = "none";
  }});

  // ── Posting ───────────────────────────────────────────────────────────
  document.getElementById("vk-post").addEventListener("click", function () {{
    var name = nameInput.value.trim();
    var message = messageInput.value.trim();
    if (!name || !message) {{
      status.textContent = "Name and comment are both required.";
      return;
    }}
    try {{ localStorage.setItem("vk-plan-comment-name", name); }} catch (e) {{}}
    var anchor = pendingQuote ? '[re: "' + pendingQuote + '"] ' : "";
    // The agent-bound format keeps the poster's name INSIDE the first
    // line, so a page comment can never parse as the assignee-only
    // "PLAN: APPROVE" command — page visitors are unauthenticated.
    var text = toAgent.checked
      ? "PLAN: [from " + name + " via plan page] " + anchor + message
      : "💬 Plan feedback from " + name + " " + anchor + ": " + message;
    status.textContent = "Posting…";
    fetch("/api/remote/issue_comments", {{
      method: "POST",
      headers: {{ "Content-Type": "application/json" }},
      body: JSON.stringify({{ id: null, issue_id: ISSUE_ID, message: text, parent_id: null }})
    }})
      .then(function (r) {{
        if (!r.ok) throw new Error("HTTP " + r.status);
        messageInput.value = "";
        pendingQuote = null;
        quoteChip.style.display = "none";
        status.textContent = toAgent.checked
          ? "Posted — the agent will pick this up within a minute and reply on the ticket."
          : "Posted.";
        toAgent.checked = false;
        load();
      }})
      .catch(function () {{ status.textContent = "Failed to post — is the app still running?"; }});
  }});

  styleFromPage();
  window.addEventListener("resize", styleFromPage);
  load();
}})();
</script>
"##
    );

    // Keep the document well-formed when the artifact has a closing body
    // tag; plain fragments just get the panel appended.
    if let Some(index) = html.rfind("</body>") {
        let mut out = html.clone();
        out.insert_str(index, &widget);
        out
    } else {
        html + &widget
    }
}

async fn delete_issue(
    State(deployment): State<DeploymentImpl>,
    Path(issue_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<()>>, ApiError> {
    let client = deployment.remote_client()?;

    // Snapshot linked workspaces BEFORE deletion — the remote rows (and the
    // local_workspace_id links) go away with the issue.
    let linked_workspaces = client
        .list_workspaces_by_issue(issue_id)
        .await
        .unwrap_or_default();

    client.delete_issue(issue_id).await?;

    // Deleting the controlling issue orphans its agent workspaces: tear them
    // down locally too (stop processes, remove worktrees, drop records).
    // Best-effort and off the request path.
    let local_ids: Vec<Uuid> = linked_workspaces
        .iter()
        .filter_map(|workspace| workspace.local_workspace_id)
        .collect();
    if !local_ids.is_empty() {
        let deployment = deployment.clone();
        tokio::spawn(async move {
            for workspace_id in local_ids {
                if let Err(e) = delete_local_workspace(&deployment, workspace_id).await {
                    tracing::warn!(
                        "Failed to delete workspace {workspace_id} for deleted issue {issue_id}: {e}"
                    );
                }
            }
        });
    }

    Ok(ResponseJson(ApiResponse::success(())))
}

/// Stops and deletes one local workspace (same steps as the workspace DELETE
/// route, minus the running-process guard: the controlling issue is gone, so
/// in-flight agents are stopped rather than blocking the cleanup). Branches
/// are kept — committed work stays salvageable from git.
async fn delete_local_workspace(
    deployment: &DeploymentImpl,
    workspace_id: Uuid,
) -> Result<(), anyhow::Error> {
    let pool = &deployment.db().pool;
    let Some(workspace) = Workspace::find_by_id(pool, workspace_id).await? else {
        return Ok(());
    };

    deployment.container().try_stop(&workspace, true).await;

    let managed_workspace = deployment
        .workspace_manager()
        .load_managed_workspace(workspace)
        .await?;
    let deletion_context = managed_workspace.prepare_deletion_context().await?;
    managed_workspace.delete_record().await?;
    WorkspaceManager::spawn_workspace_deletion_cleanup(deletion_context, false);

    tracing::info!("Deleted workspace {workspace_id} after its issue was deleted");
    Ok(())
}

// Kept at the end of the file: clippy::items_after_test_module rejects any
// item declared below a #[cfg(test)] module.
#[cfg(test)]
mod rca_tag_tests {
    use super::{customer_from_title, tag_slug};

    #[test]
    fn extracts_leading_customer_parens() {
        assert_eq!(
            customer_from_title("BUG-9796: (Salesloft) Insights DC throwing"),
            Some("salesloft".to_string())
        );
        assert_eq!(
            customer_from_title("(Noom) Intermittent errors"),
            Some("noom".to_string())
        );
    }

    #[test]
    fn ignores_non_customer_parens() {
        assert_eq!(customer_from_title("BUG-1: fix the thing (again)"), None);
        assert_eq!(customer_from_title("BUG-2: (Blocker) preview 404"), None);
    }

    #[test]
    fn board_labels_become_tag_slugs() {
        assert_eq!(tag_slug("extensions"), "extensions");
        assert_eq!(tag_slug(" Chrome Extension "), "chrome-extension");
        assert_eq!(tag_slug("target-app"), "target-app");
        assert_eq!(tag_slug("---"), "");
    }
}
