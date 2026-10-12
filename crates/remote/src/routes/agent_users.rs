//! Agent users: non-SSO automation identities (H3.1 in
//! docs/plans/hosting-and-teammate-plan.md). Admins create them per
//! organization; the server generates the secret and returns it exactly
//! once. Agents sign in with email + secret (`POST /v1/auth/agent/login`).

use api_types::{AgentLoginResponse, User};
use axum::{
    Json, Router,
    extract::{Extension, Path, State},
    http::StatusCode,
    routing::{get, post},
};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::{error::ErrorResponse, organization_members::ensure_admin_access};
use crate::{
    AppState,
    auth::{AGENT_AUTH_PROVIDER, RequestContext},
    db::{auth::AuthSessionRepository, users::UserRepository},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/organizations/{org_id}/agent_users",
            get(list_agent_users).post(create_agent_user),
        )
        .route(
            "/organizations/{org_id}/agent_users/{user_id}/rotate",
            post(rotate_agent_secret),
        )
        .route(
            "/organizations/{org_id}/agent_users/{user_id}/impersonate",
            post(impersonate_agent),
        )
}

/// Generates a random 256-bit secret (base64url) and its stored hash.
/// A plain SHA-256 is sufficient here because the secret is machine-
/// generated with full entropy — slow KDFs exist to protect weak
/// human-chosen passwords, which never occur on this path.
fn generate_secret() -> (String, String) {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    let secret = base64_url(&bytes);
    let hash = hex::encode(Sha256::digest(secret.as_bytes()));
    (secret, hash)
}

fn base64_url(bytes: &[u8]) -> String {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Hash used by the local login path to verify an agent secret.
pub(crate) fn hash_agent_secret(secret: &str) -> String {
    hex::encode(Sha256::digest(secret.as_bytes()))
}

#[derive(Debug, Deserialize)]
struct CreateAgentUserRequest {
    pub email: String,
    pub name: String,
}

#[derive(Debug, Serialize)]
struct AgentUserWithSecretResponse {
    pub user: User,
    /// Returned exactly once — store it in the runner's config.
    pub secret: String,
}

#[derive(Debug, Serialize)]
struct ListAgentUsersResponse {
    pub agents: Vec<User>,
}

async fn create_agent_user(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Path(org_id): Path<Uuid>,
    Json(payload): Json<CreateAgentUserRequest>,
) -> Result<Json<AgentUserWithSecretResponse>, ErrorResponse> {
    ensure_admin_access(state.pool(), org_id, ctx.user.id).await?;

    let email = payload.email.trim().to_ascii_lowercase();
    let name = payload.name.trim().to_string();
    if email.is_empty() || !email.contains('@') || name.is_empty() {
        return Err(ErrorResponse::new(
            StatusCode::BAD_REQUEST,
            "email and name are required",
        ));
    }

    let (secret, hash) = generate_secret();
    let user = UserRepository::new(state.pool())
        .create_agent_user(org_id, &email, &name, &hash)
        .await
        .map_err(|error| {
            tracing::warn!(?error, "failed to create agent user");
            ErrorResponse::new(
                StatusCode::BAD_REQUEST,
                "Failed to create agent user (is the email already taken?)",
            )
        })?;

    Ok(Json(AgentUserWithSecretResponse { user, secret }))
}

async fn rotate_agent_secret(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Path((org_id, user_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<AgentUserWithSecretResponse>, ErrorResponse> {
    ensure_admin_access(state.pool(), org_id, ctx.user.id).await?;

    let repo = UserRepository::new(state.pool());
    // Scope the rotation to agents that are members of this org.
    let agents = repo
        .list_agents_in_organization(org_id)
        .await
        .map_err(|error| {
            tracing::warn!(?error, "failed to list agent users");
            ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "Failed to list agents")
        })?;
    let Some(user) = agents.into_iter().find(|agent| agent.id == user_id) else {
        return Err(ErrorResponse::new(
            StatusCode::NOT_FOUND,
            "No such agent in this organization",
        ));
    };

    let (secret, hash) = generate_secret();
    let rotated = repo
        .set_agent_password_hash(user_id, &hash)
        .await
        .map_err(|error| {
            tracing::warn!(?error, "failed to rotate agent secret");
            ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "Failed to rotate secret")
        })?;
    if !rotated {
        return Err(ErrorResponse::new(
            StatusCode::NOT_FOUND,
            "Not an agent user",
        ));
    }

    Ok(Json(AgentUserWithSecretResponse { user, secret }))
}

/// Mints a session + token pair FOR an agent, authorized by an org admin —
/// the "act as agent" switcher. Only agent users can be impersonated, so
/// this cannot escalate into another human's account. The audit trail:
/// tracing carries the admin's identity alongside the target.
async fn impersonate_agent(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Path((org_id, user_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<AgentLoginResponse>, ErrorResponse> {
    ensure_admin_access(state.pool(), org_id, ctx.user.id).await?;

    let repo = UserRepository::new(state.pool());
    let agents = repo
        .list_agents_in_organization(org_id)
        .await
        .map_err(|error| {
            tracing::warn!(?error, "failed to list agent users");
            ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "Failed to list agents")
        })?;
    let Some(agent) = agents.into_iter().find(|agent| agent.id == user_id) else {
        return Err(ErrorResponse::new(
            StatusCode::NOT_FOUND,
            "No such agent in this organization",
        ));
    };

    let session_repo = AuthSessionRepository::new(state.pool());
    let session = session_repo.create(agent.id, None).await.map_err(|error| {
        tracing::error!(?error, "failed to create impersonation session");
        ErrorResponse::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to create session",
        )
    })?;
    let tokens = state
        .jwt()
        .generate_tokens(&session, &agent, AGENT_AUTH_PROVIDER)
        .map_err(|error| {
            tracing::error!(?error, "failed to generate impersonation tokens");
            ErrorResponse::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to generate tokens",
            )
        })?;
    session_repo
        .set_current_refresh_token(session.id, tokens.refresh_token_id)
        .await
        .map_err(|error| {
            tracing::error!(?error, "failed to persist impersonation refresh token");
            ErrorResponse::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to persist session",
            )
        })?;

    tracing::info!(
        admin = %ctx.user.id,
        agent = %agent.id,
        "admin impersonating agent user"
    );

    Ok(Json(AgentLoginResponse {
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
    }))
}

async fn list_agent_users(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Path(org_id): Path<Uuid>,
) -> Result<Json<ListAgentUsersResponse>, ErrorResponse> {
    ensure_admin_access(state.pool(), org_id, ctx.user.id).await?;

    let agents = UserRepository::new(state.pool())
        .list_agents_in_organization(org_id)
        .await
        .map_err(|error| {
            tracing::warn!(?error, "failed to list agent users");
            ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "Failed to list agents")
        })?;

    Ok(Json(ListAgentUsersResponse { agents }))
}
