use axum::{
    Extension, Json,
    extract::{Query, State},
    http::StatusCode,
    response::Json as ResponseJson,
};
use db::models::{
    coding_agent_turn::CodingAgentTurn,
    execution_process::{ExecutionProcess, ExecutionProcessStatus},
    workspace::{Workspace, WorkspaceError},
};
use deployment::Deployment;
use serde::Deserialize;
use services::services::{container::ContainerService, diff_stream, remote_sync};
use sqlx::Error as SqlxError;
use utils::response::ApiResponse;
use workspace_manager::WorkspaceManager;

use crate::{DeploymentImpl, error::ApiError};

#[derive(Debug, Deserialize)]
pub struct DeleteWorkspaceQuery {
    #[serde(default)]
    pub delete_remote: bool,
    #[serde(default)]
    pub delete_branches: bool,
}

pub async fn get_workspaces(
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<Vec<Workspace>>>, ApiError> {
    let pool = &deployment.db().pool;
    let workspaces = Workspace::fetch_all(pool).await?;
    Ok(ResponseJson(ApiResponse::success(workspaces)))
}

pub async fn get_workspace(
    Extension(workspace): Extension<Workspace>,
) -> Result<ResponseJson<ApiResponse<Workspace>>, ApiError> {
    Ok(ResponseJson(ApiResponse::success(workspace)))
}

#[derive(Debug, serde::Serialize, ts_rs::TS)]
pub struct LavishSession {
    /// Browser URL of the review session (lavish-axi's shared local server).
    pub url: String,
    /// Absolute path of the artifact under review.
    pub file: String,
}

#[derive(Debug, Deserialize)]
pub struct ReopenLavishRequest {
    /// Artifact path relative to a repo checkout inside this workspace
    /// (e.g. `docs/specs/BUG-1234.html`).
    pub file: String,
}

/// Re-opens the lavish-axi review gate for an artifact in this workspace's
/// worktree. Used when the original session is gone — planning ran before
/// the gate-opening playbooks, or the shared server is down and the relay
/// has not brought it back yet. Spawns the same headless command the
/// playbooks use; the UI's 5s session poll then picks the new session up,
/// and the relay starts watching it like any other.
pub async fn reopen_lavish_session(
    Extension(workspace): Extension<Workspace>,
    Json(request): Json<ReopenLavishRequest>,
) -> Result<ResponseJson<ApiResponse<()>>, ApiError> {
    let rel = std::path::Path::new(&request.file);
    if rel.as_os_str().is_empty()
        || rel.is_absolute()
        || rel
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(ApiError::BadRequest("Invalid artifact path".to_string()));
    }
    let Some(container_ref) = workspace.container_ref.as_deref() else {
        return Err(ApiError::BadRequest(
            "Workspace has no worktree".to_string(),
        ));
    };

    // The workspace dir holds one checkout per repo; the artifact path is
    // relative to a checkout root (the planning agent's cwd). Take the
    // first checkout that actually contains it.
    let root = std::path::PathBuf::from(container_ref);
    let mut found: Option<(std::path::PathBuf, std::path::PathBuf)> = None;
    if let Ok(mut entries) = tokio::fs::read_dir(&root).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let candidate = entry.path().join(rel);
            if tokio::fs::metadata(&candidate)
                .await
                .map(|meta| meta.is_file())
                .unwrap_or(false)
            {
                found = Some((entry.path(), candidate));
                break;
            }
        }
    }
    let Some((repo_dir, artifact)) = found else {
        return Err(ApiError::BadRequest(format!(
            "Artifact '{}' not found in the workspace worktree",
            request.file
        )));
    };

    services::services::lavish_relay::open_session(&artifact, &repo_dir)
        .await
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    Ok(ResponseJson(ApiResponse::success(())))
}

