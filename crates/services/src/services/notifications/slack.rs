//! Slack: an incoming webhook, a bot token and channel, or both.
//!
//! The webhook posts; the bot posts, threads and reads. With a bot token the
//! digest is also kept on a channel canvas (the message then carries only the
//! counts and the P0s), and planning escalations open a thread whose human
//! replies are fed back to the blocked agent ([`Conversational`]). Every text
//! and block is passed through [`secrets::redact`] on the way out.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use uuid::Uuid;

use super::{
    Context, Conversational, Delivery, Digest, DigestItem, DigestSection, DigestSectionId, Event,
    HTTP_TIMEOUT, Notifier, NotifyError, PlanKind, PlanningEscalation, REPLAN_DECISION, Reply,
    SEAT_EXPIRED_BODY, TargetSettings, ThreadRef, clip, investigation_summary, mention_listing,
    mr_list, parse_reply_mark, redact_value,
};
use crate::services::{
    daily,
    integrations::{Descriptor, Field, FieldKind},
    secrets,
};

const SLACK_API: &str = "https://slack.com/api";

/// Title of the channel canvas the digest maintains.
const CANVAS_TITLE: &str = "Daily Digest";

/// Most cards listed per digest section in the message before the tail is
/// summarised. A section long enough to scroll past stops being a to-do list.
const DIGEST_SECTION_CAP: usize = super::DIGEST_SECTION_CAP;

/// Lines per Slack section block. Slack rejects a section whose text exceeds
/// 3000 characters, and ticket titles are long enough that a dozen lines can
/// reach it.
const DIGEST_LINES_PER_BLOCK: usize = 4;

/// The bot half of a Slack target.
#[derive(Clone, Debug)]
struct Bot {
    token: String,
    channel: String,
}

pub struct SlackNotifier {
    client: reqwest::Client,
    api_base: String,
    webhook_url: Option<String>,
    bot: Option<Bot>,
    /// The board the digest canvas id is cached for.
    board_id: Option<Uuid>,
}

impl SlackNotifier {
    pub fn descriptor() -> Descriptor {
        Descriptor::new(
            "slack",
            "Slack",
            "Posts to a channel through an incoming webhook, a bot, or both; with a bot, \
             escalation threads carry replies back to the agent and the digest keeps a \
             channel canvas.",
            vec![
                Field::new(
                    "webhook_url",
                    "Incoming webhook URL",
                    FieldKind::Secret,
                    false,
                    "A Slack incoming-webhook URL (https://hooks.slack.com/services/…). Needed \
                     unless a bot token and channel are set.",
                ),
                Field::new(
                    "bot_token",
                    "Bot token",
                    FieldKind::Secret,
                    false,
                    "A bot token (xoxb-…) with chat:write, channels:history (or \
                     groups:history) and canvases:write; the bot must be in the channel. With a \
                     channel, escalations open a thread whose replies reach the blocked agent.",
                ),
                Field::new(
                    "channel",
                    "Channel ID",
                    FieldKind::Text,
                    false,
                    "The channel the bot posts to, e.g. C0123456.",
                ),
            ],
            "https://api.slack.com/messaging/webhooks",
        )
    }

    pub fn new(settings: &TargetSettings, context: &Context) -> Result<Self, NotifyError> {
        let bot = match (settings.get("bot_token"), settings.get("channel")) {
            (Some(token), Some(channel)) => Some(Bot {
                token: token.to_string(),
                channel: channel.to_string(),
            }),
            _ => None,
        };
        let webhook_url = settings.get("webhook_url").map(str::to_string);
        if bot.is_none() && webhook_url.is_none() {
            return Err(NotifyError::NotConfigured(
                "Slack needs a webhook URL, or a bot token and a channel".to_string(),
            ));
        }
        Ok(Self {
            client: reqwest::Client::builder()
                .timeout(HTTP_TIMEOUT)
                .build()
                .map_err(NotifyError::http)?,
            api_base: SLACK_API.to_string(),
            webhook_url,
            bot,
            board_id: context.board_id,
        })
    }

    /// Points the Web API calls somewhere else (tests).
    pub fn with_api_base(mut self, base: &str) -> Self {
        self.api_base = base.trim_end_matches('/').to_string();
        self
    }

    /// Posts to the incoming webhook.
    async fn post_webhook(
        &self,
        url: &str,
        fallback: &str,
        blocks: Option<Value>,
    ) -> Result<(), NotifyError> {
        let mut body = json!({ "text": secrets::redact(fallback) });
        if let Some(blocks) = blocks {
            body["blocks"] = redact_value(blocks);
        }
        let response = self
            .client
            .post(url)
            .json(&body)
            .send()
            .await
            .map_err(NotifyError::http)?;
        let status = response.status();
        if status.is_success() {
            Ok(())
        } else {
            Err(NotifyError::Rejected {
                status: status.as_u16(),
                body: response.text().await.unwrap_or_default(),
            })
        }
    }

    /// Calls a Web API method and returns its body once Slack says `ok`.
    async fn api(&self, bot: &Bot, method: &str, body: Value) -> Result<Value, NotifyError> {
        let response = self
            .client
            .post(format!("{}/{method}", self.api_base))
            .bearer_auth(&bot.token)
            .json(&body)
            .send()
            .await
            .map_err(NotifyError::http)?;
        let status = response.status();
        if !status.is_success() {
            return Err(NotifyError::Rejected {
                status: status.as_u16(),
                body: response.text().await.unwrap_or_default(),
            });
        }
        let body: Value = response.json().await.map_err(NotifyError::http)?;
        if body.get("ok").and_then(Value::as_bool) == Some(true) {
            Ok(body)
        } else {
            Err(NotifyError::Api(format!(
                "{method}: {}",
                body.get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("not ok")
            )))
        }
    }

