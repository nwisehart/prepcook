use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::MemberRole;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, TS)]
pub struct Organization {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub is_personal: bool,
    pub issue_prefix: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, TS)]
pub struct OrganizationWithRole {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub is_personal: bool,
    pub issue_prefix: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub user_role: MemberRole,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ListOrganizationsResponse {
    pub organizations: Vec<OrganizationWithRole>,
}

// Member types

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct OrganizationMemberWithProfile {
    pub user_id: Uuid,
    pub role: MemberRole,
    pub joined_at: DateTime<Utc>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub username: Option<String>,
    pub email: Option<String>,
    pub avatar_url: Option<String>,
    /// Non-SSO automation identity (robot badge, excluded from default
    /// assignee suggestions).
    #[serde(default)]
    pub is_agent: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ListMembersResponse {
    pub members: Vec<OrganizationMemberWithProfile>,
}
