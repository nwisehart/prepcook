//! The script runner and the secrets store over HTTP: what the Settings page
//! and the agents' MCP tools use.
//!
//! - `GET /api/secrets`, `PUT|DELETE /api/secrets/{name}` — names only, never
//!   values, go out.
//! - `GET /api/repos/{id}/scripts` — the repository's parsed
//!   `.prepcook/scripts.toml`, or why it cannot be read.
//! - `POST /api/repos/{id}/scripts/run` — one step, or the whole stand-up, in
//!   the repository's checkout, on the reproduction target its board uses;
//!   `GET /api/script-runs/{id}` for its report and
//!   `/api/script-runs/{id}/raw-logs/ws` for its output.
//! - `GET /api/app-runs/current?worktree=…`, `POST /api/app-runs`,
//!   `POST /api/app-runs/{id}/restart` — an agent's app, through the target
//!   that stood it up.

use std::path::{Path as FsPath, PathBuf};

use axum::{
    Router,
    extract::{Path, Query, State, ws::WebSocketUpgrade},
    response::{IntoResponse, Json as ResponseJson},
    routing::{get, post, put},
};
use db::models::{
    execution_process::{ExecutionProcess, ExecutionProcessRunReason},
    session::Session,
    workspace::Workspace,
    workspace_repo::WorkspaceRepo,
};
use deployment::Deployment;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use services::services::{
    reproduction::{self, ReproTarget, ReproductionSource, ReproductionTarget, StandUpRequest},
    scripts::{
        AppRunStatus, Manifest, ManifestError, RunStep, ScriptRunState, StandUpOutcome,
        StandUpReport, load_manifest,
    },
    secrets::{self, Secrets},
};
use ts_rs::TS;
use utils::{log_msg::LogMsg, response::ApiResponse};
use uuid::Uuid;

use crate::{DeploymentImpl, error::ApiError, routes::execution_processes::serve_raw_log_stream};

#[derive(Debug, Serialize, TS)]
pub struct SecretsResponse {
    pub names: Vec<String>,
    /// Where the secrets file lives.
    pub file: String,
}

#[derive(Debug, Deserialize, TS)]
pub struct SetSecretRequest {
    pub value: String,
}

/// A repository's `.prepcook/scripts.toml` as the runner reads it.
#[derive(Debug, Serialize, TS)]
pub struct RepoScriptsResponse {
    /// `None` when the file is missing or does not parse; `error` says which.
    pub manifest: Option<Manifest>,
    pub error: Option<String>,
    /// True when there is no file at all, as distinct from a broken one.
    pub missing: bool,
    /// Names in `[secrets] requires` with no value in the secrets store.
    pub required_secrets_missing: Vec<String>,
    /// The reproduction target the repository's board uses; test runs run on
    /// it.
    pub target: ReproductionTarget,
    /// Why that target cannot stand this manifest up (a board set to
    /// `remote` with no `[app.remote]`), when it cannot.
    pub target_problem: Option<String>,
}

#[derive(Debug, Deserialize, TS)]
pub struct RunScriptRequest {
    pub step: RunStep,
}

