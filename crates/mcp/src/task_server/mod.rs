mod handler;
mod tools;

use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Context;
use db::models::{requests::ContainerQuery, workspace::WorkspaceContext};
use rmcp::{handler::server::tool::ToolRouter, schemars};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use uuid::Uuid;

pub(crate) use crate::ApiResponseEnvelope;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, schemars::JsonSchema)]
pub struct McpRepoContext {
    #[schemars(description = "The unique identifier of the repository")]
    pub repo_id: Uuid,
    #[schemars(description = "The name of the repository")]
    pub repo_name: String,
    #[schemars(description = "The target branch for this repository in this workspace")]
    pub target_branch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, schemars::JsonSchema)]
pub struct McpContext {
    #[schemars(description = "The organization ID (if workspace is linked to remote)")]
    pub organization_id: Option<Uuid>,
    #[schemars(description = "The remote project ID (if workspace is linked to remote)")]
    pub project_id: Option<Uuid>,
    #[schemars(description = "The remote issue ID (if workspace is linked to a remote issue)")]
    pub issue_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(description = "The orchestrator session ID when running in orchestrator mode")]
    pub orchestrator_session_id: Option<Uuid>,
    pub workspace_id: Uuid,
    pub workspace_branch: String,
    #[schemars(
        description = "Repository info and target branches for each repo in this workspace"
    )]
    pub workspace_repos: Vec<McpRepoContext>,
}

/// The remote linkage of the workspace the agent is running in: which board,
/// which ticket, which organization. Empty when the workspace has no remote
/// counterpart (a hand-created local workspace).
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct RemoteLink {
    pub project_id: Option<Uuid>,
    pub issue_id: Option<Uuid>,
    pub organization_id: Option<Uuid>,
}

/// Per-attempt budget for the remote workspace lookup. Generous on purpose:
/// the remote is a hosted Postgres that can take seconds under load, and
/// coming back empty costs the agent its whole issue/project context — it can
/// then neither record a verdict nor read the graph.
const REMOTE_LINK_TIMEOUT: Duration = Duration::from_secs(6);
/// Attempts for *transient* failures (timeout, transport error, 5xx). A 4xx or
/// an empty payload is a definitive "not linked" and returns immediately.
const REMOTE_LINK_ATTEMPTS: u32 = 3;
/// After a transient failure, how long before another tool call retries. Keeps
/// a board outage from costing every single call the full retry budget.
const REMOTE_LINK_RETRY_COOLDOWN: Duration = Duration::from_secs(30);

/// Cached linkage plus the last transient failure, so "definitively not
/// linked" (cache it, stop asking) stays distinct from "could not reach the
/// board" (retry, but not on every call).
#[derive(Debug, Default)]
pub(crate) struct RemoteLinkCache {
    link: Option<RemoteLink>,
    failed_at: Option<Instant>,
}

/// What the startup workspace lookup found.
enum StartupContext {
    Workspace(Box<McpContext>),
    NotAWorkspace,
    /// The server could not answer; the reason is logged once.
    Unavailable(String),
}

enum ContextLookup {
    Found(Box<WorkspaceContext>),
    NotAWorkspace,
    Failed(reqwest::StatusCode),
}

#[derive(Debug, Clone)]
pub enum McpMode {
    Global,
    Orchestrator,
}

/// The backend's base URL, re-readable while the process runs.
///
/// The app binds an OS-assigned port and republishes it to the port file on
/// every start, but this bridge lives as long as the agent session that spawned
/// it — so a URL captured once goes stale the moment the app restarts, and every
/// later tool call fails to connect. Holding it behind a lock lets
/// [`McpServer::spawn_port_watcher`] follow the server instead.
#[derive(Debug, Clone)]
pub struct BaseUrl(Arc<std::sync::RwLock<String>>);

impl BaseUrl {
    fn get(&self) -> String {
        self.0
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// Returns true when the URL actually changed.
    fn set(&self, url: String) -> bool {
        let mut current = self
            .0
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if *current == url {
            return false;
        }
        *current = url;
        true
    }
}

/// Joins a base URL and a path with exactly one separator, whichever side
/// carries a slash.
fn join_url(base: &str, path: &str) -> String {
    format!(
        "{}/{}",
        base.trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}

impl From<&str> for BaseUrl {
    fn from(url: &str) -> Self {
        Self(Arc::new(std::sync::RwLock::new(url.to_string())))
    }
}

#[derive(Debug, Clone)]
pub struct McpServer {
    client: reqwest::Client,
    base_url: BaseUrl,
    tool_router: ToolRouter<McpServer>,
    context: Option<McpContext>,
    mode: McpMode,
    /// Remote linkage, resolved lazily and cached only once known. Shared so
    /// clones of the server (rmcp hands out clones per call) see one cache.
    remote_link: Arc<RwLock<RemoteLinkCache>>,
}

impl McpServer {
    pub fn new_global(base_url: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.into(),
            tool_router: Self::global_mode_router(),
            context: None,
            mode: McpMode::Global,
            remote_link: Arc::new(RwLock::new(RemoteLinkCache::default())),
        }
    }

    pub fn new_orchestrator(base_url: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.into(),
            tool_router: Self::orchestrator_mode_router(),
            context: None,
            mode: McpMode::Orchestrator,
            remote_link: Arc::new(RwLock::new(RemoteLinkCache::default())),
        }
    }

