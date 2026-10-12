use api_types::{TokenRefreshRequest, TokenRefreshResponse};
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
};
use tracing::warn;

use crate::{
    AppState,
    audit::{self, AuditAction, AuditEvent},
    auth::{AGENT_AUTH_PROVIDER, JwtError},
    db::{
        auth::{AuthSessionError, AuthSessionRepository},
        identity_errors::IdentityError,
        users::UserRepository,
    },
};

pub(super) fn public_router() -> Router<AppState> {
    Router::new().route("/tokens/refresh", post(refresh_token))
}

#[derive(Debug, thiserror::Error)]
pub(super) enum TokenRefreshError {
    #[error("invalid refresh token")]
    InvalidToken,
    #[error("session has been revoked")]
    SessionRevoked,
    #[error("refresh token expired")]
    TokenExpired,
    #[error("refresh token reused - possible token theft")]
    TokenReuseDetected,
    #[error(transparent)]
    Jwt(#[from] JwtError),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    SessionError(#[from] AuthSessionError),
    #[error(transparent)]
    Identity(#[from] IdentityError),
}

fn current_session_tokens_response(
    jwt_service: &crate::auth::JwtService,
    session: &api_types::AuthSession,
    user_id: uuid::Uuid,
    provider: &str,
) -> Result<Response, TokenRefreshError> {
    let refresh_token_id = session
        .refresh_token_id
        .ok_or(TokenRefreshError::SessionRevoked)?;
    let issued_at = session
        .refresh_token_issued_at
        .ok_or(TokenRefreshError::SessionRevoked)?;
    let tokens = jwt_service.generate_tokens_for_refresh_token_id(
        session,
        user_id,
        provider,
        refresh_token_id,
        issued_at,
    )?;

    Ok(Json(TokenRefreshResponse {
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
    })
    .into_response())
}

async fn refresh_token(
    State(state): State<AppState>,
    Json(payload): Json<TokenRefreshRequest>,
) -> Result<Response, TokenRefreshError> {
    let jwt_service = &state.jwt();
    let session_repo = AuthSessionRepository::new(state.pool());

    let token_details = match jwt_service.decode_refresh_token(&payload.refresh_token) {
        Ok(details) => details,
        Err(JwtError::TokenExpired) => return Err(TokenRefreshError::TokenExpired),
        Err(_) => return Err(TokenRefreshError::InvalidToken),
    };

    // Only agent sessions hold refresh tokens; a refresh token from any other
    // provider belongs to a login the board no longer offers.
    if token_details.provider != AGENT_AUTH_PROVIDER {
        return Err(TokenRefreshError::InvalidToken);
    }

    let session = match session_repo.get(token_details.session_id).await {
        Ok(session) => session,
        Err(AuthSessionError::NotFound) => return Err(TokenRefreshError::SessionRevoked),
        Err(error) => return Err(TokenRefreshError::SessionError(error)),
    };

    if session.revoked_at.is_some() {
        return Err(TokenRefreshError::SessionRevoked);
    }

    let is_current_refresh_token = session.refresh_token_id == Some(token_details.refresh_token_id);
    let is_previous_refresh_token = session_repo
        .is_previous_refresh_token_within_grace(&session, token_details.refresh_token_id);
    let is_revoked = session_repo
        .is_refresh_token_revoked(token_details.refresh_token_id)
        .await?;

    // Grace only applies to the immediately previous token on a session that is
    // still active. `revoked_refresh_tokens` also records normal rotation
    // lineage, so the authoritative session-revocation boundary is the
    // `session.revoked_at` check above rather than the stored revocation reason.
    if (is_revoked || !is_current_refresh_token) && !is_previous_refresh_token {
        let revoked_count = session_repo
            .revoke_auth_session(token_details.session_id)
            .await?;
        warn!(
            user_id = %token_details.user_id,
            session_id = %token_details.session_id,
            revoked_sessions = revoked_count,
            "Refresh token reuse detected. Revoked affected auth session."
        );
        audit::emit(
            AuditEvent::system(AuditAction::AuthTokenReuseDetected)
                .user(token_details.user_id, Some(token_details.session_id))
                .resource("auth_session", Some(token_details.session_id))
                .http("POST", "/v1/tokens/refresh", 401)
                .description(format!("{revoked_count} sessions revoked")),
        );
        return Err(TokenRefreshError::TokenReuseDetected);
    }

    let user_repo = UserRepository::new(state.pool());
    let user = user_repo.fetch_user(token_details.user_id).await?;

    if is_previous_refresh_token {
        return current_session_tokens_response(
            jwt_service,
            &session,
            user.id,
            &token_details.provider,
        );
    }

    let tokens = jwt_service.generate_tokens(&session, &user, &token_details.provider)?;

    let old_token_id = token_details.refresh_token_id;
    let new_token_id = tokens.refresh_token_id;

    match session_repo
        .rotate_tokens(
            session.id,
            old_token_id,
            new_token_id,
            state.config().refresh_token_overlap_secs,
        )
        .await
    {
        Ok(_) => {}
        Err(AuthSessionError::TokenReuseDetected) => {
            let latest_session = match session_repo.get(token_details.session_id).await {
                Ok(session) => session,
                Err(AuthSessionError::NotFound) => return Err(TokenRefreshError::SessionRevoked),
                Err(error) => return Err(TokenRefreshError::SessionError(error)),
            };

            if latest_session.revoked_at.is_none()
                && session_repo
                    .is_previous_refresh_token_within_grace(&latest_session, old_token_id)
            {
                return current_session_tokens_response(
                    jwt_service,
                    &latest_session,
                    user.id,
                    &token_details.provider,
                );
            }

            let revoked_count = session_repo
                .revoke_auth_session(token_details.session_id)
                .await?;
            warn!(
                user_id = %token_details.user_id,
                session_id = %token_details.session_id,
                revoked_sessions = revoked_count,
                "Detected concurrent refresh attempt; revoked affected auth session"
            );
            audit::emit(
                AuditEvent::system(AuditAction::AuthTokenReuseDetected)
                    .user(token_details.user_id, Some(token_details.session_id))
                    .resource("auth_session", Some(token_details.session_id))
                    .http("POST", "/v1/tokens/refresh", 401)
                    .description(format!(
                        "{revoked_count} sessions revoked (concurrent reuse)"
                    )),
            );
            return Err(TokenRefreshError::TokenReuseDetected);
        }
        Err(error) => return Err(TokenRefreshError::SessionError(error)),
    }

    audit::emit(
        AuditEvent::system(AuditAction::AuthTokenRefresh)
            .user(token_details.user_id, Some(token_details.session_id))
            .resource("auth_session", Some(token_details.session_id))
            .http("POST", "/v1/tokens/refresh", 200),
    );

    Ok(Json(TokenRefreshResponse {
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
    })
    .into_response())
}

impl IntoResponse for TokenRefreshError {
    fn into_response(self) -> Response {
        let (status, error_code) = match self {
            TokenRefreshError::InvalidToken => (StatusCode::UNAUTHORIZED, "invalid_token"),
            TokenRefreshError::TokenExpired => (StatusCode::UNAUTHORIZED, "expired_token"),
            TokenRefreshError::SessionRevoked => (StatusCode::UNAUTHORIZED, "session_revoked"),
            TokenRefreshError::TokenReuseDetected => {
                (StatusCode::UNAUTHORIZED, "token_reuse_detected")
            }
            TokenRefreshError::Jwt(_) => (StatusCode::UNAUTHORIZED, "invalid_token"),
            TokenRefreshError::Identity(_) => (StatusCode::UNAUTHORIZED, "identity_error"),
            TokenRefreshError::Database(ref err) => {
                tracing::error!(error = %err, "Database error during token refresh");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
            }
            TokenRefreshError::SessionError(ref err) => {
                tracing::error!(error = %err, "Session error during token refresh");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
            }
        };

        let body = serde_json::json!({
            "error": error_code,
            "message": self.to_string()
        });

        (status, Json(body)).into_response()
    }
}