/// Returns the active lavish-axi review session for an artifact inside this
/// workspace's worktree, if any. Planning agents start the session headless
/// (`npx lavish-axi <spec> --no-open`), so this is how the UI offers the
/// human a way into the review gate. Reads lavish-axi's session registry at
/// `$LAVISH_AXI_STATE_DIR/state.json` (default `~/.lavish-axi/`).
pub async fn get_lavish_session(
    Extension(workspace): Extension<Workspace>,
) -> Result<ResponseJson<ApiResponse<Option<LavishSession>>>, ApiError> {
    let Some(container_ref) = workspace.container_ref.as_deref() else {
        return Ok(ResponseJson(ApiResponse::success(None)));
    };
    // Lavish canonicalizes session file paths (macOS: /var -> /private/var,
    // and pooled worktrees resolve through their <workspace>/<repo> symlink
    // to the real ~/.treehouse path) — match against every canonical root
    // of this workspace, not just the workspace dir.
    let worktree_roots = services::services::lavish_relay::worktree_roots(container_ref);

    let state_dir = std::env::var("LAVISH_AXI_STATE_DIR")
        .map(std::path::PathBuf::from)
        .ok()
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|home| std::path::PathBuf::from(home).join(".lavish-axi"))
        });
    let Some(state_file) = state_dir.map(|dir| dir.join("state.json")) else {
        return Ok(ResponseJson(ApiResponse::success(None)));
    };
    let Ok(raw) = tokio::fs::read_to_string(&state_file).await else {
        return Ok(ResponseJson(ApiResponse::success(None)));
    };
    let Ok(state) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return Ok(ResponseJson(ApiResponse::success(None)));
    };

    let session = state
        .get("sessions")
        .and_then(|sessions| sessions.as_object())
        .and_then(|sessions| {
            sessions.values().find_map(|session| {
                let file = session.get("file")?.as_str()?;
                let url = session.get("url")?.as_str()?;
                let ended = session
                    .get("status")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|status| status == "ended");
                let in_worktree = worktree_roots
                    .iter()
                    .any(|root| std::path::Path::new(file).starts_with(root));
                (!ended && in_worktree).then(|| LavishSession {
                    url: url.to_string(),
                    file: file.to_string(),
                })
            })
        });

    // The registry outlives the shared lavish server, so a stale entry
    // would otherwise advertise a URL that refuses to connect. Probe it:
    // an unreachable (or unknown-to-the-server) session reports as None,
    // which surfaces the Reopen button — and reopening restarts the
    // shared server with the session re-registered.
    if let Some(candidate) = &session {
        let alive = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(800))
            .build()
        {
            Ok(client) => match client.get(&candidate.url).send().await {
                Ok(response) => response.status() != reqwest::StatusCode::NOT_FOUND,
                Err(_) => false,
            },
            Err(_) => true,
        };
        if !alive {
            return Ok(ResponseJson(ApiResponse::success(None)));
        }
    }

    // On hosted runners the registry URL (http://127.0.0.1:<port>/session/…)
    // is meaningless to a remote browser. LAVISH_PUBLIC_BASE names the
    // externally reachable origin that the app server proxies to the loopback
    // lavish server (see routes::lavish_proxy); rewrite the base, keep the
    // session path. The liveness probe above intentionally used the local
    // URL — that is the address the server itself can reach.
    let session = session.map(|mut session| {
        session.url = lavish_public_url(&session.url);
        session
    });

    Ok(ResponseJson(ApiResponse::success(session)))
}

/// Rewrites a loopback lavish session URL onto LAVISH_PUBLIC_BASE, preserving
/// path and query. Returns the input unchanged when the env var is unset
/// (local dev / desktop) or anything fails to parse.
fn lavish_public_url(local_url: &str) -> String {
    let Ok(base) = std::env::var("LAVISH_PUBLIC_BASE") else {
        return local_url.to_string();
    };
    let base = base.trim().trim_end_matches('/').to_string();
    if base.is_empty() {
        return local_url.to_string();
    }
    match url::Url::parse(local_url) {
        Ok(parsed) => {
            let mut public = format!("{base}{}", parsed.path());
            if let Some(query) = parsed.query() {
                public.push('?');
                public.push_str(query);
            }
            public
        }
        Err(_) => local_url.to_string(),
    }
}

