use api_types::{User, UserData};
use sqlx::{PgPool, query_as};
use uuid::Uuid;

use super::{Tx, identity_errors::IdentityError};

#[derive(Debug, Clone)]
pub struct UpsertUser<'a> {
    pub id: Uuid,
    pub email: &'a str,
    pub first_name: Option<&'a str>,
    pub last_name: Option<&'a str>,
    pub username: Option<&'a str>,
}

pub struct UserRepository<'a> {
    pool: &'a PgPool,
}

impl<'a> UserRepository<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn upsert_user(&self, user: UpsertUser<'_>) -> Result<User, IdentityError> {
        upsert_user(self.pool, &user)
            .await
            .map_err(IdentityError::from)
    }

    pub async fn fetch_user(&self, user_id: Uuid) -> Result<User, IdentityError> {
        query_as!(
            User,
            r#"
            SELECT
                id           AS "id!: Uuid",
                email        AS "email!",
                first_name   AS "first_name?",
                last_name    AS "last_name?",
                username     AS "username?",
                created_at   AS "created_at!",
                updated_at   AS "updated_at!"
            FROM users
            WHERE id = $1
            "#,
            user_id
        )
        .fetch_optional(self.pool)
        .await?
        .ok_or(IdentityError::NotFound)
    }

    /// Creates an agent user (non-SSO automation identity) and adds it to
    /// the organization as a Member, in one transaction.
    pub async fn create_agent_user(
        &self,
        organization_id: Uuid,
        email: &str,
        name: &str,
        password_hash: &str,
    ) -> Result<User, IdentityError> {
        let mut tx = self.pool.begin().await.map_err(IdentityError::from)?;
        let user = query_as!(
            User,
            r#"
            INSERT INTO users (id, email, first_name, username, is_agent, agent_password_hash)
            VALUES ($1, $2, $3, $4, TRUE, $5)
            RETURNING
                id           AS "id!: Uuid",
                email        AS "email!",
                first_name   AS "first_name?",
                last_name    AS "last_name?",
                username     AS "username?",
                created_at   AS "created_at!",
                updated_at   AS "updated_at!"
            "#,
            Uuid::new_v4(),
            email,
            name,
            name,
            password_hash
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(IdentityError::from)?;

        super::organization_members::add_member(
            &mut *tx,
            organization_id,
            user.id,
            api_types::MemberRole::Member,
        )
        .await
        .map_err(IdentityError::from)?;

        tx.commit().await.map_err(IdentityError::from)?;
        Ok(user)
    }

    /// Rotates an agent's secret. Returns false when the user doesn't exist
    /// or isn't an agent.
    pub async fn set_agent_password_hash(
        &self,
        user_id: Uuid,
        password_hash: &str,
    ) -> Result<bool, IdentityError> {
        let result = sqlx::query!(
            r#"
            UPDATE users
               SET agent_password_hash = $2, updated_at = NOW()
             WHERE id = $1 AND is_agent
            "#,
            user_id,
            password_hash
        )
        .execute(self.pool)
        .await
        .map_err(IdentityError::from)?;
        Ok(result.rows_affected() > 0)
    }

    /// Agent-credential lookup for the local login path: the user plus its
    /// stored secret hash, only for agent users.
    pub async fn fetch_agent_credentials(
        &self,
        email: &str,
    ) -> Result<Option<(User, String)>, IdentityError> {
        let row = sqlx::query!(
            r#"
            SELECT
                id           AS "id!: Uuid",
                email        AS "email!",
                first_name   AS "first_name?",
                last_name    AS "last_name?",
                username     AS "username?",
                created_at   AS "created_at!",
                updated_at   AS "updated_at!",
                agent_password_hash AS "agent_password_hash?"
            FROM users
            WHERE LOWER(email) = LOWER($1) AND is_agent
            "#,
            email
        )
        .fetch_optional(self.pool)
        .await
        .map_err(IdentityError::from)?;

        Ok(row.and_then(|row| {
            let hash = row.agent_password_hash?;
            Some((
                User {
                    id: row.id,
                    email: row.email,
                    first_name: row.first_name,
                    last_name: row.last_name,
                    username: row.username,
                    created_at: row.created_at,
                    updated_at: row.updated_at,
                },
                hash,
            ))
        }))
    }

    /// Lists the agent users that are members of an organization.
    pub async fn list_agents_in_organization(
        &self,
        organization_id: Uuid,
    ) -> Result<Vec<User>, IdentityError> {
        query_as!(
            User,
            r#"
            SELECT
                u.id           AS "id!: Uuid",
                u.email        AS "email!",
                u.first_name   AS "first_name?",
                u.last_name    AS "last_name?",
                u.username     AS "username?",
                u.created_at   AS "created_at!",
                u.updated_at   AS "updated_at!"
            FROM users u
            JOIN organization_member_metadata om ON om.user_id = u.id
            WHERE om.organization_id = $1 AND u.is_agent
            ORDER BY u.created_at
            "#,
            organization_id
        )
        .fetch_all(self.pool)
        .await
        .map_err(IdentityError::from)
    }

    pub async fn fetch_user_by_email(&self, email: &str) -> Result<Option<User>, IdentityError> {
        query_as!(
            User,
            r#"
            SELECT
                id           AS "id!: Uuid",
                email        AS "email!",
                first_name   AS "first_name?",
                last_name    AS "last_name?",
                username     AS "username?",
                created_at   AS "created_at!",
                updated_at   AS "updated_at!"
            FROM users
            WHERE LOWER(email) = LOWER($1)
            "#,
            email
        )
        .fetch_optional(self.pool)
        .await?
        .map_or(Ok(None), |user| Ok(Some(user)))
    }
}

async fn upsert_user(pool: &PgPool, user: &UpsertUser<'_>) -> Result<User, sqlx::Error> {
    query_as!(
        User,
        r#"
        INSERT INTO users (id, email, first_name, last_name, username)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (id) DO UPDATE
        SET email = EXCLUDED.email,
            -- COALESCE: an upsert that omits profile fields keeps the values
            -- the user already has.
            first_name = COALESCE(EXCLUDED.first_name, users.first_name),
            last_name = COALESCE(EXCLUDED.last_name, users.last_name),
            username = COALESCE(EXCLUDED.username, users.username)
        RETURNING
            id           AS "id!: Uuid",
            email        AS "email!",
            first_name   AS "first_name?",
            last_name    AS "last_name?",
            username     AS "username?",
            created_at   AS "created_at!",
            updated_at   AS "updated_at!"
        "#,
        user.id,
        user.email,
        user.first_name,
        user.last_name,
        user.username
    )
    .fetch_one(pool)
    .await
}

pub async fn fetch_user(tx: &mut Tx<'_>, user_id: Uuid) -> Result<Option<UserData>, IdentityError> {
    sqlx::query!(
        r#"
        SELECT
            id         AS "id!: Uuid",
            first_name AS "first_name?",
            last_name  AS "last_name?",
            username   AS "username?"
        FROM users
        WHERE id = $1
        "#,
        user_id
    )
    .fetch_optional(&mut **tx)
    .await
    .map_err(IdentityError::from)
    .map(|row_opt| {
        row_opt.map(|row| UserData {
            user_id: row.id,
            first_name: row.first_name,
            last_name: row.last_name,
            username: row.username,
        })
    })
}
