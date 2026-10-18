//! Slack read access for investigating agents. Backs the `slack_read_thread`
//! MCP tool: RCAs kept stalling with "couldn't pull the Slack thread" because
//! agents had no authenticated path to it — this reuses the conversational
//! notification target (a Slack target with a bot token and channel, Settings
//! › Integrations › Notifications), no per-agent OAuth.

use axum::{
    Router,
    extract::{Query, State},
    response::Json as ResponseJson,
    routing::get,
};
use deployment::Deployment;
use serde::{Deserialize, Serialize};
use services::services::notifications::{NotificationSource, ThreadRef};
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
    let notifications = NotificationSource::new(deployment.config().clone());
    let Some(channel) = query
        .channel
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .map(str::to_string)
        .or(notifications.conversation_channel(None).await)
    else {
        return ResponseJson(ApiResponse::error(
            "No channel given and no Slack bot target configured \
             (Settings > Integrations > Notifications)",
        ));
    };
    let thread = ThreadRef {
        channel,
        ts: query.thread_ts,
    };
    let replies = match notifications.replies(None, &thread, None).await {
        None => {
            return ResponseJson(ApiResponse::error(
                "No Slack bot target is configured (Settings > Integrations > Notifications)",
            ));
        }
        Some(Ok(replies)) if !replies.is_empty() => replies,
        Some(Ok(_)) | Some(Err(_)) => {
            return ResponseJson(ApiResponse::error(
                "No messages found (bad channel/ts, missing scopes, or the bot is not in the channel)",
            ));
        }
    };
    ResponseJson(ApiResponse::success(
        replies
            .into_iter()
            .map(|reply| SlackThreadMessage {
                ts: reply.id,
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
