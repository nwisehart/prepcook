//! `evidence_search`: the board's evidence sources (an error tracker, a log
//! service, the app's own log files) searched for a pattern over a time
//! window, through `POST /api/evidence/search` (see `docs/evidence.md`).

use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, Utc};
use rmcp::{
    ErrorData, handler::server::wrapper::Parameters, model::CallToolResult, schemars, tool,
    tool_router,
};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use super::{McpServer, ToolError};
use crate::ApiResponseEnvelope;

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct McpEvidenceSearchRequest {
    #[schemars(
        description = "Text to find, matched as a literal phrase in every source: an error message, an id, a route"
    )]
    pub pattern: String,
    #[schemars(
        description = "Start of the window: how far back from now (\"30m\", \"2h\", \"3d\", \"1w\") or an ISO 8601 time. Default: 24 hours before `until`"
    )]
    pub since: Option<String>,
    #[schemars(description = "End of the window, in the same forms. Default: now")]
    pub until: Option<String>,
    #[schemars(
        description = "One source to search, by its label or its kind (sentry, dataset, datadog, cloudwatch, log_files). Default: every source"
    )]
    pub source: Option<String>,
    #[schemars(description = "Most hits returned, 1 to 200. Default: 50")]
    pub limit: Option<u32>,
    #[schemars(
        description = "A product area (from the project profile); sources that cover other areas are skipped"
    )]
    pub area: Option<String>,
    #[schemars(
        description = "The board whose sources are searched. Default: this workspace's board"
    )]
    pub project_id: Option<Uuid>,
}

/// A time as the tool accepts it: a span back from `now` (`45s`, `30m`,
/// `2h`, `3d`, `1w`), an RFC 3339 time, a date and time without a zone (UTC),
/// or a date (its midnight, UTC).
pub(crate) fn parse_time(raw: &str, now: DateTime<Utc>) -> Result<DateTime<Utc>, String> {
    let raw = raw.trim();
    let invalid = || {
        format!(
            "{raw:?} is not a time: use a span back from now such as \"30m\", \"2h\", \"3d\" or \
             \"1w\", or an ISO 8601 time such as 2026-10-18T09:30:00Z"
        )
    };
    if let Some(unit) = raw.chars().last().filter(char::is_ascii_alphabetic)
        && let Ok(amount) = raw[..raw.len() - 1].trim().parse::<i64>()
        && amount >= 0
    {
        let span = match unit.to_ascii_lowercase() {
            's' => Duration::try_seconds(amount),
            'm' => Duration::try_minutes(amount),
            'h' => Duration::try_hours(amount),
            'd' => Duration::try_days(amount),
            'w' => Duration::try_weeks(amount),
            _ => None,
        }
        .ok_or_else(invalid)?;
        return now.checked_sub_signed(span).ok_or_else(invalid);
    }
    if let Ok(at) = DateTime::parse_from_rfc3339(raw) {
        return Ok(at.with_timezone(&Utc));
    }
    for format in [
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%dT%H:%M",
    ] {
        if let Ok(at) = NaiveDateTime::parse_from_str(raw, format) {
            return Ok(at.and_utc());
        }
    }
    if let Ok(date) = NaiveDate::parse_from_str(raw, "%Y-%m-%d") {
        return Ok(date.and_hms_opt(0, 0, 0).ok_or_else(invalid)?.and_utc());
    }
    Err(invalid())
}

