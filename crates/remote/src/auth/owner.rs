//! The board's owner: the one human identity of a single-user board. The
//! local server authenticates as the owner with owner tokens it signs with the
//! board's JWT secret (`utils::jwt::mint_owner_token`); nobody signs in.
//!
//! The owner is the user `owner@localhost`, created on first use and made an
//! admin of the default organization. Its requests share one persistent auth
//! session (the owner user's oldest unrevoked session, created on first use),
//! so audit records and anything keyed by session look the same as for a
//! signed-in user. The session is never revoked for inactivity.

use std::sync::Arc;

use api_types::User;
use sqlx::PgPool;
use tokio::sync::OnceCell;
use uuid::Uuid;

use crate::db::{
    auth::{AuthSessionError, AuthSessionRepository},
    identity_errors::IdentityError,
    organizations::OrganizationRepository,
    users::{UpsertUser, UserRepository},
};

pub(crate) const OWNER_EMAIL: &str = "owner@localhost";

#[derive(Debug, Clone)]
pub(crate) struct OwnerIdentity {
    pub user: User,
    pub session_id: Uuid,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum OwnerError {
    #[error(transparent)]
    Identity(#[from] IdentityError),
    #[error(transparent)]
    Session(#[from] AuthSessionError),
}

/// The owner identity, looked up (or created) on the first owner request and
/// kept for the life of the process, so an owner request costs no lookup
/// after the first. The owner row is never edited, so the cached copy stays
/// current.
#[derive(Clone, Default)]
pub(crate) struct OwnerIdentityCache(Arc<OnceCell<OwnerIdentity>>);

impl OwnerIdentityCache {
    pub(crate) async fn get_or_resolve(&self, pool: &PgPool) -> Result<OwnerIdentity, OwnerError> {
        self.0.get_or_try_init(|| resolve(pool)).await.cloned()
    }

    /// A cache that already holds `identity`, for tests that have no
    /// database.
    #[cfg(test)]
    pub(crate) fn resolved(identity: OwnerIdentity) -> Self {
        Self(Arc::new(OnceCell::new_with(Some(identity))))
    }
}

async fn resolve(pool: &PgPool) -> Result<OwnerIdentity, OwnerError> {
    let users = UserRepository::new(pool);
    let user = match users.fetch_user_by_email(OWNER_EMAIL).await? {
        Some(user) => user,
        None => {
            users
                .upsert_user(UpsertUser {
                    id: Uuid::new_v4(),
                    email: OWNER_EMAIL,
                    first_name: None,
                    last_name: None,
                    username: None,
                })
                .await?
        }
    };

    OrganizationRepository::new(pool)
        .ensure_default_org_and_admin_membership(user.id)
        .await?;

    let sessions = AuthSessionRepository::new(pool);
    let session_id = match sessions.oldest_active_session_id(user.id).await? {
        Some(session_id) => session_id,
        None => sessions.create(user.id, None).await?.id,
    };

    tracing::info!(user_id = %user.id, %session_id, "owner identity ready");
    Ok(OwnerIdentity { user, session_id })
}
