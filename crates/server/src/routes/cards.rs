//! `POST /api/cards`, `GET /api/cards/{id}` and `POST /api/cards/{id}/convert`:
//! cards made, read and converted from outside the board (see `docs/api.md`).
//!
//! Authentication is the approvals endpoint's (`plan_decisions`): a bearer
//! token matching the `PREPCOOK_API_TOKEN` secret, or the board owner's own
//! session. Filing a card takes the token: it is how another service — a
//! support agent, an incident tool — puts work on the board, and the token is
//! what names it as one that may.

use axum::{
    Router,
    extract::{Json, Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use deployment::Deployment;
use services::services::cards::{
    CardError, CardView, ConvertCardRequest, ConvertedCard, CreateCardRequest, CreatedCard,
};
use utils::response::ApiResponse;
use uuid::Uuid;

use super::plan_decisions::{Caller, check_caller, failure};
use crate::DeploymentImpl;

/// Whether a caller may file a card: only one holding the API token.
pub fn may_create(caller: Caller) -> bool {
    caller == Caller::Token
}

/// The HTTP status a card error answers with.
pub fn status_for(error: &CardError) -> StatusCode {
    match error {
        CardError::Invalid(_) => StatusCode::BAD_REQUEST,
        CardError::UnknownCard(_) | CardError::UnknownBoard(_) => StatusCode::NOT_FOUND,
        CardError::Conflict(_) => StatusCode::CONFLICT,
        CardError::Unavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
        CardError::Failed(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

fn card_failure(error: CardError) -> Response {
    failure(status_for(&error), &error.to_string())
}

async fn create_card(
    State(deployment): State<DeploymentImpl>,
    headers: HeaderMap,
    body: Result<Json<CreateCardRequest>, axum::extract::rejection::JsonRejection>,
) -> Response {
    let caller = match check_caller(&deployment, &headers).await {
        Ok(caller) => caller,
        Err(response) => return response,
    };
    if !may_create(caller) {
        return failure(
            StatusCode::UNAUTHORIZED,
            "filing a card needs Authorization: Bearer <PREPCOOK_API_TOKEN>",
        );
    }
    let Json(request) = match body {
        Ok(body) => body,
        Err(rejection) => return failure(StatusCode::BAD_REQUEST, &rejection.body_text()),
    };
    match deployment.cards().create(request).await {
        Ok(created) => (
            StatusCode::CREATED,
            Json(ApiResponse::<CreatedCard>::success(created)),
        )
            .into_response(),
        Err(error) => card_failure(error),
    }
}

async fn get_card(
    State(deployment): State<DeploymentImpl>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = check_caller(&deployment, &headers).await {
        return response;
    }
    match deployment.cards().view(id).await {
        Ok(Some(view)) => Json(ApiResponse::<CardView>::success(view)).into_response(),
        Ok(None) => card_failure(CardError::UnknownCard(id)),
        Err(error) => card_failure(error),
    }
}

async fn convert_card(
    State(deployment): State<DeploymentImpl>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    body: Result<Json<ConvertCardRequest>, axum::extract::rejection::JsonRejection>,
) -> Response {
    let caller = match check_caller(&deployment, &headers).await {
        Ok(caller) => caller,
        Err(response) => return response,
    };
    let Json(request) = match body {
        Ok(body) => body,
        Err(rejection) => return failure(StatusCode::BAD_REQUEST, &rejection.body_text()),
    };
    let by = request
        .by
        .map(|by| by.trim().to_string())
        .filter(|by| !by.is_empty())
        .unwrap_or_else(|| match caller {
            Caller::Owner => "the board owner".to_string(),
            Caller::Token => "an API caller".to_string(),
        });
    match deployment
        .cards()
        .convert(id, request.work_class, &by)
        .await
    {
        Ok(converted) => Json(ApiResponse::<ConvertedCard>::success(converted)).into_response(),
        Err(error) => card_failure(error),
    }
}

pub fn router() -> Router<DeploymentImpl> {
    Router::new()
        .route("/cards", post(create_card))
        .route("/cards/{id}", get(get_card))
        .route("/cards/{id}/convert", post(convert_card))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routes::plan_decisions::{AuthRefusal, authorize};

    const TOKEN: &str = "card-api-token-value";

    /// Filing needs the token; reading and converting take the board
    /// owner's session too, as the approvals endpoint does.
    #[test]
    fn filing_a_card_needs_the_api_token() {
        let bearer = format!("Bearer {TOKEN}");
        let token = authorize(Some(&bearer), Some(TOKEN), true).unwrap();
        assert!(may_create(token));
        let owner = authorize(None, Some(TOKEN), true).unwrap();
        assert!(!may_create(owner));
        assert_eq!(
            authorize(Some("Bearer wrong"), Some(TOKEN), true),
            Err(AuthRefusal::BadToken)
        );
        assert_eq!(
            authorize(Some(&bearer), None, true),
            Err(AuthRefusal::TokenNotConfigured)
        );
    }

    #[test]
    fn card_errors_answer_with_their_status() {
        assert_eq!(
            status_for(&CardError::Invalid("title is required".to_string())),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            status_for(&CardError::UnknownBoard(Uuid::nil())),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            status_for(&CardError::UnknownCard(Uuid::nil())),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            status_for(&CardError::Conflict("no Plan column".to_string())),
            StatusCode::CONFLICT
        );
    }
}
