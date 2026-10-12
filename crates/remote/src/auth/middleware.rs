use api_types::User;
use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use axum_extra::headers::{Authorization, HeaderMapExt, authorization::Bearer};
use chrono::{DateTime, Utc};
use tower_http::request_id::RequestId;
use tracing::{Span, warn};
use uuid::Uuid;

use crate::{
    AppState, audit,
    audit::{AuditAction, AuditEvent},
    configure_user_scope,
    db::{
        self,
        auth::{AuthSessionError, AuthSessionRepository, MAX_SESSION_INACTIVITY_DURATION},
        identity_errors::IdentityError,
        users::UserRepository,
    },
};

#[derive(Clone)]
pub struct RequestContext {
    pub user: User,
    pub session_id: Uuid,
    #[allow(dead_code)]
    pub access_token_expires_at: DateTime<Utc>,
}

pub(crate) async fn require_session(
    State(state): State<AppState>,
    mut req: Request<Body>,
    next: Next,
) -> Response {
    let bearer = match req.headers().typed_get::<Authorization<Bearer>>() {
        Some(Authorization(token)) => token.token().to_owned(),
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };

    let ctx = match request_context_from_access_token(&state, &bearer).await {
        Ok(ctx) => ctx,
        Err(response) => return response,
    };

    Span::current().record("user_id", tracing::field::display(ctx.user.id));

    let request_id = req
        .extensions()
        .get::<RequestId>()
        .and_then(|id| id.header_value().to_str().ok())
        .unwrap_or("")
        .to_owned();

    let tx_ctx = db::TxContext {
        user_id: ctx.user.id,
        request_id,
    };

    req.extensions_mut().insert(ctx);
    db::TX_CONTEXT.scope(Some(tx_ctx), next.run(req)).await
}

pub(super) async fn request_context_from_access_token(
    state: &AppState,
    access_token: &str,
) -> Result<RequestContext, Response> {
    let jwt = state.jwt();

    // Owner tokens first: they carry no session, so the session path below
    // cannot read them.
    match jwt.decode_owner_token(access_token) {
        Ok(Some(expires_at)) => return request_context_for_owner(state, expires_at).await,
        Ok(None) => {}
        Err(error) => {
            warn!(?error, "failed to decode access token");
            return Err(StatusCode::UNAUTHORIZED.into_response());
        }
    }

    let identity = match jwt.decode_access_token(access_token) {
        Ok(details) => details,
        Err(error) => {
            warn!(?error, "failed to decode access token");
            return Err(StatusCode::UNAUTHORIZED.into_response());
        }
    };

    let mut ctx = request_context_from_auth_session_id(state, identity.session_id).await?;
    if ctx.user.id != identity.user_id {
        warn!(
            token_user_id = %identity.user_id,
            session_user_id = %ctx.user.id,
            session_id = %identity.session_id,
            "access token user does not match session user"
        );
        return Err(StatusCode::UNAUTHORIZED.into_response());
    }

    ctx.access_token_expires_at = identity.expires_at;
    Ok(ctx)
}

/// The owner's request context: the owner user and its persistent session,
/// with no inactivity check (the owner session never expires; the secret is
/// the credential).
async fn request_context_for_owner(
    state: &AppState,
    expires_at: DateTime<Utc>,
) -> Result<RequestContext, Response> {
    let owner = match state.owner().get_or_resolve(state.pool()).await {
        Ok(owner) => owner,
        Err(error) => {
            warn!(?error, "failed to resolve the owner identity");
            return Err(StatusCode::INTERNAL_SERVER_ERROR.into_response());
        }
    };

    configure_user_scope(
        owner.user.id,
        owner.user.username.as_deref(),
        Some(owner.user.email.as_str()),
    );

    if let Err(error) = AuthSessionRepository::new(state.pool())
        .touch(owner.session_id)
        .await
    {
        warn!(?error, "failed to update session last-used timestamp");
    }

    Ok(RequestContext {
        user: owner.user,
        session_id: owner.session_id,
        access_token_expires_at: expires_at,
    })
}

