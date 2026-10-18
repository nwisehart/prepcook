//! `POST /api/evidence/search`: searches a board's evidence sources for a
//! pattern over a time window and merges what they found (see
//! `docs/evidence.md`). Agents reach it through the `evidence_search` MCP
//! tool.
//!
//! Authentication is the approvals endpoint's (`plan_decisions`): a bearer
//! token matching the `PREPCOOK_API_TOKEN` secret, or the board owner's own
//! session, which is what agent sessions on this machine are.

use std::path::{Path, PathBuf};

use axum::{
    Router,
    extract::{Json, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
};
use chrono::{DateTime, Utc};
use db::models::{repo::Repo, workspace::Workspace};
use deployment::Deployment;
use serde::Deserialize;
use services::services::{
    config::EvidenceSourceConfig,
    evidence::{self, EvidenceError, MergedSearch, SearchQuery},
    scripts::load_manifest,
};
use utils::response::ApiResponse;
use uuid::Uuid;

use super::plan_decisions::{check_caller, failure};
use crate::DeploymentImpl;

#[derive(Debug, Deserialize)]
pub struct EvidenceSearchRequest {
    /// The board whose sources are searched; unset is the default board.
    #[serde(default)]
    pub project_id: Option<Uuid>,
    /// The agent's worktree, where relative log file paths resolve and whose
    /// manifest's `[[profile.evidence]]` is read. Ignored unless it is inside
    /// a workspace or a repository Prep Cook knows.
    #[serde(default)]
    pub worktree: Option<String>,
    /// Text to find, as a literal phrase.
    pub pattern: String,
    /// Window start; unset is 24 hours before `to`.
    #[serde(default)]
    pub from: Option<DateTime<Utc>>,
    /// Window end; unset is now.
    #[serde(default)]
    pub to: Option<DateTime<Utc>>,
    /// One source, by label or kind; unset searches every source.
    #[serde(default)]
    pub source: Option<String>,
    /// Most hits returned, 1 to 200; unset is 50.
    #[serde(default)]
    pub limit: Option<usize>,
    /// A `[profile] areas` entry; sources that name other areas are skipped.
    #[serde(default)]
    pub area: Option<String>,
}

/// The window a request asks for, checked: `to` defaults to `now`, `from` to
/// a day before `to`, and `from` may not be after `to`.
pub fn window(
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> Result<(DateTime<Utc>, DateTime<Utc>), String> {
    let to = to.unwrap_or(now);
    let from = from.unwrap_or(to - evidence::DEFAULT_WINDOW);
    if from > to {
        return Err(format!("the window starts ({from}) after it ends ({to})"));
    }
    Ok((from, to))
}

/// The limit a request asks for, within 1..=[`evidence::MAX_LIMIT`].
pub fn limit(requested: Option<usize>) -> usize {
    requested
        .unwrap_or(evidence::DEFAULT_LIMIT)
        .clamp(1, evidence::MAX_LIMIT)
}

/// The HTTP status a search error answers with.
fn status_for(error: &EvidenceError) -> StatusCode {
    match error {
        EvidenceError::Invalid(_) => StatusCode::BAD_REQUEST,
        EvidenceError::NotConfigured(_) => StatusCode::NOT_FOUND,
        _ => StatusCode::BAD_GATEWAY,
    }
}

/// The board's first repository that still exists.
async fn board_repo(deployment: &DeploymentImpl, repo_ids: &[Uuid]) -> Option<Repo> {
    for id in repo_ids {
        if let Ok(Some(repo)) = Repo::find_by_id(&deployment.db().pool, *id).await {
            return Some(repo);
        }
    }
    None
}

/// `worktree` when it is inside a workspace or a known repository, so a
/// caller cannot point relative log paths anywhere on the machine.
async fn known_worktree(
    deployment: &DeploymentImpl,
    worktree: &str,
    repo: Option<&Repo>,
) -> Option<PathBuf> {
    let path = PathBuf::from(worktree.trim());
    if !path.is_absolute() || !path.is_dir() {
        return None;
    }
    if repo.is_some_and(|repo| path.starts_with(&repo.path)) {
        return Some(path);
    }
    let workspaces = Workspace::fetch_all(&deployment.db().pool).await.ok()?;
    workspaces
        .iter()
        .any(|workspace| {
            workspace
                .container_ref
                .as_deref()
                .is_some_and(|dir| !dir.is_empty() && path.starts_with(dir))
        })
        .then_some(path)
}

/// The `[[profile.evidence]]` of the manifest in `dir`, if it has one.
fn own_sources(dir: &Path) -> Option<Vec<EvidenceSourceConfig>> {
    Some(load_manifest(dir).ok()?.profile?.evidence)
}

async fn search(
    State(deployment): State<DeploymentImpl>,
    headers: HeaderMap,
    body: Result<Json<EvidenceSearchRequest>, axum::extract::rejection::JsonRejection>,
) -> Response {
    if let Err(response) = check_caller(&deployment, &headers).await {
        return response;
    }
    let Json(request) = match body {
        Ok(body) => body,
        Err(rejection) => return failure(StatusCode::BAD_REQUEST, &rejection.body_text()),
    };
    if request.pattern.trim().is_empty() {
        return failure(StatusCode::BAD_REQUEST, "pattern is empty");
    }
    let window = match window(request.from, request.to, Utc::now()) {
        Ok(window) => window,
        Err(message) => return failure(StatusCode::BAD_REQUEST, &message),
    };
    let (board, repo_ids) = {
        let config = deployment.config().read().await;
        let board = request.project_id.or_else(|| config.default_board());
        let repo_ids = board
            .map(|board| config.project_settings(board).repo_ids)
            .unwrap_or_default();
        (board, repo_ids)
    };
    let repo = board_repo(&deployment, &repo_ids).await;
    let worktree = match request.worktree.as_deref() {
        Some(worktree) => known_worktree(&deployment, worktree, repo.as_ref()).await,
        None => None,
    };
    let repo_root = repo.as_ref().map(|repo| repo.path.clone());
    let own = worktree
        .as_deref()
        .and_then(own_sources)
        .or_else(|| repo_root.as_deref().and_then(own_sources))
        .unwrap_or_default();
    let sources = deployment.evidence().current(board, &own, repo_root).await;
    let query = SearchQuery {
        pattern: request.pattern.trim().to_string(),
        window,
        limit: limit(request.limit),
        area: request.area.filter(|area| !area.trim().is_empty()),
        worktree,
    };
    match sources.search(&query, request.source.as_deref()).await {
        Ok(found) => Json(ApiResponse::<MergedSearch>::success(found)).into_response(),
        Err(error) => failure(status_for(&error), &error.to_string()),
    }
}

pub fn router() -> Router<DeploymentImpl> {
    Router::new().route("/evidence/search", post(search))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(raw: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(raw)
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn the_window_defaults_to_the_last_day_and_must_be_in_order() {
        let now = at("2026-10-18T12:00:00Z");
        assert_eq!(
            window(None, None, now).unwrap(),
            (at("2026-10-17T12:00:00Z"), now)
        );
        let to = at("2026-10-18T06:00:00Z");
        assert_eq!(
            window(None, Some(to), now).unwrap(),
            (at("2026-10-17T06:00:00Z"), to)
        );
        assert!(window(Some(now), Some(to), now).is_err());
    }

    #[test]
    fn the_limit_is_clamped() {
        assert_eq!(limit(None), 50);
        assert_eq!(limit(Some(0)), 1);
        assert_eq!(limit(Some(10_000)), 200);
    }
}
