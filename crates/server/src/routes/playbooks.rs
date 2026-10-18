//! `GET /api/playbooks/context?project_id=…`: the values a board's playbooks
//! fill their repository and product placeholders with, for agents started
//! from the board by hand (the orchestrator fills them itself). See
//! `docs/playbooks.md`.

use std::collections::BTreeMap;

use axum::{
    Router,
    extract::{Query, State},
    response::Json as ResponseJson,
    routing::get,
};
use deployment::Deployment;
use serde::{Deserialize, Serialize};
use services::services::playbook_context::context_for_repos;
use ts_rs::TS;
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::DeploymentImpl;

#[derive(Debug, Deserialize)]
pub struct PlaybookContextQuery {
    /// The board; without one, the context is the defaults.
    pub project_id: Option<Uuid>,
}

#[derive(Debug, Serialize, TS)]
pub struct PlaybookContextResponse {
    /// Placeholder name (without braces) to value. `gate_instructions` may
    /// itself refer to `{{spec_path}}`.
    pub vars: BTreeMap<String, String>,
}

async fn get_playbook_context(
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<PlaybookContextQuery>,
) -> ResponseJson<ApiResponse<PlaybookContextResponse>> {
    let (repo_ids, in_app_gate, evidence) = {
        let config = deployment.config().read().await;
        let repo_ids = query
            .project_id
            .map(|project_id| config.project_settings(project_id).repo_ids)
            .unwrap_or_default();
        (
            repo_ids,
            config.approvals.in_app_gate,
            config.evidence_sources(query.project_id),
        )
    };
    let context = context_for_repos(
        &deployment.db().pool,
        deployment.git(),
        &repo_ids,
        in_app_gate,
        &evidence,
    )
    .await;
    ResponseJson(ApiResponse::success(PlaybookContextResponse {
        vars: context.vars(),
    }))
}

pub fn router() -> Router<DeploymentImpl> {
    Router::new().route("/playbooks/context", get(get_playbook_context))
}