pub(super) async fn request_context_from_auth_session_id(
    state: &AppState,
    session_id: Uuid,
) -> Result<RequestContext, Response> {
    let pool = state.pool();
    let session_repo = AuthSessionRepository::new(pool);
    let session = match session_repo.get(session_id).await {
        Ok(session) => session,
        Err(AuthSessionError::NotFound) => {
            warn!("session `{}` not found", session_id);
            return Err(StatusCode::UNAUTHORIZED.into_response());
        }
        Err(AuthSessionError::Database(error)) => {
            warn!(?error, "failed to load session");
            return Err(StatusCode::INTERNAL_SERVER_ERROR.into_response());
        }
        Err(_) => {
            warn!("failed to load session for unknown reason");
            return Err(StatusCode::UNAUTHORIZED.into_response());
        }
    };

    if session.revoked_at.is_some() {
        warn!("session `{}` rejected (revoked)", session.id);
        return Err(StatusCode::UNAUTHORIZED.into_response());
    }

    if session.inactivity_duration(Utc::now()) > MAX_SESSION_INACTIVITY_DURATION {
        warn!(
            "session `{}` expired due to inactivity; revoking",
            session.id
        );
        if let Err(error) = session_repo.revoke(session.id).await {
            warn!(?error, "failed to revoke inactive session");
        }
        audit::emit(
            AuditEvent::system(AuditAction::AuthSessionRevoked)
                .user(session.user_id, Some(session.id))
                .resource("auth_session", Some(session.id))
                .http("", "", 401)
                .description("Session revoked due to inactivity"),
        );
        return Err(StatusCode::UNAUTHORIZED.into_response());
    }

    let user_repo = UserRepository::new(pool);
    let user = match user_repo.fetch_user(session.user_id).await {
        Ok(user) => user,
        Err(IdentityError::NotFound) => {
            warn!("user `{}` missing", session.user_id);
            return Err(StatusCode::UNAUTHORIZED.into_response());
        }
        Err(IdentityError::Database(error)) => {
            warn!(?error, "failed to load user");
            return Err(StatusCode::INTERNAL_SERVER_ERROR.into_response());
        }
        Err(_) => {
            warn!("unexpected error loading user");
            return Err(StatusCode::INTERNAL_SERVER_ERROR.into_response());
        }
    };

    configure_user_scope(user.id, user.username.as_deref(), Some(user.email.as_str()));

    let ctx = RequestContext {
        user,
        session_id: session.id,
        access_token_expires_at: Utc::now(),
    };

    match session_repo.touch(session.id).await {
        Ok(_) => {}
        Err(error) => warn!(?error, "failed to update session last-used timestamp"),
    }

    Ok(ctx)
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use api_types::{AuthSession, User};
    use axum::{Extension, Router, middleware, routing::get};
    use chrono::Utc;
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
    use reqwest::StatusCode as ReqwestStatus;
    use secrecy::SecretString;
    use sqlx::{PgPool, postgres::PgPoolOptions};
    use utils::jwt::{OWNER_TOKEN_MAX_TTL_SECONDS, OWNER_TOKEN_TTL_SECONDS, mint_owner_token};
    use uuid::Uuid;

    use super::{RequestContext, require_session};
    use crate::{
        AppState,
        auth::{
            JwtService,
            owner::{OWNER_EMAIL, OwnerIdentity, OwnerIdentityCache},
        },
        config::RemoteServerConfig,
    };

    // Standard base64 of 32 bytes, the shape `prepcook up` generates.
    const SECRET: &str = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=";
    const OTHER_SECRET: &str = "HxweHRwbGhkYFxYVFBMSERAPDg0MCwoJCAcGBQQDAgE=";
    // Nothing listens here, so any query fails fast.
    const NO_DATABASE: &str = "postgres://nobody@127.0.0.1:1/none";

    fn app_state(pool: PgPool, database_url: &str) -> AppState {
        AppState::new(
            pool,
            RemoteServerConfig::for_tests(database_url, SECRET),
            Arc::new(JwtService::new(SecretString::new(SECRET.into()))),
            "http://127.0.0.1:1".to_string(),
            reqwest::Client::new(),
            None,
        )
    }

    fn unreachable_pool() -> PgPool {
        PgPoolOptions::new()
            .acquire_timeout(Duration::from_millis(200))
            .connect_lazy(NO_DATABASE)
            .unwrap()
    }

    fn owner_identity() -> OwnerIdentity {
        let now = Utc::now();
        OwnerIdentity {
            user: User {
                id: Uuid::new_v4(),
                email: OWNER_EMAIL.to_string(),
                first_name: None,
                last_name: None,
                username: None,
                created_at: now,
                updated_at: now,
            },
            session_id: Uuid::new_v4(),
        }
    }

    async fn whoami(Extension(ctx): Extension<RequestContext>) -> String {
        format!("{} {} {}", ctx.user.email, ctx.user.id, ctx.session_id)
    }

    /// Serves `/whoami` behind the session middleware and returns its URL.
    async fn serve(state: AppState) -> String {
        let router = Router::new()
            .route("/whoami", get(whoami))
            .layer(middleware::from_fn_with_state(
                state.clone(),
                require_session,
            ))
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        format!("http://{addr}/whoami")
    }

    async fn call(url: &str, token: &str) -> (ReqwestStatus, String) {
        let response = reqwest::Client::new()
            .get(url)
            .bearer_auth(token)
            .send()
            .await
            .unwrap();
        let status = response.status();
        (status, response.text().await.unwrap())
    }

    fn signed_token(claims: serde_json::Value, secret: &str) -> String {
        encode(
            &Header::new(Algorithm::HS256),
            &claims,
            &EncodingKey::from_base64_secret(secret).unwrap(),
        )
        .unwrap()
    }

    fn session_tokens(jwt: &JwtService, user_id: Uuid, session_id: Uuid) -> String {
        let session = AuthSession {
            id: session_id,
            user_id,
            created_at: Utc::now(),
            last_used_at: None,
            revoked_at: None,
            refresh_token_id: None,
            refresh_token_issued_at: None,
            previous_refresh_token_id: None,
            previous_refresh_token_grace_expires_at: None,
        };
        jwt.generate_tokens_for_refresh_token_id(
            &session,
            user_id,
            crate::auth::AGENT_AUTH_PROVIDER,
            Uuid::new_v4(),
            Utc::now(),
        )
        .unwrap()
        .access_token
    }

    #[tokio::test]
    async fn an_owner_token_acts_as_the_owner_in_its_persistent_session() {
        let owner = owner_identity();
        let state = app_state(unreachable_pool(), NO_DATABASE)
            .with_owner(OwnerIdentityCache::resolved(owner.clone()));
        let url = serve(state).await;

        let token = mint_owner_token(SECRET, Utc::now(), OWNER_TOKEN_TTL_SECONDS).unwrap();
        let expected = format!("{OWNER_EMAIL} {} {}", owner.user.id, owner.session_id);
        assert_eq!(
            call(&url, &token).await,
            (ReqwestStatus::OK, expected.clone())
        );

        // A second, freshly minted token lands in the same session.
        let again = mint_owner_token(SECRET, Utc::now(), 60).unwrap();
        assert_eq!(call(&url, &again).await, (ReqwestStatus::OK, expected));
    }

    #[tokio::test]
    async fn owner_tokens_that_are_expired_foreign_or_too_long_are_refused() {
        let state = app_state(unreachable_pool(), NO_DATABASE)
            .with_owner(OwnerIdentityCache::resolved(owner_identity()));
        let url = serve(state).await;
        let now = Utc::now().timestamp();

        let expired =
            mint_owner_token(SECRET, Utc::now() - chrono::Duration::hours(2), 60).unwrap();
        let other_secret = mint_owner_token(OTHER_SECRET, Utc::now(), 60).unwrap();
        let too_long =
            mint_owner_token(SECRET, Utc::now(), OWNER_TOKEN_MAX_TTL_SECONDS * 24).unwrap();
        // Signed with the board's secret, but for an audience the board does
        // not serve: neither an owner token nor a session token.
        let wrong_audience = signed_token(
            serde_json::json!({ "aud": "prepcook-admin", "iat": now, "exp": now + 60 }),
            SECRET,
        );

        for token in [expired, other_secret, too_long, wrong_audience] {
            assert_eq!(call(&url, &token).await.0, ReqwestStatus::UNAUTHORIZED);
        }
    }

    #[tokio::test]
    async fn a_session_token_takes_the_session_path() {
        let jwt = JwtService::new(SecretString::new(SECRET.into()));
        let (user_id, session_id) = (Uuid::new_v4(), Uuid::new_v4());
        let access = session_tokens(&jwt, user_id, session_id);

        assert!(jwt.decode_owner_token(&access).unwrap().is_none());
        let details = jwt.decode_access_token(&access).unwrap();
        assert_eq!((details.user_id, details.session_id), (user_id, session_id));

        // Through the middleware it goes on to the session lookup (which,
        // with no database, fails as a server error, not as a refused token).
        let state = app_state(unreachable_pool(), NO_DATABASE)
            .with_owner(OwnerIdentityCache::resolved(owner_identity()));
        let url = serve(state).await;
        assert_eq!(
            call(&url, &access).await.0,
            ReqwestStatus::INTERNAL_SERVER_ERROR
        );
    }

    /// End to end against a real database: set
    /// `PREPCOOK_REMOTE_TEST_DATABASE_URL` to an empty Postgres database (the
    /// test migrates it). Without it the test does nothing.
    #[tokio::test]
    async fn owner_and_session_tokens_against_a_database() {
        let Ok(database_url) = std::env::var("PREPCOOK_REMOTE_TEST_DATABASE_URL") else {
            eprintln!("PREPCOOK_REMOTE_TEST_DATABASE_URL not set; skipping");
            return;
        };
        let pool = crate::db::create_pool(&database_url).await.unwrap();
        crate::db::migrate(&pool).await.unwrap();

        // The owner is created on first use, with one persistent session that
        // a restarted board (a fresh cache) finds again.
        let token = mint_owner_token(SECRET, Utc::now(), OWNER_TOKEN_TTL_SECONDS).unwrap();
        let first = call(&serve(app_state(pool.clone(), &database_url)).await, &token).await;
        let second = call(&serve(app_state(pool.clone(), &database_url)).await, &token).await;
        assert_eq!(first.0, ReqwestStatus::OK);
        assert!(first.1.starts_with(OWNER_EMAIL));
        assert_eq!(first, second);

        // A normal session token still works for its own user.
        let users = crate::db::users::UserRepository::new(&pool);
        let email = format!("agent-{}@example.com", Uuid::new_v4());
        let user = users
            .upsert_user(crate::db::users::UpsertUser {
                id: Uuid::new_v4(),
                email: &email,
                first_name: None,
                last_name: None,
                username: None,
            })
            .await
            .unwrap();
        let session = crate::db::auth::AuthSessionRepository::new(&pool)
            .create(user.id, None)
            .await
            .unwrap();
        let jwt = JwtService::new(SecretString::new(SECRET.into()));
        let access = session_tokens(&jwt, user.id, session.id);
        let url = serve(app_state(pool, &database_url)).await;
        assert_eq!(
            call(&url, &access).await,
            (
                ReqwestStatus::OK,
                format!("{email} {} {}", user.id, session.id)
            )
        );
    }
}