    /// Posts with the bot (chat.postMessage), optionally under a thread.
    async fn post_bot(
        &self,
        bot: &Bot,
        fallback: &str,
        blocks: Option<Value>,
        thread_ts: Option<&str>,
    ) -> Result<ThreadRef, NotifyError> {
        let mut body = json!({
            "channel": bot.channel,
            "text": secrets::redact(fallback),
        });
        if let Some(blocks) = blocks {
            body["blocks"] = redact_value(blocks);
        }
        if let Some(ts) = thread_ts {
            body["thread_ts"] = json!(ts);
        }
        let body = self.api(bot, "chat.postMessage", body).await?;
        let channel = body
            .get("channel")
            .and_then(Value::as_str)
            .unwrap_or(&bot.channel)
            .to_string();
        let ts = body
            .get("ts")
            .and_then(Value::as_str)
            .ok_or_else(|| NotifyError::Api("chat.postMessage: no ts".to_string()))?
            .to_string();
        Ok(ThreadRef { channel, ts })
    }

    /// Posts one message: through the webhook when there is one, else the bot.
    async fn post(&self, fallback: &str, blocks: Option<Value>) -> Result<(), NotifyError> {
        if let Some(url) = &self.webhook_url {
            return self.post_webhook(url, fallback, blocks).await;
        }
        let bot = self
            .bot
            .as_ref()
            .ok_or_else(|| NotifyError::NotConfigured("no webhook and no bot".to_string()))?;
        self.post_bot(bot, fallback, blocks, None).await.map(|_| ())
    }

    async fn send_escalation(
        &self,
        escalation: &PlanningEscalation,
    ) -> Result<Delivery, NotifyError> {
        let fallback = format!(
            "{} {}: {} — waiting in Plan for a human",
            escalation_emoji(escalation),
            escalation.issue.key,
            escalation.blocked_on
        );
        // Bot path: the card in the channel, the long summary in its thread,
        // and the thread remembered so replies can be read back. Bot-authored
        // replies are invisible to the reply relay (it filters `is_bot`), so
        // the detail can never echo back into a waiting agent.
        if let Some(bot) = &self.bot {
            match self
                .post_bot(
                    bot,
                    &fallback,
                    Some(escalation_blocks(escalation, true)),
                    None,
                )
                .await
            {
                Ok(thread) => {
                    let detail = json!([{
                        "type": "section",
                        "text": {
                            "type": "mrkdwn",
                            "text": format!(
                                "*Full planning summary*\n{}",
                                escape_text(&clip(&escalation.summary, 2900))
                            )
                        }
                    }]);
                    if let Err(error) = self
                        .post_bot(bot, "Full planning summary", Some(detail), Some(&thread.ts))
                        .await
                    {
                        tracing::warn!("slack: escalation detail reply failed: {error}");
                    }
                    return Ok(Delivery {
                        thread: Some(thread),
                    });
                }
                Err(error) if self.webhook_url.is_some() => {
                    // Fall back so the human is still paged.
                    tracing::warn!("slack: escalation post failed, using the webhook: {error}");
                }
                Err(error) => return Err(error),
            }
        }
        // The webhook cannot thread, so it carries a trimmed summary instead.
        let url = self
            .webhook_url
            .as_deref()
            .ok_or_else(|| NotifyError::NotConfigured("no webhook".to_string()))?;
        self.post_webhook(url, &fallback, Some(escalation_blocks(escalation, false)))
            .await?;
        Ok(Delivery::default())
    }

    async fn send_digest(&self, digest: &Digest) -> Result<Delivery, NotifyError> {
        // The canvas is written first: the message links to it and reports
        // whether it is current, so posting a pointer to a stale page is worse
        // than posting the whole list.
        let canvas_id = match &self.bot {
            Some(bot) => self.publish_digest_canvas(bot, digest).await,
            None => None,
        };
        let blocks = digest_blocks(digest, canvas_id.is_some());
        // The fallback is what a notification and a client that ignores blocks
        // shows, so it carries the counts rather than the heading alone.
        let fallback = format!("Orchestrator: {}", digest.counts_line());
        self.post(&fallback, Some(blocks)).await?;
        Ok(Delivery::default())
    }

    async fn publish_digest_canvas(&self, bot: &Bot, digest: &Digest) -> Option<String> {
        let tz = daily::parse_timezone(Some(&digest.timezone));
        let markdown = canvas_markdown(
            &digest
                .generated_at
                .with_timezone(&tz)
                .format("%A %-d %B, %-I:%M %p %Z")
                .to_string(),
            &digest.next_run.format("%A %-d %B").to_string(),
            &digest.sections,
        );
        let known = self
            .board_id
            .and_then(|board| daily::read(|state| state.canvas_id(board).map(str::to_string)));
        let id = self
            .publish_channel_canvas(bot, &markdown, known.as_deref())
            .await;
        if let (Some(board), Some(id)) = (self.board_id, id.as_deref()) {
            daily::update(|state| state.record_canvas_id(board, id));
        }
        id
    }

    /// Writes `markdown` to the channel's canvas, creating it on the first
    /// run, and returns the canvas id for the caller to cache.
    ///
    /// `known_id` is the cached id and is tried first, so the steady state is
    /// one API call. Every other path exists for a canvas that was deleted or
    /// replaced by hand: an edit that fails re-reads the channel, and a channel
    /// with no canvas gets a new one. Best-effort throughout.
    async fn publish_channel_canvas(
        &self,
        bot: &Bot,
        markdown: &str,
        known_id: Option<&str>,
    ) -> Option<String> {
        if let Some(id) = known_id
            && self.replace_canvas(bot, id, markdown).await
        {
            return Some(id.to_string());
        }
        if let Some(id) = self.channel_canvas_id(bot).await
            && self.replace_canvas(bot, &id, markdown).await
        {
            return Some(id);
        }
        // Creating fails with `channel_canvas_already_exists` when one is
        // there, which is why the channel is read first.
        match self
            .api(
                bot,
                "conversations.canvases.create",
                json!({
                    "channel_id": bot.channel,
                    "title": CANVAS_TITLE,
                    "document_content": { "type": "markdown", "markdown": markdown },
                }),
            )
            .await
        {
            Ok(body) => body
                .get("canvas_id")
                .and_then(Value::as_str)
                .map(str::to_string),
            Err(error) => {
                tracing::warn!("slack: canvas create failed: {error}");
                None
            }
        }
    }

