//! Datadog Logs, searched with the Logs Search API v2
//! (`POST /api/v2/logs/events/search`, an API key and an application key).

use async_trait::async_trait;
use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{Value, json};

use super::{
    EvidenceError, EvidenceKind, EvidenceSource, HTTP_TIMEOUT, Hit, McpServerEntry, SearchQuery,
    SearchResult, SourceSettings, capped, quoted, urlencode, with_common_fields,
};
use crate::services::integrations::{Descriptor, Field, FieldKind};

const DEFAULT_SITE: &str = "datadoghq.com";
/// The most logs one request returns.
const PAGE_CAP: usize = 1_000;

pub struct DatadogSource {
    settings: SourceSettings,
    /// `https://api.datadoghq.com`, or the region's API host.
    api: String,
    /// `https://app.datadoghq.com`, for links.
    app: String,
    api_key: String,
    application_key: String,
    /// Narrows every search (`service:web env:production`).
    base_query: Option<String>,
    client: reqwest::Client,
}

impl DatadogSource {
    pub fn descriptor() -> Descriptor {
        Descriptor::new(
            EvidenceKind::Datadog.as_str(),
            EvidenceKind::Datadog.display_name(),
            "Logs in Datadog, searched with the Logs Search API.",
            with_common_fields(
                EvidenceKind::Datadog,
                vec![
                    Field::new(
                        "api_key",
                        "API key",
                        FieldKind::Secret,
                        true,
                        "An API key (Organization Settings → API Keys).",
                    ),
                    Field::new(
                        "application_key",
                        "Application key",
                        FieldKind::Secret,
                        true,
                        "An application key with the logs_read_data scope (Organization Settings → \
                         Application Keys).",
                    ),
                    Field::new(
                        "site",
                        "Site",
                        FieldKind::Text,
                        false,
                        "Your Datadog site, e.g. datadoghq.eu or us5.datadoghq.com. Blank is \
                         datadoghq.com.",
                    ),
                    Field::new(
                        "query",
                        "Base query",
                        FieldKind::Text,
                        false,
                        "Log search terms every search is narrowed by, e.g. service:web \
                         env:production.",
                    ),
                ],
            ),
            "https://docs.datadoghq.com/api/latest/logs/#search-logs",
        )
    }

    pub fn new(settings: &SourceSettings) -> Result<Self, EvidenceError> {
        let api_key = settings
            .get("api_key")
            .ok_or_else(|| EvidenceError::NotConfigured("a Datadog API key".to_string()))?
            .to_string();
        let application_key = settings
            .get("application_key")
            .ok_or_else(|| EvidenceError::NotConfigured("a Datadog application key".to_string()))?
            .to_string();
        let site = settings.get("site").unwrap_or(DEFAULT_SITE);
        // A full URL is used as written, for a proxy or a test double.
        let (api, app) = if site.starts_with("http://") || site.starts_with("https://") {
            let site = site.trim_end_matches('/').to_string();
            (site.clone(), site)
        } else {
            let site = site.trim_end_matches('/');
            (format!("https://api.{site}"), format!("https://app.{site}"))
        };
        let client = reqwest::Client::builder()
            .timeout(HTTP_TIMEOUT)
            .build()
            .map_err(EvidenceError::http)?;
        Ok(Self {
            settings: settings.clone(),
            api,
            app,
            api_key,
            application_key,
            base_query: settings.get("query").map(str::to_string),
            client,
        })
    }

    fn query_for(&self, pattern: &str) -> String {
        let pattern = pattern.trim();
        match (&self.base_query, pattern.is_empty()) {
            (Some(base), false) => format!("{base} {}", quoted(pattern)),
            (Some(base), true) => base.clone(),
            (None, false) => quoted(pattern),
            (None, true) => "*".to_string(),
        }
    }

