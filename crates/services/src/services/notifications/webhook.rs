//! A generic JSON webhook: every event POSTed as one signed JSON document.
//!
//! The payload is `{ "type", "severity", "occurred_at", "prepcook": {
//! "version", "board_url" }, "data" }`, with `data` the event's fields
//! (`docs/notifications.md` lists them per type). Headers name the event
//! (`X-PrepCook-Event`), the delivery (`X-PrepCook-Delivery`, the same uuid on
//! every retry so a receiver can drop duplicates) and, with a secret
//! configured, sign the exact body bytes (`X-PrepCook-Signature:
//! sha256=<hex HMAC-SHA256>`). A 5xx or a timeout is retried three times with
//! backoff; any other failure is final.

use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use hmac::{Hmac, Mac};
use serde_json::{Value, json};
use sha2::Sha256;

use super::{
    Context, Conversational, Delivery, Event, HTTP_TIMEOUT, Notifier, NotifyError, TargetSettings,
    redact_value,
};
use crate::services::integrations::{Descriptor, Field, FieldKind};

/// Retries after the first attempt.
const RETRIES: u32 = 3;
/// The first retry's delay; each later one doubles it.
const BACKOFF: Duration = Duration::from_millis(500);

pub const EVENT_HEADER: &str = "X-PrepCook-Event";
pub const DELIVERY_HEADER: &str = "X-PrepCook-Delivery";
pub const SIGNATURE_HEADER: &str = "X-PrepCook-Signature";

pub struct WebhookNotifier {
    client: reqwest::Client,
    url: String,
    secret: Option<String>,
    board_url: String,
    backoff: Duration,
}

impl WebhookNotifier {
    pub fn descriptor() -> Descriptor {
        Descriptor::new(
            "webhook",
            "Webhook",
            "POSTs every event as signed JSON to a URL you run, for wiring Prepcook into \
             anything else.",
            vec![
                Field::new(
                    "url",
                    "URL",
                    FieldKind::Url,
                    true,
                    "Where events are POSTed. A 5xx or timeout is retried three times.",
                ),
                Field::new(
                    "secret",
                    "Signing secret",
                    FieldKind::Secret,
                    false,
                    "Signs each body with HMAC-SHA256 in the X-PrepCook-Signature header. \
                     Unset sends unsigned requests.",
                ),
            ],
            "https://github.com/nwisehart/prepcook/blob/main/docs/notifications.md",
        )
    }

    pub fn new(settings: &TargetSettings, context: &Context) -> Result<Self, NotifyError> {
        let url = settings
            .get("url")
            .ok_or_else(|| NotifyError::NotConfigured("a webhook needs a URL".to_string()))?;
        Ok(Self {
            client: reqwest::Client::builder()
                .timeout(HTTP_TIMEOUT)
                .build()
                .map_err(NotifyError::http)?,
            url: url.to_string(),
            secret: settings.get("secret").map(str::to_string),
            board_url: context.board_url.clone(),
            backoff: BACKOFF,
        })
    }

    /// Shortens the retry backoff (tests).
    pub fn with_backoff(mut self, backoff: Duration) -> Self {
        self.backoff = backoff;
        self
    }

    /// The document POSTed for an event of `kind`.
    fn payload(&self, kind: &str, severity: &str, data: Value) -> Value {
        redact_value(json!({
            "type": kind,
            "severity": severity,
            "occurred_at": Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            "prepcook": {
                "version": utils::version::APP_VERSION,
                "board_url": self.board_url,
            },
            "data": data,
        }))
    }

    /// POSTs `payload`, retrying 5xx answers and timeouts. Returns the final
    /// status.
    async fn deliver(&self, kind: &str, payload: &Value) -> Result<u16, NotifyError> {
        let body = serde_json::to_vec(payload).map_err(|e| NotifyError::Api(e.to_string()))?;
        let delivery = uuid::Uuid::new_v4().to_string();
        let signature = self.secret.as_deref().map(|secret| sign(secret, &body));
        let mut attempt = 0;
        loop {
            let mut request = self
                .client
                .post(&self.url)
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .header(EVENT_HEADER, kind)
                .header(DELIVERY_HEADER, &delivery)
                .body(body.clone());
            if let Some(signature) = &signature {
                request = request.header(SIGNATURE_HEADER, signature);
            }
            let outcome = match request.send().await {
                Ok(response) if response.status().is_success() => {
                    return Ok(response.status().as_u16());
                }
                Ok(response) => {
                    let status = response.status();
                    let error = NotifyError::Rejected {
                        status: status.as_u16(),
                        body: response.text().await.unwrap_or_default(),
                    };
                    if !status.is_server_error() {
                        return Err(error);
                    }
                    error
                }
                Err(error) if error.is_timeout() || error.is_connect() => NotifyError::http(error),
                Err(error) => return Err(NotifyError::http(error)),
            };
            if attempt == RETRIES {
                return Err(outcome);
            }
            tokio::time::sleep(self.backoff * 2u32.pow(attempt)).await;
            attempt += 1;
        }
    }
}