    /// Replaces a canvas's entire contents. `false` on any failure, including
    /// a canvas that no longer exists.
    async fn replace_canvas(&self, bot: &Bot, canvas_id: &str, markdown: &str) -> bool {
        self.api(
            bot,
            "canvases.edit",
            json!({
                "canvas_id": canvas_id,
                "changes": [{
                    "operation": "replace",
                    "document_content": { "type": "markdown", "markdown": markdown },
                }],
            }),
        )
        .await
        .map_err(|error| tracing::warn!("slack: canvases.edit failed: {error}"))
        .is_ok()
    }

    /// The channel's existing canvas id, from `conversations.info`.
    async fn channel_canvas_id(&self, bot: &Bot) -> Option<String> {
        let response = self
            .client
            .get(format!("{}/conversations.info", self.api_base))
            .bearer_auth(&bot.token)
            .query(&[("channel", bot.channel.as_str())])
            .send()
            .await
            .map_err(|error| tracing::warn!("slack: conversations.info failed: {error}"))
            .ok()?;
        let body: Value = response.json().await.ok()?;
        if body.get("ok").and_then(Value::as_bool) != Some(true) {
            tracing::warn!(error = ?body.get("error"), "slack: conversations.info not ok");
            return None;
        }
        body.pointer("/channel/properties/canvas/document_id")
            .and_then(Value::as_str)
            .map(str::to_string)
    }
}

#[async_trait]
impl Notifier for SlackNotifier {
    fn kind(&self) -> &'static str {
        "slack"
    }

    async fn send(&self, event: &Event) -> Result<Delivery, NotifyError> {
        match event {
            Event::PlanningEscalated(escalation) => self.send_escalation(escalation).await,
            Event::DailyDigest(digest) => self.send_digest(digest).await,
            other => {
                self.post(&render_text(other), None).await?;
                Ok(Delivery::default())
            }
        }
    }

    fn conversational(&self) -> Option<&dyn Conversational> {
        self.bot.is_some().then_some(self as &dyn Conversational)
    }

    async fn test(&self) -> Result<String, NotifyError> {
        if let Some(bot) = &self.bot {
            let body = self.api(bot, "auth.test", json!({})).await?;
            let field = |key: &str| {
                body.get(key)
                    .and_then(Value::as_str)
                    .unwrap_or("?")
                    .to_string()
            };
            return Ok(format!(
                "Signed in to {} as {}",
                field("team"),
                field("user")
            ));
        }
        // An incoming webhook has nothing to read; posting is the only check.
        self.post("Prep Cook test notification: this webhook works.", None)
            .await?;
        Ok("Posted a test message through the webhook".to_string())
    }
}

#[async_trait]
impl Conversational for SlackNotifier {
    async fn replies(
        &self,
        thread: &ThreadRef,
        since: Option<DateTime<Utc>>,
    ) -> Result<Vec<Reply>, NotifyError> {
        let bot = self
            .bot
            .as_ref()
            .ok_or_else(|| NotifyError::NotConfigured("no bot token".to_string()))?;
        let response = self
            .client
            .get(format!("{}/conversations.replies", self.api_base))
            .bearer_auth(&bot.token)
            .query(&[
                ("channel", thread.channel.as_str()),
                ("ts", thread.ts.as_str()),
            ])
            .send()
            .await
            .map_err(NotifyError::http)?;
        let body: Value = response.json().await.map_err(NotifyError::http)?;
        if body.get("ok").and_then(Value::as_bool) != Some(true) {
            return Err(NotifyError::Api(format!(
                "conversations.replies: {}",
                body.get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("not ok")
            )));
        }
        let mut replies: Vec<Reply> = body
            .get("messages")
            .and_then(Value::as_array)
            .map(|messages| messages.iter().filter_map(parse_message).collect())
            .unwrap_or_default();
        replies.retain(|reply| since.is_none_or(|since| reply.at > since));
        replies.sort_by_key(|reply| reply.at);
        Ok(replies)
    }

    async fn reply(&self, thread: &ThreadRef, text: &str) -> Result<(), NotifyError> {
        let bot = self
            .bot
            .as_ref()
            .ok_or_else(|| NotifyError::NotConfigured("no bot token".to_string()))?;
        let bot = Bot {
            token: bot.token.clone(),
            channel: thread.channel.clone(),
        };
        self.post_bot(&bot, text, None, Some(&thread.ts))
            .await
            .map(|_| ())
    }
}

