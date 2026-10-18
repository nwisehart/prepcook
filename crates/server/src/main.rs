mod cli;

use anyhow::{self, Error as AnyhowError};
use axum::Router;
use cli::Command;
use deployment::{Deployment, DeploymentError};
use server::{DeploymentImpl, middleware::origin::validate_origin, routes};
use services::services::{board_secret, container::ContainerService};
use sqlx::Error as SqlxError;
use strip_ansi_escapes::strip;
use thiserror::Error;
use tokio_util::sync::CancellationToken;
use tower_http::validate_request::ValidateRequestHeaderLayer;
use tracing_subscriber::{EnvFilter, prelude::*};
use utils::{
    assets::asset_dir,
    port_file::write_port_file_with_proxy,
    sentry::{self as sentry_utils, SentrySource, sentry_layer},
};

#[derive(Debug, Error)]
pub enum PrepCookError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Sqlx(#[from] SqlxError),
    #[error(transparent)]
    Deployment(#[from] DeploymentError),
    #[error(transparent)]
    Other(#[from] AnyhowError),
}

fn main() -> Result<(), PrepCookError> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = match cli::parse(&args) {
        Ok(command) => command,
        Err(message) => {
            eprintln!("{message}\n\n{}", cli::USAGE);
            std::process::exit(2);
        }
    };

    let serve = match &command {
        Command::Secret(rest) => std::process::exit(secret_command(rest)),
        Command::IntegrationsDescribe { json } => std::process::exit(integrations_describe(*json)),
        Command::Help => {
            println!("{}", cli::USAGE);
            return Ok(());
        }
        Command::Serve => ServeOptions::default(),
        Command::Up(options) => {
            if !options.board_only && std::env::var_os("PREPCOOK_SHARED_API_BASE").is_none() {
                // SAFETY: no other thread exists yet; the runtime is built
                // below, after this write.
                unsafe { std::env::set_var("PREPCOOK_SHARED_API_BASE", cli::up::BOARD_URL) };
            }
            let board_base = std::env::var("PREPCOOK_SHARED_API_BASE").unwrap_or_default();
            ServeOptions {
                open_browser: !options.no_browser && !cfg!(debug_assertions),
                print_url: true,
                // A board elsewhere has its own `.env`, on its own machine;
                // its secret reaches this server as PREPCOOK_BOARD_SECRET.
                board_env_file: board_secret::is_local_board(&board_base)
                    .then(|| cli::up::board_dir().join(".env")),
            }
        }
        Command::Down(_) | Command::Status => ServeOptions::default(),
    };

    // Install rustls crypto provider before any TLS operations
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        match command {
            Command::Up(options) => {
                if let Err(message) = cli::up::up(&options).await {
                    eprintln!("{message}");
                    std::process::exit(1);
                }
                if options.board_only {
                    return Ok(());
                }
                run_server(serve).await
            }
            Command::Down(options) => {
                if let Err(message) = cli::up::down(&options).await {
                    eprintln!("{message}");
                    std::process::exit(1);
                }
                Ok(())
            }
            Command::Status => {
                cli::up::status().await;
                Ok(())
            }
            _ => run_server(serve).await,
        }
    })
}

/// How the server announces itself once it is listening.
struct ServeOptions {
    open_browser: bool,
    /// Print the UI's address on stdout (the log line alone is easy to miss
    /// after `prepcook up`'s Docker output).
    print_url: bool,
    /// The `.env` `prepcook up` started the board with: the server's board
    /// secret is its `PREPCOOK_JWT_SECRET`, ahead of any other source.
    board_env_file: Option<std::path::PathBuf>,
}

impl Default for ServeOptions {
    fn default() -> Self {
        // A debug build serves a placeholder page; the UI comes from the Vite
        // dev server, so there is nothing to open.
        Self {
            open_browser: !cfg!(debug_assertions),
            print_url: false,
            board_env_file: None,
        }
    }
}