/// The route's request body for a tool call: the window made absolute, the
/// limit checked, blanks dropped.
pub(crate) fn search_body(
    request: &McpEvidenceSearchRequest,
    now: DateTime<Utc>,
    project_id: Option<Uuid>,
    worktree: Option<&str>,
) -> Result<Value, String> {
    let pattern = request.pattern.trim();
    if pattern.is_empty() {
        return Err("pattern is empty: give the text to look for".to_string());
    }
    let present = |value: &Option<String>| {
        value
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    let to = present(&request.until)
        .map(|raw| parse_time(&raw, now))
        .transpose()?
        .unwrap_or(now);
    let from = present(&request.since)
        .map(|raw| parse_time(&raw, now))
        .transpose()?
        .unwrap_or(to - Duration::hours(24));
    if from > to {
        return Err(format!(
            "since ({}) is after until ({}); since is the earlier end",
            from.to_rfc3339(),
            to.to_rfc3339()
        ));
    }
    if let Some(limit) = request.limit
        && !(1..=200).contains(&limit)
    {
        return Err(format!("limit {limit} is out of range: 1 to 200"));
    }
    let mut body = json!({
        "pattern": pattern,
        "from": from.to_rfc3339(),
        "to": to.to_rfc3339(),
    });
    let fields = [
        ("source", present(&request.source).map(Value::from)),
        ("area", present(&request.area).map(Value::from)),
        ("limit", request.limit.map(Value::from)),
        (
            "project_id",
            project_id.map(|id| Value::from(id.to_string())),
        ),
        ("worktree", worktree.map(Value::from)),
    ];
    for (key, value) in fields {
        if let Some(value) = value {
            body[key] = value;
        }
    }
    Ok(body)
}

impl McpServer {
    /// Posts a search and reads the answer whatever its status, so the
    /// route's own message (an unknown source, a bad window) reaches the
    /// agent rather than a bare status code.
    async fn post_evidence_search(&self, body: &Value) -> Result<Value, ToolError> {
        let response = self
            .client
            .post(self.url("/api/evidence/search"))
            .json(body)
            .send()
            .await
            .map_err(|e| {
                ToolError::new(
                    "Failed to connect to the Prep Cook API",
                    Some(e.to_string()),
                )
            })?;
        let status = response.status();
        let envelope = response
            .json::<ApiResponseEnvelope<Value>>()
            .await
            .map_err(|e| {
                ToolError::new(
                    format!("The evidence search answered {status} with no readable body"),
                    Some(e.to_string()),
                )
            })?;
        match (envelope.success, envelope.data) {
            (true, Some(data)) => Ok(data),
            _ => Err(ToolError::message(envelope.message.unwrap_or_else(|| {
                format!("the evidence search failed ({status})")
            }))),
        }
    }
}

#[tool_router(router = evidence_tools_router, vis = "pub")]
impl McpServer {
    #[tool(
        description = "Search the board's evidence sources — its error tracker, log services and the app's own log files, as the planning instructions describe them — for a pattern over a time window, and merge what they found. Returns `hits` (newest first: at, source, message, link, fields), `truncated` when more matched than were returned, and `sources`: what each source did (searched, hits, error, or why it was skipped). Use it before guessing at a cause."
    )]
    async fn evidence_search(
        &self,
        Parameters(request): Parameters<McpEvidenceSearchRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        // An unresolved board is not an error here: the server falls back to
        // its default board.
        let project_id = match request.project_id {
            Some(id) => Some(id),
            None => self.resolve_project_id(None).await.ok(),
        };
        let worktree = Self::app_worktree().ok();
        let body = match search_body(&request, Utc::now(), project_id, worktree.as_deref()) {
            Ok(body) => body,
            Err(message) => return Ok(Self::tool_error(ToolError::message(message))),
        };
        match self.post_evidence_search(&body).await {
            Ok(found) => McpServer::success(&found),
            Err(error) => Ok(Self::tool_error(error)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(raw: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(raw)
            .unwrap()
            .with_timezone(&Utc)
    }

    fn now() -> DateTime<Utc> {
        at("2026-10-18T12:00:00Z")
    }

    #[test]
    fn times_are_spans_back_from_now_or_iso() {
        assert_eq!(parse_time("2h", now()).unwrap(), at("2026-10-18T10:00:00Z"));
        assert_eq!(
            parse_time(" 30m ", now()).unwrap(),
            at("2026-10-18T11:30:00Z")
        );
        assert_eq!(
            parse_time("45s", now()).unwrap(),
            at("2026-10-18T11:59:15Z")
        );
        assert_eq!(parse_time("3d", now()).unwrap(), at("2026-10-15T12:00:00Z"));
        assert_eq!(parse_time("1W", now()).unwrap(), at("2026-10-11T12:00:00Z"));
        assert_eq!(
            parse_time("2026-10-18T09:30:00+02:00", now()).unwrap(),
            at("2026-10-18T07:30:00Z")
        );
        assert_eq!(
            parse_time("2026-10-18 09:30:00", now()).unwrap(),
            at("2026-10-18T09:30:00Z")
        );
        assert_eq!(
            parse_time("2026-10-18T09:30", now()).unwrap(),
            at("2026-10-18T09:30:00Z")
        );
        assert_eq!(
            parse_time("2026-10-17", now()).unwrap(),
            at("2026-10-17T00:00:00Z")
        );
        for bad in ["", "yesterday", "2x", "-2h", "h", "2026-13-01"] {
            assert!(parse_time(bad, now()).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn the_body_carries_an_absolute_window_and_only_what_was_given() {
        let request = McpEvidenceSearchRequest {
            pattern: "  SeedMissing ".to_string(),
            since: Some("2h".to_string()),
            source: Some("App log".to_string()),
            area: Some(" ".to_string()),
            limit: Some(20),
            ..Default::default()
        };
        let board = Uuid::nil();
        let body = search_body(&request, now(), Some(board), Some("/work/tree")).unwrap();
        assert_eq!(
            body,
            json!({
                "pattern": "SeedMissing",
                "from": "2026-10-18T10:00:00+00:00",
                "to": "2026-10-18T12:00:00+00:00",
                "source": "App log",
                "limit": 20,
                "project_id": board.to_string(),
                "worktree": "/work/tree",
            })
        );

        let defaults = search_body(
            &McpEvidenceSearchRequest {
                pattern: "x".to_string(),
                until: Some("2026-10-18T06:00:00Z".to_string()),
                ..Default::default()
            },
            now(),
            None,
            None,
        )
        .unwrap();
        assert_eq!(defaults["from"], "2026-10-17T06:00:00+00:00");
        assert_eq!(defaults["to"], "2026-10-18T06:00:00+00:00");
        assert!(defaults.get("project_id").is_none());
    }

    #[test]
    fn bad_arguments_are_explained() {
        let bad = |request: McpEvidenceSearchRequest| search_body(&request, now(), None, None);
        assert!(
            bad(McpEvidenceSearchRequest::default())
                .unwrap_err()
                .contains("pattern")
        );
        assert!(
            bad(McpEvidenceSearchRequest {
                pattern: "x".to_string(),
                since: Some("1h".to_string()),
                until: Some("2h".to_string()),
                ..Default::default()
            })
            .unwrap_err()
            .contains("after until")
        );
        assert!(
            bad(McpEvidenceSearchRequest {
                pattern: "x".to_string(),
                limit: Some(0),
                ..Default::default()
            })
            .unwrap_err()
            .contains("1 to 200")
        );
        assert!(
            bad(McpEvidenceSearchRequest {
                pattern: "x".to_string(),
                since: Some("last tuesday".to_string()),
                ..Default::default()
            })
            .unwrap_err()
            .contains("not a time")
        );
    }

    /// The merged answer the route sends back reads as the tool returns it.
    #[test]
    fn a_merged_answer_parses_from_the_envelope() {
        let raw = json!({
            "success": true,
            "data": {
                "hits": [
                    {
                        "at": "2026-10-18T11:00:00Z",
                        "source": "Worker log",
                        "message": "SeedMissing",
                        "link": null,
                        "fields": { "file": "logs/worker.log", "line": 3 }
                    }
                ],
                "truncated": false,
                "sources": [
                    { "label": "Worker log", "kind": "log_files", "searched": true, "hits": 1,
                      "truncated": false, "error": null, "note": null }
                ]
            },
            "message": null
        });
        let envelope: ApiResponseEnvelope<Value> = serde_json::from_value(raw).unwrap();
        let data = envelope.data.unwrap();
        assert_eq!(data["hits"][0]["source"], "Worker log");
        assert_eq!(data["sources"][0]["kind"], "log_files");
    }
}