fn parse_message(message: &Value) -> Option<Reply> {
    let ts = message.get("ts").and_then(Value::as_str)?.to_string();
    let user = message
        .get("user")
        .and_then(Value::as_str)
        .map(str::to_string);
    Some(Reply {
        at: parse_reply_mark(&ts)?,
        text: message
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        // A human message has a `user` and no `bot_id`/`subtype`.
        is_bot: message.get("bot_id").is_some()
            || message.get("subtype").is_some()
            || user.is_none(),
        user,
        id: ts,
    })
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// Escapes the three characters Slack mrkdwn treats specially in text.
pub fn escape_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
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

/// The one-message text for every event without a richer layout.
fn render_text(event: &Event) -> String {
    match event {
        Event::SeatLimitHit { window, minutes } => format!(
            ":hourglass: *Prep Cook: Claude seat {window} limit hit.* Agent launches are \
             paused for ~{minutes}m, until the window reopens."
        ),
        Event::SeatExhausted { rounds, report } => format!(
            ":rotating_light: *Prep Cook: the Claude seat keeps running out.* {rounds} \
             windows in a row exhausted with nothing getting through between them, so no \
             board is advancing — that needs less burn or more budget, not more waiting.\n\
             The seat reported: `{report}`"
        ),
        Event::SeatExpired {} => {
            format!(":warning: *Prep Cook: Claude seat expired.* {SEAT_EXPIRED_BODY}")
        }
        Event::SeatRecovered {} => {
            ":white_check_mark: *Prep Cook: Claude seat is healthy again.*".to_string()
        }
        Event::TicketClosedWithOpenMrs { issue, mr_numbers } => format!(
            ":warning: *{} was closed in the tracker with {} still open.* The card has been \
             retired; that merge request needs closing or merging by hand.",
            escape_text(&issue.key),
            mr_list(mr_numbers)
        ),
        Event::RunnerFault {
            issue,
            step,
            outcome,
            detail,
        } => format!(
            ":construction: *{}: {step} cannot run — script runner reported `{outcome}`.* \
             This is the setup's problem, not the branch's; the card holds until it is \
             fixed.\n{detail}",
            escape_text(&issue.key)
        ),
        Event::ReviewLoopCapped { issue, rounds } => format!(
            "🔴 {}: review loop hit the {rounds}-round cap — waiting in Review for a human",
            escape_text(&issue.key)
        ),
        Event::ReviewRoundAbandoned { issue, reason } => format!(
            "🔴 {}: review findings could not be applied — {}. Waiting in Review for a human",
            escape_text(&issue.key),
            escape_text(reason)
        ),
        Event::PipelineEscalated {
            issue,
            mr_url,
            reason,
        } => format!(
            "🔴 *{}* — MR pipeline needs a human: {}\nMR: {mr_url}",
            escape_text(&issue.key),
            escape_text(&clip(reason, 700))
        ),
        Event::ReplanRequested {
            issue,
            reason,
            mr_url,
        } => format!(
            "🧭 *{}* — MR review feedback rejects the approach, so the agent stopped instead \
             of rewriting it.\n{}\n\n{REPLAN_DECISION}{}",
            escape_text(&issue.key),
            escape_text(&clip(reason, 700)),
            mr_url
                .as_deref()
                .map(|url| format!("\nMR: {url}"))
                .unwrap_or_default()
        ),
        Event::BlockerMrOpened {
            issue,
            mr_url,
            summary,
        } => format!(
            "Blocker merge request needs review — {}\n{}\n{mr_url}",
            escape_text(&issue.key),
            escape_text(summary)
        ),
        Event::MentionNeedsHuman { issue, mentions } => format!(
            "💬 *{}* — the agent was mentioned on the tracker but no session is running:\n{}",
            escape_text(&issue.key),
            escape_text(&clip(&mention_listing(mentions), 700))
        ),
        Event::InvestigationCompleted(completed) => format!(
            "🔎 *{}* — investigation triaged: {}\n{}",
            escape_text(&completed.issue.key),
            escape_text(completed.issue.short_title()),
            escape_text(&investigation_summary(completed))
        ),
        Event::CardConverted {
            issue,
            from,
            to,
            by,
            column,
        } => format!(
            "↪️ *{}* converted from {} to {} by {} — now in {column}",
            escape_text(&issue.key),
            from.as_str(),
            to.as_str(),
            escape_text(by)
        ),
        // Rendered as blocks by their own paths; this is their fallback text.
        Event::DailyDigest(digest) => format!("Orchestrator: {}", digest.counts_line()),
        Event::PlanningEscalated(escalation) => format!(
            "{}: {} — waiting in Plan for a human",
            escalation.issue.key, escalation.blocked_on
        ),
    }
}

fn escalation_emoji(escalation: &PlanningEscalation) -> &'static str {
    match (escalation.plan_kind, escalation.confidence) {
        (PlanKind::Rca, Some(super::Confidence::High)) => "🟢",
        (PlanKind::Rca, Some(super::Confidence::Medium)) => "🟡",
        (PlanKind::Rca, _) => "🔴",
        (PlanKind::Spec, _) => "🟠",
    }
}

/// The escalation card: severity emoji, linked headline, a one-line root
/// cause, and the agent's labelled next steps — skimmable, not the essay.
/// `threaded` is the bot path, whose full summary goes into the thread; the
/// webhook path cannot thread and carries a trimmed summary instead.
fn escalation_blocks(escalation: &PlanningEscalation, threaded: bool) -> Value {
    let issue = &escalation.issue;
    // `|` would terminate the mrkdwn link label early.
    let title_label = issue.title.replace('|', "¦");
    let headline = format!("{}: {}", escape_text(&issue.key), escape_text(&title_label));
    let headline = match &issue.card_url {
        Some(url) => format!("{} *<{url}|{headline}>*", escalation_emoji(escalation)),
        None => format!("{} *{headline}*", escalation_emoji(escalation)),
    };
    let mut blocks = vec![
        json!({ "type": "section", "text": { "type": "mrkdwn", "text": headline } }),
        json!({
            "type": "section",
            "fields": [
                {
                    "type": "mrkdwn",
                    "text": format!("*Blocked on*\n{}", escape_text(&escalation.blocked_on))
                },
                { "type": "mrkdwn", "text": "*Status*\nWaiting in Plan for a human" }
            ]
        }),
        json!({
            "type": "section",
            "text": {
                "type": "mrkdwn",
                "text": format!(
                    "*Root cause (one line)*\n{}",
                    escape_text(&escalation.root_cause)
                )
            }
        }),
    ];
    if let Some(reason) = escalation
        .why_not_higher
        .as_deref()
        .filter(|reason| !reason.trim().is_empty())
    {
        blocks.push(json!({
            "type": "section",
            "text": {
                "type": "mrkdwn",
                "text": format!("*Why not higher*\n{}", escape_text(&clip(reason, 300)))
            }
        }));
    }
    if !escalation.next_steps.is_empty() {
        let joined = escalation
            .next_steps
            .iter()
            .map(|item| format!("• {}", escape_text(item)))
            .collect::<Vec<_>>()
            .join("\n");
        blocks.push(json!({
            "type": "section",
            "text": { "type": "mrkdwn", "text": format!("*Next steps*\n{joined}") }
        }));
    }
    if !threaded {
        blocks.push(json!({
            "type": "section",
            "text": {
                "type": "mrkdwn",
                "text": format!(
                    "*Summary*\n{}",
                    escape_text(&clip(&escalation.summary, 700))
                )
            }
        }));
    }
    let mut context = match &issue.card_url {
        Some(url) => format!("<{url}|Open ticket in Prep Cook>"),
        None => String::new(),
    };
    if let (Some(url), Some(tracker)) = (&issue.url, &escalation.tracker) {
        if !context.is_empty() {
            context.push_str(" · ");
        }
        context.push_str(&format!("<{url}|{} in {tracker}>", issue.key));
    }
    if threaded {
        if !context.is_empty() {
            context.push_str(" · ");
        }
        context.push_str("full summary in this thread — reply here to steer the agent");
    }
    if !context.is_empty() {
        blocks.push(json!({
            "type": "context",
            "elements": [{ "type": "mrkdwn", "text": context }]
        }));
    }
    Value::Array(blocks)
}

