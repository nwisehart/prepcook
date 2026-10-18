//! Sentry: the errors the app raised where it runs for real, grouped into
//! issues. Agent sessions get Sentry's own MCP server (`@sentry/mcp-server`)
//! for reading issues in full; `search` lists a project's issues through the
//! REST API (`GET /api/0/projects/{org}/{project}/issues/`).

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use executors::mcp_config::PRECONFIGURED_MCP_SERVERS;
use serde_json::Value;

use super::{
    EvidenceError, EvidenceKind, EvidenceSource, HTTP_TIMEOUT, Hit, McpServerEntry, SearchQuery,
    SearchResult, SourceSettings, capped, quoted, urlencode, with_common_fields,
};
use crate::services::integrations::{Descriptor, Field, FieldKind};

const SAAS: &str = "https://sentry.io";
/// The most issues one request asks for; Sentry pages at 100.
const PAGE_CAP: usize = 100;

pub struct SentrySource {
    settings: SourceSettings,
    token: String,
    /// `https://sentry.io`, or the self-hosted instance's origin.
    base: String,
    /// The hostname handed to the MCP server's `--host`, when not SaaS.
    host: Option<String>,
    org: Option<String>,
    project: Option<String>,
    environment: Option<String>,
    client: reqwest::Client,
}

impl SentrySource {
    pub fn descriptor() -> Descriptor {
        Descriptor::new(
            EvidenceKind::Sentry.as_str(),
            EvidenceKind::Sentry.display_name(),
            "The errors the app raised in production, grouped into issues: agent sessions get \
             Sentry's MCP server, and evidence_search lists a project's issues seen in a window.",
            with_common_fields(
                EvidenceKind::Sentry,
                vec![
                    Field::new(
                        "access_token",
                        "Access token",
                        FieldKind::Secret,
                        true,
                        "A user auth token (Settings → Account → API → Auth Tokens) with \
                         org:read, project:read and event:read.",
                    ),
                    Field::new(
                        "host",
                        "Host",
                        FieldKind::Text,
                        false,
                        "The host name of a self-hosted Sentry, e.g. sentry.example.com. Blank is \
                         sentry.io.",
                    ),
                    Field::new(
                        "org",
                        "Organization slug",
                        FieldKind::Text,
                        false,
                        "The organization in Sentry's URLs, e.g. acme. With the project, \
                         evidence_search can list issues; without them the agent has the MCP \
                         server only.",
                    ),
                    Field::new(
                        "project",
                        "Project slug",
                        FieldKind::Text,
                        false,
                        "The project the app reports to, e.g. web.",
                    ),
                    Field::new(
                        "environment",
                        "Environment",
                        FieldKind::Text,
                        false,
                        "Only issues seen in this environment, e.g. production. Blank is every \
                         environment.",
                    ),
                ],
            ),
            "https://docs.sentry.io/api/events/list-a-projects-issues/",
        )
    }

    pub fn new(settings: &SourceSettings) -> Result<Self, EvidenceError> {
        let token = settings
            .get("access_token")
            .ok_or_else(|| EvidenceError::NotConfigured("a Sentry access token".to_string()))?
            .to_string();
        let raw_host = settings.get("host");
        let base = match raw_host {
            None => SAAS.to_string(),
            Some(host) if host.starts_with("http://") || host.starts_with("https://") => {
                host.trim_end_matches('/').to_string()
            }
            Some(host) => format!("https://{}", host.trim_end_matches('/')),
        };
        let host = raw_host
            .map(|host| {
                host.trim_start_matches("https://")
                    .trim_start_matches("http://")
                    .trim_end_matches('/')
                    .to_string()
            })
            .filter(|host| host != "sentry.io");
        let client = reqwest::Client::builder()
            .timeout(HTTP_TIMEOUT)
            .build()
            .map_err(EvidenceError::http)?;
        Ok(Self {
            settings: settings.clone(),
            token,
            base,
            host,
            org: settings.get("org").map(str::to_string),
            project: settings.get("project").map(str::to_string),
            environment: settings.get("environment").map(str::to_string),
            client,
        })
    }

    fn project_path(&self) -> Option<String> {
        Some(format!(
            "/api/0/projects/{}/{}/",
            self.org.as_deref()?,
            self.project.as_deref()?
        ))
    }

