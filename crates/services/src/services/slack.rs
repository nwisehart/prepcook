//! Best-effort Slack notifications via an incoming webhook.
//!
//! Used by the orchestrator for events that need a human: low-confidence
//! RCA escalations, autonomy review-loop caps, and repeated launch
//! failures. Notification delivery must never break automation, so every
//! failure here is logged and swallowed. Every text and block is passed
//! through [`secrets::redact`] on the way out.

use std::time::Duration;

use serde_json::json;

use crate::services::secrets;

const SLACK_TIMEOUT: Duration = Duration::from_secs(10);

/// `value` with every string in it redacted.
fn redact_value(value: serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match value {
        Value::String(text) => Value::String(secrets::redact(&text)),
        Value::Array(items) => Value::Array(items.into_iter().map(redact_value).collect()),
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, value)| (key, redact_value(value)))
                .collect(),
        ),
        other => other,
    }
}

/// Posts `text` to a Slack incoming-webhook URL. Best-effort: returns
/// whether the post succeeded, but callers are free to ignore it.
pub async fn notify(webhook_url: &str, text: &str) -> bool {
    let client = match reqwest::Client::builder().timeout(SLACK_TIMEOUT).build() {
        Ok(client) => client,
        Err(error) => {
            tracing::warn!(?error, "slack: failed to build http client");
            return false;
        }
    };

    match client
        .post(webhook_url)
        .json(&json!({ "text": secrets::redact(text) }))
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => true,
        Ok(response) => {
            tracing::warn!(status = %response.status(), "slack: webhook returned non-success");
            false
        }
        Err(error) => {
            tracing::warn!(?error, "slack: webhook post failed");
            false
        }
    }
}

/// Convenience wrapper for optional config: no-op when the webhook URL is
/// unset or blank.
pub async fn notify_if_configured(webhook_url: Option<&str>, text: &str) -> bool {
    match webhook_url.map(str::trim).filter(|url| !url.is_empty()) {
        Some(url) => notify(url, text).await,
        None => false,
    }
}

/// Rich (Block Kit) variant: `fallback` renders in notifications and
/// clients that ignore blocks. Same best-effort semantics as `notify`.
pub async fn notify_blocks_if_configured(
    webhook_url: Option<&str>,
    fallback: &str,
    blocks: serde_json::Value,
) -> bool {
    let Some(url) = webhook_url.map(str::trim).filter(|url| !url.is_empty()) else {
        return false;
    };
    let client = match reqwest::Client::builder().timeout(SLACK_TIMEOUT).build() {
        Ok(client) => client,
        Err(error) => {
            tracing::warn!(?error, "slack: failed to build http client");
            return false;
        }
    };
    match client
        .post(url)
        .json(&json!({ "text": secrets::redact(fallback), "blocks": redact_value(blocks) }))
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => true,
        Ok(response) => {
            tracing::warn!(status = %response.status(), "slack: webhook returned non-success");
            false
        }
        Err(error) => {
            tracing::warn!(?error, "slack: webhook post failed");
            false
        }
    }
}

/// Escapes the three characters Slack mrkdwn treats specially in text.
pub fn escape_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Reference to a posted message, used to poll its reply thread.
#[derive(Debug, Clone)]
pub struct ThreadRef {
    pub channel: String,
    pub ts: String,
}

/// One message in an escalation thread.
#[derive(Debug, Clone)]
pub struct Reply {
    pub ts: String,
    pub text: String,
    /// True for the bot's own posts (the escalation itself, bot replies);
    /// only human replies should be forwarded to the agent.
    pub is_bot: bool,
    /// Slack user id of the author, when present (human messages).
    pub user: Option<String>,
}

fn web_client() -> Option<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(SLACK_TIMEOUT)
        .build()
        .map_err(|error| tracing::warn!(?error, "slack: failed to build http client"))
        .ok()
}