pub async fn update_workspace(
    Extension(workspace): Extension<Workspace>,
    State(deployment): State<DeploymentImpl>,
    Json(request): Json<db::models::requests::UpdateWorkspace>,
) -> Result<ResponseJson<ApiResponse<Workspace>>, ApiError> {
    let pool = &deployment.db().pool;
    let is_archiving = request.archived == Some(true) && !workspace.archived;

    Workspace::update(
        pool,
        workspace.id,
        request.archived,
        request.pinned,
        request.name.as_deref(),
    )
    .await?;
    let updated = Workspace::find_by_id(pool, workspace.id)
        .await?
        .ok_or(WorkspaceError::WorkspaceNotFound)?;

    if (request.archived.is_some() || request.name.is_some())
        && let Ok(client) = deployment.remote_client()
    {
        let ws = updated.clone();
        let name = request.name.clone();
        let archived = request.archived;
        let stats =
            diff_stream::compute_diff_stats(&deployment.db().pool, deployment.git(), &ws).await;
        tokio::spawn(async move {
            remote_sync::sync_workspace_to_remote(
                &client,
                ws.id,
                name.map(Some),
                archived,
                stats.as_ref(),
            )
            .await;
        });
    }

    if is_archiving && let Err(e) = deployment.container().archive_workspace(workspace.id).await {
        tracing::error!("Failed to archive workspace {}: {}", workspace.id, e);
    }

    Ok(ResponseJson(ApiResponse::success(updated)))
}

pub async fn get_first_user_message(
    Extension(workspace): Extension<Workspace>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<Option<String>>>, ApiError> {
    let pool = &deployment.db().pool;
    let message = Workspace::get_first_user_message(pool, workspace.id).await?;
    Ok(ResponseJson(ApiResponse::success(message)))
}

pub async fn delete_workspace(
    Extension(workspace): Extension<Workspace>,
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<DeleteWorkspaceQuery>,
) -> Result<(StatusCode, ResponseJson<ApiResponse<()>>), ApiError> {
    let pool = &deployment.db().pool;
    let workspace_manager = deployment.workspace_manager();
    let workspace_id = workspace.id;

    if ExecutionProcess::has_running_non_dev_server_processes_for_workspace(pool, workspace_id)
        .await?
    {
        return Err(ApiError::Conflict(
            "Cannot delete workspace while processes are running. Stop all processes first."
                .to_string(),
        ));
    }

    let dev_servers =
        ExecutionProcess::find_running_dev_servers_by_workspace(pool, workspace_id).await?;

    for dev_server in dev_servers {
        tracing::info!(
            "Stopping dev server {} before deleting workspace {}",
            dev_server.id,
            workspace_id
        );

        if let Err(e) = deployment
            .container()
            .stop_execution(&dev_server, ExecutionProcessStatus::Killed)
            .await
        {
            tracing::error!(
                "Failed to stop dev server {} for workspace {}: {}",
                dev_server.id,
                workspace_id,
                e
            );
        }
    }

    let managed_workspace = workspace_manager.load_managed_workspace(workspace).await?;
    let deletion_context = managed_workspace.prepare_deletion_context().await?;
    let rows_affected = managed_workspace.delete_record().await?;

    if rows_affected == 0 {
        return Err(ApiError::Database(SqlxError::RowNotFound));
    }

    deployment
        .track_if_analytics_allowed(
            "workspace_deleted",
            serde_json::json!({
                "workspace_id": workspace_id.to_string(),
            }),
        )
        .await;

    if query.delete_remote {
        if let Ok(client) = deployment.remote_client() {
            match client.delete_workspace(workspace_id).await {
                Ok(()) => {
                    tracing::info!("Deleted remote workspace for {}", workspace_id);
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to delete remote workspace for {}: {}",
                        workspace_id,
                        e
                    );
                }
            }
        } else {
            tracing::debug!(
                "Remote client not available, skipping remote deletion for {}",
                workspace_id
            );
        }
    }

    WorkspaceManager::spawn_workspace_deletion_cleanup(deletion_context, query.delete_branches);

    Ok((StatusCode::ACCEPTED, ResponseJson(ApiResponse::success(()))))
}

#[axum::debug_handler]
pub async fn mark_seen(
    Extension(workspace): Extension<Workspace>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<()>>, ApiError> {
    let pool = &deployment.db().pool;
    CodingAgentTurn::mark_seen_by_workspace_id(pool, workspace.id).await?;
    Ok(ResponseJson(ApiResponse::success(())))
}
