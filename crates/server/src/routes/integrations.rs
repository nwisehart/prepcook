//! What Settings needs to render the integration areas: every provider's
//! descriptor, and a connection test per provider.

use std::collections::BTreeMap;

use axum::{
    Router,
    extract::{Json, Path, State},
    response::Json as ResponseJson,
    routing::{get, post},
};
use deployment::Deployment;
use serde::Deserialize;
use services::services::integrations::{self, Descriptor, TestOutcome};
use ts_rs::TS;
use utils::response::ApiResponse;

use crate::DeploymentImpl;

/// The settings to test. Absent tests the first configured entry of the kind.
#[derive(Debug, Default, Deserialize, TS)]
pub struct IntegrationTestRequest {
    #[serde(default)]
    #[ts(optional, type = "Record<string, unknown>")]
    pub fields: Option<BTreeMap<String, serde_json::Value>>,
}

async fn get_descriptors() -> ResponseJson<ApiResponse<BTreeMap<String, Vec<Descriptor>>>> {
    ResponseJson(ApiResponse::success(integrations::descriptors()))
}

async fn test_integration(
    State(deployment): State<DeploymentImpl>,
    Path((area, kind)): Path<(String, String)>,
    body: Option<Json<IntegrationTestRequest>>,
) -> ResponseJson<ApiResponse<TestOutcome>> {
    let fields = body.and_then(|Json(request)| request.fields);
    let config = deployment.config().read().await.clone();
    ResponseJson(ApiResponse::success(
        integrations::test(&config, &area, &kind, fields).await,
    ))
}

pub fn router() -> Router<DeploymentImpl> {
    Router::new()
        .route("/integrations/descriptors", get(get_descriptors))
        .route("/integrations/{area}/{kind}/test", post(test_integration))
}