/// Posts a Block Kit message via the Web API (chat.postMessage) using a bot
/// token, returning the channel + message `ts` so the thread can be polled.
/// Best-effort: `None` on any failure.
pub async fn post_message(
    bot_token: &str,
    channel: &str,
    fallback: &str,
    blocks: serde_json::Value,
) -> Option<ThreadRef> {
    let client = web_client()?;
    let resp = client
        .post("https://slack.com/api/chat.postMessage")
        .bearer_auth(bot_token)
        .json(&json!({
            "channel": channel,
            "text": secrets::redact(fallback),
            "blocks": redact_value(blocks),
        }))
        .send()
        .await
        .map_err(|error| tracing::warn!(?error, "slack: chat.postMessage failed"))
        .ok()?;
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|error| tracing::warn!(?error, "slack: chat.postMessage bad json"))
        .ok()?;
    if body.get("ok").and_then(serde_json::Value::as_bool) != Some(true) {
        tracing::warn!(error = ?body.get("error"), "slack: chat.postMessage not ok");
        return None;
    }
    let channel = body.get("channel").and_then(|v| v.as_str())?.to_string();
    let ts = body.get("ts").and_then(|v| v.as_str())?.to_string();
    Some(ThreadRef { channel, ts })
}

/// Posts a threaded reply under an existing message (chat.postMessage with
/// `thread_ts`). Keeps escalation headlines skimmable in the channel while
/// the long detail lives in the thread. Bot-authored replies are invisible
/// to the unblock relay (it filters on `is_bot`), so a detail reply can
/// never echo back into a waiting agent. Best-effort: `false` on failure.
pub async fn post_thread_reply(
    bot_token: &str,
    channel: &str,
    thread_ts: &str,
    fallback: &str,
    blocks: serde_json::Value,
) -> bool {
    let Some(client) = web_client() else {
        return false;
    };
    let Ok(resp) = client
        .post("https://slack.com/api/chat.postMessage")
        .bearer_auth(bot_token)
        .json(&json!({
            "channel": channel,
            "thread_ts": thread_ts,
            "text": secrets::redact(fallback),
            "blocks": redact_value(blocks),
        }))
        .send()
        .await
        .map_err(|error| tracing::warn!(?error, "slack: thread reply failed"))
    else {
        return false;
    };
    let Ok(body) = resp
        .json::<serde_json::Value>()
        .await
        .map_err(|error| tracing::warn!(?error, "slack: thread reply bad json"))
    else {
        return false;
    };
    let ok = body.get("ok").and_then(serde_json::Value::as_bool) == Some(true);
    if !ok {
        tracing::warn!(error = ?body.get("error"), "slack: thread reply not ok");
    }
    ok
}

/// Escapes one cell of a canvas markdown table.
///
/// Canvas content is real markdown, not Slack mrkdwn, so the angle-bracket
/// escaping [`escape_text`] does is wrong here. Three things bite instead: an
/// unescaped pipe ends the cell, a newline ends the row, and square brackets
/// open a link — and nearly every ticket title starts `[P0][Customer]`, which
/// is reference-link syntax verbatim.
pub fn escape_cell(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    for ch in text.chars() {
        match ch {
            '\\' | '|' | '[' | ']' => {
                out.push('\\');
                out.push(ch);
            }
            '\n' | '\r' => out.push(' '),
            _ => out.push(ch),
        }
    }
    out
}

/// Replaces a canvas's entire contents (`canvases.edit`, `replace` with no
/// section id). Best-effort: `false` on any failure, including a canvas that
/// no longer exists — the caller re-resolves the id in that case.
pub async fn replace_canvas(bot_token: &str, canvas_id: &str, markdown: &str) -> bool {
    let Some(client) = web_client() else {
        return false;
    };
    let Ok(resp) = client
        .post("https://slack.com/api/canvases.edit")
        .bearer_auth(bot_token)
        .json(&json!({
            "canvas_id": canvas_id,
            "changes": [{
                "operation": "replace",
                "document_content": { "type": "markdown", "markdown": markdown },
            }],
        }))
        .send()
        .await
        .map_err(|error| tracing::warn!(?error, "slack: canvases.edit failed"))
    else {
        return false;
    };
    let Ok(body) = resp
        .json::<serde_json::Value>()
        .await
        .map_err(|error| tracing::warn!(?error, "slack: canvases.edit bad json"))
    else {
        return false;
    };
    let ok = body.get("ok").and_then(serde_json::Value::as_bool) == Some(true);
    if !ok {
        tracing::warn!(error = ?body.get("error"), "slack: canvases.edit not ok");
    }
    ok
}

