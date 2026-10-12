use serde::{Deserialize, Serialize};
use sqlx::Type;
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type, TS)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[sqlx(type_name = "member_role", rename_all = "lowercase")]
#[ts(use_ts_enum)]
#[ts(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MemberRole {
    Admin,
    Member,
}
