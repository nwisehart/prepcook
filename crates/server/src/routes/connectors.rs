//! Connector visibility: reports the MCP servers Claude Code loads into
//! every agent session and their health, by running `claude mcp list` inside
//! the app's own environment (same `CLAUDE_CONFIG_DIR` the agents use).
//! Surfaced in Settings → Connectors so "unauthorized connector" stops being
//! something discovered inside an RCA. The probe is read-only; fixing auth is
//! guided but manual (remote-server OAuth runs via `/mcp` in an interactive
//! `claude` session; claude.ai account connectors via claude.ai settings).

use axum::{Router, extract::State, response::Json as ResponseJson, routing::get};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use utils::response::ApiResponse;

use crate::DeploymentImpl;

/// Hard cap on the probe; `claude mcp list` health-checks every server.
const PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

#[derive(Debug, Serialize, Deserialize, TS)]
pub struct ConnectorStatus {
    pub name: String,
    /// The remainder of the status line as printed (command/URL + health).
    pub detail: String,
    /// Parsed health: Some(true)=connected, Some(false)=failed/needs auth,
    /// None=could not classify (line shown as-is).
    pub healthy: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
pub struct ClaudeConnectorsResponse {
    /// False when the `claude` binary was missing or the probe errored;
    /// `raw` then carries the error text.
    pub available: bool,
    pub connectors: Vec<ConnectorStatus>,
    pub raw: String,
}

fn classify(detail: &str) -> Option<bool> {
    let lowered = detail.to_ascii_lowercase();
    if lowered.contains("fail")
        || lowered.contains("error")
        || lowered.contains("needs auth")
        || lowered.contains("unauthorized")
        || lowered.contains("not connected")
        || lowered.contains("disconnected")
    {
        return Some(false);
    }
    if lowered.contains("connected") || lowered.contains('✓') || lowered.contains('✔') {
        return Some(true);
    }
    None
}

/// Tolerant parse of `claude mcp list` output: one server per line as
/// `name: <command/url> - <health>`. Unknown lines are skipped (the raw
/// output ships alongside for humans).
fn parse_mcp_list(stdout: &str) -> Vec<ConnectorStatus> {
    stdout
        .lines()
        .filter_map(|line| {
            let (name, detail) = line.split_once(':')?;
            let name = name.trim();
            // Server names are single identifiers; prose lines are not.
            if name.is_empty() || name.contains(' ') {
                return None;
            }
            let detail = detail.trim().to_string();
            Some(ConnectorStatus {
                name: name.to_string(),
                healthy: classify(&detail),
                detail,
            })
        })
        .collect()
}

async fn get_claude_connectors(
    State(_deployment): State<DeploymentImpl>,
) -> ResponseJson<ApiResponse<ClaudeConnectorsResponse>> {
    let program = utils::shell::resolve_executable_path_blocking("claude")
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_else(|| "claude".to_string());

    let mut command = tokio::process::Command::new(&program);
    command
        .arg("mcp")
        .arg("list")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);

    let output = match command.spawn() {
        Ok(child) => match tokio::time::timeout(PROBE_TIMEOUT, child.wait_with_output()).await {
            Ok(Ok(output)) => output,
            Ok(Err(error)) => {
                return ResponseJson(ApiResponse::success(ClaudeConnectorsResponse {
                    available: false,
                    connectors: Vec::new(),
                    raw: format!("probe errored: {error}"),
                }));
            }
            Err(_) => {
                return ResponseJson(ApiResponse::success(ClaudeConnectorsResponse {
                    available: false,
                    connectors: Vec::new(),
                    raw: "probe timed out after 30s".to_string(),
                }));
            }
        },
        Err(error) => {
            return ResponseJson(ApiResponse::success(ClaudeConnectorsResponse {
                available: false,
                connectors: Vec::new(),
                raw: format!("could not run `claude` ({program}): {error}"),
            }));
        }
    };

    let raw = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    ResponseJson(ApiResponse::success(ClaudeConnectorsResponse {
        available: output.status.success(),
        connectors: parse_mcp_list(&String::from_utf8_lossy(&output.stdout)),
        raw,
    }))
}

pub fn router() -> Router<DeploymentImpl> {
    Router::new().route("/claude-connectors", get(get_claude_connectors))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mixed_health_lines() {
        let out = "Checking MCP server health...\n\n\
                   prepcook: /usr/local/bin/prepcook-mcp - ✓ Connected\n\
                   sentry: npx -y @sentry/mcp-server@latest - ✗ Failed to connect\n";
        let parsed = parse_mcp_list(out);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].name, "prepcook");
        assert_eq!(parsed[0].healthy, Some(true));
        assert_eq!(parsed[1].name, "sentry");
        assert_eq!(parsed[1].healthy, Some(false));
    }
}
