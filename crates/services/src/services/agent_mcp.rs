//! The MCP servers the agent sessions Prep Cook launches get, kept in one file
//! in the app data directory that each executor points its agent at per
//! invocation (see `executors::mcp_config::session_mcp_config_path`).
//!
//! The user's own agent config is never written: an entry there would load in
//! every agent session on the machine, including the user's own work in other
//! repositories, not just the ones Prep Cook launched. Settings → MCP servers
//! edits that config only when the user asks it to.

use executors::mcp_config::{
    prepcook_session_server, session_mcp_config_path, write_session_mcp_config,
};
use serde_json::{Map, Value};
use uuid::Uuid;

use crate::services::{
    config::Config,
    evidence::{self, McpServerEntry},
    secrets,
};

/// Rewrites the session MCP config from the current settings. Called at
/// startup and whenever the evidence sources change; sessions launched
/// afterwards pick the new file up.
pub fn write_session_config(config: &Config) {
    let servers = session_servers(config);
    if !servers.contains_key("prepcook") {
        tracing::warn!(
            "No prepcook-mcp binary next to this server or in ~/.local/bin; agent \
             sessions will run without the Prep Cook tools until one is installed."
        );
    }
    let path = session_mcp_config_path();
    match write_session_mcp_config(&path, &servers) {
        Ok(()) => tracing::debug!("Wrote the agent sessions' MCP config to {}", path.display()),
        Err(error) => tracing::warn!(
            ?error,
            "Could not write the agent sessions' MCP config to {}",
            path.display()
        ),
    }
}

fn session_servers(config: &Config) -> Map<String, Value> {
    let mut servers = Map::new();
    if let Some(prepcook) = prepcook_session_server() {
        servers.insert("prepcook".to_string(), prepcook);
    }
    for entry in evidence_servers(config) {
        servers.entry(entry.name).or_insert(entry.server);
    }
    servers
}

/// Every evidence source's MCP server: the global sources' and every board's.
/// The file is shared by all sessions, so a board's own source reaches every
/// session; one server per distinct entry.
fn evidence_servers(config: &Config) -> Vec<McpServerEntry> {
    let store = secrets::load();
    let boards = config
        .orchestrator
        .project_defaults
        .keys()
        .filter_map(|key| Uuid::parse_str(key).ok())
        .map(Some);
    let mut servers: Vec<McpServerEntry> = Vec::new();
    for board in std::iter::once(None).chain(boards) {
        let settings = evidence::resolve_sources(&config.evidence_sources(board), &[], &store);
        for entry in evidence::build(&settings, None).mcp_servers() {
            evidence::add_mcp_server(&mut servers, entry);
        }
    }
    servers
}

/// Whether the session MCP file needs rewriting after a config change.
pub fn servers_changed(old: &Config, new: &Config) -> bool {
    old.evidence != new.evidence
        || old
            .orchestrator
            .project_defaults
            .iter()
            .map(|(id, board)| (id, &board.evidence))
            .ne(new
                .orchestrator
                .project_defaults
                .iter()
                .map(|(id, board)| (id, &board.evidence)))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::json;

    use super::*;
    use crate::services::{
        config::{EvidenceSourceConfig, ProjectAutomationDefaults},
        evidence::EvidenceKind,
    };

    fn sentry(token: &str, host: Option<&str>) -> EvidenceSourceConfig {
        let mut fields = BTreeMap::from([("access_token".to_string(), json!(token))]);
        if let Some(host) = host {
            fields.insert("host".to_string(), json!(host));
        }
        EvidenceSourceConfig {
            kind: EvidenceKind::Sentry,
            label: None,
            fields,
        }
    }

    #[test]
    fn every_sentry_source_gets_a_server_once() {
        let mut config = Config::default();
        config.evidence.sources = vec![
            sentry("global-token-test", None),
            EvidenceSourceConfig {
                kind: EvidenceKind::LogFiles,
                label: None,
                fields: BTreeMap::from([("paths".to_string(), json!("logs/app.log"))]),
            },
        ];
        let mut board = ProjectAutomationDefaults::default();
        board.evidence.sources = vec![sentry("board-token-test", Some("sentry.example.com"))];
        config
            .orchestrator
            .project_defaults
            .insert(Uuid::new_v4().to_string(), board);

        let servers = evidence_servers(&config);
        let names: Vec<&str> = servers.iter().map(|s| s.name.as_str()).collect();
        // The board also keeps the global source; it is not listed twice.
        assert_eq!(names, ["sentry", "sentry-2"]);
        assert_eq!(
            servers[0].server["env"]["SENTRY_ACCESS_TOKEN"],
            json!("global-token-test")
        );
        assert_eq!(
            servers[1].server["args"][2],
            json!("--host=sentry.example.com")
        );
        assert!(
            session_servers(&config).contains_key("sentry"),
            "the session file carries it"
        );
    }

    #[test]
    fn only_evidence_changes_rewrite_the_file() {
        let old = Config::default();
        let mut new = old.clone();
        new.git_branch_prefix = "other".to_string();
        assert!(!servers_changed(&old, &new));
        new.evidence.sources.push(sentry("t", None));
        assert!(servers_changed(&old, &new));
        let mut board_changed = old.clone();
        board_changed.orchestrator.project_defaults.insert(
            Uuid::new_v4().to_string(),
            ProjectAutomationDefaults::default(),
        );
        assert!(servers_changed(&old, &board_changed));
    }
}