    async fn get(&self, path_and_query: &str) -> Result<reqwest::Response, EvidenceError> {
        let response = self
            .client
            .get(format!("{}{path_and_query}", self.base))
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(EvidenceError::http)?;
        if !response.status().is_success() {
            return Err(EvidenceError::rejected(response).await);
        }
        Ok(response)
    }
}

fn timestamp(value: &Value) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value.as_str()?)
        .ok()
        .map(|at| at.with_timezone(&Utc))
}

/// Whether a `Link` header says another page has results.
fn has_next_page(link: Option<&str>) -> bool {
    link.is_some_and(|link| {
        link.split(',')
            .any(|part| part.contains("rel=\"next\"") && part.contains("results=\"true\""))
    })
}

#[async_trait]
impl EvidenceSource for SentrySource {
    fn kind(&self) -> &'static str {
        EvidenceKind::Sentry.as_str()
    }

    fn label(&self) -> String {
        self.settings.label()
    }

    fn describe(&self) -> String {
        let scope = match (&self.org, &self.project) {
            (Some(org), Some(project)) => format!(" (organization `{org}`, project `{project}`)"),
            _ => String::new(),
        };
        let search = if self.project_path().is_some() {
            format!(
                " `evidence_search` with source \"{}\" lists the issues matching a pattern that \
                 were seen in the window, newest first, each with its short id and link.",
                self.label()
            )
        } else {
            String::new()
        };
        let mut out = format!(
            "{}: the errors the app raised where it runs for real{scope}, grouped into issues \
             with their stack traces, tags and the release each was first seen in. The Sentry \
             MCP server in this session reads an issue's events and stack traces in full.{search} \
             Look for an issue first seen around the time of the report, or in the release the \
             reporter was on, and cite its short id in the RCA.",
            self.settings.heading()
        );
        if let Some(notes) = self.settings.notes() {
            out.push(' ');
            out.push_str(&notes);
        }
        out
    }

    fn mcp_server(&self) -> Option<McpServerEntry> {
        let mut server = PRECONFIGURED_MCP_SERVERS.get("sentry")?.clone();
        if let Some(env) = server.get_mut("env").and_then(Value::as_object_mut) {
            env.insert(
                "SENTRY_ACCESS_TOKEN".to_string(),
                Value::String(self.token.clone()),
            );
        }
        if let Some(host) = &self.host
            && let Some(args) = server.get_mut("args").and_then(Value::as_array_mut)
        {
            args.push(Value::String(format!("--host={host}")));
        }
        Some(McpServerEntry {
            name: "sentry".to_string(),
            server,
        })
    }

    async fn search(&self, query: &SearchQuery) -> Result<Option<SearchResult>, EvidenceError> {
        let Some(project_path) = self.project_path() else {
            return Ok(None);
        };
        let (from, to) = query.window;
        let terms = format!(
            "{} lastSeen:>={} lastSeen:<={}",
            quoted(&query.pattern),
            from.format("%Y-%m-%dT%H:%M:%S"),
            to.format("%Y-%m-%dT%H:%M:%S"),
        );
        let mut path = format!(
            "{project_path}issues/?query={}&per_page={}",
            urlencode(&terms),
            (query.limit + 1).min(PAGE_CAP)
        );
        if let Some(environment) = &self.environment {
            path.push_str(&format!("&environment={}", urlencode(environment)));
        }
        let response = self.get(&path).await?;
        let next = has_next_page(
            response
                .headers()
                .get(reqwest::header::LINK)
                .and_then(|value| value.to_str().ok()),
        );
        let issues: Vec<Value> = response.json().await.map_err(|e| {
            EvidenceError::Api(format!("Sentry answered something unreadable: {e}"))
        })?;
        let fetched = issues.len();
        let label = self.label();
        let hits: Vec<Hit> = issues
            .into_iter()
            .filter_map(|issue| {
                let at = timestamp(&issue["lastSeen"]);
                if at.is_some_and(|at| !query.contains(at)) {
                    return None;
                }
                let title = issue["title"].as_str().unwrap_or("(untitled issue)");
                let message = match issue["culprit"].as_str().filter(|c| !c.is_empty()) {
                    Some(culprit) => format!("{title} — in {culprit}"),
                    None => title.to_string(),
                };
                Some(
                    Hit::new(&label, at, &message)
                        .link(issue["permalink"].as_str().map(str::to_string))
                        .field("short_id", issue["shortId"].clone())
                        .field("level", issue["level"].clone())
                        .field("count", issue["count"].clone())
                        .field("user_count", issue["userCount"].clone())
                        .field("first_seen", issue["firstSeen"].clone())
                        .field("status", issue["status"].clone()),
                )
            })
            .collect();
        Ok(Some(capped(
            hits,
            query.limit,
            next || fetched > query.limit,
        )))
    }

    async fn test(&self) -> Result<String, EvidenceError> {
        match self.project_path() {
            Some(path) => {
                let project: Value = self
                    .get(&path)
                    .await?
                    .json()
                    .await
                    .map_err(|e| EvidenceError::Api(e.to_string()))?;
                Ok(format!(
                    "Sentry project {} is readable",
                    project["slug"]
                        .as_str()
                        .or(self.project.as_deref())
                        .unwrap_or_default()
                ))
            }
            None => {
                self.get("/api/0/").await?;
                Ok(
                    "Sentry accepted the token; set the organization and project to search it"
                        .to_string(),
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{header, method, path, query_param},
    };

    use super::*;
    use crate::services::evidence::{EvidenceProvider, test_support::*};

    fn source(server: &MockServer, extra: &[(&str, &str)]) -> SentrySource {
        let uri = server.uri();
        let mut fields = vec![("access_token", "sntrys_test"), ("host", uri.as_str())];
        fields.extend_from_slice(extra);
        SentrySource::new(&settings(EvidenceKind::Sentry, &fields)).unwrap()
    }

    /// A trimmed `issues/` answer as Sentry gives it.
    fn issues_fixture() -> Value {
        json!([
            {
                "id": "4501",
                "shortId": "WEB-1A",
                "title": "TypeError: Cannot read properties of undefined (reading 'items')",
                "culprit": "GET /lists/:id",
                "permalink": "https://sentry.example.com/organizations/acme/issues/4501/",
                "level": "error",
                "status": "unresolved",
                "count": "37",
                "userCount": 4,
                "firstSeen": "2026-10-17T22:10:00Z",
                "lastSeen": "2026-10-18T09:30:00Z"
            },
            {
                "id": "4502",
                "shortId": "WEB-1B",
                "title": "SeedMissing",
                "culprit": "",
                "permalink": "https://sentry.example.com/organizations/acme/issues/4502/",
                "level": "warning",
                "status": "unresolved",
                "count": "2",
                "userCount": 0,
                "firstSeen": "2026-10-01T00:00:00Z",
                "lastSeen": "2026-10-02T00:00:00Z"
            }
        ])
    }

    #[tokio::test]
    async fn search_lists_issues_seen_in_the_window() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/0/projects/acme/web/issues/"))
            .and(header("authorization", "Bearer sntrys_test"))
            .and(query_param(
                "query",
                "\"items\" lastSeen:>=2026-10-18T00:00:00 lastSeen:<=2026-10-18T12:00:00",
            ))
            .and(query_param("per_page", "11"))
            .and(query_param("environment", "production"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(issues_fixture())
                    .insert_header(
                        "link",
                        "<https://x/?cursor=0:0:1>; rel=\"previous\"; results=\"false\", \
                         <https://x/?cursor=0:100:0>; rel=\"next\"; results=\"false\"",
                    ),
            )
            .mount(&server)
            .await;
        let sentry = source(
            &server,
            &[
                ("org", "acme"),
                ("project", "web"),
                ("environment", "production"),
                ("label", "Production errors"),
            ],
        );
        let result = sentry
            .search(&query(
                "items",
                "2026-10-18T00:00:00Z",
                "2026-10-18T12:00:00Z",
                10,
            ))
            .await
            .unwrap()
            .unwrap();
        // The second issue was last seen before the window.
        assert_eq!(result.hits.len(), 1);
        assert!(!result.truncated);
        let hit = &result.hits[0];
        assert_eq!(hit.source, "Production errors");
        assert_eq!(hit.at, Some(at("2026-10-18T09:30:00Z")));
        assert_eq!(
            hit.message,
            "TypeError: Cannot read properties of undefined (reading 'items') — in GET /lists/:id"
        );
        assert_eq!(
            hit.link.as_deref(),
            Some("https://sentry.example.com/organizations/acme/issues/4501/")
        );
        assert_eq!(hit.fields["short_id"], json!("WEB-1A"));
        assert_eq!(hit.fields["count"], json!("37"));
    }

    #[tokio::test]
    async fn a_next_page_with_results_truncates() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/0/projects/acme/web/issues/"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!([]))
                    .insert_header("link", "<https://x/>; rel=\"next\"; results=\"true\""),
            )
            .mount(&server)
            .await;
        let sentry = source(&server, &[("org", "acme"), ("project", "web")]);
        let result = sentry
            .search(&query(
                "x",
                "2026-10-18T00:00:00Z",
                "2026-10-18T12:00:00Z",
                5,
            ))
            .await
            .unwrap()
            .unwrap();
        assert!(result.truncated);
    }

    #[tokio::test]
    async fn without_a_project_there_is_nothing_to_search_and_the_test_checks_the_token() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/0/"))
            .and(header("authorization", "Bearer sntrys_test"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "version": "0" })))
            .mount(&server)
            .await;
        let sentry = source(&server, &[]);
        assert!(
            sentry
                .search(&query(
                    "x",
                    "2026-10-18T00:00:00Z",
                    "2026-10-18T12:00:00Z",
                    5
                ))
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            sentry.test().await.unwrap(),
            "Sentry accepted the token; set the organization and project to search it"
        );
    }

    #[tokio::test]
    async fn the_test_reads_the_project_and_reports_a_rejection() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/0/projects/acme/web/"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "slug": "web" })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/0/projects/acme/gone/"))
            .respond_with(ResponseTemplate::new(404).set_body_json(json!({ "detail": "nope" })))
            .mount(&server)
            .await;
        let ok = source(&server, &[("org", "acme"), ("project", "web")]);
        assert_eq!(ok.test().await.unwrap(), "Sentry project web is readable");
        let gone = source(&server, &[("org", "acme"), ("project", "gone")]);
        let error = gone.test().await.unwrap_err();
        assert!(matches!(error, EvidenceError::Rejected { status: 404, .. }));
    }

    #[test]
    fn the_mcp_entry_carries_the_token_and_a_self_hosted_host() {
        let saas = SentrySource::new(&settings(
            EvidenceKind::Sentry,
            &[("access_token", " token ")],
        ))
        .unwrap();
        let entry = saas.mcp_server().unwrap();
        assert_eq!(entry.name, "sentry");
        assert_eq!(entry.server["env"]["SENTRY_ACCESS_TOKEN"], json!("token"));
        assert_eq!(entry.server["args"].as_array().unwrap().len(), 2);

        let hosted = SentrySource::new(&settings(
            EvidenceKind::Sentry,
            &[("access_token", "token"), ("host", "sentry.example.com")],
        ))
        .unwrap();
        let entry = hosted.mcp_server().unwrap();
        assert_eq!(entry.server["args"][2], json!("--host=sentry.example.com"));
        assert_eq!(hosted.base, "https://sentry.example.com");

        assert!(SentrySource::new(&settings(EvidenceKind::Sentry, &[])).is_err());
    }

    #[test]
    fn describe_snapshot() {
        let full = SentrySource::new(&settings(
            EvidenceKind::Sentry,
            &[
                ("access_token", "token"),
                ("org", "acme"),
                ("project", "web"),
                ("label", "Production errors"),
                ("notes", "Kept 90 days"),
            ],
        ))
        .unwrap();
        assert_eq!(
            EvidenceProvider::from(full).describe(),
            "Production errors (Sentry): the errors the app raised where it runs for real \
             (organization `acme`, project `web`), grouped into issues with their stack traces, \
             tags and the release each was first seen in. The Sentry MCP server in this session \
             reads an issue's events and stack traces in full. `evidence_search` with source \
             \"Production errors\" lists the issues matching a pattern that were seen in the \
             window, newest first, each with its short id and link. Look for an issue first seen \
             around the time of the report, or in the release the reporter was on, and cite its \
             short id in the RCA. Kept 90 days."
        );
        let bare =
            SentrySource::new(&settings(EvidenceKind::Sentry, &[("access_token", "t")])).unwrap();
        assert_eq!(
            bare.describe(),
            "Sentry: the errors the app raised where it runs for real, grouped into issues with \
             their stack traces, tags and the release each was first seen in. The Sentry MCP \
             server in this session reads an issue's events and stack traces in full. Look for \
             an issue first seen around the time of the report, or in the release the reporter \
             was on, and cite its short id in the RCA."
        );
    }
}