    /// Like [`Self::new_global`] but with the lean playbook-agent tool set
    /// (`agent_mode_router`). Behaves as Global everywhere else — context is
    /// optional, no orchestrator session scoping — only the advertised tool
    /// schemas shrink.
    pub fn new_agent(base_url: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.into(),
            tool_router: Self::agent_mode_router(),
            context: None,
            mode: McpMode::Global,
            remote_link: Arc::new(RwLock::new(RemoteLinkCache::default())),
        }
    }

    fn url(&self, path: &str) -> String {
        join_url(&self.base_url.get(), path)
    }

    /// Follows the server's published port for the rest of the process's life.
    ///
    /// `host` is carried over because the port file records only ports. Callers
    /// skip this when the URL came from `PREPCOOK_BACKEND_URL` or `MCP_PORT`: an
    /// explicit target is a deliberate pin and must not be second-guessed.
    pub fn spawn_port_watcher(&self, host: String) {
        const INTERVAL: Duration = Duration::from_secs(5);
        let base_url = self.base_url.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(INTERVAL).await;
                let Ok(port) = utils::port_file::read_port_file("prepcook").await else {
                    continue;
                };
                if base_url.set(format!("http://{host}:{port}")) {
                    tracing::info!("Backend moved to port {port}; following it");
                }
            }
        });
    }

    /// Resolves the workspace the session runs in and settles the tool set.
    ///
    /// Never fails: outside a workspace, or with the server down, the bridge
    /// still starts, serving the global tool set without `get_context`, and
    /// says why in one log line. A session that is not one of Prep Cook's
    /// (an engineer's own checkout) then sees a working server, not a broken
    /// one.
    pub async fn init(mut self) -> anyhow::Result<Self> {
        match self.fetch_context_at_startup().await {
            StartupContext::Workspace(context) => {
                tracing::info!("Prep Cook workspace context loaded");
                self.context = Some(*context);
            }
            StartupContext::NotAWorkspace => {
                self.reduce();
                tracing::info!("not inside a Prep Cook workspace; workspace tools disabled");
            }
            StartupContext::Unavailable(reason) => {
                self.reduce();
                tracing::info!("{reason}; workspace tools disabled");
            }
        }
        Ok(self)
    }

    /// The tool set for a session with no workspace context: what global mode
    /// serves, without `get_context`, which has nothing to report.
    fn reduce(&mut self) {
        if matches!(self.mode, McpMode::Orchestrator) {
            self.tool_router = Self::global_mode_router();
            self.mode = McpMode::Global;
        }
        self.tool_router.map.remove("get_context");
        self.context = None;
    }

    pub fn mode(&self) -> &McpMode {
        &self.mode
    }

    /// Names of the tools this server advertises.
    pub fn tool_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .tool_router
            .map
            .keys()
            .map(|name| name.to_string())
            .collect();
        names.sort();
        names
    }

    async fn fetch_context_at_startup(&self) -> StartupContext {
        let Ok(current_dir) = std::env::current_dir() else {
            return StartupContext::NotAWorkspace;
        };
        let canonical_path = current_dir.canonicalize().unwrap_or(current_dir);
        let normalized_path = utils::path::normalize_macos_private_alias(&canonical_path);

        match self.try_fetch_attempt_context(&normalized_path).await {
            Ok(ContextLookup::Found(ctx)) => StartupContext::Workspace(Box::new(
                self.build_mcp_context_from_workspace_context(&ctx).await,
            )),
            Ok(ContextLookup::NotAWorkspace) => StartupContext::NotAWorkspace,
            Ok(ContextLookup::Failed(status)) => StartupContext::Unavailable(format!(
                "Prep Cook server answered {status} to the workspace lookup"
            )),
            Err(error) => {
                tracing::debug!("workspace lookup failed: {error:#}");
                StartupContext::Unavailable(format!(
                    "Prep Cook server not reachable at {}",
                    self.base_url.get()
                ))
            }
        }
    }

    async fn try_fetch_attempt_context(&self, path: &Path) -> anyhow::Result<ContextLookup> {
        let url = self.url("/api/containers/attempt-context");
        let query = ContainerQuery {
            container_ref: path.to_string_lossy().to_string(),
        };

        let response = tokio::time::timeout(
            std::time::Duration::from_millis(500),
            self.client.get(&url).query(&query).send(),
        )
        .await
        .context("Timed out fetching /api/containers/attempt-context")?
        .context("Failed to fetch /api/containers/attempt-context")?;

        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(ContextLookup::NotAWorkspace);
        }
        if !status.is_success() {
            return Ok(ContextLookup::Failed(status));
        }

        let api_response: ApiResponseEnvelope<WorkspaceContext> = response
            .json()
            .await
            .context("Failed to parse /api/containers/attempt-context response")?;

        match api_response.data.filter(|_| api_response.success) {
            Some(ctx) => Ok(ContextLookup::Found(Box::new(ctx))),
            None => Ok(ContextLookup::NotAWorkspace),
        }
    }

    async fn build_mcp_context_from_workspace_context(&self, ctx: &WorkspaceContext) -> McpContext {
        let workspace_repos: Vec<McpRepoContext> = ctx
            .workspace_repos
            .iter()
            .map(|rwb| McpRepoContext {
                repo_id: rwb.repo.id,
                repo_name: rwb.repo.name.clone(),
                target_branch: rwb.target_branch.clone(),
            })
            .collect();

        let workspace_id = ctx.workspace.id;
        let workspace_branch = ctx.workspace.branch.clone();
        let orchestrator_session_id = if matches!(self.mode(), McpMode::Orchestrator) {
            ctx.orchestrator_session_id
        } else {
            None
        };

        // One attempt only: the MCP `initialize` handshake waits on this, and
        // tools now re-resolve on demand — so a slow board costs a snapshot
        // that gets filled in later, not a stalled startup.
        let link = self.fetch_remote_workspace_context(workspace_id, 1).await;
        if let Some(link) = link {
            self.remote_link.write().await.link = Some(link);
        }
        let link = link.unwrap_or_default();

        McpContext {
            organization_id: link.organization_id,
            project_id: link.project_id,
            issue_id: link.issue_id,
            orchestrator_session_id,
            workspace_id,
            workspace_branch,
            workspace_repos,
        }
    }

    /// The workspace's remote linkage, or `None` when the lookup failed in a
    /// way that is worth retrying later. A definitive answer — including "this
    /// workspace has no remote row" — comes back as `Some`, so callers can
    /// cache it and stop asking.
    async fn fetch_remote_workspace_context(
        &self,
        local_workspace_id: Uuid,
        attempts: u32,
    ) -> Option<RemoteLink> {
        let url = self.url(&format!(
            "/api/remote/workspaces/by-local-id/{}",
            local_workspace_id
        ));

        for attempt in 1..=attempts.max(1) {
            match tokio::time::timeout(REMOTE_LINK_TIMEOUT, self.client.get(&url).send()).await {
                Ok(Ok(response)) if response.status().is_success() => {
                    let envelope: ApiResponseEnvelope<api_types::Workspace> =
                        match response.json().await {
                            Ok(envelope) => envelope,
                            // Treated as transient: retrying is cheap, and
                            // classifying it as "not linked" would cache a lie.
                            Err(error) => {
                                tracing::warn!(
                                    attempt,
                                    %error,
                                    "remote workspace lookup returned an unreadable body"
                                );
                                continue;
                            }
                        };
                    let success = envelope.success;
                    let Some(remote_ws) = envelope.data.filter(|_| success) else {
                        tracing::warn!(
                            %local_workspace_id,
                            "workspace has no remote counterpart — agent runs without \
                             project/issue context"
                        );
                        return Some(RemoteLink::default());
                    };
                    let project_id = remote_ws.project_id;
                    // A project always has an organization, so `None` here is a
                    // failed lookup, not an answer — reporting it as one would
                    // cache "no organization" permanently.
                    let Some(organization_id) = self.fetch_remote_organization_id(project_id).await
                    else {
                        tracing::warn!(%project_id, "organization lookup failed; will retry");
                        return None;
                    };
                    return Some(RemoteLink {
                        project_id: Some(project_id),
                        issue_id: remote_ws.issue_id,
                        organization_id: Some(organization_id),
                    });
                }
                // 4xx: the answer will not change on a retry.
                Ok(Ok(response)) if response.status().is_client_error() => {
                    tracing::warn!(
                        %local_workspace_id,
                        status = %response.status(),
                        "remote workspace lookup rejected — no project/issue context"
                    );
                    return Some(RemoteLink::default());
                }
                Ok(Ok(response)) => tracing::warn!(
                    attempt,
                    status = %response.status(),
                    "remote workspace lookup failed"
                ),
                Ok(Err(error)) => {
                    tracing::warn!(attempt, %error, "remote workspace lookup errored")
                }
                Err(_) => tracing::warn!(
                    attempt,
                    timeout_secs = REMOTE_LINK_TIMEOUT.as_secs(),
                    "remote workspace lookup timed out"
                ),
            }
            if attempt < attempts {
                tokio::time::sleep(Duration::from_millis(500 * u64::from(attempt))).await;
            }
        }

        tracing::warn!(
            %local_workspace_id,
            "remote workspace lookup gave up; tools will retry on demand"
        );
        None
    }

    /// Remote linkage for the current workspace: `Some` once known (an empty
    /// `RemoteLink` meaning "genuinely not linked"), `None` while the board
    /// cannot be reached.
    ///
    /// Only known answers are cached. A slow remote at startup used to leave
    /// the agent permanently without an issue or project — the snapshot was
    /// taken once and never revisited — which surfaced as tools reporting "no
    /// linked issue" for an issue that was linked all along.
    pub(crate) async fn remote_link(&self) -> Option<RemoteLink> {
        {
            let cache = self.remote_link.read().await;
            if let Some(link) = cache.link {
                return Some(link);
            }
            if let Some(failed_at) = cache.failed_at
                && failed_at.elapsed() < REMOTE_LINK_RETRY_COOLDOWN
            {
                return None;
            }
        }
        let Some(workspace_id) = self.context.as_ref().map(|ctx| ctx.workspace_id) else {
            // No workspace at all (global mode): nothing to link, definitively.
            return Some(RemoteLink::default());
        };
        match self
            .fetch_remote_workspace_context(workspace_id, REMOTE_LINK_ATTEMPTS)
            .await
        {
            Some(link) => {
                let mut cache = self.remote_link.write().await;
                cache.link = Some(link);
                cache.failed_at = None;
                Some(link)
            }
            None => {
                self.remote_link.write().await.failed_at = Some(Instant::now());
                None
            }
        }
    }

    async fn fetch_remote_organization_id(&self, project_id: Uuid) -> Option<Uuid> {
        let url = self.url(&format!("/api/remote/projects/{}", project_id));

        let response = tokio::time::timeout(REMOTE_LINK_TIMEOUT, self.client.get(&url).send())
            .await
            .ok()?
            .ok()?;

        if !response.status().is_success() {
            return None;
        }

        let api_response: ApiResponseEnvelope<api_types::Project> = response.json().await.ok()?;
        let project = api_response.data?;
        Some(project.organization_id)
    }
}

