//! CloudWatch Logs, searched with `FilterLogEvents` through the `aws` CLI on
//! this machine (`aws logs filter-log-events`), the way the GitHub Issues
//! tracker drives `gh`: the CLI's own configuration (a profile, SSO, an
//! instance role) supplies the credentials, so there is no key to enter, and
//! the AWS SDK's dependency tree stays out of the build.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;

use super::{
    EvidenceError, EvidenceKind, EvidenceSource, Hit, McpServerEntry, SearchQuery, SearchResult,
    SourceSettings, capped, quoted, with_common_fields,
};
use crate::services::integrations::{Descriptor, Field, FieldKind};

/// How long one `aws` call may take; `filter-log-events` pages through the
/// whole window, which is slow on a busy group.
const CLI_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// Runs the `aws` CLI. A seam so tests can answer with recorded output.
#[async_trait]
pub trait AwsRunner: Send + Sync {
    /// Runs `aws` with `args` and returns stdout.
    async fn run(&self, args: &[String]) -> Result<Vec<u8>, EvidenceError>;
}

/// The real `aws` on this machine.
pub struct AwsCli;

#[async_trait]
impl AwsRunner for AwsCli {
    async fn run(&self, args: &[String]) -> Result<Vec<u8>, EvidenceError> {
        use std::process::Stdio;

        use utils::command_ext::NoWindowExt;

        let aws = utils::shell::resolve_executable_path("aws")
            .await
            .ok_or_else(|| {
                EvidenceError::NotConfigured(
                    "the aws CLI is not installed on this machine (it reads CloudWatch Logs)"
                        .to_string(),
                )
            })?;
        let mut command = tokio::process::Command::new(aws);
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .no_window();
        let child = command
            .spawn()
            .map_err(|error| EvidenceError::Http(format!("could not start aws: {error}")))?;
        let output = tokio::time::timeout(CLI_TIMEOUT, child.wait_with_output())
            .await
            .map_err(|_| EvidenceError::Http("aws did not answer in time".to_string()))?
            .map_err(|error| EvidenceError::Http(format!("aws failed: {error}")))?;
        if output.status.success() {
            return Ok(output.stdout);
        }
        Err(EvidenceError::Api(format!(
            "aws: {}",
            super::clip(String::from_utf8_lossy(&output.stderr).trim(), 400)
        )))
    }
}

pub struct CloudWatchSource {
    settings: SourceSettings,
    log_group: String,
    region: Option<String>,
    profile: Option<String>,
    runner: Arc<dyn AwsRunner>,
}

impl CloudWatchSource {
    pub fn descriptor() -> Descriptor {
        Descriptor::new(
            EvidenceKind::Cloudwatch.as_str(),
            EvidenceKind::Cloudwatch.display_name(),
            "A CloudWatch Logs group, searched through the aws CLI on this machine with the \
             credentials it is configured with.",
            with_common_fields(
                EvidenceKind::Cloudwatch,
                vec![
                    Field::new(
                        "log_group",
                        "Log group",
                        FieldKind::Text,
                        true,
                        "The log group's name, e.g. /ecs/pantry-web.",
                    ),
                    Field::new(
                        "region",
                        "Region",
                        FieldKind::Text,
                        false,
                        "e.g. us-east-1. Blank uses the aws CLI's configured region.",
                    ),
                    Field::new(
                        "profile",
                        "Profile",
                        FieldKind::Text,
                        false,
                        "The aws CLI profile to use. Blank uses its default credentials.",
                    ),
                ],
            ),
            "https://docs.aws.amazon.com/cli/latest/reference/logs/filter-log-events.html",
        )
    }

    pub fn new(settings: &SourceSettings) -> Result<Self, EvidenceError> {
        Self::with_runner(settings, Arc::new(AwsCli))
    }

    pub(crate) fn with_runner(
        settings: &SourceSettings,
        runner: Arc<dyn AwsRunner>,
    ) -> Result<Self, EvidenceError> {
        let log_group = settings
            .get("log_group")
            .ok_or_else(|| EvidenceError::NotConfigured("a CloudWatch log group".to_string()))?
            .to_string();
        Ok(Self {
            settings: settings.clone(),
            log_group,
            region: settings.get("region").map(str::to_string),
            profile: settings.get("profile").map(str::to_string),
            runner,
        })
    }

