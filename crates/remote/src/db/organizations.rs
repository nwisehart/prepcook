pub use api_types::{MemberRole, Organization, OrganizationWithRole};
use sqlx::{PgPool, query_as};
use uuid::Uuid;

use super::{
    identity_errors::IdentityError,
    organization_members::{
        add_member, assert_admin as check_admin, assert_membership as check_membership,
        check_user_role as get_user_role,
    },
    projects::ProjectRepository,
};

/// Slug of the one organization this server has. Every user becomes an
/// admin of it on sign-in and agent users are created inside it; the row is
/// created on first start if it does not exist.
pub const DEFAULT_ORGANIZATION_SLUG: &str = "default";
const DEFAULT_ORGANIZATION_NAME: &str = "Default";

pub struct OrganizationRepository<'a> {
    pool: &'a PgPool,
}

impl<'a> OrganizationRepository<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn assert_membership(
        &self,
        organization_id: Uuid,
        user_id: Uuid,
    ) -> Result<(), IdentityError> {
        check_membership(self.pool, organization_id, user_id).await
    }

    pub async fn fetch_organization(
        &self,
        organization_id: Uuid,
    ) -> Result<Organization, IdentityError> {
        query_as!(
            Organization,
            r#"
            SELECT
                id           AS "id!: Uuid",
                name         AS "name!",
                slug         AS "slug!",
                is_personal  AS "is_personal!",
                issue_prefix AS "issue_prefix!",
                created_at   AS "created_at!",
                updated_at   AS "updated_at!"
            FROM organizations
            WHERE id = $1
            "#,
            organization_id
        )
        .fetch_optional(self.pool)
        .await?
        .ok_or(IdentityError::NotFound)
    }

    /// Returns the server's organization, creating it (with its initial
    /// project, tags and statuses) when this is the first start.
    pub async fn ensure_default_organization(&self) -> Result<Organization, IdentityError> {
        if let Some(org) = find_organization_by_slug(self.pool, DEFAULT_ORGANIZATION_SLUG).await? {
            return Ok(org);
        }

        let mut tx = super::begin_tx(self.pool).await?;

        // Two sign-ins can race here on a fresh database; the slug is unique,
        // so the loser sees a conflict and simply reads the winner's row.
        let inserted = sqlx::query_as::<_, Organization>(
            r#"
            INSERT INTO organizations (name, slug, issue_prefix)
            VALUES ($1, $2, $3)
            RETURNING id, name, slug, is_personal, issue_prefix, created_at, updated_at
            "#,
        )
        .bind(DEFAULT_ORGANIZATION_NAME)
        .bind(DEFAULT_ORGANIZATION_SLUG)
        .bind(derive_issue_prefix(DEFAULT_ORGANIZATION_NAME))
        .fetch_one(&mut *tx)
        .await;
        let org = match inserted {
            Ok(org) => org,
            Err(error)
                if error
                    .as_database_error()
                    .is_some_and(|db_err| db_err.is_unique_violation()) =>
            {
                tx.rollback().await?;
                return find_organization_by_slug(self.pool, DEFAULT_ORGANIZATION_SLUG)
                    .await?
                    .ok_or(IdentityError::NotFound);
            }
            Err(error) => return Err(error.into()),
        };

        ProjectRepository::create_initial_project_tx(&mut tx, org.id)
            .await
            .map_err(|e| {
                IdentityError::Database(sqlx::Error::Protocol(format!(
                    "Failed to create initial project: {e}"
                )))
            })?;

        tx.commit().await?;
        Ok(org)
    }

    /// Makes the user an admin of the server's organization (creating the
    /// organization if needed) and returns it. Called on every sign-in so a
    /// user is never left outside the organization.
    pub async fn ensure_default_org_and_admin_membership(
        &self,
        user_id: Uuid,
    ) -> Result<Organization, IdentityError> {
        let org = self.ensure_default_organization().await?;
        add_member(self.pool, org.id, user_id, MemberRole::Admin).await?;
        Ok(org)
    }

    pub async fn check_user_role(
        &self,
        organization_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<MemberRole>, IdentityError> {
        get_user_role(self.pool, organization_id, user_id).await
    }

    pub async fn assert_admin(
        &self,
        organization_id: Uuid,
        user_id: Uuid,
    ) -> Result<(), IdentityError> {
        check_admin(self.pool, organization_id, user_id).await
    }

    pub async fn list_user_organizations(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<OrganizationWithRole>, IdentityError> {
        let orgs = sqlx::query_as!(
            OrganizationWithRole,
            r#"
            SELECT
                o.id           AS "id!: Uuid",
                o.name         AS "name!",
                o.slug         AS "slug!",
                o.is_personal  AS "is_personal!",
                o.issue_prefix AS "issue_prefix!",
                o.created_at   AS "created_at!",
                o.updated_at   AS "updated_at!",
                m.role         AS "user_role!: MemberRole"
            FROM organizations o
            JOIN organization_member_metadata m ON m.organization_id = o.id
            WHERE m.user_id = $1
            ORDER BY o.created_at DESC
            "#,
            user_id
        )
        .fetch_all(self.pool)
        .await?;

        Ok(orgs)
    }
}

async fn find_organization_by_slug(
    pool: &PgPool,
    slug: &str,
) -> Result<Option<Organization>, sqlx::Error> {
    query_as!(
        Organization,
        r#"
        SELECT
            id           AS "id!: Uuid",
            name         AS "name!",
            slug         AS "slug!",
            is_personal  AS "is_personal!",
            issue_prefix AS "issue_prefix!",
            created_at   AS "created_at!",
            updated_at   AS "updated_at!"
        FROM organizations
        WHERE slug = $1
        "#,
        slug
    )
    .fetch_optional(pool)
    .await
}

/// Derive an issue prefix from an organization name.
/// Takes the first 3 uppercase letters from the name.
/// Examples: "Bloop" -> "BLO", "My Project" -> "MYP"
fn derive_issue_prefix(name: &str) -> String {
    let letters: String = name.chars().filter(|c| c.is_ascii_alphabetic()).collect();
    let prefix: String = letters.chars().take(3).collect::<String>().to_uppercase();
    if prefix.is_empty() {
        "ISS".to_string()
    } else {
        prefix
    }
}