async fn run_server(options: ServeOptions) -> Result<(), PrepCookError> {
    sentry_utils::init_once(SentrySource::Backend);

    let log_level = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
    let filter_string = format!(
        "warn,server={level},services={level},db={level},executors={level},deployment={level},local_deployment={level},utils={level},codex_core=off",
        level = log_level
    );
    let env_filter = EnvFilter::try_new(&filter_string).expect("Failed to create tracing filter");
    // Also mirror to a file so `./mo` runs leave a diagnosable record after
    // the terminal scrolls away or closes (see utils::log_file).
    let file_layer = utils::log_file::open_log_file("server").map(|file| {
        let file_filter =
            EnvFilter::try_new(&filter_string).expect("Failed to create tracing filter");
        tracing_subscriber::fmt::layer()
            .with_writer(file)
            .with_ansi(false)
            .with_filter(file_filter)
    });
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_filter(env_filter))
        .with(file_layer)
        .with(sentry_layer())
        .init();

    // Create asset directory if it doesn't exist
    if !asset_dir().exists() {
        std::fs::create_dir_all(asset_dir())?;
    }

    // Copy old database to new location for safe downgrades
    let old_db = asset_dir().join("db.sqlite");
    let new_db = asset_dir().join("db.v2.sqlite");
    if !new_db.exists() && old_db.exists() {
        tracing::info!(
            "Copying database to new location: {:?} -> {:?}",
            old_db,
            new_db
        );
        std::fs::copy(&old_db, &new_db).expect("Failed to copy database file");
        tracing::info!("Database copy complete");
    }

    let shutdown_token = CancellationToken::new();

    if let Some(env_file) = &options.board_env_file {
        board_secret::use_board_env_file(env_file.clone());
    }
    let deployment = DeploymentImpl::new(shutdown_token.clone()).await?;
    deployment.update_sentry_scope().await?;
    deployment
        .container()
        .cleanup_orphan_executions()
        .await
        .map_err(DeploymentError::from)?;
    deployment
        .container()
        .backfill_before_head_commits()
        .await
        .map_err(DeploymentError::from)?;
    deployment
        .container()
        .backfill_repo_names()
        .await
        .map_err(DeploymentError::from)?;
    deployment
        .track_if_analytics_allowed("session_start", serde_json::json!({}))
        .await;
    // The MCP servers the agent sessions this server launches get, passed to
    // each agent per invocation; the user's own agent config is not touched.
    let integrations = deployment.config().read().await.integrations.clone();
    services::services::agent_mcp::write_session_config(&integrations);
    let port = std::env::var("BACKEND_PORT")
        .or_else(|_| std::env::var("PORT"))
        .ok()
        .and_then(|s| {
            // Remove any ANSI codes, then turn into String
            let cleaned =
                String::from_utf8(strip(s.as_bytes())).expect("UTF-8 after stripping ANSI");
            cleaned.trim().parse::<u16>().ok()
        })
        .unwrap_or_else(|| {
            tracing::info!("No PORT environment variable set, using port 0 for auto-assignment");
            0
        }); // Use 0 to find free port if no specific port provided

    let proxy_port = std::env::var("PREVIEW_PROXY_PORT")
        .ok()
        .and_then(|s| s.trim().parse::<u16>().ok())
        .unwrap_or(0);

    let host = std::env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_string());

    let main_listener = tokio::net::TcpListener::bind(format!("{host}:{port}")).await?;
    let actual_main_port = main_listener.local_addr()?.port();

    let proxy_listener = tokio::net::TcpListener::bind(format!("{host}:{proxy_port}")).await?;
    let actual_proxy_port = proxy_listener.local_addr()?.port();

    if let Err(e) = write_port_file_with_proxy(actual_main_port, Some(actual_proxy_port)).await {
        tracing::warn!("Failed to write port file: {}", e);
    }

    tracing::info!(
        "Main server on :{}, Preview proxy on :{}",
        actual_main_port,
        actual_proxy_port
    );

    deployment
        .client_info()
        .set_server_addr(main_listener.local_addr()?)
        .expect("client server address already set");
    deployment
        .client_info()
        .set_preview_proxy_port(actual_proxy_port)
        .expect("client preview proxy port already set");

    let app_router = routes::router(deployment.clone());

    if options.print_url {
        println!(
            "Prep Cook is running at http://127.0.0.1:{actual_main_port} (Ctrl+C to stop it; `prepcook down` stops the board)"
        );
    }
    if options.open_browser {
        tracing::info!("Opening browser...");
        let browser_port = actual_main_port;
        tokio::spawn(async move {
            if let Err(e) =
                utils::browser::open_browser(&format!("http://127.0.0.1:{browser_port}")).await
            {
                tracing::warn!(
                    "Failed to open browser automatically: {}. Please open http://127.0.0.1:{} manually.",
                    e,
                    browser_port
                );
            }
        });
    }

    let proxy_router: Router = routes::preview::subdomain_router(deployment.clone())
        .layer(ValidateRequestHeaderLayer::custom(validate_origin));

    let main_shutdown = shutdown_token.clone();
    let proxy_shutdown = shutdown_token.clone();

    let main_server = axum::serve(main_listener, app_router)
        .with_graceful_shutdown(async move { main_shutdown.cancelled().await });
    let proxy_server = axum::serve(proxy_listener, proxy_router)
        .with_graceful_shutdown(async move { proxy_shutdown.cancelled().await });

    let main_handle = tokio::spawn(async move {
        if let Err(e) = main_server.await {
            tracing::error!("Main server error: {}", e);
        }
    });
    let proxy_handle = tokio::spawn(async move {
        if let Err(e) = proxy_server.await {
            tracing::error!("Preview proxy error: {}", e);
        }
    });

    tokio::select! {
        _ = shutdown_signal() => {
            tracing::info!("Shutdown signal received");
        }
        _ = main_handle => {}
        _ = proxy_handle => {}
    }

    shutdown_token.cancel();

    perform_cleanup_actions(&deployment).await;

    Ok(())
}