    /// `args` with the region and profile flags appended.
    fn with_target(&self, mut args: Vec<String>) -> Vec<String> {
        if let Some(region) = &self.region {
            args.extend(["--region".to_string(), region.clone()]);
        }
        if let Some(profile) = &self.profile {
            args.extend(["--profile".to_string(), profile.clone()]);
        }
        args.extend(["--output".to_string(), "json".to_string()]);
        args
    }

    /// The console page of one log stream, when the region is known.
    fn stream_link(&self, stream: &str) -> Option<String> {
        let region = self.region.as_deref()?;
        // The console's fragment double-encodes: `/` is `$252F`.
        let escape = |value: &str| {
            url::form_urlencoded::byte_serialize(value.as_bytes())
                .collect::<String>()
                .replace('%', "$25")
        };
        Some(format!(
            "https://{region}.console.aws.amazon.com/cloudwatch/home?region={region}#logsV2:log-groups/log-group/{}/log-events/{}",
            escape(&self.log_group),
            escape(stream)
        ))
    }

    async fn run_json(&self, args: Vec<String>) -> Result<Value, EvidenceError> {
        let stdout = self.runner.run(&self.with_target(args)).await?;
        serde_json::from_slice(&stdout)
            .map_err(|e| EvidenceError::Api(format!("aws printed something unreadable: {e}")))
    }
}