    async fn search_logs(
        &self,
        query: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        limit: usize,
    ) -> Result<Value, EvidenceError> {
        let response = self
            .client
            .post(format!("{}/api/v2/logs/events/search", self.api))
            .header("DD-API-KEY", &self.api_key)
            .header("DD-APPLICATION-KEY", &self.application_key)
            .json(&json!({
                "filter": {
                    "query": query,
                    "from": from.to_rfc3339_opts(SecondsFormat::Secs, true),
                    "to": to.to_rfc3339_opts(SecondsFormat::Secs, true),
                },
                "sort": "-timestamp",
                "page": { "limit": limit },
            }))
            .send()
            .await
            .map_err(EvidenceError::http)?;
        if !response.status().is_success() {
            return Err(EvidenceError::rejected(response).await);
        }
        response
            .json()
            .await
            .map_err(|e| EvidenceError::Api(format!("Datadog answered something unreadable: {e}")))
    }
}

#[async_trait]
impl EvidenceSource for DatadogSource {
    fn kind(&self) -> &'static str {
        EvidenceKind::Datadog.as_str()
    }

    fn label(&self) -> String {
        self.settings.label()
    }

    fn describe(&self) -> String {
        let narrowed = self
            .base_query
            .as_deref()
            .map(|query| format!(", narrowed to `{query}`"))
            .unwrap_or_default();
        let mut out = format!(
            "{}: the app's logs in Datadog{narrowed}. `evidence_search` with source \"{}\" \
             searches them for the pattern over the window and returns the newest matching logs \
             with their service, host, status and attributes, and a link to the same search in \
             Datadog. Look for the failing request and the error logged with it, then at what the \
             same service logged around it.",
            self.settings.heading(),
            self.label()
        );
        if let Some(notes) = self.settings.notes() {
            out.push(' ');
            out.push_str(&notes);
        }
        out
    }

    fn mcp_server(&self) -> Option<McpServerEntry> {
        None
    }

    async fn search(&self, query: &SearchQuery) -> Result<Option<SearchResult>, EvidenceError> {
        let (from, to) = query.window;
        let terms = self.query_for(&query.pattern);
        let body = self
            .search_logs(&terms, from, to, (query.limit + 1).min(PAGE_CAP))
            .await?;
        let logs = body["data"].as_array().cloned().unwrap_or_default();
        let more = body["meta"]["page"]["after"].is_string();
        let fetched = logs.len();
        let link = format!(
            "{}/logs?query={}&from_ts={}&to_ts={}&live=false",
            self.app,
            urlencode(&terms),
            from.timestamp_millis(),
            to.timestamp_millis()
        );
        let label = self.label();
        let hits = logs
            .into_iter()
            .map(|log| {
                let attributes = &log["attributes"];
                let at = attributes["timestamp"]
                    .as_str()
                    .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
                    .map(|at| at.with_timezone(&Utc));
                let mut hit = Hit::new(
                    &label,
                    at,
                    attributes["message"].as_str().unwrap_or_default(),
                )
                .link(Some(link.clone()))
                .field("id", log["id"].clone())
                .field("service", attributes["service"].clone())
                .field("host", attributes["host"].clone())
                .field("status", attributes["status"].clone());
                if let Some(extra) = attributes["attributes"].as_object() {
                    for (key, value) in extra {
                        hit = hit.field(key, value.clone());
                    }
                }
                hit
            })
            .collect();
        Ok(Some(capped(
            hits,
            query.limit,
            more || fetched > query.limit,
        )))
    }

    async fn test(&self) -> Result<String, EvidenceError> {
        let to = Utc::now();
        let from = to - chrono::Duration::minutes(15);
        self.search_logs(&self.query_for(""), from, to, 1).await?;
        Ok("Datadog accepted both keys and answered a log search".to_string())
    }
}