pub async fn shutdown_signal() {
    // Always wait for Ctrl+C
    let ctrl_c = async {
        if let Err(e) = tokio::signal::ctrl_c().await {
            tracing::error!("Failed to install Ctrl+C handler: {e}");
        }
    };

    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};

        // Try to install SIGTERM handler, but don't panic if it fails
        let terminate = async {
            if let Ok(mut sigterm) = signal(SignalKind::terminate()) {
                sigterm.recv().await;
            } else {
                tracing::error!("Failed to install SIGTERM handler");
                // Fallback: never resolves
                std::future::pending::<()>().await;
            }
        };

        tokio::select! {
            _ = ctrl_c => {},
            _ = terminate => {},
        }
    }

    #[cfg(not(unix))]
    {
        // Only ctrl_c is available, so just await it
        ctrl_c.await;
    }
}

pub async fn perform_cleanup_actions(deployment: &DeploymentImpl) {
    deployment
        .container()
        .kill_all_running_processes()
        .await
        .expect("Failed to cleanly kill running execution processes");
}

const SECRET_USAGE: &str = "usage:\n  prepcook secret set <NAME>    read the value from stdin (prompts on a terminal)\n  prepcook secret list          print the names that have a value\n  prepcook secret unset <NAME>  remove a value from the secrets file";

/// `prepcook secret set|list|unset`. Returns the process exit code.
/// Prints every integration provider's descriptor: JSON for
/// `scripts/gen-config-docs.py`, or a short listing for a person.
fn integrations_describe(json: bool) -> i32 {
    let descriptors = services::services::integrations::descriptors();
    if json {
        match serde_json::to_string_pretty(&descriptors) {
            Ok(text) => {
                println!("{text}");
                0
            }
            Err(e) => {
                eprintln!("{e}");
                1
            }
        }
    } else {
        for (area, providers) in &descriptors {
            println!("{area}");
            for provider in providers {
                println!("  {:<14} {}", provider.kind, provider.summary);
                for field in &provider.fields {
                    let required = if field.required { " (required)" } else { "" };
                    println!("    {}{required}: {}", field.key, field.label);
                }
            }
        }
        0
    }
}

fn secret_command(args: &[String]) -> i32 {
    use services::services::secrets;

    match (args.first().map(String::as_str), args.get(1)) {
        (Some("set"), Some(name)) => {
            let value = match read_secret_value(name) {
                Ok(value) => value,
                Err(e) => {
                    eprintln!("could not read the value: {e}");
                    return 1;
                }
            };
            if value.is_empty() {
                eprintln!("empty value; nothing stored");
                return 1;
            }
            let mut store = secrets::load();
            match store.set(name, &value) {
                Ok(()) => {
                    eprintln!("stored {name} in {}", store.path().display());
                    0
                }
                Err(e) => {
                    eprintln!("{e}");
                    1
                }
            }
        }
        (Some("unset"), Some(name)) => {
            let mut store = secrets::load();
            match store.unset(name) {
                Ok(true) => {
                    eprintln!("removed {name} from {}", store.path().display());
                    0
                }
                Ok(false) => {
                    eprintln!("{name} is not in {}", store.path().display());
                    1
                }
                Err(e) => {
                    eprintln!("{e}");
                    1
                }
            }
        }
        (Some("list"), None) => {
            let store = secrets::load();
            for name in store.names() {
                println!("{name}");
            }
            eprintln!("(file: {})", store.path().display());
            0
        }
        _ => {
            eprintln!("{SECRET_USAGE}");
            2
        }
    }
}

/// One line from stdin, without its line ending. On a terminal it prompts
/// and turns echo off while the value is typed.
fn read_secret_value(name: &str) -> std::io::Result<String> {
    use std::io::{BufRead, IsTerminal, Write};

    let stdin = std::io::stdin();
    let interactive = stdin.is_terminal();
    if interactive {
        eprint!("value for {name}: ");
        std::io::stderr().flush()?;
        set_terminal_echo(false);
    }
    let mut line = String::new();
    let read = stdin.lock().read_line(&mut line);
    if interactive {
        set_terminal_echo(true);
        eprintln!();
    }
    read?;
    Ok(line.trim_end_matches(['\n', '\r']).to_string())
}

#[cfg(unix)]
fn set_terminal_echo(on: bool) {
    let _ = std::process::Command::new("stty")
        .arg(if on { "echo" } else { "-echo" })
        .stdin(std::process::Stdio::inherit())
        .status();
}

#[cfg(not(unix))]
fn set_terminal_echo(_on: bool) {}
