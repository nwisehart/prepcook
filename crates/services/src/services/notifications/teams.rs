//! Microsoft Teams: an incoming webhook (a Teams workflow's "post to a channel
//! when a webhook request is received", or a legacy connector URL), one
//! Adaptive Card per event.

use async_trait::async_trait;
use serde_json::{Value, json};

use super::{
    Conversational, Delivery, Event, HTTP_TIMEOUT, Notifier, NotifyError, Severity, TargetSettings,
    redact_value,
};
use crate::services::integrations::{Descriptor, Field, FieldKind};

pub struct TeamsNotifier {
    client: reqwest::Client,
    webhook_url: String,
}

impl TeamsNotifier {
    pub fn descriptor() -> Descriptor {
        Descriptor::new(
            "teams",
            "Microsoft Teams",
            "Posts each event as an Adaptive Card to a Teams channel through an incoming \
             webhook.",
            vec![Field::new(
                "webhook_url",
                "Webhook URL",
                FieldKind::Secret,
                true,
                "The URL of a Teams workflow that posts webhook requests to a channel, or of a \
                 channel's Incoming Webhook connector.",
            )],
            "https://learn.microsoft.com/en-us/microsoftteams/platform/webhooks-and-connectors/how-to/add-incoming-webhook",
        )
    }

    pub fn new(settings: &TargetSettings) -> Result<Self, NotifyError> {
        let webhook_url = settings
            .get("webhook_url")
            .ok_or_else(|| NotifyError::NotConfigured("Teams needs a webhook URL".to_string()))?;
        Ok(Self {
            client: reqwest::Client::builder()
                .timeout(HTTP_TIMEOUT)
                .build()
                .map_err(NotifyError::http)?,
            webhook_url: webhook_url.to_string(),
        })
    }

    async fn post_card(&self, card: Value) -> Result<(), NotifyError> {
        let body = redact_value(json!({
            "type": "message",
            "attachments": [{
                "contentType": "application/vnd.microsoft.card.adaptive",
                "contentUrl": null,
                "content": card,
            }],
        }));
        let response = self
            .client
            .post(&self.webhook_url)
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
}

/// The Adaptive Card colour name for a severity.
fn color(severity: Severity) -> &'static str {
    match severity {
        Severity::Info => "Good",
        Severity::Warn => "Warning",
        Severity::Urgent => "Attention",
    }
}

/// The Adaptive Card for an event: the headline, the body, and a button to
/// the event's link.
pub(crate) fn card(event: &Event) -> Value {
    let mut body = vec![json!({
        "type": "TextBlock",
        "text": event.headline(),
        "weight": "Bolder",
        "size": "Medium",
        "color": color(event.severity()),
        "wrap": true,
    })];
    let text = event.body();
    if !text.is_empty() {
        body.push(json!({ "type": "TextBlock", "text": text, "wrap": true }));
    }
    let mut card = json!({
        "$schema": "http://adaptivecards.io/schemas/adaptive-card.json",
        "type": "AdaptiveCard",
        "version": "1.4",
        "body": body,
    });
    if let Some(link) = event.link() {
        card["actions"] = json!([{ "type": "Action.OpenUrl", "title": "Open", "url": link }]);
    }
    card
}

#[async_trait]
impl Notifier for TeamsNotifier {
    fn kind(&self) -> &'static str {
        "teams"
    }

    async fn send(&self, event: &Event) -> Result<Delivery, NotifyError> {
        self.post_card(card(event)).await?;
        Ok(Delivery::default())
    }

    fn conversational(&self) -> Option<&dyn Conversational> {
        None
    }

    async fn test(&self) -> Result<String, NotifyError> {
        // An incoming webhook has nothing to read; posting is the only check.
        self.post_card(json!({
            "$schema": "http://adaptivecards.io/schemas/adaptive-card.json",
            "type": "AdaptiveCard",
            "version": "1.4",
            "body": [{
                "type": "TextBlock",
                "text": "Prepcook test notification: this webhook works.",
                "wrap": true,
            }],
        }))
        .await?;
        Ok("Posted a test card through the webhook".to_string())
    }
}

#[cfg(test)]
mod tests {
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{body_partial_json, method, path},
    };

    use super::{
        super::{NotifierKind, test_support::*},
        *,
    };

    fn notifier(url: &str) -> TeamsNotifier {
        TeamsNotifier::new(&settings(NotifierKind::Teams, &[("webhook_url", url)])).unwrap()
    }

    #[tokio::test]
    async fn an_event_is_posted_as_an_adaptive_card() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/workflows/1"))
            .and(body_partial_json(json!({
                "type": "message",
                "attachments": [{ "contentType": "application/vnd.microsoft.card.adaptive" }]
            })))
            .respond_with(ResponseTemplate::new(202))
            .expect(1)
            .mount(&server)
            .await;
        notifier(&format!("{}/workflows/1", server.uri()))
            .send(&Event::BlockerMrOpened {
                issue: issue(),
                mr_url: "https://gitlab.example.com/acme/app/-/merge_requests/12".to_string(),
                summary: "Export drops the last row".to_string(),
            })
            .await
            .unwrap();
        let request = &server.received_requests().await.unwrap()[0];
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        let card = &body["attachments"][0]["content"];
        assert_eq!(card["type"], "AdaptiveCard");
        assert_eq!(
            card["body"][0]["text"],
            "Blocker merge request needs review — BUG-7"
        );
        assert_eq!(card["body"][0]["color"], "Warning");
        assert_eq!(
            card["actions"][0]["url"],
            "https://gitlab.example.com/acme/app/-/merge_requests/12"
        );
    }

    #[test]
    fn every_event_has_a_headline() {
        for event in every_event() {
            let card = card(&event);
            assert!(!card["body"][0]["text"].as_str().unwrap().is_empty());
        }
    }

    #[tokio::test]
    async fn the_test_posts_a_card() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;
        assert_eq!(
            notifier(&server.uri()).test().await.unwrap(),
            "Posted a test card through the webhook"
        );
    }
}