#[derive(Debug, Serialize, TS)]
pub struct RunScriptResponse {
    pub run_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct WorktreeQuery {
    pub worktree: String,
}

#[derive(Debug, Deserialize, TS)]
pub struct StartAppRunRequest {
    pub worktree: String,
}

/// A stood-up (or restarted) app and how the stand-up went.
#[derive(Debug, Serialize, TS)]
pub struct AppRunResponse {
    pub status: AppRunStatus,
    pub report: StandUpReport,
}

async fn list_secrets() -> ResponseJson<ApiResponse<SecretsResponse>> {
    let store = secrets::load();
    ResponseJson(ApiResponse::success(SecretsResponse {
        names: store.names(),
        file: store.path().to_string_lossy().into_owned(),
    }))
}

async fn set_secret(
    Path(name): Path<String>,
    ResponseJson(payload): ResponseJson<SetSecretRequest>,
) -> ResponseJson<ApiResponse<SecretsResponse>> {
    let mut store = secrets::load();
    if let Err(e) = store.set(&name, &payload.value) {
        return ResponseJson(ApiResponse::error(&e.to_string()));
    }
    ResponseJson(ApiResponse::success(SecretsResponse {
        names: store.names(),
        file: store.path().to_string_lossy().into_owned(),
    }))
}

async fn delete_secret(Path(name): Path<String>) -> ResponseJson<ApiResponse<SecretsResponse>> {
    let mut store = secrets::load();
    if let Err(e) = store.unset(&name) {
        return ResponseJson(ApiResponse::error(&e.to_string()));
    }
    ResponseJson(ApiResponse::success(SecretsResponse {
        names: store.names(),
        file: store.path().to_string_lossy().into_owned(),
    }))
}

async fn get_repo_scripts(
    State(deployment): State<DeploymentImpl>,
    Path(repo_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<RepoScriptsResponse>>, ApiError> {
    let repo = deployment
        .repo()
        .get_by_id(&deployment.db().pool, repo_id)
        .await?;
    let target = reproduction_source(&deployment)
        .target_for_repos(&[repo_id])
        .await;
    let response = match load_manifest(&repo.path) {
        Ok(manifest) => {
            let required_secrets_missing = Secrets::load()
                .check_required(&manifest.secrets.requires)
                .err()
                .unwrap_or_default();
            let target_problem = reproduction::build(
                deployment.script_runner(),
                target,
                Some(&manifest),
                &repo.path,
            )
            .err()
            .map(|e| e.to_string());
            RepoScriptsResponse {
                manifest: Some(manifest),
                error: None,
                missing: false,
                required_secrets_missing,
                target,
                target_problem,
            }
        }
        Err(e) => RepoScriptsResponse {
            manifest: None,
            missing: matches!(e, ManifestError::Missing { .. }),
            error: Some(e.to_string()),
            required_secrets_missing: Vec::new(),
            target,
            target_problem: None,
        },
    };
    Ok(ResponseJson(ApiResponse::success(response)))
}

async fn run_repo_script(
    State(deployment): State<DeploymentImpl>,
    Path(repo_id): Path<Uuid>,
    ResponseJson(payload): ResponseJson<RunScriptRequest>,
) -> Result<ResponseJson<ApiResponse<RunScriptResponse>>, ApiError> {
    let repo = deployment
        .repo()
        .get_by_id(&deployment.db().pool, repo_id)
        .await?;
    let target = match reproduction_source(&deployment)
        .for_repos(&[repo_id], &repo.path)
        .await
    {
        Ok(target) => target,
        Err(e) => return Ok(ResponseJson(ApiResponse::error(&e.to_string()))),
    };
    let run_id =
        deployment
            .script_runner()
            .spawn_script_run(target, repo.path.clone(), payload.step);
    Ok(ResponseJson(ApiResponse::success(RunScriptResponse {
        run_id,
    })))
}

async fn get_script_run(
    State(deployment): State<DeploymentImpl>,
    Path(run_id): Path<Uuid>,
) -> ResponseJson<ApiResponse<ScriptRunState>> {
    match deployment.script_runner().script_run(run_id) {
        Some(state) => ResponseJson(ApiResponse::success(state)),
        None => ResponseJson(ApiResponse::error(&format!("no script run {run_id}"))),
    }
}

async fn stream_script_run_logs_ws(
    ws: WebSocketUpgrade,
    State(deployment): State<DeploymentImpl>,
    Path(run_id): Path<Uuid>,
) -> impl IntoResponse {
    let raw_stream = deployment
        .script_runner()
        .script_run_log(run_id)
        .map(|store| {
            store
                .history_plus_stream()
                .filter(|msg| {
                    futures_util::future::ready(matches!(
                        msg,
                        Ok(LogMsg::Stdout(..) | LogMsg::Stderr(..) | LogMsg::Finished)
                    ))
                })
                .boxed()
        });
    ws.on_upgrade(move |socket| async move {
        if let Err(e) = serve_raw_log_stream(socket, raw_stream).await {
            tracing::warn!("script run logs WS closed: {}", e);
        }
    })
}

/// The run serving a worktree, with its target's check of whether the app
/// answers.
async fn current_app_run(
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<WorktreeQuery>,
) -> ResponseJson<ApiResponse<Option<AppRunStatus>>> {
    let runner = deployment.script_runner();
    let Some((run, workspace_id)) = runner.current_run_for_worktree(FsPath::new(&query.worktree))
    else {
        return ResponseJson(ApiResponse::success(None));
    };
    let mut status = run.status(workspace_id);
    status.health = Some(reproduction::for_run(runner, &run).status(&run).await);
    ResponseJson(ApiResponse::success(Some(status)))
}

async fn restart_app_run(
    State(deployment): State<DeploymentImpl>,
    Path(run_id): Path<Uuid>,
) -> ResponseJson<ApiResponse<AppRunResponse>> {
    let runner = deployment.script_runner();
    let kind = runner.adopted_kind(run_id).unwrap_or_default();
    match reproduction::for_kind(runner, kind).restart(run_id).await {
        Ok((status, report)) => {
            ResponseJson(ApiResponse::success(AppRunResponse { status, report }))
        }
        Err(message) => ResponseJson(ApiResponse::error(&secrets::redact(&message))),
    }
}

/// Stands an app up in a workspace's worktree for the agent working there,
/// when none is running, on the target the workspace's board uses: the run
/// is handed to the workspace and torn down when the agent's current process
/// finishes.
async fn start_app_run(
    State(deployment): State<DeploymentImpl>,
    ResponseJson(payload): ResponseJson<StartAppRunRequest>,
) -> Result<ResponseJson<ApiResponse<AppRunResponse>>, ApiError> {
    let runner = deployment.script_runner();
    let worktree = PathBuf::from(&payload.worktree);
    if let Some(current) = runner.current_for_worktree(&worktree) {
        return Ok(ResponseJson(ApiResponse::error(&format!(
            "an app is already running for this worktree at {} (run {}); restart it instead",
            current.url, current.run_id
        ))));
    }
    let pool = &deployment.db().pool;
    let Some(workspace) = Workspace::fetch_all(pool)
        .await?
        .into_iter()
        .find(|workspace| {
            workspace
                .container_ref
                .as_deref()
                .is_some_and(|dir| !dir.is_empty() && worktree.starts_with(dir))
        })
    else {
        return Ok(ResponseJson(ApiResponse::error(
            "this path is not inside a Prep Cook workspace",
        )));
    };
    let process = agent_process_for(pool, workspace.id).await?;
    let Some(process_id) = process else {
        return Ok(ResponseJson(ApiResponse::error(
            "no agent is running in this workspace to own the app",
        )));
    };
    let repo_ids: Vec<Uuid> = WorkspaceRepo::find_repos_for_workspace(pool, workspace.id)
        .await?
        .iter()
        .map(|repo| repo.id)
        .collect();
    let stood_up = match reproduction_source(&deployment)
        .for_repos(&repo_ids, &worktree)
        .await
    {
        Ok(target) => target.stand_up(StandUpRequest::new(worktree.clone())).await,
        Err(e) => Err(e),
    };
    match stood_up {
        Ok(run) => {
            let status = run.status(Some(workspace.id));
            let report = StandUpReport {
                steps: run.steps.clone(),
                outcome: StandUpOutcome::Ok,
                message: None,
            };
            runner.adopt_for_process(run, workspace.id, process_id);
            Ok(ResponseJson(ApiResponse::success(AppRunResponse {
                status,
                report,
            })))
        }
        Err(e) => {
            let report = e.into_report();
            Ok(ResponseJson(ApiResponse::error(&format!(
                "the app did not stand up: {}\n\n{}",
                report.outcome.label(),
                report.evidence()
            ))))
        }
    }
}

fn reproduction_source(deployment: &DeploymentImpl) -> ReproductionSource {
    ReproductionSource::new(
        deployment.config().clone(),
        deployment.script_runner().clone(),
    )
}

/// The running coding-agent process in a workspace, if any.
async fn agent_process_for(
    pool: &sqlx::SqlitePool,
    workspace_id: Uuid,
) -> Result<Option<Uuid>, ApiError> {
    let sessions = Session::find_by_workspace_id(pool, workspace_id).await?;
    let running = ExecutionProcess::find_running(pool).await?;
    Ok(running
        .into_iter()
        .filter(|process| process.run_reason == ExecutionProcessRunReason::CodingAgent)
        .find(|process| sessions.iter().any(|s| s.id == process.session_id))
        .map(|process| process.id))
}

pub fn router() -> Router<DeploymentImpl> {
    Router::new()
        .route("/secrets", get(list_secrets))
        .route("/secrets/{name}", put(set_secret).delete(delete_secret))
        .route("/repos/{repo_id}/scripts", get(get_repo_scripts))
        .route("/repos/{repo_id}/scripts/run", post(run_repo_script))
        .route("/script-runs/{run_id}", get(get_script_run))
        .route(
            "/script-runs/{run_id}/raw-logs/ws",
            get(stream_script_run_logs_ws),
        )
        .route("/app-runs", post(start_app_run))
        .route("/app-runs/current", get(current_app_run))
        .route("/app-runs/{run_id}/restart", post(restart_app_run))
}