/// `sha256=<hex HMAC-SHA256 of body under secret>`.
pub fn sign(secret: &str, body: &[u8]) -> String {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(body);
    format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
}

#[async_trait]
impl Notifier for WebhookNotifier {
    fn kind(&self) -> &'static str {
        "webhook"
    }

    async fn send(&self, event: &Event) -> Result<Delivery, NotifyError> {
        let payload = self.payload(event.kind(), event.severity().as_str(), event.data());
        self.deliver(event.kind(), &payload).await?;
        Ok(Delivery::default())
    }

    fn conversational(&self) -> Option<&dyn Conversational> {
        None
    }

    async fn test(&self) -> Result<String, NotifyError> {
        // A receiver has nothing to read; a `ping` exercises the whole path,
        // signature included.
        let payload = self.payload("ping", "info", json!({}));
        let status = self.deliver("ping", &payload).await?;
        Ok(format!("Delivered a ping (HTTP {status})"))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{header, header_exists, method, path},
    };

    use super::{
        super::{NotifierKind, test_support::*},
        *,
    };

    fn notifier(url: &str, secret: Option<&str>) -> WebhookNotifier {
        let mut fields = vec![("url", url)];
        if let Some(secret) = secret {
            fields.push(("secret", secret));
        }
        WebhookNotifier::new(&settings(NotifierKind::Webhook, &fields), &context())
            .unwrap()
            .with_backoff(Duration::from_millis(1))
    }

    #[tokio::test]
    async fn the_body_is_signed_and_the_envelope_carries_the_event() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/hook"))
            .and(header(EVENT_HEADER, "review_loop_capped"))
            .and(header_exists(DELIVERY_HEADER))
            .and(header_exists(SIGNATURE_HEADER))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;
        let webhook = notifier(&format!("{}/hook", server.uri()), Some("shh-its-a-secret"));
        webhook
            .send(&Event::ReviewLoopCapped {
                issue: issue(),
                rounds: 3,
            })
            .await
            .unwrap();

        let request = &server.received_requests().await.unwrap()[0];
        let signature = request
            .headers
            .get(SIGNATURE_HEADER)
            .unwrap()
            .to_str()
            .unwrap();
        // The signature is over the exact bytes received.
        assert_eq!(signature, sign("shh-its-a-secret", &request.body));
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        assert_eq!(body["type"], "review_loop_capped");
        assert_eq!(body["severity"], "urgent");
        assert_eq!(
            body["prepcook"]["board_url"],
            "https://prepcook.example.com/projects/board-1"
        );
        assert_eq!(body["prepcook"]["version"], utils::version::APP_VERSION);
        assert!(body["occurred_at"].as_str().unwrap().ends_with('Z'));
        assert_eq!(body["data"]["rounds"], 3);
        assert_eq!(body["data"]["issue"]["key"], "BUG-7");
    }

    #[test]
    fn the_signature_is_hex_hmac_sha256() {
        // RFC 4231 test case 2.
        assert_eq!(
            sign("Jefe", b"what do ya want for nothing?"),
            "sha256=5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[tokio::test]
    async fn without_a_secret_no_signature_is_sent() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;
        notifier(&server.uri(), None)
            .send(&Event::SeatRecovered {})
            .await
            .unwrap();
        let request = &server.received_requests().await.unwrap()[0];
        assert!(request.headers.get(SIGNATURE_HEADER).is_none());
    }

    #[tokio::test]
    async fn a_server_error_is_retried_with_the_same_delivery_id() {
        let server = MockServer::start().await;
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = calls.clone();
        Mock::given(method("POST"))
            .respond_with(move |_: &wiremock::Request| {
                // Two failures, then success.
                if counter.fetch_add(1, Ordering::SeqCst) < 2 {
                    ResponseTemplate::new(503)
                } else {
                    ResponseTemplate::new(200)
                }
            })
            .mount(&server)
            .await;
        notifier(&server.uri(), Some("secret-value"))
            .send(&Event::SeatExpired {})
            .await
            .unwrap();
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 3);
        let ids: Vec<_> = requests
            .iter()
            .map(|request| request.headers.get(DELIVERY_HEADER).unwrap().clone())
            .collect();
        assert!(ids.windows(2).all(|pair| pair[0] == pair[1]));
    }

    #[tokio::test]
    async fn retries_stop_after_three() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(500))
            .expect(4)
            .mount(&server)
            .await;
        let error = notifier(&server.uri(), None)
            .send(&Event::SeatExpired {})
            .await
            .unwrap_err();
        assert!(matches!(error, NotifyError::Rejected { status: 500, .. }));
    }

    #[tokio::test]
    async fn a_client_error_is_not_retried() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(410))
            .expect(1)
            .mount(&server)
            .await;
        assert!(
            notifier(&server.uri(), None)
                .send(&Event::SeatExpired {})
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn the_test_sends_a_ping() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(header(EVENT_HEADER, "ping"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;
        assert_eq!(
            notifier(&server.uri(), None).test().await.unwrap(),
            "Delivered a ping (HTTP 200)"
        );
    }
}
