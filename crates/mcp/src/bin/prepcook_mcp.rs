use mcp::task_server::McpServer;
use rmcp::{ServiceExt, transport::stdio};
use tracing_subscriber::{EnvFilter, prelude::*};
use utils::{
    port_file::read_port_file,
    sentry::{self as sentry_utils, SentrySource, sentry_layer},
};

const HOST_ENV: &str = "MCP_HOST";
const PORT_ENV: &str = "MCP_PORT";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum McpLaunchMode {
    Global,
    Orchestrator,
    /// Lean playbook-agent tool set (see `McpServer::agent_mode_router`).
    Agent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LaunchConfig {
    mode: McpLaunchMode,
}

fn main() -> anyhow::Result<()> {
    let launch_config = resolve_launch_config()?;

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async move {
            let version = env!("CARGO_PKG_VERSION");
            init_process_logging("prepcook-mcp", version);

            let backend = resolve_base_url("prepcook-mcp").await;
            let LaunchConfig { mode } = launch_config;

            let server = match mode {
                McpLaunchMode::Global => McpServer::new_global(&backend.url),
                McpLaunchMode::Orchestrator => McpServer::new_orchestrator(&backend.url),
                McpLaunchMode::Agent => McpServer::new_agent(&backend.url),
            };

            let server = server.init().await?;
            if let Some(host) = backend.refresh_host {
                server.spawn_port_watcher(host);
            }

            let service = server.serve(stdio()).await.map_err(|error| {
                tracing::error!("serving error: {:?}", error);
                error
            })?;

            service.waiting().await?;
            Ok(())
        })
}

fn resolve_launch_config() -> anyhow::Result<LaunchConfig> {
    resolve_launch_config_from_iter(std::env::args().skip(1))
}

fn resolve_launch_config_from_iter<I>(mut args: I) -> anyhow::Result<LaunchConfig>
where
    I: Iterator<Item = String>,
{
    let mut mode = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--mode" => {
                mode = Some(args.next().ok_or_else(|| {
                    anyhow::anyhow!(
                        "Missing value for --mode. Expected 'global', 'orchestrator' or 'agent'"
                    )
                })?);
            }
            "-h" | "--help" => {
                println!("Usage: prepcook-mcp --mode <global|orchestrator|agent>");
                std::process::exit(0);
            }
            _ => {
                return Err(anyhow::anyhow!(
                    "Unknown argument '{arg}'. Usage: prepcook-mcp --mode <global|orchestrator|agent>"
                ));
            }
        }
    }

    let mode = match mode
        .as_deref()
        .unwrap_or("global")
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "global" => McpLaunchMode::Global,
        "orchestrator" => McpLaunchMode::Orchestrator,
        "agent" => McpLaunchMode::Agent,
        value => {
            return Err(anyhow::anyhow!(
                "Invalid MCP mode '{value}'. Expected 'global', 'orchestrator' or 'agent'"
            ));
        }
    };

    Ok(LaunchConfig { mode })
}

/// Where this bridge talks to, and whether that target may move.
struct Backend {
    url: String,
    /// Host to rebuild the URL against when the port file changes. `None` when
    /// the environment pinned the target, which nothing should override.
    refresh_host: Option<String>,
}

/// Where to reach the server. Infallible on purpose: with no port to go on
/// (the server has never run on this machine) the bridge still starts, against
/// a placeholder the port watcher replaces once a server publishes its port,
/// and the startup workspace lookup reports the server unreachable.
async fn resolve_base_url(log_prefix: &str) -> Backend {
    if let Ok(url) = std::env::var("PREPCOOK_BACKEND_URL") {
        tracing::debug!(
            "[{}] Using backend URL from PREPCOOK_BACKEND_URL: {}",
            log_prefix,
            url
        );
        return Backend {
            url,
            refresh_host: None,
        };
    }

    // `localhost`, not `127.0.0.1`: the server binds "localhost:0", which on
    // modern macOS resolves to ::1 (IPv6) first. Dialing the IPv4 literal
    // then hits a socket nothing is listening on.
    let host = std::env::var(HOST_ENV)
        .or_else(|_| std::env::var("HOST"))
        .unwrap_or_else(|_| "localhost".to_string());

    // A port from the environment is fixed for the life of the process; one read
    // from the port file is a snapshot of a server that can rebind, so record
    // the host and let the caller follow it.
    let pinned = std::env::var(PORT_ENV)
        .or_else(|_| std::env::var("BACKEND_PORT"))
        .or_else(|_| std::env::var("PORT"))
        .ok()
        .and_then(|port_str| match port_str.trim().parse::<u16>() {
            Ok(port) => Some(port),
            Err(error) => {
                tracing::debug!("[{log_prefix}] Ignoring invalid port '{port_str}': {error}");
                None
            }
        });
    let (port, refresh_host) = match pinned {
        Some(port) => (port, None),
        None => match read_port_file("prepcook").await {
            Ok(port) => (port, Some(host.clone())),
            Err(error) => {
                tracing::debug!("[{log_prefix}] No port file: {error}");
                (0, Some(host.clone()))
            }
        },
    };

    let url = format!("http://{}:{}", host, port);
    tracing::debug!("[{}] Using backend URL: {}", log_prefix, url);
    Backend { url, refresh_host }
}

fn init_process_logging(log_prefix: &str, version: &str) {
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    sentry_utils::init_once(SentrySource::Mcp);

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stderr()))
                // The agent keeps an MCP server's stderr as its log; INFO keeps
                // that to the line saying how the bridge started.
                // `PREPCOOK_MCP_LOG` rather than `RUST_LOG`, which the bridge
                // would otherwise inherit from whatever launched the agent.
                // rmcp's own INFO lines (handshake, peer info, stream closed)
                // say nothing a user needs.
                .with_filter(
                    EnvFilter::try_from_env("PREPCOOK_MCP_LOG")
                        .unwrap_or_else(|_| EnvFilter::new("info,rmcp=warn")),
                ),
        )
        .with(sentry_layer())
        .init();

    tracing::debug!(
        "[{}] Starting Prep Cook MCP server version {}...",
        log_prefix,
        version
    );
}

#[cfg(test)]
mod tests {
    use super::{LaunchConfig, McpLaunchMode, resolve_launch_config_from_iter};

    #[test]
    fn orchestrator_mode_does_not_require_session_id() {
        let config = resolve_launch_config_from_iter(
            ["--mode".to_string(), "orchestrator".to_string()].into_iter(),
        )
        .expect("config should parse");

        assert_eq!(
            config,
            LaunchConfig {
                mode: McpLaunchMode::Orchestrator
            }
        );
    }

    #[test]
    fn session_id_flag_is_rejected() {
        let error = resolve_launch_config_from_iter(
            [
                "--mode".to_string(),
                "orchestrator".to_string(),
                "--session-id".to_string(),
                "x".to_string(),
            ]
            .into_iter(),
        )
        .expect_err("session id flag should be rejected");

        assert!(
            error
                .to_string()
                .contains("Unknown argument '--session-id'")
        );
    }
}