/// The channel's existing canvas id, read from `conversations.info`. `None`
/// when the channel has no canvas, or the read failed.
pub async fn channel_canvas_id(bot_token: &str, channel: &str) -> Option<String> {
    let client = web_client()?;
    let resp = client
        .get("https://slack.com/api/conversations.info")
        .bearer_auth(bot_token)
        .query(&[("channel", channel)])
        .send()
        .await
        .map_err(|error| tracing::warn!(?error, "slack: conversations.info failed"))
        .ok()?;
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|error| tracing::warn!(?error, "slack: conversations.info bad json"))
        .ok()?;
    if body.get("ok").and_then(serde_json::Value::as_bool) != Some(true) {
        tracing::warn!(error = ?body.get("error"), "slack: conversations.info not ok");
        return None;
    }
    body.pointer("/channel/properties/canvas/document_id")
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

/// Creates the channel canvas. Fails with `channel_canvas_already_exists` when
/// one is already there, which is why the caller looks first.
async fn create_channel_canvas(
    bot_token: &str,
    channel: &str,
    title: &str,
    markdown: &str,
) -> Option<String> {
    let client = web_client()?;
    let resp = client
        .post("https://slack.com/api/conversations.canvases.create")
        .bearer_auth(bot_token)
        .json(&json!({
            "channel_id": channel,
            "title": title,
            "document_content": { "type": "markdown", "markdown": markdown },
        }))
        .send()
        .await
        .map_err(|error| tracing::warn!(?error, "slack: canvas create failed"))
        .ok()?;
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|error| tracing::warn!(?error, "slack: canvas create bad json"))
        .ok()?;
    if body.get("ok").and_then(serde_json::Value::as_bool) != Some(true) {
        tracing::warn!(error = ?body.get("error"), "slack: canvas create not ok");
        return None;
    }
    body.get("canvas_id")
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

/// Writes `markdown` to the channel's canvas, creating it on the first run,
/// and returns the canvas id for the caller to cache.
///
/// `known_id` is the cached id and is tried first, so the steady state is one
/// API call. Every other path exists for a canvas that was deleted or replaced
/// by hand: an edit that fails re-reads the channel, and a channel with no
/// canvas gets a new one. Best-effort throughout — a Slack outage must not
/// stop the sweep that called this.
pub async fn publish_channel_canvas(
    bot_token: &str,
    channel: &str,
    title: &str,
    markdown: &str,
    known_id: Option<&str>,
) -> Option<String> {
    if let Some(id) = known_id
        && replace_canvas(bot_token, id, markdown).await
    {
        return Some(id.to_string());
    }
    if let Some(id) = channel_canvas_id(bot_token, channel).await
        && replace_canvas(bot_token, &id, markdown).await
    {
        return Some(id);
    }
    create_channel_canvas(bot_token, channel, title, markdown).await
}

/// Reads a thread's messages via the Web API (conversations.replies). The
/// first element is the thread root; replies follow. Best-effort: empty vec
/// on any failure.
pub async fn thread_replies(bot_token: &str, channel: &str, thread_ts: &str) -> Vec<Reply> {
    let Some(client) = web_client() else {
        return Vec::new();
    };
    let resp = match client
        .get("https://slack.com/api/conversations.replies")
        .bearer_auth(bot_token)
        .query(&[("channel", channel), ("ts", thread_ts)])
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(error) => {
            tracing::warn!(?error, "slack: conversations.replies failed");
            return Vec::new();
        }
    };
    let body: serde_json::Value = match resp.json().await {
        Ok(body) => body,
        Err(error) => {
            tracing::warn!(?error, "slack: conversations.replies bad json");
            return Vec::new();
        }
    };
    if body.get("ok").and_then(serde_json::Value::as_bool) != Some(true) {
        tracing::warn!(error = ?body.get("error"), "slack: conversations.replies not ok");
        return Vec::new();
    }
    body.get("messages")
        .and_then(|m| m.as_array())
        .map(|messages| {
            messages
                .iter()
                .map(|msg| Reply {
                    ts: msg
                        .get("ts")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    text: msg
                        .get("text")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    // A human message has a `user` and no `bot_id`/`subtype`.
                    is_bot: msg.get("bot_id").is_some()
                        || msg.get("subtype").is_some()
                        || msg.get("user").and_then(|v| v.as_str()).is_none(),
                    user: msg.get("user").and_then(|v| v.as_str()).map(str::to_string),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Slack `ts` values sort chronologically as floats ("1700000000.000100").
pub fn ts_gt(a: &str, b: &str) -> bool {
    a.parse::<f64>().unwrap_or(0.0) > b.parse::<f64>().unwrap_or(0.0)
}
