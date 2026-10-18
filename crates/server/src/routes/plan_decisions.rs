//! `POST /api/approvals/{issue_id}` and `GET /api/approvals/{issue_id}`: plan
//! decisions from outside the board (see `docs/approvals.md`).
//!
//! A request with no bearer token is the board owner's (the board's own pages
//! and the local UI), accepted while this server holds a board secret.
//! Scripts and other services send `Authorization: Bearer <token>`, where the
//! token is the `PREPCOOK_API_TOKEN` secret. A request that carries a bearer
//! token is judged on the token alone; with no token configured, bearer
//! requests are refused and only the owner's are accepted.

use axum::{
    Router,
    extract::{Json, Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use deployment::Deployment;
use serde::Deserialize;
use services::services::{
    approvals::decisions::{
        ApprovalRecord, Decision, DecisionError, DecisionRequest, DecisionSource,
    },
    secrets,
};
use sha2::{Digest, Sha256};
use ts_rs::TS;
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::DeploymentImpl;

/// The secret external callers present as a bearer token.
pub const API_TOKEN_SECRET: &str = "PREPCOOK_API_TOKEN";

#[derive(Debug, Deserialize, TS)]
pub struct ApprovalDecisionRequest {
    pub decision: Decision,
    /// Reasons, or a note on an approval.
    #[serde(default)]
    #[ts(optional)]
    pub comment: Option<String>,
    /// Who decided, as the caller names them.
    #[serde(default)]
    #[ts(optional)]
    pub by: Option<String>,
}

/// How a request proved who it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Caller {
    /// The board owner: no bearer token, and this server holds a board
    /// secret.
    Owner,
    /// A bearer token matching `PREPCOOK_API_TOKEN`.
    Token,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthRefusal {
    /// No bearer token, and this server has no board secret.
    NoBoardSecret,
    /// A bearer token was sent but none is configured.
    TokenNotConfigured,
    /// A bearer token was sent and does not match, or the header is not a
    /// bearer token.
    BadToken,
}

impl AuthRefusal {
    fn message(self) -> &'static str {
        match self {
            Self::NoBoardSecret => {
                "this server has no board secret; send Authorization: Bearer <PREPCOOK_API_TOKEN>"
            }
            Self::TokenNotConfigured => {
                "no API token is configured (prepcook secret set PREPCOOK_API_TOKEN); this route accepts \
                 the board owner's requests only"
            }
            Self::BadToken => "the bearer token is not valid",
        }
    }
}

/// Decides whether a request may make a decision. Pure, so the whole matrix
/// is testable without a server.
pub fn authorize(
    authorization: Option<&str>,
    configured_token: Option<&str>,
    has_board_secret: bool,
) -> Result<Caller, AuthRefusal> {
    let Some(header) = authorization
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return if has_board_secret {
            Ok(Caller::Owner)
        } else {
            Err(AuthRefusal::NoBoardSecret)
        };
    };
    let presented = header
        .split_once(' ')
        .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("bearer"))
        .map(|(_, token)| token.trim())
        .filter(|token| !token.is_empty())
        .ok_or(AuthRefusal::BadToken)?;
    let configured = configured_token
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .ok_or(AuthRefusal::TokenNotConfigured)?;
    if tokens_match(presented, configured) {
        Ok(Caller::Token)
    } else {
        Err(AuthRefusal::BadToken)
    }
}

/// Compares two tokens in time independent of where they differ. Both are
/// hashed first, so the comparison is over equal-length digests and the
/// configured token's length is not revealed either.
fn tokens_match(presented: &str, configured: &str) -> bool {
    let presented = Sha256::digest(presented.as_bytes());
    let configured = Sha256::digest(configured.as_bytes());
    presented
        .iter()
        .zip(configured.iter())
        .fold(0u8, |diff, (a, b)| diff | (a ^ b))
        == 0
}

pub(crate) async fn check_caller(
    deployment: &DeploymentImpl,
    headers: &HeaderMap,
) -> Result<Caller, Response> {
    let authorization = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok());
    let configured = secrets::load().get(API_TOKEN_SECRET).map(str::to_string);
    authorize(
        authorization,
        configured.as_deref(),
        deployment.has_board_secret(),
    )
    .map_err(|refusal| failure(StatusCode::UNAUTHORIZED, refusal.message()))
}

pub(crate) fn failure(status: StatusCode, message: &str) -> Response {
    (status, Json(ApiResponse::<()>::error(message))).into_response()
}

