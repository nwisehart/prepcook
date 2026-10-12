//! Token-usage ledger endpoint (BUG-9777): per-workspace and per-model
//! rollups of the persisted turn usage, so seat burn is attributable and
//! optimizations verifiable. Read-only; v1 surface is this JSON endpoint
//! (workspace names carry the attribution: ticket titles, "Review: …",
//! "Knowledge: RCA analysis").

use axum::{
    Router,
    extract::{Query, State},
    response::Json as ResponseJson,
    routing::get,
};
use chrono::{DateTime, Duration, Utc};
use db::models::execution_process_usage::{ExecutionProcessUsage, ModelUsage, WorkspaceUsage};
use deployment::Deployment;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use utils::response::ApiResponse;

use crate::{DeploymentImpl, error::ApiError};

#[derive(Debug, Deserialize)]
pub struct UsageQuery {
    /// Only count turns started in the last N days. Omit for all time.
    pub days: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
pub struct UsageTotals {
    pub turns: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_tokens: i64,
}

#[derive(Debug, Serialize, Deserialize, TS)]
pub struct UsageSummary {
    /// Window start actually applied (None = all time).
    pub since: Option<DateTime<Utc>>,
    pub totals: UsageTotals,
    pub workspaces: Vec<WorkspaceUsage>,
    pub models: Vec<ModelUsage>,
}

async fn get_usage(
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<UsageQuery>,
) -> Result<ResponseJson<ApiResponse<UsageSummary>>, ApiError> {
    let since = query
        .days
        .map(|days| Utc::now() - Duration::days(i64::from(days)));
    let pool = &deployment.db().pool;

    let workspaces = ExecutionProcessUsage::workspace_rollup(pool, since).await?;
    let models = ExecutionProcessUsage::model_rollup(pool, since).await?;

    let totals = workspaces.iter().fold(
        UsageTotals {
            turns: 0,
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
        },
        |mut acc, row| {
            acc.turns += row.turns;
            acc.input_tokens += row.input_tokens;
            acc.output_tokens += row.output_tokens;
            acc.cache_read_tokens += row.cache_read_tokens;
            acc.cache_creation_tokens += row.cache_creation_tokens;
            acc
        },
    );

    Ok(ResponseJson(ApiResponse::success(UsageSummary {
        since,
        totals,
        workspaces,
        models,
    })))
}

pub fn router() -> Router<DeploymentImpl> {
    Router::new().route("/usage", get(get_usage))
}
