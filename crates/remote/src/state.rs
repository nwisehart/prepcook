use std::sync::Arc;

use sqlx::PgPool;

use crate::{
    analytics::AnalyticsService,
    auth::{JwtService, OwnerIdentityCache},
    config::RemoteServerConfig,
};

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: RemoteServerConfig,
    pub jwt: Arc<JwtService>,
    pub server_public_base_url: String,
    pub http_client: reqwest::Client,
    owner: OwnerIdentityCache,
    analytics: Option<AnalyticsService>,
}

impl AppState {
    pub fn new(
        pool: PgPool,
        config: RemoteServerConfig,
        jwt: Arc<JwtService>,
        server_public_base_url: String,
        http_client: reqwest::Client,
        analytics: Option<AnalyticsService>,
    ) -> Self {
        Self {
            pool,
            config,
            jwt,
            server_public_base_url,
            http_client,
            owner: OwnerIdentityCache::default(),
            analytics,
        }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub fn config(&self) -> &RemoteServerConfig {
        &self.config
    }

    pub fn jwt(&self) -> Arc<JwtService> {
        Arc::clone(&self.jwt)
    }

    pub(crate) fn owner(&self) -> &OwnerIdentityCache {
        &self.owner
    }

    /// Replaces the owner cache, so a test can supply an owner without a
    /// database.
    #[cfg(test)]
    pub(crate) fn with_owner(mut self, owner: OwnerIdentityCache) -> Self {
        self.owner = owner;
        self
    }

    pub fn analytics(&self) -> Option<&AnalyticsService> {
        self.analytics.as_ref()
    }
}
