use std::{net::SocketAddr, sync::Arc};

use anyhow::Context;
use secrecy::ExposeSecret;
use tracing::instrument;

use crate::{
    AppState,
    analytics::{AnalyticsConfig, AnalyticsService},
    auth::JwtService,
    config::RemoteServerConfig,
    db::{self, organizations::OrganizationRepository},
    routes,
};

pub struct Server;

impl Server {
    #[instrument(
        name = "remote_server",
        skip(config),
        fields(listen_addr = %config.listen_addr)
    )]
    pub async fn run(config: RemoteServerConfig) -> anyhow::Result<()> {
        let pool = db::create_pool(&config.database_url)
            .await
            .context("failed to create postgres pool")?;

        db::migrate(&pool)
            .await
            .context("failed to run database migrations")?;

        if let Some(password) = config.electric_role_password.as_ref() {
            db::ensure_electric_role_password(&pool, password.expose_secret())
                .await
                .context("failed to set electric role password")?;
        }

        if !config.electric_publication_names.is_empty() {
            db::electric_publications::ensure_electric_publications(
                &pool,
                &config.electric_publication_names,
            )
            .await
            .context("failed to sync Electric publications")?;
        }

        let organization = OrganizationRepository::new(&pool)
            .ensure_default_organization()
            .await
            .context("failed to ensure the default organization")?;
        tracing::info!(organization_id = %organization.id, "organization ready");

        let auth_config = config.auth.clone();
        let jwt = Arc::new(JwtService::new(auth_config.jwt_secret().clone()));

        let server_public_base_url = config.server_public_base_url.clone().ok_or_else(|| {
            anyhow::anyhow!(
                "SERVER_PUBLIC_BASE_URL is not set. Please set it in your .env.remote file."
            )
        })?;

        let http_client = reqwest::Client::builder()
            .user_agent("PrepCookRemote/1.0")
            .build()
            .context("failed to create HTTP client")?;

        let analytics = match AnalyticsConfig::from_env() {
            Some(analytics_config) => {
                tracing::info!("PostHog analytics configured");
                Some(AnalyticsService::new(analytics_config))
            }
            None => {
                tracing::info!(
                    "PostHog analytics not configured (POSTHOG_API_KEY and/or POSTHOG_API_ENDPOINT not set)"
                );
                None
            }
        };

        let state = AppState::new(
            pool.clone(),
            config.clone(),
            jwt,
            server_public_base_url,
            http_client,
            analytics,
        );

        let router = routes::router(state);
        let addr: SocketAddr = config
            .listen_addr
            .parse()
            .context("listen address is invalid")?;
        let tcp_listener = tokio::net::TcpListener::bind(addr)
            .await
            .context("failed to bind tcp listener")?;

        tracing::info!(%addr, "shared sync server listening");

        let make_service = router.into_make_service();

        axum::serve(tcp_listener, make_service)
            .await
            .context("shared sync server failure")?;

        Ok(())
    }
}
