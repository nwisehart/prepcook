//! Agent login: agent users (admin-created automation identities) exchange
//! their email + secret for a session and a token pair. The board's owner
//! never signs in; it authenticates with owner tokens (see `owner.rs`).

use api_types::{AgentLoginRequest, AgentLoginResponse};
use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};

use crate::{
    AppState,
    db::{auth::AuthSessionRepository, users::UserRepository},
};

/// The `provider` claim of agent refresh tokens. Refresh tokens already
/// issued carry `local`, so the value stays.
pub(crate) const AGENT_AUTH_PROVIDER: &str = "local";

#[derive(Debug, thiserror::Error)]
pub(crate) enum AgentAuthError {
    #[error("invalid credentials")]
    InvalidCredentials,
    #[error("internal error")]
    Internal,
}

pub(crate) async fn login(
    state: &AppState,
    payload: &AgentLoginRequest,
) -> Result<AgentLoginResponse, AgentAuthError> {
    let user_repo = UserRepository::new(state.pool());
    let normalized_email = payload.email.trim().to_ascii_lowercase();

    let credentials = user_repo
        .fetch_agent_credentials(&normalized_email)
        .await
        .map_err(|error| {
            tracing::error!(?error, "failed to fetch agent credentials");
            AgentAuthError::Internal
        })?;
    let Some((agent, stored_hash)) = credentials else {
        return Err(AgentAuthError::InvalidCredentials);
    };
    if crate::routes::agent_users::hash_agent_secret(&payload.password) != stored_hash {
        return Err(AgentAuthError::InvalidCredentials);
    }
    issue_session(state, agent.id).await
}

/// Creates a session + token pair for an already-provisioned agent (no
/// upsert, no organization bootstrap: agents live in the organization an
/// admin created them in).
async fn issue_session(
    state: &AppState,
    user_id: uuid::Uuid,
) -> Result<AgentLoginResponse, AgentAuthError> {
    let user_repo = UserRepository::new(state.pool());
    let session_repo = AuthSessionRepository::new(state.pool());

    let user = user_repo.fetch_user(user_id).await.map_err(|error| {
        tracing::error!(?error, "failed to fetch agent user");
        AgentAuthError::Internal
    })?;

    let session = session_repo.create(user.id, None).await.map_err(|error| {
        tracing::error!(?error, "failed to create agent auth session");
        AgentAuthError::Internal
    })?;

    let tokens = state
        .jwt()
        .generate_tokens(&session, &user, AGENT_AUTH_PROVIDER)
        .map_err(|error| {
            tracing::error!(?error, "failed to generate agent auth tokens");
            AgentAuthError::Internal
        })?;

    session_repo
        .set_current_refresh_token(session.id, tokens.refresh_token_id)
        .await
        .map_err(|error| {
            tracing::error!(?error, "failed to persist agent refresh token");
            AgentAuthError::Internal
        })?;

    if let Some(analytics) = state.analytics() {
        analytics.track(
            user.id,
            "$identify",
            serde_json::json!({ "email": user.email, "is_agent": true }),
        );
    }

    Ok(AgentLoginResponse {
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
    })
}

impl IntoResponse for AgentAuthError {
    fn into_response(self) -> Response {
        let (status, error) = match self {
            AgentAuthError::InvalidCredentials => (StatusCode::UNAUTHORIZED, "invalid_credentials"),
            AgentAuthError::Internal => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
        };

        (
            status,
            Json(serde_json::json!({
                "error": error,
                "message": self.to_string(),
            })),
        )
            .into_response()
    }
}
