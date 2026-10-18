use axum::{
    Router,
    routing::{IntoMakeService, get},
};
use tower_http::{compression::CompressionLayer, validate_request::ValidateRequestHeaderLayer};

use crate::{DeploymentImpl, middleware};

pub mod approvals;
pub mod auth;
pub mod cards;
pub mod config;
pub mod containers;
pub mod filesystem;
// pub mod github;
pub mod attachments;
pub mod connectors;
pub mod events;
pub mod evidence;
pub mod execution_processes;
pub mod frontend;
pub mod health;
pub mod integrations;
pub mod knowledge;
pub mod lavish_proxy;
pub mod organizations;
pub mod plan_decisions;
pub mod playbooks;
pub mod preview;
pub mod releases;
pub mod remote;
pub mod repo;
pub mod scratch;
pub mod scripts;
pub mod search;
pub mod sessions;
pub mod slack;
pub mod stats;
pub mod tags;
pub mod terminal;
pub mod tracker;
pub mod usage;
pub mod workspaces;

pub fn router(deployment: DeploymentImpl) -> IntoMakeService<Router> {
    let api_routes = Router::new()
        .route("/health", get(health::health_check))
        .merge(config::router())
        .merge(containers::router(&deployment))
        .merge(workspaces::router(&deployment))
        .merge(execution_processes::router(&deployment))
        .merge(tags::router(&deployment))
        .merge(usage::router())
        .merge(auth::router())
        .merge(organizations::router())
        .merge(filesystem::router())
        .merge(repo::router())
        .merge(scripts::router())
        .merge(tracker::router())
        .merge(slack::router())
        .merge(integrations::router())
        .merge(connectors::router())
        .merge(knowledge::router())
        .merge(events::router(&deployment))
        .merge(approvals::router())
        .merge(plan_decisions::router())
        .merge(cards::router())
        .merge(evidence::router())
        .merge(playbooks::router())
        .merge(scratch::router(&deployment))
        .merge(search::router(&deployment))
        .merge(preview::api_router())
        .merge(releases::router())
        .merge(stats::router())
        .merge(sessions::router(&deployment))
        .merge(terminal::router())
        .nest("/remote", remote::router())
        .nest("/attachments", attachments::routes())
        .layer(ValidateRequestHeaderLayer::custom(
            middleware::validate_origin,
        ))
        .layer(axum::middleware::from_fn(middleware::log_server_errors))
        .with_state(deployment);

    Router::new()
        .route("/", get(frontend::serve_frontend_root))
        .route("/{*path}", get(frontend::serve_frontend))
        .nest("/api", api_routes)
        // Outermost so it intercepts before routing: requests addressed to
        // the lavish public hostname (LAVISH_PUBLIC_BASE, hosted runners) are
        // forwarded to the loopback lavish-axi review server. No-op when the
        // env var is unset. All methods, all paths.
        .layer(axum::middleware::from_fn(lavish_proxy::route_lavish_host))
        .layer(CompressionLayer::new())
        .into_make_service()
}
