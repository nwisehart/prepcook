use std::{collections::HashMap, sync::Arc};

use api_types::BoardStatus;
use async_trait::async_trait;
use client_info::ClientInfo;
use db::DBService;
use deployment::{Deployment, DeploymentError, RemoteClientNotConfigured};
use executors::profile::ExecutorConfigs;
use git::GitService;
use preview_proxy::PreviewProxyService;
use remote_info::RemoteInfo;
use services::services::{
    analytics::{AnalyticsConfig, AnalyticsContext, AnalyticsService, generate_user_id},
    approvals::{Approvals, decisions::Decisions},
    board_secret,
    cards::Cards,
    config::{Config, load_config_from_file, save_config_to_file},
    container::ContainerService,
    events::EventService,
    file::FileService,
    file_search::FileSearchCache,
    filesystem::FilesystemService,
    lavish_relay::LavishRelayService,
    notifications::NotificationSource,
    pr_monitor::PrMonitorService,
    queued_message::QueuedMessageService,
    remote_client::RemoteClient,
    repo::RepoService,
    scripts::{RunnerSettings, ScriptRunner},
    seat_monitor::SeatMonitorService,
    stage_orchestrator::StageOrchestratorService,
    tracker::TrackerSource,
    tracker_watcher::TrackerWatcherService,
};
use tokio::sync::{Notify, RwLock};
use tokio_util::sync::CancellationToken;
use utils::{assets::config_path, msg_store::MsgStore};
use workspace_manager::WorkspaceManager;
use worktree_manager::WorktreeManager;

use crate::{container::LocalContainerService, pty::PtyService};
mod command;
pub mod container;
mod copy;
pub mod pty;

#[derive(Clone)]
pub struct LocalDeployment {
    config: Arc<RwLock<Config>>,
    user_id: String,
    db: DBService,
    workspace_manager: WorkspaceManager,
    analytics: Option<AnalyticsService>,
    container: LocalContainerService,
    git: GitService,
    repo: RepoService,
    file: FileService,
    filesystem: FilesystemService,
    events: EventService,
    file_search_cache: Arc<FileSearchCache>,
    approvals: Approvals,
    queued_message_service: QueuedMessageService,
    remote_client: Result<RemoteClient, RemoteClientNotConfigured>,
    client_info: ClientInfo,
    remote_info: RemoteInfo,
    preview_proxy: PreviewProxyService,
    pty: PtyService,
    pr_sync_notify: Arc<Notify>,
    script_runner: ScriptRunner,
    tracker: TrackerSource,
    decisions: Decisions,
    cards: Cards,
}