#[cfg(test)]
mod tests {
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{body_partial_json, header, method, path},
    };

    use super::*;
    use crate::services::evidence::test_support::*;

    fn source(server: &MockServer, extra: &[(&str, &str)]) -> DatadogSource {
        let uri = server.uri();
        let mut fields = vec![
            ("api_key", "dd-api"),
            ("application_key", "dd-app"),
            ("site", uri.as_str()),
        ];
        fields.extend_from_slice(extra);
        DatadogSource::new(&settings(EvidenceKind::Datadog, &fields)).unwrap()
    }

    /// A trimmed Logs Search v2 answer as Datadog gives it.
    fn logs_fixture(after: Option<&str>) -> Value {
        json!({
            "data": [
                {
                    "id": "AQAAAY1",
                    "type": "log",
                    "attributes": {
                        "timestamp": "2026-10-18T10:00:00.123Z",
                        "message": "share failed: SeedMissing household",
                        "service": "pantry-web",
                        "host": "web-1",
                        "status": "error",
                        "tags": ["env:production"],
                        "attributes": { "http": { "status_code": 500 } }
                    }
                }
            ],
            "meta": { "page": after.map(|cursor| json!({ "after": cursor })).unwrap_or(json!({})) }
        })
    }

    #[tokio::test]
    async fn search_posts_the_window_and_reads_the_logs() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v2/logs/events/search"))
            .and(header("DD-API-KEY", "dd-api"))
            .and(header("DD-APPLICATION-KEY", "dd-app"))
            .and(body_partial_json(json!({
                "filter": {
                    "query": "service:pantry-web \"SeedMissing\"",
                    "from": "2026-10-18T00:00:00Z",
                    "to": "2026-10-18T12:00:00Z",
                },
                "sort": "-timestamp",
                "page": { "limit": 11 },
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(logs_fixture(None)))
            .mount(&server)
            .await;
        let datadog = source(&server, &[("query", "service:pantry-web")]);
        let result = datadog
            .search(&query(
                "SeedMissing",
                "2026-10-18T00:00:00Z",
                "2026-10-18T12:00:00Z",
                10,
            ))
            .await
            .unwrap()
            .unwrap();
        assert!(!result.truncated);
        let hit = &result.hits[0];
        assert_eq!(hit.at, Some(at("2026-10-18T10:00:00.123Z")));
        assert_eq!(hit.message, "share failed: SeedMissing household");
        assert_eq!(hit.fields["service"], json!("pantry-web"));
        assert_eq!(hit.fields["http"], json!({ "status_code": 500 }));
        assert!(
            hit.link
                .as_deref()
                .unwrap()
                .contains("from_ts=1792281600000")
        );
    }

    #[tokio::test]
    async fn a_next_page_cursor_truncates_and_rejections_carry_the_status() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v2/logs/events/search"))
            .and(header("DD-APPLICATION-KEY", "dd-app"))
            .respond_with(ResponseTemplate::new(200).set_body_json(logs_fixture(Some("c1"))))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/v2/logs/events/search"))
            .and(header("DD-APPLICATION-KEY", "wrong"))
            .respond_with(
                ResponseTemplate::new(403).set_body_json(json!({ "errors": ["Forbidden"] })),
            )
            .mount(&server)
            .await;
        let result = source(&server, &[])
            .search(&query(
                "x",
                "2026-10-18T00:00:00Z",
                "2026-10-18T12:00:00Z",
                10,
            ))
            .await
            .unwrap()
            .unwrap();
        assert!(result.truncated);
        let wrong = source(&server, &[("application_key", "wrong")]);
        assert!(matches!(
            wrong.test().await.unwrap_err(),
            EvidenceError::Rejected { status: 403, .. }
        ));
    }

    #[tokio::test]
    async fn the_test_runs_a_one_log_search() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v2/logs/events/search"))
            .and(body_partial_json(
                json!({ "page": { "limit": 1 }, "filter": { "query": "*" } }),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(logs_fixture(None)))
            .mount(&server)
            .await;
        assert_eq!(
            source(&server, &[]).test().await.unwrap(),
            "Datadog accepted both keys and answered a log search"
        );
    }

    #[test]
    fn describe_snapshot() {
        let datadog = DatadogSource::new(&settings(
            EvidenceKind::Datadog,
            &[
                ("api_key", "a"),
                ("application_key", "b"),
                ("site", "datadoghq.eu"),
                ("label", "Web logs"),
                ("query", "service:web"),
            ],
        ))
        .unwrap();
        assert_eq!(datadog.api, "https://api.datadoghq.eu");
        assert_eq!(
            datadog.describe(),
            "Web logs (Datadog): the app's logs in Datadog, narrowed to `service:web`. \
             `evidence_search` with source \"Web logs\" searches them for the pattern over the \
             window and returns the newest matching logs with their service, host, status and \
             attributes, and a link to the same search in Datadog. Look for the failing request \
             and the error logged with it, then at what the same service logged around it."
        );
        assert!(datadog.mcp_server().is_none());
        assert!(DatadogSource::new(&settings(EvidenceKind::Datadog, &[("api_key", "a")])).is_err());
    }
}
