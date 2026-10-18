//! DataSet (formerly Scalyr): the logs the app's servers ship there,
//! searched with the query API (`POST /api/query`, a "Read Logs" token in the
//! body).

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::{Value, json};

use super::{
    EvidenceError, EvidenceKind, EvidenceSource, HTTP_TIMEOUT, Hit, McpServerEntry, SearchQuery,
    SearchResult, SourceSettings, capped, quoted, urlencode, with_common_fields,
};
use crate::services::integrations::{Descriptor, Field, FieldKind};

const DEFAULT_SERVER: &str = "https://app.scalyr.com";
/// The most events one query returns.
const MAX_COUNT: usize = 5_000;

pub struct DataSetSource {
    settings: SourceSettings,
    server: String,
    token: String,
    /// A filter every search is narrowed by (`serverHost == "web-1"`).
    filter: Option<String>,
    client: reqwest::Client,
}

impl DataSetSource {
    pub fn descriptor() -> Descriptor {
        Descriptor::new(
            EvidenceKind::Dataset.as_str(),
            EvidenceKind::Dataset.display_name(),
            "Logs shipped to DataSet (Scalyr), searched with its query API.",
            with_common_fields(
                EvidenceKind::Dataset,
                vec![
                    Field::new(
                        "read_token",
                        "Read Logs token",
                        FieldKind::Secret,
                        true,
                        "A \"Read Logs\" API token (API Keys in DataSet's settings).",
                    ),
                    Field::new(
                        "server",
                        "Server",
                        FieldKind::Url,
                        false,
                        "Your account's region, e.g. https://app.eu.scalyr.com. Blank is \
                         https://app.scalyr.com.",
                    ),
                    Field::new(
                        "filter",
                        "Base filter",
                        FieldKind::Text,
                        false,
                        "A DataSet filter every search is narrowed by, e.g. serverHost == \
                         \"web-1\" or $logfile contains \"app\".",
                    ),
                ],
            ),
            "https://app.scalyr.com/help/api#query",
        )
    }

    pub fn new(settings: &SourceSettings) -> Result<Self, EvidenceError> {
        let token = settings
            .get("read_token")
            .ok_or_else(|| EvidenceError::NotConfigured("a DataSet Read Logs token".to_string()))?
            .to_string();
        let client = reqwest::Client::builder()
            .timeout(HTTP_TIMEOUT)
            .build()
            .map_err(EvidenceError::http)?;
        Ok(Self {
            settings: settings.clone(),
            server: settings
                .get("server")
                .unwrap_or(DEFAULT_SERVER)
                .trim_end_matches('/')
                .to_string(),
            token,
            filter: settings.get("filter").map(str::to_string),
            client,
        })
    }

    /// The base filter and the pattern, ANDed.
    fn filter_for(&self, pattern: &str) -> String {
        let pattern = pattern.trim();
        match (&self.filter, pattern.is_empty()) {
            (Some(base), false) => format!("({base}) {}", quoted(pattern)),
            (Some(base), true) => base.clone(),
            (None, false) => quoted(pattern),
            (None, true) => String::new(),
        }
    }

    async fn query(
        &self,
        filter: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        max_count: usize,
    ) -> Result<Value, EvidenceError> {
        let response = self
            .client
            .post(format!("{}/api/query", self.server))
            .json(&json!({
                "token": self.token,
                "queryType": "log",
                "filter": filter,
                "startTime": from.timestamp().to_string(),
                "endTime": to.timestamp().to_string(),
                "maxCount": max_count,
                "pageMode": "tail",
                "priority": "low",
            }))
            .send()
            .await
            .map_err(EvidenceError::http)?;
        if !response.status().is_success() {
            return Err(EvidenceError::rejected(response).await);
        }
        let body: Value = response.json().await.map_err(|e| {
            EvidenceError::Api(format!("DataSet answered something unreadable: {e}"))
        })?;
        if body["status"].as_str() != Some("success") {
            return Err(EvidenceError::Api(format!(
                "DataSet: {}",
                body["message"].as_str().unwrap_or("the query failed")
            )));
        }
        Ok(body)
    }
}

/// DataSet timestamps are nanoseconds since the epoch, as a string.
fn timestamp(value: &Value) -> Option<DateTime<Utc>> {
    let nanos: i64 = match value {
        Value::String(text) => text.parse().ok()?,
        Value::Number(number) => number.as_i64()?,
        _ => return None,
    };
    Some(DateTime::from_timestamp_nanos(nanos))
}

