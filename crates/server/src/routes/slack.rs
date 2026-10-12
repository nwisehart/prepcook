//! Slack read access for investigating agents. Backs the `slack_read_thread`
//! MCP tool: RCAs kept stalling with "couldn't pull the Slack thread" because
//! agents had no authenticated path to it — this reuses the app's configured
//! bot token (Settings › Orchestrator), no per-agent OAuth.

use axum::{
    Router,
    extract::{Query, State},
    response::Json as ResponseJson,
    routing::get,
};
use deployment::Deployment;
use serde::{Deserialize, Serialize};
use utils::response::ApiResponse;

use crate::DeploymentImpl;

#[derive(Debug, Deserialize)]
pub struct SlackThreadQuery {
    /// Channel id (e.g. `C0123456`). Falls back to the configured escalation
    /// channel when omitted.
    pub channel: Option<String>,
    /// Thread root timestamp (e.g. `1700000000.000100`).
    pub thread_ts: String,
}

#[derive(Debug, Serialize)]
pub struct SlackThreadMessage {
    pub ts: String,
    pub text: String,
    pub is_bot: bool,
    pub user: Option<String>,
}

async fn get_slack_thread(
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<SlackThreadQuery>,
) -> ResponseJson<ApiResponse<Vec<SlackThreadMessage>>> {
    let (bot_token, default_channel) = {
        let config = deployment.config().read().await;
        (config.slack_bot_token.clone(), config.slack_channel.clone())
    };
    let Some(bot_token) = bot_token
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
    else {
        return ResponseJson(ApiResponse::error(
            "Slack bot token is not configured (Settings > Orchestrator)",
        ));
    };
    let Some(channel) = query
        .channel
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .map(str::to_string)
        .or(default_channel)
    else {
        return ResponseJson(ApiResponse::error(
            "No channel given and no default Slack channel configured",
        ));
    };

    let replies =
        services::services::slack::thread_replies(bot_token, &channel, &query.thread_ts).await;
    if replies.is_empty() {
        return ResponseJson(ApiResponse::error(
            "No messages found (bad channel/ts, missing scopes, or the bot is not in the channel)",
        ));
    }
    ResponseJson(ApiResponse::success(
        replies
            .into_iter()
            .map(|reply| SlackThreadMessage {
                ts: reply.ts,
                text: reply.text,
                is_bot: reply.is_bot,
                user: reply.user,
            })
            .collect(),
    ))
}

pub fn router() -> Router<DeploymentImpl> {
    Router::new().route("/slack/thread", get(get_slack_thread))
}