#[async_trait]
impl Deployment for LocalDeployment {
    async fn new(_shutdown: CancellationToken) -> Result<Self, DeploymentError> {
        // Run one-time process logs migration from DB to filesystem
        services::services::execution_process::migrate_execution_logs_to_files()
            .await
            .map_err(|e| DeploymentError::Other(anyhow::anyhow!("Migration failed: {}", e)))?;

        let mut raw_config = load_config_from_file(&config_path()).await;

        let profiles = ExecutorConfigs::get_cached();
        if !raw_config.onboarding_acknowledged
            && let Ok(recommended_executor) = profiles.get_recommended_executor_profile().await
        {
            raw_config.executor_profile = recommended_executor;
        }

        // Check if app version has changed and set release notes flag
        {
            let current_version = utils::version::APP_VERSION;
            let stored_version = raw_config.last_app_version.as_deref();

            if stored_version != Some(current_version) {
                // Show release notes only if this is an upgrade (not first install)
                raw_config.show_release_notes = stored_version.is_some();
                raw_config.last_app_version = Some(current_version.to_string());
            }
        }

        // Always save config (may have been migrated or version updated)
        save_config_to_file(&raw_config, &config_path()).await?;

        if let Some(workspace_dir) = &raw_config.workspace_dir {
            let path = utils::path::expand_tilde(workspace_dir);
            WorktreeManager::set_workspace_dir_override(path);
        }

        {
            let pool = &raw_config.worktree_pool;
            let root = pool
                .root
                .as_deref()
                .map(str::trim)
                .filter(|root| !root.is_empty())
                .map(utils::path::expand_tilde);
            WorktreeManager::set_worktree_pool_config(pool.enabled, pool.max_trees, root);
        }

        let config = Arc::new(RwLock::new(raw_config));
        let user_id = generate_user_id();
        let analytics = AnalyticsConfig::new().map(AnalyticsService::new);
        let git = GitService::new();
        let repo = RepoService::new();
        let msg_stores = Arc::new(RwLock::new(HashMap::new()));
        let filesystem = FilesystemService::new();

        // Create shared components for EventService
        let events_msg_store = Arc::new(MsgStore::new());
        let events_entry_count = Arc::new(RwLock::new(0));

        // Create DB with event hooks
        let db = {
            let hook = EventService::create_hook(
                events_msg_store.clone(),
                events_entry_count.clone(),
                DBService::new().await?, // Temporary DB service for the hook
            );
            DBService::new_with_after_connect(hook).await?
        };

        let file = FileService::new(db.clone().pool)?;
        {
            let file_service = file.clone();
            tokio::spawn(async move {
                tracing::info!("Starting orphaned file cleanup...");
                if let Err(e) = file_service.delete_orphaned_files().await {
                    tracing::error!("Failed to clean up orphaned files: {}", e);
                }
            });
        }

        let approvals = Approvals::new();
        let queued_message_service = QueuedMessageService::new();

        let api_base = std::env::var("PREPCOOK_SHARED_API_BASE")
            .ok()
            .or_else(|| option_env!("PREPCOOK_SHARED_API_BASE").map(|s| s.to_string()));
        let remote_info = RemoteInfo::new();
        if let Some(api_base) = api_base.clone() {
            remote_info
                .set_api_base(api_base)
                .expect("api_base already set");
        }

        // Server-side calls may use a different base than the browser:
        // PREPCOOK_SHARED_API_BASE is what the frontend gets (the HTTPS/HTTP2
        // endpoint, whose Caddy-internal cert the browser trusts via the
        // system keychain), but rustls in this process does not trust that
        // CA — so server-to-server traffic (orchestrator, importer, MCP
        // proxies) goes over the plain-HTTP endpoint when one is provided.
        let internal_api_base = std::env::var("PREPCOOK_INTERNAL_API_BASE")
            .ok()
            .or_else(|| remote_info.get_api_base());
        let board_secret = match board_secret::resolve(api_base.as_deref()) {
            Some((secret, source)) => {
                tracing::info!("board secret read from {source}");
                Some(secret)
            }
            None => {
                if api_base.is_some() {
                    tracing::warn!(
                        "no board secret: run `prepcook up`, or set {} for a board on another machine",
                        board_secret::BOARD_SECRET_NAME
                    );
                }
                None
            }
        };
        let remote_client = match internal_api_base {
            Some(url) => match RemoteClient::new(&url, board_secret) {
                Ok(client) => {
                    tracing::info!("Remote client initialized with URL: {}", url);
                    Ok(client)
                }
                Err(e) => {
                    tracing::error!(?e, "failed to create remote client");
                    Err(RemoteClientNotConfigured)
                }
            },
            None => {
                tracing::info!("PREPCOOK_SHARED_API_BASE not set; remote features disabled");
                Err(RemoteClientNotConfigured)
            }
        };

        let client_info = ClientInfo::new();
        let preview_proxy = PreviewProxyService::new();

        // We need to make analytics accessible to the ContainerService
        // TODO: Handle this more gracefully
        let analytics_ctx = analytics.as_ref().map(|s| AnalyticsContext {
            user_id: user_id.clone(),
            analytics_service: s.clone(),
        });
        let workspace_manager = WorkspaceManager::new(db.clone());
        let script_runner = {
            let app_runs = &config.read().await.app_runs;
            ScriptRunner::new(RunnerSettings::from_config(
                &app_runs.port_range,
                app_runs.max_parallel,
            ))
        };
        let container = LocalContainerService::new(
            db.clone(),
            workspace_manager.clone(),
            msg_stores.clone(),
            config.clone(),
            git.clone(),
            file.clone(),
            analytics_ctx,
            approvals.clone(),
            queued_message_service.clone(),
            remote_client.clone().ok(),
            script_runner.clone(),
        )
        .await;

        let events = EventService::new(db.clone(), events_msg_store, events_entry_count);

        let file_search_cache = Arc::new(FileSearchCache::new());

        let pty = PtyService::new();
        // macOS (and Linux desktop) GUI launches inherit a bare PATH —
        // /usr/bin:/bin:/usr/sbin:/sbin, with no homebrew or npm-global dirs.
        // Agents then can't spawn `claude` OR the `npx` fallback, and every
        // workspace create fails with an opaque 500. Resolving one executable
        // through the login-shell resolver merges the real PATH into this
        // process, so every later spawn (claude, npx, glab, treehouse,
        // lavish-axi) sees it. Terminal launches already have it and hit the
        // fast path.
        match utils::shell::resolve_executable_path("claude").await {
            Some(path) => tracing::info!("Resolved claude for agent launches: {}", path.display()),
            None => tracing::warn!(
                "No `claude` binary on PATH after login-shell refresh — agent launches will \
                 fall back to npx. Install it with `npm i -g @anthropic-ai/claude-code`."
            ),
        }

        let pr_sync_notify = Arc::new(Notify::new());
        {
            let db = db.clone();
            let analytics = analytics.as_ref().map(|s| AnalyticsContext {
                user_id: user_id.clone(),
                analytics_service: s.clone(),
            });
            let container = container.clone();
            let rc = remote_client.clone().ok();
            PrMonitorService::spawn(db, analytics, container, rc, pr_sync_notify.clone()).await;
        }
        let tracker = TrackerSource::new(config.clone());
        let notifications = NotificationSource::new(config.clone());
        let (decisions, cards) = {
            let config = config.clone();
            let db = db.clone();
            let container = container.clone();
            let workspace_manager = workspace_manager.clone();
            let rc = remote_client.clone().ok();
            let orchestrator = StageOrchestratorService::spawn(
                config,
                db,
                container,
                workspace_manager,
                rc,
                tracker.clone(),
                notifications.clone(),
            )
            .await;
            (
                Decisions::new(orchestrator.clone()),
                Cards::new(orchestrator),
            )
        };
        {
            let db = db.clone();
            let container = container.clone();
            LavishRelayService::spawn(db, container, decisions.clone()).await;
        }
        {
            let config = config.clone();
            let rc = remote_client.clone().ok();
            TrackerWatcherService::spawn(config, rc, tracker.clone()).await;
        }
        {
            // Claude subscription-seat health probe, when
            // `orchestrator.seat_monitor` is on: notifies and raises a UI
            // banner when the seat's OAuth login expires.
            let config = config.clone();
            SeatMonitorService::spawn(config, notifications).await;
        }

        let deployment = Self {
            config,
            user_id,
            db,
            workspace_manager,
            analytics,
            container,
            git,
            repo,
            file,
            filesystem,
            events,
            file_search_cache,
            approvals,
            queued_message_service,
            remote_client,
            client_info,
            remote_info,
            preview_proxy,
            pty,
            pr_sync_notify,
            script_runner,
            tracker,
            decisions,
            cards,
        };

        Ok(deployment)
    }