fn decision_failure(error: DecisionError) -> Response {
    let status = match &error {
        DecisionError::UnknownCard(_) => StatusCode::NOT_FOUND,
        DecisionError::WrongStage { .. } | DecisionError::NotForClass(_) => StatusCode::CONFLICT,
        DecisionError::Unavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
        DecisionError::Failed(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    failure(status, &error.to_string())
}

async fn post_decision(
    State(deployment): State<DeploymentImpl>,
    Path(issue_id): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<ApprovalDecisionRequest>,
) -> Response {
    let caller = match check_caller(&deployment, &headers).await {
        Ok(caller) => caller,
        Err(response) => return response,
    };
    let by = request
        .by
        .map(|by| by.trim().to_string())
        .filter(|by| !by.is_empty())
        .unwrap_or_else(|| match caller {
            Caller::Owner => "the board owner".to_string(),
            Caller::Token => "an API caller".to_string(),
        });
    let outcome = deployment
        .decisions()
        .apply_decision(DecisionRequest {
            issue_id,
            decision: request.decision,
            comment: request.comment.unwrap_or_default(),
            by,
            source: DecisionSource::Api,
        })
        .await;
    match outcome {
        Ok(outcome) => Json(ApiResponse::<ApprovalRecord>::success(outcome.record)).into_response(),
        Err(error) => decision_failure(error),
    }
}

async fn get_decisions(
    State(deployment): State<DeploymentImpl>,
    Path(issue_id): Path<Uuid>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = check_caller(&deployment, &headers).await {
        return response;
    }
    match deployment.decisions().records(issue_id).await {
        Ok(Some(records)) => {
            Json(ApiResponse::<Vec<ApprovalRecord>>::success(records)).into_response()
        }
        Ok(None) => decision_failure(DecisionError::UnknownCard(issue_id)),
        Err(error) => decision_failure(error),
    }
}

pub fn router() -> Router<DeploymentImpl> {
    // `{id}` rather than `{issue_id}`: the tool-approval routes already own
    // `/approvals/{id}/…`, and one segment takes one parameter name.
    Router::new().route("/approvals/{id}", get(get_decisions).post(post_decision))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "s3cret-token-value";

    #[test]
    fn the_owner_needs_no_token() {
        assert_eq!(authorize(None, None, true), Ok(Caller::Owner));
        assert_eq!(authorize(None, Some(TOKEN), true), Ok(Caller::Owner));
        assert_eq!(authorize(Some("  "), Some(TOKEN), true), Ok(Caller::Owner));
    }

    #[test]
    fn no_board_secret_and_no_token_is_refused() {
        assert_eq!(
            authorize(None, Some(TOKEN), false),
            Err(AuthRefusal::NoBoardSecret)
        );
        assert_eq!(
            authorize(None, None, false),
            Err(AuthRefusal::NoBoardSecret)
        );
    }

    #[test]
    fn a_good_token_is_accepted_without_a_board_secret() {
        let header = format!("Bearer {TOKEN}");
        assert_eq!(
            authorize(Some(&header), Some(TOKEN), false),
            Ok(Caller::Token)
        );
        let header = format!("bearer   {TOKEN} ");
        assert_eq!(
            authorize(Some(&header), Some(TOKEN), true),
            Ok(Caller::Token)
        );
    }

    #[test]
    fn a_bad_token_is_refused_even_with_a_board_secret() {
        for header in [
            "Bearer wrong",
            "Bearer s3cret-token-valu",
            "Bearer s3cret-token-value-and-more",
            "Basic czNjcmV0",
            "Bearer",
            TOKEN,
        ] {
            assert_eq!(
                authorize(Some(header), Some(TOKEN), true),
                Err(AuthRefusal::BadToken),
                "{header:?}"
            );
        }
    }

    #[test]
    fn with_no_token_configured_only_the_owner_is_accepted() {
        let header = format!("Bearer {TOKEN}");
        assert_eq!(
            authorize(Some(&header), None, true),
            Err(AuthRefusal::TokenNotConfigured)
        );
        assert_eq!(
            authorize(Some(&header), Some("  "), false),
            Err(AuthRefusal::TokenNotConfigured)
        );
        assert_eq!(authorize(None, None, true), Ok(Caller::Owner));
    }

    #[test]
    fn tokens_compare_by_value() {
        assert!(tokens_match(TOKEN, TOKEN));
        assert!(!tokens_match(TOKEN, "s3cret-token-valuf"));
        assert!(!tokens_match("", TOKEN));
    }
}