#[cfg(test)]
mod base_url_tests {
    use super::{BaseUrl, join_url};

    #[test]
    fn url_joins_with_one_separator() {
        assert_eq!(
            join_url("http://localhost:3000/", "/api/issues"),
            "http://localhost:3000/api/issues"
        );
        assert_eq!(
            join_url("http://localhost:3000", "api/issues"),
            "http://localhost:3000/api/issues"
        );
    }

    #[test]
    fn requests_follow_a_rebound_server() {
        let url: BaseUrl = "http://localhost:3000".into();
        assert!(url.set("http://localhost:4100".to_string()));
        assert_eq!(
            join_url(&url.get(), "api/issues"),
            "http://localhost:4100/api/issues"
        );
        // Writing the same URL is not a move, so the watcher stays quiet.
        assert!(!url.set("http://localhost:4100".to_string()));
    }

    #[test]
    fn clones_share_one_url() {
        let url: BaseUrl = "http://localhost:3000".into();
        let clone = url.clone();
        clone.set("http://localhost:4100".to_string());
        assert_eq!(url.get(), "http://localhost:4100");
    }
}

#[cfg(test)]
mod startup_tests {
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    use super::{McpMode, McpServer};

    /// A server that answers every request with `status`.
    async fn answering(status: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let mut buf = [0u8; 4096];
                let _ = socket.read(&mut buf).await;
                let body = r#"{"success":false,"data":null,"message":"No Prep Cook workspace contains this path."}"#;
                let response = format!(
                    "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = socket.write_all(response.as_bytes()).await;
            }
        });
        url
    }

    fn global_tools_without_context() -> Vec<String> {
        let mut names: Vec<String> = McpServer::global_mode_router()
            .map
            .keys()
            .filter(|name| *name != "get_context")
            .map(|name| name.to_string())
            .collect();
        names.sort();
        names
    }

    #[tokio::test]
    async fn outside_a_workspace_task_mode_starts_with_the_global_tools() {
        let url = answering("404 Not Found").await;
        let server = McpServer::new_orchestrator(&url).init().await.unwrap();
        assert!(matches!(server.mode(), McpMode::Global));
        assert_eq!(server.tool_names(), global_tools_without_context());
    }

    #[tokio::test]
    async fn an_unreachable_server_starts_reduced_too() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        drop(listener);
        let server = McpServer::new_orchestrator(&url).init().await.unwrap();
        assert!(matches!(server.mode(), McpMode::Global));
        assert_eq!(server.tool_names(), global_tools_without_context());
    }
}
