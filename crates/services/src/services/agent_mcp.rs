//! The MCP servers the agent sessions Prep Cook launches get, kept in one file
//! in the app data directory that each executor points its agent at per
//! invocation (see `executors::mcp_config::session_mcp_config_path`).
//!
//! The user's own agent config is never written: an entry there would load in
//! every agent session on the machine, including the user's own work in other
//! repositories, not just the ones Prep Cook launched. Settings → MCP servers
//! edits that config only when the user asks it to.

use executors::mcp_config::{
    PRECONFIGURED_MCP_SERVERS, prepcook_session_server, session_mcp_config_path,
    write_session_mcp_config,
};
use serde_json::{Map, Value};

use crate::services::{config::IntegrationsConfig, secrets};

/// Rewrites the session MCP config from the current integrations settings.
/// Called at startup and whenever the Sentry settings change; sessions launched
/// afterwards pick the new file up.
pub fn write_session_config(integrations: &IntegrationsConfig) {
    let servers = session_servers(integrations);
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

fn session_servers(integrations: &IntegrationsConfig) -> Map<String, Value> {
    let mut servers = Map::new();
    if let Some(prepcook) = prepcook_session_server() {
        servers.insert("prepcook".to_string(), prepcook);
    }
    if let Some(sentry) = PRECONFIGURED_MCP_SERVERS.get("sentry")
        && sentry_token_configured(integrations)
    {
        servers.insert("sentry".to_string(), sentry_server(sentry, integrations));
    }
    servers
}

/// Whether a Sentry access token is configured anywhere the MCP server can
/// read it: the integrations config, the environment, or the secrets store
/// (whose values every agent session receives as environment variables).
fn sentry_token_configured(integrations: &IntegrationsConfig) -> bool {
    let set = |value: Option<&str>| value.is_some_and(|value| !value.trim().is_empty());
    set(integrations.sentry_access_token.as_deref())
        || set(std::env::var("SENTRY_ACCESS_TOKEN").ok().as_deref())
        || set(secrets::load().get("SENTRY_ACCESS_TOKEN"))
}

/// The `@sentry/mcp-server` stdio entry. A token set in the integrations
/// config is baked into its env; otherwise the entry keeps its
/// `${SENTRY_ACCESS_TOKEN}` placeholder, which Claude Code resolves from the
/// session's environment. A configured `sentry_host` targets a self-hosted
/// instance via `--host`.
fn sentry_server(preconfigured: &Value, integrations: &IntegrationsConfig) -> Value {
    let mut entry = preconfigured.clone();
    if let Some(token) = integrations
        .sentry_access_token
        .as_deref()
        .map(str::trim)
        .filter(|token| !token.is_empty())
        && let Some(env) = entry.get_mut("env").and_then(Value::as_object_mut)
    {
        env.insert(
            "SENTRY_ACCESS_TOKEN".to_string(),
            Value::String(token.to_string()),
        );
    }
    if let Some(host) = integrations
        .sentry_host
        .as_deref()
        .map(str::trim)
        .filter(|host| !host.is_empty())
        && let Some(args) = entry.get_mut("args").and_then(Value::as_array_mut)
    {
        args.push(Value::String(format!("--host={host}")));
    }
    entry
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_token_in_the_integrations_config_counts_as_configured() {
        let integrations = IntegrationsConfig {
            sentry_access_token: Some("token".to_string()),
            ..Default::default()
        };
        assert!(sentry_token_configured(&integrations));
    }

    #[test]
    fn the_sentry_entry_carries_the_configured_token_and_host() {
        let preconfigured = json!({
            "command": "npx",
            "args": ["-y", "@sentry/mcp-server@latest"],
            "env": { "SENTRY_ACCESS_TOKEN": "${SENTRY_ACCESS_TOKEN}" }
        });
        let integrations = IntegrationsConfig {
            sentry_access_token: Some(" token ".to_string()),
            sentry_host: Some("sentry.example.com".to_string()),
            ..Default::default()
        };
        let entry = sentry_server(&preconfigured, &integrations);
        assert_eq!(entry["env"]["SENTRY_ACCESS_TOKEN"], json!("token"));
        assert_eq!(entry["args"][2], json!("--host=sentry.example.com"));
    }
}