/// One digest line in the message. Links to the merge request where one
/// exists, because that is what the reader of this section is about to open;
/// to the ticket otherwise. The label names whichever it is, so a glance down
/// the column separates "there is a diff waiting" from "there is not".
fn digest_line(item: &DigestItem) -> String {
    let link = match (item.mr_url.as_deref(), item.mr_number()) {
        (Some(url), Some(number)) => format!("<{url}|!{number}>"),
        (Some(url), None) => format!("<{url}|MR>"),
        (None, _) => match item.url.as_deref() {
            Some(url) => format!("<{url}|{}>", item.key),
            None => item.key.clone(),
        },
    };
    let marker = if item.urgent { ":red_circle: " } else { "" };
    let note = match item.note.as_deref() {
        Some(note) if !note.is_empty() => format!(" — {note}. Waiting on an engineer"),
        _ => String::new(),
    };
    let age = match item.days {
        Some(days) => format!(" · {days}d"),
        None => String::new(),
    };
    // The key rides along when the link points at a merge request, so the
    // line is still searchable by ticket.
    let key = if item.mr_url.is_some() {
        format!(" `{}`", item.key)
    } else {
        String::new()
    };
    format!(
        "{marker}{link}{key} {}{age}{note}",
        escape_text(item.short_title())
    )
}

/// The digest message. With a canvas the full list is a tap away, so the
/// message answers only the question that cannot wait for a tap: is anything
/// on fire. Without one the message is the only surface and carries the whole
/// list, capped.
fn digest_blocks(digest: &Digest, canvas: bool) -> Value {
    let mut blocks = vec![json!({
        "type": "header",
        "text": { "type": "plain_text", "text": digest.heading(), "emoji": true }
    })];
    let total = digest.total();
    if canvas {
        blocks.push(json!({
            "type": "context",
            "elements": [{
                "type": "mrkdwn",
                "text": format!(
                    "{} to merge · {} to answer · {} not moving · {} awaiting staging — full list on the *{CANVAS_TITLE}* canvas at the top of this channel",
                    digest.count(DigestSectionId::NeedsMerge),
                    digest.count(DigestSectionId::NeedsAnswer),
                    digest.count(DigestSectionId::NotMoving),
                    digest.awaiting_staging
                )
            }]
        }));
        let mut any_urgent = false;
        for section in &digest.sections {
            let urgent: Vec<String> = section
                .items
                .iter()
                .filter(|item| item.urgent)
                .map(digest_line)
                .collect();
            if urgent.is_empty() {
                continue;
            }
            any_urgent = true;
            blocks.push(json!({ "type": "divider" }));
            blocks.push(json!({
                "type": "section",
                "text": {
                    "type": "mrkdwn",
                    "text": format!("*{}* · {} P0", section.title, urgent.len())
                }
            }));
            for batch in urgent.chunks(DIGEST_LINES_PER_BLOCK) {
                blocks.push(json!({
                    "type": "section",
                    "text": { "type": "mrkdwn", "text": batch.join("\n") }
                }));
            }
        }
        if !any_urgent {
            blocks.push(json!({
                "type": "context",
                "elements": [{
                    "type": "mrkdwn",
                    "text": if total == 0 {
                        "Nothing is waiting on a person today."
                    } else {
                        "No P0s waiting — the canvas has the rest."
                    }
                }]
            }));
        }
    } else {
        for section in &digest.sections {
            if section.items.is_empty() {
                continue;
            }
            blocks.push(json!({ "type": "divider" }));
            blocks.push(json!({
                "type": "section",
                "text": {
                    "type": "mrkdwn",
                    "text": format!(
                        "*{}* · {} waiting on {}",
                        section.title,
                        section.items.len(),
                        section.waiting_on
                    )
                }
            }));
            // Slack rejects a section over 3000 characters, and a long line
            // list reaches that well before the section cap does. Batched so
            // a busy day degrades into more blocks rather than a dropped post.
            let rendered: Vec<String> = section
                .items
                .iter()
                .take(DIGEST_SECTION_CAP)
                .map(digest_line)
                .collect();
            for batch in rendered.chunks(DIGEST_LINES_PER_BLOCK) {
                blocks.push(json!({
                    "type": "section",
                    "text": { "type": "mrkdwn", "text": batch.join("\n") }
                }));
            }
            if section.items.len() > DIGEST_SECTION_CAP {
                blocks.push(json!({
                    "type": "context",
                    "elements": [{
                        "type": "mrkdwn",
                        "text": format!("…and {} more", section.items.len() - DIGEST_SECTION_CAP)
                    }]
                }));
            }
        }
        if total == 0 {
            blocks.push(json!({
                "type": "section",
                "text": { "type": "mrkdwn", "text": "Nothing is waiting on a person today." }
            }));
        }
    }
    Value::Array(blocks)
}

