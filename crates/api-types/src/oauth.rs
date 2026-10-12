use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

/// An agent user's email + secret, exchanged at `POST /v1/auth/agent/login`.
#[derive(Debug, Serialize, Deserialize, Clone, TS)]
pub struct AgentLoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, TS)]
pub struct AgentLoginResponse {
    pub access_token: String,
    pub refresh_token: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, TS)]
pub struct TokenRefreshRequest {
    pub refresh_token: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, TS)]
pub struct TokenRefreshResponse {
    pub access_token: String,
    pub refresh_token: String,
}

/// The user a board token acts as.
#[derive(Debug, Serialize, Deserialize, Clone, TS)]
pub struct ProfileResponse {
    pub user_id: Uuid,
    pub username: Option<String>,
    pub email: String,
}

/// Whether this server can work with its board.
#[derive(Debug, Serialize, Deserialize, Clone, TS)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum BoardStatus {
    /// The board answered a request signed with this server's board secret;
    /// `profile` is the board's owner.
    Connected { profile: ProfileResponse },
    /// The server holds a board secret, but the board did not answer (or
    /// refused the secret).
    NotAnswering,
    /// The server knows the board's URL but has no board secret, so it cannot
    /// authenticate to it.
    NoSecret,
    /// No board URL (`PREPCOOK_SHARED_API_BASE`) is set.
    NoBoard,
}
