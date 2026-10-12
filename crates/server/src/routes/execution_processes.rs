use anyhow;
use axum::{
    Extension, Router,
    extract::{
        Path, Query, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::header,
    middleware::from_fn_with_state,
    response::{IntoResponse, Json as ResponseJson},
    routing::{get, post},
};
use db::models::{
    execution_process::{ExecutionProcess, ExecutionProcessError, ExecutionProcessStatus},
    execution_process_repo_state::ExecutionProcessRepoState,
};
use deployment::Deployment;
use futures_util::{SinkExt, StreamExt, TryStreamExt};
use serde::Deserialize;
use services::services::{container::ContainerService, execution_process};
use utils::{log_msg::LogMsg, response::ApiResponse};
use uuid::Uuid;

use crate::{DeploymentImpl, error::ApiError, middleware::load_execution_process_middleware};

#[derive(Debug, Deserialize)]
struct SessionExecutionProcessQuery {
    pub session_id: Uuid,
    /// If true, include soft-deleted (dropped) processes in results/stream
    #[serde(default)]
    pub show_soft_deleted: Option<bool>,
}

async fn get_execution_process_by_id(
    Extension(execution_process): Extension<ExecutionProcess>,
    State(_deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<ExecutionProcess>>, ApiError> {
    Ok(ResponseJson(ApiResponse::success(execution_process)))
}

/// The run's stored log, as a file.
///
/// Everything the console shows is derived from these lines, and until now the
/// only way to read them whole was the database or the runner's disk. A turn
/// that went wrong is worth reading in full, and the person who needs to read
/// it is not always the person with an ssh key.
async fn download_raw_logs(
    Extension(execution_process): Extension<ExecutionProcess>,
    State(deployment): State<DeploymentImpl>,
) -> Result<impl IntoResponse, ApiError> {
    let jsonl = execution_process::raw_log_jsonl(&deployment.db().pool, execution_process.id)
        .await
        .ok_or_else(|| {
            ApiError::ExecutionProcess(ExecutionProcessError::ExecutionProcessNotFound)
        })?;

    let filename = format!("execution-{}.jsonl", execution_process.id);
    Ok((
        [
            (header::CONTENT_TYPE, "application/x-ndjson".to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{filename}\""),
            ),
        ],
        jsonl,
    ))
}

async fn stream_raw_logs_ws(
    ws: WebSocketUpgrade,
    State(deployment): State<DeploymentImpl>,
    Path(exec_id): Path<Uuid>,
) -> impl IntoResponse {
    // Always accept the WebSocket upgrade — handle "not found" inside the
    // connection by sending `finished` and closing cleanly, instead of
    // rejecting with HTTP 404 which the browser surfaces as an opaque
    // connection failure.
    ws.on_upgrade(move |socket| async move {
        if let Err(e) = handle_raw_logs_ws(socket, deployment, exec_id).await {
            tracing::warn!("raw logs WS closed: {}", e);
        }
    })
}

async fn handle_raw_logs_ws(
    socket: WebSocket,
    deployment: DeploymentImpl,
    exec_id: Uuid,
) -> anyhow::Result<()> {
    let raw_stream = deployment.container().stream_raw_logs(&exec_id).await;
    serve_raw_log_stream(socket, raw_stream).await
}

/// Streams raw stdout/stderr lines over a WebSocket as JSON patches, the shape
/// the log viewer reads. `None` sends `finished` and closes cleanly. Shared by
/// execution processes and script runs.
pub(crate) async fn serve_raw_log_stream(
    mut socket: WebSocket,
    raw_stream: Option<futures_util::stream::BoxStream<'static, Result<LogMsg, std::io::Error>>>,
) -> anyhow::Result<()> {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use executors::logs::utils::patch::ConversationPatch;

    // Get the raw stream — if not found, send finished and close cleanly
    let raw_stream = match raw_stream {
        Some(stream) => stream,
        None => {
            // No logs available: send finished so the client gets a clean
            // close instead of retrying endlessly.
            let _ = socket
                .send(LogMsg::Finished.to_ws_message_unchecked())
                .await;
            let _ = socket.close().await;
            return Ok(());
        }
    };

    let counter = Arc::new(AtomicUsize::new(0));
    let mut stream = raw_stream.map_ok({
        let counter = counter.clone();
        move |m| match m {
            LogMsg::Stdout(content) => {
                let index = counter.fetch_add(1, Ordering::SeqCst);
                let patch = ConversationPatch::add_stdout(index, content);
                LogMsg::JsonPatch(patch).to_ws_message_unchecked()
            }
            LogMsg::Stderr(content) => {
                let index = counter.fetch_add(1, Ordering::SeqCst);
                let patch = ConversationPatch::add_stderr(index, content);
                LogMsg::JsonPatch(patch).to_ws_message_unchecked()
            }
            LogMsg::Finished => LogMsg::Finished.to_ws_message_unchecked(),
            _ => unreachable!("Raw stream should only have Stdout/Stderr/Finished"),
        }
    });

    loop {
        tokio::select! {
            item = stream.next() => {
                match item {
                    Some(Ok(msg)) => {
                        if socket.send(msg).await.is_err() {
                            break;
                        }
                    }
                    Some(Err(e)) => {
                        tracing::error!("stream error: {}", e);
                        break;
                    }
                    None => break,
                }
            }
            inbound = socket.recv() => {
                match inbound {
                    Some(Ok(Message::Close(_))) => break,
                    Some(Ok(_)) => {}
                    None => break,
                    Some(Err(_)) => break,
                }
            }
        }
    }
    // Send a proper close frame so the client sees code 1000 (normal closure)
    // instead of an abnormal TCP drop that triggers reconnection attempts.
    let _ = socket.close().await;
    Ok(())
}

async fn stream_normalized_logs_ws(
    ws: WebSocketUpgrade,
    State(deployment): State<DeploymentImpl>,
    Path(exec_id): Path<Uuid>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| async move {
        let stream = deployment
            .container()
            .stream_normalized_logs(&exec_id)
            .await;

        match stream {
            Some(stream) => {
                let stream = stream.err_into::<anyhow::Error>().into_stream();
                if let Err(e) = handle_normalized_logs_ws(socket, stream).await {
                    tracing::warn!("normalized logs WS closed: {}", e);
                }
            }
            None => {
                // No logs available: send finished and close cleanly
                let mut socket = socket;
                let _ = socket
                    .send(utils::log_msg::LogMsg::Finished.to_ws_message_unchecked())
                    .await;
                let _ = socket.close().await;
            }
        }
    })
}

async fn handle_normalized_logs_ws(
    mut socket: WebSocket,
    stream: impl futures_util::Stream<Item = anyhow::Result<LogMsg>> + Unpin + Send + 'static,
) -> anyhow::Result<()> {
    let mut stream = stream.map_ok(|msg| msg.to_ws_message_unchecked());
    loop {
        tokio::select! {
            item = stream.next() => {
                match item {
                    Some(Ok(msg)) => {
                        if socket.send(msg).await.is_err() {
                            break;
                        }
                    }
                    Some(Err(e)) => {
                        tracing::error!("stream error: {}", e);
                        break;
                    }
                    None => break,
                }
            }
            inbound = socket.recv() => {
                match inbound {
                    Some(Ok(Message::Close(_))) => break,
                    Some(Ok(_)) => {}
                    None => break,
                    Some(Err(_)) => break,
                }
            }
        }
    }
    let _ = socket.close().await;
    Ok(())
}

async fn stop_execution_process(
    Extension(execution_process): Extension<ExecutionProcess>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<()>>, ApiError> {
    deployment
        .container()
        .stop_execution(&execution_process, ExecutionProcessStatus::Killed)
        .await?;

    Ok(ResponseJson(ApiResponse::success(())))
}

async fn stream_execution_processes_by_session_ws(
    ws: WebSocketUpgrade,
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<SessionExecutionProcessQuery>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| async move {
        if let Err(e) = handle_execution_processes_by_session_ws(
            socket,
            deployment,
            query.session_id,
            query.show_soft_deleted.unwrap_or(false),
        )
        .await
        {
            tracing::warn!("execution processes by session WS closed: {}", e);
        }
    })
}

async fn handle_execution_processes_by_session_ws(
    mut socket: WebSocket,
    deployment: DeploymentImpl,
    session_id: uuid::Uuid,
    show_soft_deleted: bool,
) -> anyhow::Result<()> {
    // Get the raw stream and convert LogMsg to WebSocket messages
    let mut stream = deployment
        .events()
        .stream_execution_processes_for_session_raw(session_id, show_soft_deleted)
        .await?
        .map_ok(|msg| msg.to_ws_message_unchecked());

    loop {
        tokio::select! {
            item = stream.next() => {
                match item {
                    Some(Ok(msg)) => {
                        if socket.send(msg).await.is_err() {
                            break;
                        }
                    }
                    Some(Err(e)) => {
                        tracing::error!("stream error: {}", e);
                        break;
                    }
                    None => break,
                }
            }
            inbound = socket.recv() => {
                match inbound {
                    Some(Ok(Message::Close(_))) => break,
                    Some(Ok(_)) => {}
                    None => break,
                    Some(Err(_)) => break,
                }
            }
        }
    }
    Ok(())
}

async fn get_execution_process_repo_states(
    Extension(execution_process): Extension<ExecutionProcess>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<Vec<ExecutionProcessRepoState>>>, ApiError> {
    let pool = &deployment.db().pool;
    let repo_states =
        ExecutionProcessRepoState::find_by_execution_process_id(pool, execution_process.id).await?;
    Ok(ResponseJson(ApiResponse::success(repo_states)))
}

pub(super) fn router(deployment: &DeploymentImpl) -> Router<DeploymentImpl> {
    let workspace_id_router = Router::new()
        .route("/", get(get_execution_process_by_id))
        .route("/stop", post(stop_execution_process))
        .route("/repo-states", get(get_execution_process_repo_states))
        .route("/raw-logs/ws", get(stream_raw_logs_ws))
        .route("/raw-logs/download", get(download_raw_logs))
        .route("/normalized-logs/ws", get(stream_normalized_logs_ws))
        .layer(from_fn_with_state(
            deployment.clone(),
            load_execution_process_middleware,
        ));

    let workspaces_router = Router::new()
        .route(
            "/stream/session/ws",
            get(stream_execution_processes_by_session_ws),
        )
        .nest("/{id}", workspace_id_router);

    Router::new().nest("/execution-processes", workspaces_router)
}