/// Off for "Needs an answer": those cards are parked in planning and none of
/// them has a merge request, so the column would be a stripe of dashes.
fn has_mr_column(section: &DigestSection) -> bool {
    section.id != DigestSectionId::NeedsAnswer
}

/// On only for "Not moving", the one section whose cards carry a reason.
/// Elsewhere the heading already says what everything is waiting on.
fn shows_why(section: &DigestSection) -> bool {
    section.id == DigestSectionId::NotMoving
}

/// One row of a canvas table.
///
/// Canvas tables size their columns to fit the page, so every column costs
/// the others width. Two therefore earn their keep by not existing: the
/// reason a card is stuck rides under its title, and the urgency marker leads
/// the title rather than holding a column of its own — six columns pushed the
/// table past the right edge and squeezed the ticket key until it wrapped
/// mid-word.
///
/// The marker leads the *title* and not the ticket because the ticket cell is
/// barely wider than `BUG-10165`, and an emoji in front of it puts that wrap
/// straight back.
///
/// There is no checkbox column: canvas markdown has no syntax for one. `[]` in
/// a cell is what the editor turns into a box as you type, but sent through
/// the API it stays literal text.
///
/// Links are `[text](url)`, not Slack's `<url|text>`; canvas content is real
/// markdown.
fn canvas_row(item: &DigestItem, mr_column: bool, show_why: bool) -> String {
    let mut row = String::from("|");
    if mr_column {
        row.push_str(&match (item.mr_url.as_deref(), item.mr_number()) {
            (Some(url), Some(number)) => format!("[!{number}]({url})"),
            (Some(url), None) => format!("[MR]({url})"),
            (None, _) => "—".to_string(),
        });
        row.push('|');
    }
    row.push_str(&match item.url.as_deref() {
        Some(url) => format!("[{}]({url})", escape_cell(&item.key)),
        None => escape_cell(&item.key),
    });
    row.push('|');
    if item.urgent {
        row.push_str(":red_circle: ");
    }
    row.push_str(&escape_cell(item.short_title()));
    if show_why && let Some(note) = item.note.as_deref().filter(|note| !note.is_empty()) {
        // `<br>` is the one way to get a second line inside a cell.
        row.push_str(&format!("<br>_{}_", escape_cell(note)));
    }
    row.push('|');
    row.push_str(&match item.days {
        Some(days) => format!("{days}d"),
        None => "—".to_string(),
    });
    row.push('|');
    row
}

