//! Discord: a channel webhook, one embed per event.

use async_trait::async_trait;
use serde_json::{Value, json};

use super::{
    Conversational, Delivery, Event, HTTP_TIMEOUT, Notifier, NotifyError, Severity, TargetSettings,
    clip, redact_value,
};
use crate::services::integrations::{Descriptor, Field, FieldKind};

/// Discord's limits: an embed title is 256 characters, a description 4096.
const TITLE_MAX: usize = 256;
const DESCRIPTION_MAX: usize = 4000;

pub struct DiscordNotifier {
    client: reqwest::Client,
    webhook_url: String,
}

impl DiscordNotifier {
    pub fn descriptor() -> Descriptor {
        Descriptor::new(
            "discord",
            "Discord",
            "Posts each event as an embed to a Discord channel through its webhook.",
            vec![Field::new(
                "webhook_url",
                "Webhook URL",
                FieldKind::Secret,
                true,
                "The channel's webhook (Channel settings → Integrations → Webhooks), \
                 https://discord.com/api/webhooks/….",
            )],
            "https://support.discord.com/hc/en-us/articles/228383668",
        )
    }

    pub fn new(settings: &TargetSettings) -> Result<Self, NotifyError> {
        let webhook_url = settings
            .get("webhook_url")
            .ok_or_else(|| NotifyError::NotConfigured("Discord needs a webhook URL".to_string()))?;
        Ok(Self {
            client: reqwest::Client::builder()
                .timeout(HTTP_TIMEOUT)
                .build()
                .map_err(NotifyError::http)?,
            webhook_url: webhook_url.to_string(),
        })
    }
}

fn color(severity: Severity) -> u32 {
    match severity {
        Severity::Info => 0x2E_B6_7D,
        Severity::Warn => 0xEC_B2_2E,
        Severity::Urgent => 0xE0_1E_5A,
    }
}

/// The embed for an event.
pub(crate) fn embed(event: &Event) -> Value {
    let mut embed = json!({
        "title": clip(&event.headline(), TITLE_MAX),
        "description": clip(&event.body(), DESCRIPTION_MAX),
        "color": color(event.severity()),
        "footer": { "text": format!("Prepcook · {}", event.kind()) },
    });
    if let Some(link) = event.link() {
        embed["url"] = json!(link);
    }
    embed
}

#[async_trait]
impl Notifier for DiscordNotifier {
    fn kind(&self) -> &'static str {
        "discord"
    }

    async fn send(&self, event: &Event) -> Result<Delivery, NotifyError> {
        let body = redact_value(json!({
            "username": "Prepcook",
            "embeds": [embed(event)],
            // Event text quotes people and tickets; it must never ping anyone.
            "allowed_mentions": { "parse": [] },
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
            Ok(Delivery::default())
        } else {
            Err(NotifyError::Rejected {
                status: status.as_u16(),
                body: response.text().await.unwrap_or_default(),
            })
        }
    }

    fn conversational(&self) -> Option<&dyn Conversational> {
        None
    }

    async fn test(&self) -> Result<String, NotifyError> {
        // A GET on a webhook URL returns the webhook itself.
        let response = self
            .client
            .get(&self.webhook_url)
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
        Ok(format!(
            "Webhook “{}” is reachable",
            body.get("name").and_then(Value::as_str).unwrap_or("?")
        ))
    }
}

#[cfg(test)]
mod tests {
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    use super::{
        super::{NotifierKind, test_support::*},
        *,
    };

    fn notifier(url: &str) -> DiscordNotifier {
        DiscordNotifier::new(&settings(NotifierKind::Discord, &[("webhook_url", url)])).unwrap()
    }

    #[tokio::test]
    async fn an_event_is_posted_as_one_embed() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/webhooks/1/abc"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;
        let discord = notifier(&format!("{}/api/webhooks/1/abc", server.uri()));
        discord
            .send(&Event::PipelineEscalated {
                issue: issue(),
                mr_url: "https://gitlab.example.com/acme/app/-/merge_requests/12".to_string(),
                reason: "lint failed twice".to_string(),
            })
            .await
            .unwrap();
        let request = &server.received_requests().await.unwrap()[0];
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        let embed = &body["embeds"][0];
        assert_eq!(embed["title"], "BUG-7: MR pipeline needs a human");
        assert_eq!(
            embed["url"],
            "https://gitlab.example.com/acme/app/-/merge_requests/12"
        );
        assert_eq!(embed["color"], 0xE0_1E_5A);
        assert!(
            embed["description"]
                .as_str()
                .unwrap()
                .contains("lint failed twice")
        );
        assert_eq!(body["allowed_mentions"]["parse"], json!([]));
    }

    #[tokio::test]
    async fn every_event_renders_within_discords_limits() {
        for event in every_event() {
            let embed = embed(&event);
            assert!(!embed["title"].as_str().unwrap().is_empty());
            assert!(embed["description"].as_str().unwrap().chars().count() <= DESCRIPTION_MAX);
        }
    }

    #[tokio::test]
    async fn the_test_reads_the_webhook() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/webhooks/1/abc"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({ "name": "prepcook-alerts" })),
            )
            .mount(&server)
            .await;
        let discord = notifier(&format!("{}/api/webhooks/1/abc", server.uri()));
        assert_eq!(
            discord.test().await.unwrap(),
            "Webhook “prepcook-alerts” is reachable"
        );
    }

    #[tokio::test]
    async fn a_rejected_post_is_an_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;
        assert!(matches!(
            notifier(&server.uri()).send(&Event::SeatExpired {}).await,
            Err(NotifyError::Rejected { status: 401, .. })
        ));
    }
}