    fn user_id(&self) -> &str {
        &self.user_id
    }

    fn config(&self) -> &Arc<RwLock<Config>> {
        &self.config
    }

    fn db(&self) -> &DBService {
        &self.db
    }

    fn analytics(&self) -> &Option<AnalyticsService> {
        &self.analytics
    }

    fn container(&self) -> &impl ContainerService {
        &self.container
    }

    fn git(&self) -> &GitService {
        &self.git
    }

    fn repo(&self) -> &RepoService {
        &self.repo
    }

    fn file(&self) -> &FileService {
        &self.file
    }

    fn filesystem(&self) -> &FilesystemService {
        &self.filesystem
    }

    fn events(&self) -> &EventService {
        &self.events
    }

    fn file_search_cache(&self) -> &Arc<FileSearchCache> {
        &self.file_search_cache
    }

    fn approvals(&self) -> &Approvals {
        &self.approvals
    }

    fn queued_message_service(&self) -> &QueuedMessageService {
        &self.queued_message_service
    }

    fn client_info(&self) -> &ClientInfo {
        &self.client_info
    }

    fn remote_info(&self) -> &RemoteInfo {
        &self.remote_info
    }

    fn preview_proxy(&self) -> &PreviewProxyService {
        &self.preview_proxy
    }

    fn script_runner(&self) -> &ScriptRunner {
        &self.script_runner
    }

    fn tracker(&self) -> &TrackerSource {
        &self.tracker
    }

    fn decisions(&self) -> &Decisions {
        &self.decisions
    }

    fn cards(&self) -> &Cards {
        &self.cards
    }
}

impl LocalDeployment {
    pub fn workspace_manager(&self) -> &WorkspaceManager {
        &self.workspace_manager
    }

    pub fn remote_client(&self) -> Result<RemoteClient, RemoteClientNotConfigured> {
        self.remote_client.clone()
    }

    /// Whether this server can work with its board: no board URL, no secret,
    /// the board not answering (or refusing the secret), or connected as the
    /// board's owner.
    pub async fn board_status(&self) -> BoardStatus {
        let Ok(client) = self.remote_client() else {
            return BoardStatus::NoBoard;
        };
        if !client.has_secret() {
            return BoardStatus::NoSecret;
        }
        match client.profile().await {
            Ok(profile) => BoardStatus::Connected { profile },
            Err(error) => {
                tracing::debug!(?error, "the board did not answer an authenticated request");
                BoardStatus::NotAnswering
            }
        }
    }

    /// Whether the server holds a board URL and a board secret.
    pub fn has_board_secret(&self) -> bool {
        self.remote_client
            .as_ref()
            .is_ok_and(|client| client.has_secret())
    }

    pub fn pty(&self) -> &PtyService {
        &self.pty
    }

    pub fn trigger_pr_sync(&self) {
        self.pr_sync_notify.notify_one();
    }
}