/// The canvas body: every card, in tables.
///
/// The channel message is capped because a long message is unreadable in a
/// scroll-back; a page opened on purpose is not, so nothing is hidden here.
/// That is the whole reason for the split — the truncated tail was the part
/// nobody ever saw.
fn canvas_markdown(updated: &str, next: &str, sections: &[DigestSection]) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "Last updated **{updated}** · next update **{next}**\n\n"
    ));
    out.push_str(
        "_Rewritten in full by the orchestrator on every run. Notes typed here are lost — \
         put them on the ticket._\n",
    );

    let mut any = false;
    for section in sections {
        if section.items.is_empty() {
            continue;
        }
        any = true;
        let mr_column = has_mr_column(section);
        out.push_str(&format!(
            "\n---\n\n## {} — {}\n\nWaiting on {}.\n\n",
            section.title,
            section.items.len(),
            section.waiting_on
        ));
        let mut header = String::from("|");
        let mut rule = String::from("|");
        if mr_column {
            header.push_str("MR|");
            rule.push_str("---|");
        }
        header.push_str("Ticket|Title|Age|");
        rule.push_str("---|---|---|");
        out.push_str(&header);
        out.push('\n');
        out.push_str(&rule);
        out.push('\n');
        for item in &section.items {
            out.push_str(&canvas_row(item, mr_column, shows_why(section)));
            out.push('\n');
        }
    }
    if !any {
        out.push_str("\n---\n\nNothing is waiting on a person today.\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{body_partial_json, header, method, path, query_param},
    };

    use super::{
        super::{NotifierKind, test_support::*},
        *,
    };

    fn item(key: &str, title: &str, mr: Option<&str>) -> DigestItem {
        DigestItem {
            key: key.to_string(),
            url: Some(format!("https://example.atlassian.net/browse/{key}")),
            title: title.to_string(),
            mr_url: mr.map(str::to_string),
            urgent: false,
            days: Some(7),
            note: None,
        }
    }

    fn section(id: DigestSectionId, items: Vec<DigestItem>) -> DigestSection {
        DigestSection {
            id,
            title: match id {
                DigestSectionId::NeedsMerge => "Needs a merge",
                DigestSectionId::NeedsAnswer => "Needs an answer",
                DigestSectionId::NotMoving => "Not moving",
            }
            .to_string(),
            waiting_on: "a person to settle a question".to_string(),
            items,
        }
    }

    fn notifier(fields: &[(&str, &str)], api_base: &str) -> SlackNotifier {
        SlackNotifier::new(&settings(NotifierKind::Slack, fields), &context())
            .unwrap()
            .with_api_base(api_base)
    }

    #[test]
    fn a_canvas_row_escapes_the_brackets_every_ticket_title_starts_with() {
        // "[P0][Blackbaud] …" is reference-link syntax verbatim; unescaped, the
        // title renders as a broken link.
        let line = item(
            "BUG-10079",
            "BUG-10079: [P0][Blackbaud] Generated PPT requires repair",
            Some("https://gitlab.com/example-org/example-app/-/merge_requests/9241"),
        );
        let row = canvas_row(&line, true, false);
        assert!(row.contains("\\[P0\\]\\[Blackbaud\\]"), "{row}");
        // The importer's key prefix is dropped: the ticket cell already links
        // it.
        assert!(!row.contains("BUG-10079: "), "{row}");
        assert!(
            row.contains(
                "[!9241](https://gitlab.com/example-org/example-app/-/merge_requests/9241)"
            ),
            "{row}"
        );
    }

    #[test]
    fn a_pipe_in_a_title_does_not_end_the_cell() {
        let row = canvas_row(&item("BUG-1", "BUG-1: a | b", None), true, false);
        assert!(row.contains("a \\| b"), "{row}");
        // MR, ticket, title, age — four cells, so five bars.
        assert_eq!(row.matches('|').count() - 1, 5, "{row}");
    }

    #[test]
    fn a_newline_in_a_title_cannot_end_the_row() {
        let row = canvas_row(&item("BUG-1", "BUG-1: first\nsecond", None), true, false);
        assert!(row.contains("first second"), "{row}");
        assert_eq!(row.lines().count(), 1, "{row}");
    }

    #[test]
    fn an_urgent_row_is_marked_at_the_head_of_its_title() {
        // Not the ticket cell, which is barely wider than the key itself, and
        // not a column of its own — both cost the title width it needs.
        let mut line = item("BUG-1", "BUG-1: t", None);
        line.urgent = true;
        assert!(canvas_row(&line, true, false).ends_with("|:red_circle: t|7d|"));
        assert!(canvas_row(&item("BUG-1", "BUG-1: t", None), true, false).ends_with("|t|7d|"));
    }

    #[test]
    fn a_reason_rides_under_the_title_rather_than_in_its_own_column() {
        let mut line = item("BUG-1", "BUG-1: t", None);
        line.note = Some("its merge request was closed unmerged".to_string());
        let row = canvas_row(&line, true, true);
        assert!(
            row.contains("|t<br>_its merge request was closed unmerged_|"),
            "{row}"
        );
        // Four cells, so five bars — no escaped pipe in this title to discount.
        assert_eq!(row.matches('|').count(), 5, "{row}");
        assert!(!canvas_row(&line, true, false).contains("closed unmerged"));
    }

    #[test]
    fn the_canvas_lists_every_card_where_the_message_caps() {
        // The cap is the reason for the canvas: 20 cards is more than
        // DIGEST_SECTION_CAP, and all 20 must appear.
        let items: Vec<DigestItem> = (0..20)
            .map(|n| item(&format!("BUG-{n}"), &format!("BUG-{n}: title {n}"), None))
            .collect();
        assert!(items.len() > DIGEST_SECTION_CAP);
        let markdown = canvas_markdown(
            "Friday 4 September, 8:00 AM PDT",
            "Monday 7 September",
            &[section(DigestSectionId::NeedsAnswer, items)],
        );
        assert_eq!(markdown.matches("\n|[BUG-").count(), 20, "one row per card");
        for n in 0..20 {
            assert!(markdown.contains(&format!("BUG-{n}]")), "missing BUG-{n}");
        }
        assert!(markdown.contains("## Needs an answer — 20"));
        assert!(markdown.contains("Waiting on a person to settle a question."));
        // Header and rule carry one fewer column without the merge requests.
        assert!(markdown.contains("|Ticket|Title|Age|"));
        assert!(markdown.contains("|---|---|---|"));
        assert!(markdown.contains("Last updated **Friday 4 September, 8:00 AM PDT**"));
        assert!(markdown.contains("next update **Monday 7 September**"));
        assert!(!markdown.contains("and 8 more"));
    }

    #[test]
    fn an_empty_section_prints_no_table_at_all() {
        let markdown = canvas_markdown(
            "Friday 4 September, 8:00 AM PDT",
            "Monday 7 September",
            &[section(DigestSectionId::NotMoving, Vec::new())],
        );
        assert!(!markdown.contains("## Not moving"));
        assert!(markdown.contains("Nothing is waiting on a person today."));
    }

    #[test]
    fn a_line_with_a_merge_request_links_to_it_by_number() {
        let out = digest_line(&item(
            "BUG-10079",
            "BUG-10079: Generated PPT requires repair",
            Some("https://gitlab.com/example-org/example-app/-/merge_requests/9256"),
        ));
        assert!(out.contains("|!9256>"), "{out}");
        // The importer's key prefix is dropped; the key stays searchable.
        assert!(out.contains("Generated PPT requires repair"), "{out}");
        assert!(!out.contains("BUG-10079: Generated"), "{out}");
        assert!(out.contains("`BUG-10079`"), "{out}");
    }

    #[test]
    fn a_line_without_one_links_to_the_ticket() {
        let out = digest_line(&item(
            "BUG-9714",
            "BUG-9714: Account Name dropdown is cut off",
            None,
        ));
        assert!(out.contains("|BUG-9714>"), "{out}");
        assert!(out.contains("Account Name dropdown is cut off"), "{out}");
    }

    #[test]
    fn the_message_caps_a_long_section_when_there_is_no_canvas() {
        let items: Vec<DigestItem> = (0..20)
            .map(|n| item(&format!("BUG-{n}"), &format!("BUG-{n}: t"), None))
            .collect();
        let digest = Digest {
            sections: vec![section(DigestSectionId::NeedsMerge, items)],
            ..match every_event().remove(5) {
                Event::DailyDigest(digest) => digest,
                _ => unreachable!(),
            }
        };
        let text = digest_blocks(&digest, false).to_string();
        assert!(text.contains("…and 8 more"), "{text}");
        assert!(!text.contains("BUG-12>"), "{text}");
    }

    #[test]
    fn slack_needs_a_webhook_or_a_bot_with_a_channel() {
        let missing = |fields: &[(&str, &str)]| {
            matches!(
                SlackNotifier::new(&settings(NotifierKind::Slack, fields), &context()),
                Err(NotifyError::NotConfigured(_))
            )
        };
        assert!(missing(&[]));
        assert!(missing(&[("bot_token", "xoxb-1")]));
        assert!(!missing(&[("bot_token", "xoxb-1"), ("channel", "C1")]));
        assert!(!missing(&[("webhook_url", "https://hooks.slack.com/x")]));
    }

    #[tokio::test]
    async fn a_webhook_target_posts_the_rendered_text() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/hook"))
            .and(body_partial_json(json!({
                "text": "🔴 BUG-7: review loop hit the 3-round cap — waiting in Review for a human"
            })))
            .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
            .expect(1)
            .mount(&server)
            .await;
        let slack = notifier(
            &[("webhook_url", &format!("{}/hook", server.uri()))],
            &server.uri(),
        );
        let delivery = slack
            .send(&Event::ReviewLoopCapped {
                issue: issue(),
                rounds: 3,
            })
            .await
            .unwrap();
        assert_eq!(delivery, Delivery::default());
        assert!(slack.conversational().is_none());
    }

    #[tokio::test]
    async fn a_webhook_failure_is_reported() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(404).set_body_string("no_service"))
            .mount(&server)
            .await;
        let slack = notifier(
            &[("webhook_url", &format!("{}/hook", server.uri()))],
            &server.uri(),
        );
        let error = slack.send(&Event::SeatRecovered {}).await.unwrap_err();
        assert!(
            matches!(error, NotifyError::Rejected { status: 404, .. }),
            "{error}"
        );
    }

    #[tokio::test]
    async fn the_bot_opens_an_escalation_thread_and_posts_the_detail_into_it() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat.postMessage"))
            .and(header("authorization", "Bearer xoxb-test"))
            .and(body_partial_json(
                json!({ "thread_ts": "1700000000.000100" }),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "ok": true, "channel": "C1", "ts": "1700000001.000200"
            })))
            .expect(1)
            .named("detail reply")
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/chat.postMessage"))
            .and(body_partial_json(json!({ "channel": "C1" })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "ok": true, "channel": "C1", "ts": "1700000000.000100"
            })))
            .expect(1)
            .mount(&server)
            .await;
        let slack = notifier(
            &[("bot_token", "xoxb-test"), ("channel", "C1")],
            &server.uri(),
        );
        let escalation = every_event().remove(6);
        let delivery = slack.send(&escalation).await.unwrap();
        assert_eq!(
            delivery.thread,
            Some(ThreadRef {
                channel: "C1".to_string(),
                ts: "1700000000.000100".to_string()
            })
        );
        assert!(slack.conversational().is_some());
    }

    #[tokio::test]
    async fn a_failed_bot_post_falls_back_to_the_webhook() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat.postMessage"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({ "ok": false, "error": "not_in_channel" })),
            )
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/hook"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;
        let slack = notifier(
            &[
                ("bot_token", "xoxb-test"),
                ("channel", "C1"),
                ("webhook_url", &format!("{}/hook", server.uri())),
            ],
            &server.uri(),
        );
        let delivery = slack.send(&every_event().remove(6)).await.unwrap();
        assert!(delivery.thread.is_none());
    }

    #[tokio::test]
    async fn replies_are_read_back_newest_after_the_mark() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/conversations.replies"))
            .and(query_param("channel", "C1"))
            .and(query_param("ts", "1700000000.000100"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "ok": true,
                "messages": [
                    { "ts": "1700000000.000100", "text": "escalation", "bot_id": "B1" },
                    { "ts": "1700000005.000000", "text": "old answer", "user": "U1" },
                    { "ts": "1700000009.000000", "text": "try the cache", "user": "U2" },
                ]
            })))
            .mount(&server)
            .await;
        let slack = notifier(
            &[("bot_token", "xoxb-test"), ("channel", "C1")],
            &server.uri(),
        );
        let thread = ThreadRef {
            channel: "C1".to_string(),
            ts: "1700000000.000100".to_string(),
        };
        let all = slack
            .conversational()
            .unwrap()
            .replies(&thread, None)
            .await
            .unwrap();
        assert_eq!(all.len(), 3);
        assert!(all[0].is_bot);
        let newer = slack
            .conversational()
            .unwrap()
            .replies(&thread, parse_reply_mark("1700000005.000000"))
            .await
            .unwrap();
        assert_eq!(newer.len(), 1);
        assert_eq!(newer[0].text, "try the cache");
        assert_eq!(newer[0].user.as_deref(), Some("U2"));
        assert!(!newer[0].is_bot);
    }

    #[tokio::test]
    async fn the_bot_test_reads_its_identity() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/auth.test"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "ok": true, "team": "Acme", "user": "prepcook"
            })))
            .mount(&server)
            .await;
        let slack = notifier(
            &[("bot_token", "xoxb-test"), ("channel", "C1")],
            &server.uri(),
        );
        assert_eq!(slack.test().await.unwrap(), "Signed in to Acme as prepcook");
    }

    #[tokio::test]
    async fn the_digest_keeps_a_canvas_with_a_bot_and_posts_the_summary() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/conversations.info"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "ok": true,
                "channel": { "properties": { "canvas": { "document_id": "F1" } } }
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/canvases.edit"))
            .and(body_partial_json(json!({ "canvas_id": "F1" })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "ok": true })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/chat.postMessage"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "ok": true, "channel": "C1", "ts": "1.0"
            })))
            .expect(1)
            .mount(&server)
            .await;
        let slack = notifier(
            &[("bot_token", "xoxb-test"), ("channel", "C1")],
            &server.uri(),
        );
        slack.send(&every_event().remove(5)).await.unwrap();
        let posted = &server.received_requests().await.unwrap();
        let message = posted
            .iter()
            .find(|request| request.url.path() == "/chat.postMessage")
            .unwrap();
        let body: Value = serde_json::from_slice(&message.body).unwrap();
        assert!(
            body.to_string()
                .contains("canvas at the top of this channel")
        );
        // The P0 is called out in the message even though the list is on the
        // canvas.
        assert!(body.to_string().contains("P0"));
    }
}