#[async_trait]
impl EvidenceSource for DataSetSource {
    fn kind(&self) -> &'static str {
        EvidenceKind::Dataset.as_str()
    }

    fn label(&self) -> String {
        self.settings.label()
    }

    fn describe(&self) -> String {
        let narrowed = self
            .filter
            .as_deref()
            .map(|filter| format!(", narrowed to `{filter}`"))
            .unwrap_or_default();
        let mut out = format!(
            "{}: the logs the app's servers ship to DataSet{narrowed}. `evidence_search` with \
             source \"{}\" runs a DataSet query for the pattern over the window and returns the \
             newest matching events with their attributes and a link to the same query in \
             DataSet. Look for the request or error the report describes, then at what the same \
             host or session logged just before it.",
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
        let filter = self.filter_for(&query.pattern);
        let wanted = (query.limit + 1).min(MAX_COUNT);
        let body = self.query(&filter, from, to, wanted).await?;
        let matches = body["matches"].as_array().cloned().unwrap_or_default();
        let fetched = matches.len();
        let link = format!(
            "{}/events?filter={}&startTime={}&endTime={}",
            self.server,
            urlencode(&filter),
            from.timestamp(),
            to.timestamp()
        );
        let label = self.label();
        let hits = matches
            .into_iter()
            .map(|event| {
                let mut hit = Hit::new(
                    &label,
                    timestamp(&event["timestamp"]),
                    event["message"].as_str().unwrap_or_default(),
                )
                .link(Some(link.clone()))
                .field("severity", event["severity"].clone())
                .field("thread", event["thread"].clone());
                if let Some(attributes) = event["attributes"].as_object() {
                    for (key, value) in attributes {
                        hit = hit.field(key, value.clone());
                    }
                }
                hit
            })
            .collect();
        Ok(Some(capped(hits, query.limit, fetched > query.limit)))
    }

    async fn test(&self) -> Result<String, EvidenceError> {
        let to = Utc::now();
        let from = to - chrono::Duration::minutes(10);
        let body = self.query(&self.filter_for(""), from, to, 1).await?;
        let found = body["matches"].as_array().map_or(0, Vec::len);
        Ok(if found > 0 {
            "DataSet answered with events from the last 10 minutes".to_string()
        } else {
            "DataSet accepted the token; nothing matched in the last 10 minutes".to_string()
        })
    }
}

#[cfg(test)]
mod tests {
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{body_partial_json, method, path},
    };

    use super::*;
    use crate::services::evidence::test_support::*;

    fn source(server: &MockServer, extra: &[(&str, &str)]) -> DataSetSource {
        let uri = server.uri();
        let mut fields = vec![("read_token", "0readlogs"), ("server", uri.as_str())];
        fields.extend_from_slice(extra);
        DataSetSource::new(&settings(EvidenceKind::Dataset, &fields)).unwrap()
    }

    /// A trimmed `/api/query` answer as DataSet gives it.
    fn query_fixture() -> Value {
        json!({
            "status": "success",
            "matches": [
                {
                    "timestamp": "1792317600000000000",
                    "message": "POST /lists/7/share 500 SeedMissing: no household",
                    "severity": 5,
                    "thread": "1",
                    "attributes": { "status": 500, "path": "/lists/7/share" }
                },
                {
                    "timestamp": "1792314000000000000",
                    "message": "POST /lists/7/share 500 SeedMissing: no household",
                    "severity": 5,
                    "attributes": { "status": 500 }
                }
            ],
            "sessions": {}
        })
    }

    #[tokio::test]
    async fn search_queries_the_window_and_reads_the_matches() {
        let server = MockServer::start().await;
        let from = at("2026-10-18T00:00:00Z");
        let to = at("2026-10-18T12:00:00Z");
        Mock::given(method("POST"))
            .and(path("/api/query"))
            .and(body_partial_json(json!({
                "token": "0readlogs",
                "queryType": "log",
                "filter": "(serverHost == \"web-1\") \"SeedMissing\"",
                "startTime": from.timestamp().to_string(),
                "endTime": to.timestamp().to_string(),
                "maxCount": 2,
                "pageMode": "tail",
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(query_fixture()))
            .mount(&server)
            .await;
        let dataset = source(&server, &[("filter", "serverHost == \"web-1\"")]);
        let result = dataset
            .search(&query(
                "SeedMissing",
                "2026-10-18T00:00:00Z",
                "2026-10-18T12:00:00Z",
                1,
            ))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(result.hits.len(), 1);
        assert!(result.truncated, "two matches for a limit of one");
        let hit = &result.hits[0];
        assert_eq!(hit.source, "DataSet");
        assert_eq!(hit.at, Some(at("2026-10-18T10:00:00Z")));
        assert_eq!(
            hit.message,
            "POST /lists/7/share 500 SeedMissing: no household"
        );
        assert_eq!(hit.fields["status"], json!(500));
        assert_eq!(hit.fields["severity"], json!(5));
        assert!(
            hit.link
                .as_deref()
                .unwrap()
                .starts_with(&format!("{}/events?filter=", server.uri()))
        );
    }

    #[tokio::test]
    async fn an_error_status_in_the_body_is_an_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/query"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "status": "error/client/badParam",
                "message": "Couldn't decode API token"
            })))
            .mount(&server)
            .await;
        let dataset = source(&server, &[]);
        let error = dataset.test().await.unwrap_err();
        assert_eq!(error.to_string(), "DataSet: Couldn't decode API token");
    }

    #[tokio::test]
    async fn the_test_runs_a_one_event_query() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/query"))
            .and(body_partial_json(json!({ "maxCount": 1, "filter": "" })))
            .respond_with(ResponseTemplate::new(200).set_body_json(query_fixture()))
            .mount(&server)
            .await;
        assert_eq!(
            source(&server, &[]).test().await.unwrap(),
            "DataSet answered with events from the last 10 minutes"
        );
    }

    #[test]
    fn describe_snapshot() {
        let dataset = DataSetSource::new(&settings(
            EvidenceKind::Dataset,
            &[("read_token", "t"), ("filter", "$serverHost == 'web'")],
        ))
        .unwrap();
        assert_eq!(
            dataset.describe(),
            "DataSet: the logs the app's servers ship to DataSet, narrowed to `$serverHost == \
             'web'`. `evidence_search` with source \"DataSet\" runs a DataSet query for the \
             pattern over the window and returns the newest matching events with their \
             attributes and a link to the same query in DataSet. Look for the request or error \
             the report describes, then at what the same host or session logged just before it."
        );
        assert!(dataset.mcp_server().is_none());
        assert!(DataSetSource::new(&settings(EvidenceKind::Dataset, &[])).is_err());
    }
}
