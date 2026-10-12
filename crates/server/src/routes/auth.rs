//! `/api/auth/*`: what the frontend needs to talk to the board directly. The
//! server holds the board secret and signs owner tokens with it; there is no
//! sign-in.

use axum::{Router, extract::State, response::Json as ResponseJson, routing::get};
use chrono::{DateTime, Utc};
use serde::Serialize;
use ts_rs::TS;
use utils::{jwt::extract_expiration, response::ApiResponse};

use crate::{DeploymentImpl, error::ApiError};

/// Response from GET /api/auth/token: a freshly minted owner token for the
/// board, for the frontend's Electric shape requests.
#[derive(Debug, Serialize, TS)]
pub struct TokenResponse {
    pub access_token: String,
    pub expires_at: Option<DateTime<Utc>>,
}

/// Response from GET /api/auth/user: the board's owner, whom this server acts
/// as.
#[derive(Debug, Serialize, TS)]
pub struct CurrentUserResponse {
    pub user_id: String,
}

pub fn router() -> Router<DeploymentImpl> {
    Router::new()
        .route("/auth/token", get(get_token))
        .route("/auth/user", get(get_current_user))
}

async fn get_token(
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<TokenResponse>>, ApiError> {
    let access_token = deployment.remote_client()?.access_token()?;
    let expires_at = extract_expiration(&access_token).ok();

    Ok(ResponseJson(ApiResponse::success(TokenResponse {
        access_token,
        expires_at,
    })))
}

async fn get_current_user(
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<CurrentUserResponse>>, ApiError> {
    let profile = deployment.remote_client()?.profile().await?;

    Ok(ResponseJson(ApiResponse::success(CurrentUserResponse {
        user_id: profile.user_id.to_string(),
    })))
}
