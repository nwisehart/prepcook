//! The app Prep Cook stood up for the agent's worktree (see `docs/scripts.md`):
//! where it is, and a way to restart it after a change.

use rmcp::{ErrorData, model::CallToolResult, tool, tool_router};
use serde_json::Value;

use super::{McpServer, ToolError};

impl McpServer {
    /// The worktree this agent works in: `$PREPCOOK_WORKTREE` when Prep Cook set it,
    /// else the directory the MCP server was started in.
    pub(super) fn app_worktree() -> Result<String, ToolError> {
        if let Some(worktree) = std::env::var_os("PREPCOOK_WORKTREE").filter(|w| !w.is_empty()) {
            return Ok(worktree.to_string_lossy().into_owned());
        }
        std::env::current_dir()
            .map(|dir| dir.to_string_lossy().into_owned())
            .map_err(|e| ToolError::new("Cannot tell which worktree this is", Some(e.to_string())))
    }

    async fn current_app_run(&self, worktree: &str) -> Result<Option<Value>, ToolError> {
        let url = self.url("/api/app-runs/current");
        self.send_json::<Option<Value>>(self.client.get(&url).query(&[("worktree", worktree)]))
            .await
    }
}

#[tool_router(router = app_runs_tools_router, vis = "pub")]
impl McpServer {
    #[tool(
        description = "Show the app Prep Cook stood up for this worktree from .prepcook/scripts.toml — served on this machine, or deployed with [app.remote] when the board reproduces against a remote target: its URL, target_kind, port (local only), the names of the variables the seed script wrote (values are in your environment), when it started, and health (whether it answers now). Returns running: false when no app is up."
    )]
    async fn app_status(&self) -> Result<CallToolResult, ErrorData> {
        let worktree = match Self::app_worktree() {
            Ok(worktree) => worktree,
            Err(e) => return Ok(Self::tool_error(e)),
        };
        match self.current_app_run(&worktree).await {
            Ok(Some(run)) => McpServer::success(&serde_json::json!({
                "running": true,
                "run": run,
            })),
            Ok(None) => McpServer::success(&serde_json::json!({
                "running": false,
                "worktree": worktree,
                "message": "No app is running for this worktree. app_restart stands one up.",
            })),
            Err(e) => Ok(Self::tool_error(e)),
        }
    }

    #[tool(
        description = "Stand the app up again from this worktree's .prepcook/scripts.toml, so a candidate change can be tested against a running app: on this machine, setup, build, start, ready, seed; on a remote target, the [app.remote] redeploy (or stop then start), ready, seed. Stands one up when none is running. Returns the app's URL and each step's outcome; on failure, the failing step's output."
    )]
    async fn app_restart(&self) -> Result<CallToolResult, ErrorData> {
        let worktree = match Self::app_worktree() {
            Ok(worktree) => worktree,
            Err(e) => return Ok(Self::tool_error(e)),
        };
        let current = match self.current_app_run(&worktree).await {
            Ok(current) => current,
            Err(e) => return Ok(Self::tool_error(e)),
        };
        let request = match current
            .as_ref()
            .and_then(|run| run.get("run_id"))
            .and_then(Value::as_str)
        {
            Some(run_id) => self
                .client
                .post(self.url(&format!("/api/app-runs/{run_id}/restart"))),
            None => self
                .client
                .post(self.url("/api/app-runs"))
                .json(&serde_json::json!({ "worktree": worktree })),
        };
        match self.send_json::<Value>(request).await {
            Ok(response) => McpServer::success(&response),
            Err(e) => Ok(Self::tool_error(e)),
        }
    }
}
