//! Board authentication routes. The owner never signs in (owner tokens, see
//! `auth/owner.rs`); agent users exchange their email + secret for tokens
//! here.

use api_types::{AgentLoginRequest, AgentLoginResponse, ProfileResponse};
use axum::{
    Json, Router,
    extract::{Extension, State},
    routing::{get, post},
};

use crate::{
    AppState,
    auth::{AgentAuthError, RequestContext, agent_login as agent_login_flow},
};

pub(super) fn public_router() -> Router<AppState> {
    Router::new().route("/auth/agent/login", post(agent_login))
}

pub(super) fn protected_router() -> Router<AppState> {
    Router::new().route("/profile", get(profile))
}

async fn agent_login(
    State(state): State<AppState>,
    Json(payload): Json<AgentLoginRequest>,
) -> Result<Json<AgentLoginResponse>, AgentAuthError> {
    let response = agent_login_flow(&state, &payload).await?;
    Ok(Json(response))
}

async fn profile(Extension(ctx): Extension<RequestContext>) -> Json<ProfileResponse> {
    Json(ProfileResponse {
        user_id: ctx.user.id,
        username: ctx.user.username.clone(),
        email: ctx.user.email.clone(),
    })
}