#[async_trait]
impl EvidenceSource for CloudWatchSource {
    fn kind(&self) -> &'static str {
        EvidenceKind::Cloudwatch.as_str()
    }

    fn label(&self) -> String {
        self.settings.label()
    }

    fn describe(&self) -> String {
        let mut out = format!(
            "{}: the log group `{}` in CloudWatch Logs. `evidence_search` with source \"{}\" \
             filters it for the pattern (an exact, case-sensitive phrase) over the window and \
             returns the newest matching events with their log stream. Look for the failing \
             request and the error logged with it, then at what the same stream logged around it.",
            self.settings.heading(),
            self.log_group,
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
        let mut args: Vec<String> = [
            "logs",
            "filter-log-events",
            "--log-group-name",
            &self.log_group,
            "--start-time",
            &from.timestamp_millis().to_string(),
            "--end-time",
            &to.timestamp_millis().to_string(),
            "--max-items",
            &(query.limit + 1).to_string(),
        ]
        .into_iter()
        .map(str::to_string)
        .collect();
        if !query.pattern.trim().is_empty() {
            args.extend(["--filter-pattern".to_string(), quoted(&query.pattern)]);
        }
        let body = self.run_json(args).await?;
        let events = body["events"].as_array().cloned().unwrap_or_default();
        let more = body["NextToken"].is_string() || body["nextToken"].is_string();
        let fetched = events.len();
        let label = self.label();
        let hits = events
            .into_iter()
            .map(|event| {
                let at = event["timestamp"]
                    .as_i64()
                    .and_then(DateTime::<Utc>::from_timestamp_millis);
                let stream = event["logStreamName"].as_str().unwrap_or_default();
                Hit::new(&label, at, event["message"].as_str().unwrap_or_default())
                    .link(self.stream_link(stream))
                    .field("log_stream", event["logStreamName"].clone())
                    .field("event_id", event["eventId"].clone())
            })
            .collect();
        Ok(Some(capped(
            hits,
            query.limit,
            more || fetched > query.limit,
        )))
    }

    async fn test(&self) -> Result<String, EvidenceError> {
        let body = self
            .run_json(vec![
                "logs".to_string(),
                "describe-log-groups".to_string(),
                "--log-group-name-prefix".to_string(),
                self.log_group.clone(),
                "--max-items".to_string(),
                "5".to_string(),
            ])
            .await?;
        let found = body["logGroups"].as_array().is_some_and(|groups| {
            groups
                .iter()
                .any(|group| group["logGroupName"].as_str() == Some(self.log_group.as_str()))
        });
        if found {
            Ok(format!(
                "CloudWatch log group {} is readable",
                self.log_group
            ))
        } else {
            Err(EvidenceError::Api(format!(
                "the aws CLI answered, but has no log group named {}",
                self.log_group
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use serde_json::json;

    use super::*;
    use crate::services::evidence::test_support::*;

    /// Answers each `aws` call with recorded output and keeps the arguments.
    struct Recorded {
        answer: Value,
        calls: Mutex<Vec<Vec<String>>>,
    }

    #[async_trait]
    impl AwsRunner for Recorded {
        async fn run(&self, args: &[String]) -> Result<Vec<u8>, EvidenceError> {
            self.calls.lock().unwrap().push(args.to_vec());
            Ok(serde_json::to_vec(&self.answer).unwrap())
        }
    }

    fn recorded(answer: Value) -> Arc<Recorded> {
        Arc::new(Recorded {
            answer,
            calls: Mutex::new(Vec::new()),
        })
    }

    fn source(runner: Arc<Recorded>) -> CloudWatchSource {
        CloudWatchSource::with_runner(
            &settings(
                EvidenceKind::Cloudwatch,
                &[
                    ("log_group", "/ecs/pantry-web"),
                    ("region", "us-east-1"),
                    ("profile", "pantry-readonly"),
                ],
            ),
            runner,
        )
        .unwrap()
    }

    /// `filter-log-events` output as the CLI prints it.
    fn events_fixture() -> Value {
        json!({
            "events": [
                {
                    "logStreamName": "web/web/abc123",
                    "timestamp": 1792314000000_i64,
                    "message": "ERROR SeedMissing: household 7",
                    "ingestionTime": 1792314000500_i64,
                    "eventId": "3791"
                },
                {
                    "logStreamName": "web/web/abc123",
                    "timestamp": 1792317600000_i64,
                    "message": "ERROR SeedMissing: household 8",
                    "ingestionTime": 1792317600500_i64,
                    "eventId": "3792"
                }
            ],
            "searchedLogStreams": [],
            "NextToken": "eyJuZXh0VG9rZW4iOiBudWxsfQ=="
        })
    }

    #[tokio::test]
    async fn search_filters_the_group_over_the_window() {
        let runner = recorded(events_fixture());
        let cloudwatch = source(runner.clone());
        let result = cloudwatch
            .search(&query(
                "SeedMissing",
                "2026-10-18T00:00:00Z",
                "2026-10-18T12:00:00Z",
                10,
            ))
            .await
            .unwrap()
            .unwrap();
        let args = runner.calls.lock().unwrap()[0].clone();
        assert_eq!(
            args,
            [
                "logs",
                "filter-log-events",
                "--log-group-name",
                "/ecs/pantry-web",
                "--start-time",
                "1792281600000",
                "--end-time",
                "1792324800000",
                "--max-items",
                "11",
                "--filter-pattern",
                "\"SeedMissing\"",
                "--region",
                "us-east-1",
                "--profile",
                "pantry-readonly",
                "--output",
                "json",
            ]
        );
        // The CLI handed back a token: there is more than it printed.
        assert!(result.truncated);
        let messages: Vec<&str> = result.hits.iter().map(|h| h.message.as_str()).collect();
        assert_eq!(
            messages,
            [
                "ERROR SeedMissing: household 8",
                "ERROR SeedMissing: household 7"
            ]
        );
        let hit = &result.hits[0];
        assert_eq!(hit.at, Some(at("2026-10-18T10:00:00Z")));
        assert_eq!(hit.fields["log_stream"], json!("web/web/abc123"));
        assert_eq!(
            hit.link.as_deref(),
            Some(
                "https://us-east-1.console.aws.amazon.com/cloudwatch/home?region=us-east-1\
                 #logsV2:log-groups/log-group/$252Fecs$252Fpantry-web/log-events/web$252Fweb$252Fabc123"
            )
        );
    }

    #[tokio::test]
    async fn the_test_looks_the_group_up() {
        let found = source(recorded(json!({
            "logGroups": [ { "logGroupName": "/ecs/pantry-web" } ]
        })));
        assert_eq!(
            found.test().await.unwrap(),
            "CloudWatch log group /ecs/pantry-web is readable"
        );
        let missing = source(recorded(json!({ "logGroups": [] })));
        assert!(missing.test().await.is_err());
    }

    #[test]
    fn describe_snapshot() {
        let cloudwatch = source(recorded(json!({})));
        assert_eq!(
            cloudwatch.describe(),
            "CloudWatch Logs: the log group `/ecs/pantry-web` in CloudWatch Logs. \
             `evidence_search` with source \"CloudWatch Logs\" filters it for the pattern (an \
             exact, case-sensitive phrase) over the window and returns the newest matching events \
             with their log stream. Look for the failing request and the error logged with it, \
             then at what the same stream logged around it."
        );
        assert!(cloudwatch.mcp_server().is_none());
        assert!(CloudWatchSource::new(&settings(EvidenceKind::Cloudwatch, &[])).is_err());
    }
}
