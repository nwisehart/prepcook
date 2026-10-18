//! PagerDuty behind [`Tracker`]: the source in `incidents` mode, where open
//! incidents become investigation cards (`docs/incidents.md`).
//!
//! REST API v2 at `api.pagerduty.com` with an API key (`Token token=…`).
//! Writes name the configured user in the `From` header, so notes and status
//! changes are recorded as that user; it is also the identity tracker
//! comments are recognised by.
//!
//! The mapping, method by method:
//! - search: open incidents matching the filter. Terms: `service:<id>[,…]`,
//!   `urgency:high|low`, `status:triggered|acknowledged|resolved[,…]`,
//!   `assigned:@me|<user id>`, `team:<id>[,…]`. Without `status:` only open
//!   incidents (triggered, acknowledged); without `service:` the configured
//!   `service_ids`.
//! - get: the incident; title is the summary, urgency the priority (`high` →
//!   P1, `low` → P3), the service the card's area tag, the description the
//!   triggering alert's body.
//! - comments: notes. Posting a comment adds a note.
//! - transition: `acknowledged` acknowledges, `resolved` resolves; any other
//!   name is a no-op. Entering Planning acknowledges a triggered incident,
//!   Triaged resolves it only with `incidents.resolve_on_triage`, Clean-up
//!   resolves it; the other stages leave it alone.
//! - assignee: the first assignment; assigning to self reassigns to the
//!   configured user.
//! - set summary: updates the title. Blocks and remote links are notes.
//! - add watcher: not supported. The API's way to add someone is a responder
//!   request, which pages them, and Prep Cook never pages.
//! - create issue: a low-urgency incident on the first configured service.
//! - sub-tasks: not supported. Attachments: none.
//! - identity: the configured user. Assignable users: the users the
//!   configured services' escalation policies reach.
//!
//! Incidents do not reopen and notes carry no mentions, so the reopen and
//! mention sweeps find nothing here.

use std::{
    collections::HashMap,
    path::Path,
    sync::{Mutex, OnceLock},
};

use anyhow::{Context, anyhow};
use async_trait::async_trait;
use chrono::{DateTime, FixedOffset, Utc};
use serde_json::{Value, json};

use super::{
    CreatedSubtask, FetchedIssue, IssueFields, NewIssue, NewSubtask, Query, RemoteLink,
    TicketStage, Tracker, TrackerAttachment, TrackerComment, TrackerError, TrackerIdentity,
    TrackerResult, TrackerUser, at_mentions, clip_text,
};
use crate::services::{
    config::{TrackerConfig, TrackerKind},
    integrations::{Descriptor, Field, FieldKind},
};

const KIND: TrackerKind = TrackerKind::PagerDuty;
const API_BASE: &str = "https://api.pagerduty.com";
const ACCEPT: &str = "application/vnd.pagerduty+json;version=2";
const PAGE_SIZE: usize = 100;
const MAX_PAGES: usize = 5;
/// PagerDuty rejects a note over 25 000 characters.
const NOTE_LIMIT: usize = 24_000;
/// Statuses an open incident is in.
const OPEN_STATUSES: [&str; 2] = ["triggered", "acknowledged"];
const STATUSES: [&str; 3] = ["triggered", "acknowledged", "resolved"];

/// A parsed incident filter (module docs list the terms).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct IncidentFilter {
    services: Vec<String>,
    urgencies: Vec<String>,
    statuses: Vec<String>,
    /// User ids; `@me` is resolved at query time.
    assigned: Vec<String>,
    teams: Vec<String>,
}

pub(crate) fn parse_filter(text: &str) -> Result<IncidentFilter, String> {
    let mut filter = IncidentFilter::default();
    for term in text.split_whitespace() {
        let Some((key, value)) = term.split_once(':') else {
            return Err(format!(
                "`{term}` is not a PagerDuty filter term (use service:, urgency:, status:, \
                 assigned: or team:)"
            ));
        };
        let values: Vec<String> = value
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect();
        match key.to_ascii_lowercase().as_str() {
            "service" => filter.services.extend(values),
            "team" => filter.teams.extend(values),
            "assigned" => filter.assigned.extend(values),
            "urgency" => {
                for urgency in values {
                    let urgency = urgency.to_ascii_lowercase();
                    if urgency != "high" && urgency != "low" {
                        return Err(format!("urgency is high or low, not `{urgency}`"));
                    }
                    filter.urgencies.push(urgency);
                }
            }
            "status" => {
                for status in values {
                    let status = status.to_ascii_lowercase();
                    if !STATUSES.contains(&status.as_str()) {
                        return Err(format!(
                            "status is triggered, acknowledged or resolved, not `{status}`"
                        ));
                    }
                    filter.statuses.push(status);
                }
            }
            _ => {
                return Err(format!(
                    "`{term}` is not a PagerDuty filter term (use service:, urgency:, status:, \
                     assigned: or team:)"
                ));
            }
        }
    }
    Ok(filter)
}

fn timestamp(value: &Value) -> Option<DateTime<FixedOffset>> {
    value
        .as_str()
        .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
}

/// Turns a non-success response into an error, telling a credentials
/// problem apart from everything else.
async fn rejected(response: reqwest::Response, what: &str) -> TrackerError {
    let status = response.status().as_u16();
    if status == 401 {
        return TrackerError::AuthFailed;
    }
    let text = response.text().await.unwrap_or_default();
    TrackerError::Other(anyhow!("PagerDuty {what} rejected (HTTP {status}): {text}"))
}

/// The alert body as text: the trigger's details, one `key: value` line per
/// field when they are an object.
fn alert_body(incident: &Value) -> String {
    let channel = &incident["first_trigger_log_entry"]["channel"];
    let details = match &channel["details"] {
        Value::Object(fields) => fields
            .iter()
            .map(|(key, value)| match value {
                Value::String(text) => format!("{key}: {text}"),
                other => format!("{key}: {other}"),
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Value::String(text) => text.clone(),
        _ => String::new(),
    };
    [
        details,
        channel["summary"].as_str().unwrap_or_default().to_string(),
        incident["description"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
    ]
    .into_iter()
    .map(|text| text.trim().to_string())
    .find(|text| !text.is_empty())
    .unwrap_or_default()
}

/// The board priority an urgency reads as.
fn priority_for(urgency: Option<&str>) -> Option<String> {
    match urgency? {
        "high" => Some("P1".to_string()),
        "low" => Some("P3".to_string()),
        _ => None,
    }
}

/// `resolved` is done; triggered and acknowledged are both still open work
/// someone may want investigated, so both read as new.
fn category_for(status: Option<&str>) -> Option<String> {
    status.map(|status| {
        if status == "resolved" {
            "done".to_string()
        } else {
            "new".to_string()
        }
    })
}

pub struct PagerDutyTracker {
    config: TrackerConfig,
    base: String,
    http: reqwest::Client,
    /// The configured user, read once: its email is the `From` every write
    /// needs.
    me: tokio::sync::OnceCell<(TrackerIdentity, String)>,
    /// Incident id → its page, as the API reported it; `issue_url` is
    /// answered from here.
    urls: Mutex<HashMap<String, String>>,
    /// `https://<subdomain>.pagerduty.com`, from the first page URL seen.
    web: OnceLock<String>,
}

impl PagerDutyTracker {
    pub fn new(config: TrackerConfig) -> Self {
        Self::with_base(config, API_BASE.to_string())
    }

    pub(crate) fn with_base(config: TrackerConfig, base: String) -> Self {
        Self {
            config,
            base,
            http: reqwest::Client::new(),
            me: tokio::sync::OnceCell::new(),
            urls: Mutex::new(HashMap::new()),
            web: OnceLock::new(),
        }
    }

    pub fn descriptor() -> Descriptor {
        Descriptor::new(
            KIND.as_str(),
            "PagerDuty",
            "Incidents mode: open incidents become investigation cards, and the triage report \
             goes back to the incident as a note. Prep Cook never pages anyone and resolves an \
             incident only when configured to.",
            vec![
                Field::new(
                    "pagerduty_api_token",
                    "API key",
                    FieldKind::Secret,
                    true,
                    "A REST API key (Integrations → API Access Keys) that can read incidents \
                     and write notes.",
                ),
                Field::new(
                    "user_id",
                    "User id",
                    FieldKind::Text,
                    true,
                    "The PagerDuty user Prep Cook writes as, e.g. PABC123: a dedicated user, \
                     not on any schedule, so nothing pages it.",
                ),
                Field::new(
                    "service_ids",
                    "Service ids",
                    FieldKind::Text,
                    false,
                    "Comma-separated services whose incidents are picked up, e.g. PSVC123. \
                     Blank picks up every service the key can see.",
                ),
                Field::new(
                    "pickup_filter",
                    "Pickup filter",
                    FieldKind::Text,
                    false,
                    "Which incidents are picked up: service:PSVC123, urgency:high|low, \
                     status:triggered,acknowledged, assigned:@me, team:PTEAM12. Default: \
                     status:triggered,acknowledged.",
                ),
                Field::new(
                    "import_filter",
                    "Import filter",
                    FieldKind::Text,
                    false,
                    "Incidents the import dialog lists, in the same terms. Default: \
                     status:triggered,acknowledged.",
                ),
            ],
            "https://developer.pagerduty.com/api-reference/",
        )
    }

    fn token(&self) -> TrackerResult<&str> {
        self.config.pagerduty_api_token().ok_or_else(|| {
            TrackerError::NotConfigured("the PagerDuty API key is not set".to_string())
        })
    }

    fn user_id(&self) -> TrackerResult<&str> {
        self.config.user_id().ok_or_else(|| {
            TrackerError::NotConfigured("the PagerDuty user id is not set".to_string())
        })
    }

    fn request(
        &self,
        method: reqwest::Method,
        path: &str,
    ) -> TrackerResult<reqwest::RequestBuilder> {
        Ok(self
            .http
            .request(method, format!("{}/{path}", self.base))
            .header(
                reqwest::header::AUTHORIZATION,
                format!("Token token={}", self.token()?),
            )
            .header(reqwest::header::ACCEPT, ACCEPT))
    }

    async fn get(&self, path: &str, query: &[(&str, String)], what: &str) -> TrackerResult<Value> {
        let response = self
            .request(reqwest::Method::GET, path)?
            .query(query)
            .send()
            .await
            .with_context(|| format!("failed to reach PagerDuty ({what})"))?;
        if !response.status().is_success() {
            return Err(rejected(response, what).await);
        }
        Ok(response
            .json()
            .await
            .with_context(|| format!("failed to parse the PagerDuty {what} response"))?)
    }

    /// A write, recorded as the configured user.
    async fn write(
        &self,
        method: reqwest::Method,
        path: &str,
        body: &Value,
        what: &str,
    ) -> TrackerResult<Value> {
        let (_, email) = self.me().await?;
        let response = self
            .request(method, path)?
            .header("From", email)
            .json(body)
            .send()
            .await
            .with_context(|| format!("failed to reach PagerDuty ({what})"))?;
        if !response.status().is_success() {
            return Err(rejected(response, what).await);
        }
        Ok(response.json().await.unwrap_or(Value::Null))
    }

    /// The configured user, and its email.
    async fn me(&self) -> TrackerResult<&(TrackerIdentity, String)> {
        self.me
            .get_or_try_init(|| async {
                let user_id = self.user_id()?;
                let user = self
                    .get(&format!("users/{user_id}"), &[], "user read")
                    .await?;
                let user = &user["user"];
                let email = user["email"]
                    .as_str()
                    .ok_or_else(|| anyhow!("PagerDuty user {user_id} has no email"))?
                    .to_string();
                let account_id = self.config.bot_identity().unwrap_or(user_id).to_string();
                Ok::<_, TrackerError>((
                    TrackerIdentity {
                        display_name: user["name"].as_str().unwrap_or(&account_id).to_string(),
                        account_id,
                    },
                    email,
                ))
            })
            .await
    }

    fn remember_url(&self, incident: &Value) {
        let (Some(id), Some(url)) = (incident["id"].as_str(), incident["html_url"].as_str()) else {
            return;
        };
        if let Some(at) = url.find("/incidents/") {
            let _ = self.web.set(url[..at].to_string());
        }
        self.urls
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(id.to_string(), url.to_string());
    }

    fn to_fetched(&self, incident: &Value) -> Option<FetchedIssue> {
        let id = incident["id"].as_str()?;
        self.remember_url(incident);
        let status = incident["status"].as_str();
        Some(FetchedIssue {
            key: id.to_string(),
            summary: incident["title"]
                .as_str()
                .or(incident["summary"].as_str())
                .unwrap_or_default()
                .to_string(),
            issue_type: "Incident".to_string(),
            description_text: alert_body(incident),
            parent_key: None,
            parent_type: None,
            priority_name: priority_for(incident["urgency"].as_str()),
            created: timestamp(&incident["created_at"]),
            updated: timestamp(&incident["updated_at"])
                .or_else(|| timestamp(&incident["last_status_change_at"])),
            assignee_account_id: incident["assignments"][0]["assignee"]["id"]
                .as_str()
                .map(str::to_string),
            status_category: category_for(status),
            status_name: status.map(str::to_string),
            labels: Vec::new(),
            area: incident["service"]["summary"].as_str().map(str::to_string),
        })
    }

    /// Lists incidents. `since` bounds `created_at`; without it every date
    /// is searched.
    async fn list(
        &self,
        filter: &IncidentFilter,
        since: Option<DateTime<Utc>>,
    ) -> TrackerResult<Vec<FetchedIssue>> {
        let mut query: Vec<(&str, String)> = vec![
            ("limit", PAGE_SIZE.to_string()),
            ("sort_by", "created_at:asc".to_string()),
            ("include[]", "first_trigger_log_entries".to_string()),
        ];
        let statuses: Vec<String> = if filter.statuses.is_empty() {
            OPEN_STATUSES.iter().map(|s| s.to_string()).collect()
        } else {
            filter.statuses.clone()
        };
        query.extend(statuses.into_iter().map(|status| ("statuses[]", status)));
        let services = if filter.services.is_empty() {
            self.config.service_ids()
        } else {
            filter.services.clone()
        };
        query.extend(services.into_iter().map(|id| ("service_ids[]", id)));
        query.extend(
            filter
                .urgencies
                .iter()
                .map(|urgency| ("urgencies[]", urgency.clone())),
        );
        query.extend(filter.teams.iter().map(|team| ("team_ids[]", team.clone())));
        for user in &filter.assigned {
            let user = if user.eq_ignore_ascii_case("@me") {
                self.user_id()?.to_string()
            } else {
                user.clone()
            };
            query.push(("user_ids[]", user));
        }
        match since {
            Some(since) => {
                query.push(("since", since.to_rfc3339()));
                query.push(("until", Utc::now().to_rfc3339()));
            }
            None => query.push(("date_range", "all".to_string())),
        }

        let mut out = Vec::new();
        for page in 0..MAX_PAGES {
            let mut paged = query.clone();
            paged.push(("offset", (page * PAGE_SIZE).to_string()));
            let value = self.get("incidents", &paged, "incident search").await?;
            out.extend(
                value["incidents"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|incident| self.to_fetched(incident)),
            );
            if value["more"].as_bool() != Some(true) {
                break;
            }
        }
        Ok(out)
    }

    async fn incident(&self, id: &str) -> TrackerResult<Value> {
        let value = self
            .get(
                &format!("incidents/{id}"),
                &[("include[]", "first_trigger_log_entries".to_string())],
                "incident read",
            )
            .await?;
        self.remember_url(&value["incident"]);
        Ok(value["incident"].clone())
    }

    async fn set_status(&self, key: &str, status: &str) -> TrackerResult<()> {
        self.write(
            reqwest::Method::PUT,
            &format!("incidents/{key}"),
            &json!({ "incident": { "type": "incident_reference", "status": status } }),
            "status update",
        )
        .await?;
        Ok(())
    }

    async fn add_note(&self, key: &str, text: &str) -> TrackerResult<()> {
        self.write(
            reqwest::Method::POST,
            &format!("incidents/{key}/notes"),
            &json!({ "note": { "content": clip_text(&at_mentions(text), NOTE_LIMIT, "PagerDuty") } }),
            "note",
        )
        .await?;
        Ok(())
    }

    /// Resolves the incident unless it already is.
    async fn resolve(&self, key: &str) -> TrackerResult<()> {
        if self.incident(key).await?["status"].as_str() == Some("resolved") {
            return Ok(());
        }
        self.set_status(key, "resolved").await
    }
}

#[async_trait]
impl Tracker for PagerDutyTracker {
    fn kind(&self) -> TrackerKind {
        KIND
    }

    fn is_configured(&self) -> bool {
        self.config.pagerduty_api_token().is_some()
            && self.config.user_id().is_some()
            && self.config.mode_mismatch().is_none()
    }

    /// The page the API reported for the incident; before it has been read,
    /// the account's incident URL shape, or PagerDuty's own redirecting one.
    fn issue_url(&self, key: &str) -> String {
        if let Some(url) = self
            .urls
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(key)
        {
            return url.clone();
        }
        let web = self
            .web
            .get()
            .cloned()
            .unwrap_or_else(|| "https://app.pagerduty.com".to_string());
        format!("{web}/incidents/{key}")
    }

    /// Incident ids: letters and digits.
    fn is_valid_key(&self, key: &str) -> bool {
        (5..=20).contains(&key.len())
            && key.chars().all(|c| c.is_ascii_alphanumeric())
            && key.chars().any(|c| c.is_ascii_alphabetic())
    }

    /// `users/{user_id}`: proves the key and that the user exists.
    async fn test(&self) -> TrackerResult<String> {
        let (identity, email) = self.me().await?;
        let services = self.config.service_ids();
        Ok(format!(
            "Signed in to PagerDuty; notes are written as {} ({email}){}.",
            identity.display_name,
            if services.is_empty() {
                ", picking up incidents on every service".to_string()
            } else {
                format!(", picking up incidents on {}", services.join(", "))
            }
        ))
    }

    async fn search(&self, query: &str) -> TrackerResult<Vec<FetchedIssue>> {
        let filter = parse_filter(query).map_err(|e| TrackerError::Other(anyhow!(e)))?;
        self.list(&filter, None).await
    }

    async fn find(&self, query: Query<'_>) -> TrackerResult<Vec<FetchedIssue>> {
        let parse =
            |filter: &str| parse_filter(filter).map_err(|e| TrackerError::Other(anyhow!(e)));
        match query {
            Query::Created { filter, within } => {
                self.list(&parse(filter)?, Some(Utc::now() - within)).await
            }
            // No API filter on `updated_at`: open incidents are few, so the
            // window is applied to the answer.
            Query::Updated { filter, within } => {
                let since = Utc::now() - within;
                let mut found: Vec<FetchedIssue> = self
                    .list(&parse(filter)?, None)
                    .await?
                    .into_iter()
                    .filter(|issue| issue.updated.is_none_or(|at| at >= since))
                    .collect();
                found.sort_by_key(|issue| issue.updated);
                Ok(found)
            }
            Query::Reopened { .. } | Query::Active { .. } => Ok(Vec::new()),
            Query::InStatuses {
                statuses, within, ..
            } => {
                let statuses: Vec<String> = statuses
                    .iter()
                    .map(|status| status.trim().to_ascii_lowercase())
                    .filter(|status| STATUSES.contains(&status.as_str()))
                    .collect();
                if statuses.is_empty() {
                    return Ok(Vec::new());
                }
                self.list(
                    &IncidentFilter {
                        statuses,
                        ..Default::default()
                    },
                    Some(Utc::now() - within),
                )
                .await
            }
        }
    }

    async fn fetch(&self, keys: &[String]) -> TrackerResult<Vec<FetchedIssue>> {
        let mut out = Vec::new();
        for key in keys.iter().filter(|key| self.is_valid_key(key)) {
            match self.incident(key).await {
                Ok(incident) => out.extend(self.to_fetched(&incident)),
                Err(TrackerError::Other(error)) if error.to_string().contains("HTTP 404") => {}
                Err(error) => return Err(error),
            }
        }
        Ok(out)
    }

    async fn get_issue(&self, key: &str) -> TrackerResult<IssueFields> {
        let incident = self.incident(key).await?;
        let fetched = self
            .to_fetched(&incident)
            .ok_or_else(|| anyhow!("PagerDuty returned no incident {key}"))?;
        Ok(IssueFields {
            summary: fetched.summary,
            issue_type: Some(fetched.issue_type),
            priority: fetched.priority_name,
            status_category: fetched.status_category,
            assignee_account_id: fetched.assignee_account_id,
            reporter_account_id: None,
            status_name: fetched.status_name,
            labels: fetched.labels,
        })
    }

    async fn comments(&self, key: &str) -> TrackerResult<Vec<TrackerComment>> {
        let value = self
            .get(&format!("incidents/{key}/notes"), &[], "notes")
            .await?;
        let mut notes: Vec<TrackerComment> = value["notes"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|note| TrackerComment {
                author: note["user"]["summary"]
                    .as_str()
                    .unwrap_or("Unknown")
                    .to_string(),
                author_account_id: note["user"]["id"].as_str().map(str::to_string),
                mentions: Vec::new(),
                created: timestamp(&note["created_at"]),
                body_text: note["content"]
                    .as_str()
                    .unwrap_or_default()
                    .trim()
                    .to_string(),
            })
            .collect();
        notes.sort_by_key(|note| note.created);
        Ok(notes)
    }

    async fn post_comment(&self, key: &str, text: &str) -> TrackerResult<()> {
        self.add_note(key, text).await
    }

    async fn transition(&self, key: &str, status_name: &str) -> TrackerResult<()> {
        match status_name.trim().to_ascii_lowercase().as_str() {
            "acknowledged" | "acknowledge" => self.transition_to_in_progress(key).await,
            "resolved" | "resolve" => self.resolve(key).await,
            other => {
                tracing::debug!("PagerDuty: no status move for `{other}` on {key}");
                Ok(())
            }
        }
    }

    /// Acknowledges a triggered incident; one already acknowledged or
    /// resolved is left as it is.
    async fn transition_to_in_progress(&self, key: &str) -> TrackerResult<()> {
        if self.incident(key).await?["status"].as_str() != Some("triggered") {
            return Ok(());
        }
        self.set_status(key, "acknowledged").await
    }

    /// Planning acknowledges (and never reassigns: the responders keep the
    /// incident), Triaged resolves only with `resolve_on_triage`, Clean-up
    /// resolves, and the rest leave the incident alone.
    async fn enter_stage(&self, key: &str, stage: TicketStage) -> TrackerResult<()> {
        match stage {
            TicketStage::Planning => self.transition_to_in_progress(key).await,
            TicketStage::Triaged if self.config.incidents.resolve_on_triage => {
                self.resolve(key).await
            }
            TicketStage::Done => self.resolve(key).await,
            other => {
                tracing::debug!("PagerDuty: {other:?} leaves {key} as it is");
                Ok(())
            }
        }
    }

    async fn assignee(&self, key: &str) -> TrackerResult<Option<TrackerUser>> {
        let incident = self.incident(key).await?;
        let assignee = &incident["assignments"][0]["assignee"];
        Ok(assignee["id"].as_str().map(|id| TrackerUser {
            account_id: id.to_string(),
            display_name: assignee["summary"].as_str().unwrap_or(id).to_string(),
        }))
    }

    async fn assign_to_self(&self, key: &str) -> TrackerResult<()> {
        let user_id = self.user_id()?.to_string();
        self.write(
            reqwest::Method::PUT,
            &format!("incidents/{key}"),
            &json!({ "incident": {
                "type": "incident_reference",
                "assignments": [{ "assignee": { "id": user_id, "type": "user_reference" } }]
            } }),
            "reassignment",
        )
        .await?;
        Ok(())
    }

    async fn set_summary(&self, key: &str, summary: &str) -> TrackerResult<()> {
        self.write(
            reqwest::Method::PUT,
            &format!("incidents/{key}"),
            &json!({ "incident": { "type": "incident_reference", "title": summary } }),
            "title update",
        )
        .await?;
        Ok(())
    }

    async fn link_blocks(&self, blocker: &str, blocked: &str) -> TrackerResult<()> {
        self.add_note(
            blocked,
            &format!("Blocked by {blocker}: {}", self.issue_url(blocker)),
        )
        .await
    }

    /// Notes cannot be edited, so a link already noted is not noted again.
    async fn set_remote_link(&self, key: &str, link: RemoteLink<'_>) -> TrackerResult<()> {
        let text = format!("{}: {}", link.title, link.url);
        if self
            .comments(key)
            .await?
            .iter()
            .any(|note| note.body_text.contains(link.url))
        {
            return Ok(());
        }
        self.add_note(key, &text).await
    }

    async fn add_watcher(&self, _key: &str, _account_id: &str) -> TrackerResult<()> {
        Err(TrackerError::Unsupported {
            kind: KIND,
            operation: "add a responder without paging them",
        })
    }

    /// A low-urgency incident on the first configured service.
    async fn create_issue(&self, issue: &NewIssue) -> TrackerResult<String> {
        let service = self
            .config
            .service_ids()
            .into_iter()
            .next()
            .ok_or_else(|| {
                TrackerError::NotConfigured(
                    "creating an incident needs a service id in ticketing.service_ids".to_string(),
                )
            })?;
        let created = self
            .write(
                reqwest::Method::POST,
                "incidents",
                &json!({ "incident": {
                    "type": "incident",
                    "title": issue.summary,
                    "urgency": "low",
                    "service": { "id": service, "type": "service_reference" },
                    "body": {
                        "type": "incident_body",
                        "details": clip_text(&issue.description, NOTE_LIMIT, "PagerDuty")
                    }
                } }),
                "incident create",
            )
            .await?;
        self.remember_url(&created["incident"]);
        created["incident"]["id"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| TrackerError::Other(anyhow!("PagerDuty create returned no incident id")))
    }

    async fn create_subtasks(
        &self,
        _parent: &str,
        _items: &[NewSubtask],
    ) -> TrackerResult<Vec<CreatedSubtask>> {
        Err(TrackerError::Unsupported {
            kind: KIND,
            operation: "create sub-tasks",
        })
    }

    /// Incidents carry no files.
    async fn download_attachments(
        &self,
        _key: &str,
        _dir: &Path,
    ) -> TrackerResult<Vec<TrackerAttachment>> {
        Ok(Vec::new())
    }

    async fn identity(&self) -> TrackerResult<TrackerIdentity> {
        Ok(self.me().await?.0.clone())
    }

    /// The users the configured services' escalation policies reach: named
    /// users, and the members of the schedules they name.
    async fn assignable_users(&self, _project: &str) -> TrackerResult<Vec<TrackerUser>> {
        let agent = self.me().await.ok().map(|(me, _)| me.account_id.clone());
        let mut users: Vec<TrackerUser> = Vec::new();
        let mut push = |id: &str, name: &str| {
            if agent.as_deref() != Some(id) && !users.iter().any(|user| user.account_id == id) {
                users.push(TrackerUser {
                    account_id: id.to_string(),
                    display_name: name.to_string(),
                });
            }
        };
        let mut policies: Vec<String> = Vec::new();
        for service in self.config.service_ids() {
            let value = self
                .get(&format!("services/{service}"), &[], "service read")
                .await?;
            if let Some(policy) = value["service"]["escalation_policy"]["id"].as_str()
                && !policies.iter().any(|seen| seen == policy)
            {
                policies.push(policy.to_string());
            }
        }
        for policy in policies {
            let value = self
                .get(
                    &format!("escalation_policies/{policy}"),
                    &[],
                    "escalation policy read",
                )
                .await?;
            let targets: Vec<Value> = value["escalation_policy"]["escalation_rules"]
                .as_array()
                .into_iter()
                .flatten()
                .flat_map(|rule| rule["targets"].as_array().cloned().unwrap_or_default())
                .collect();
            for target in targets {
                let Some(id) = target["id"].as_str() else {
                    continue;
                };
                match target["type"].as_str() {
                    Some("user_reference" | "user") => {
                        push(id, target["summary"].as_str().unwrap_or(id));
                    }
                    Some("schedule_reference" | "schedule") => {
                        let members = self
                            .get(&format!("schedules/{id}/users"), &[], "schedule read")
                            .await?;
                        for user in members["users"].as_array().into_iter().flatten() {
                            if let Some(user_id) = user["id"].as_str() {
                                push(user_id, user["name"].as_str().unwrap_or(user_id));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(users)
    }
}

#[cfg(test)]
mod tests {
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{body_json, header, method, path, query_param},
    };

    use super::*;
    use crate::services::config::TicketingMode;

    fn config() -> TrackerConfig {
        TrackerConfig {
            mode: TicketingMode::Incidents,
            kind: TrackerKind::PagerDuty,
            pagerduty_api_token: Some("pd-token".to_string()),
            user_id: Some("PBOT1".to_string()),
            service_ids: Some("PSVC1".to_string()),
            ..Default::default()
        }
    }

    fn tracker(server: &MockServer, config: TrackerConfig) -> PagerDutyTracker {
        PagerDutyTracker::with_base(config, server.uri())
    }

    async fn mount_user(server: &MockServer) {
        Mock::given(method("GET"))
            .and(path("/users/PBOT1"))
            .and(header("Authorization", "Token token=pd-token"))
            .and(header("Accept", ACCEPT))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "user": {"id": "PBOT1", "name": "Prep Cook", "email": "prepcook@acme.test"}
            })))
            .mount(server)
            .await;
    }

    fn incident(id: &str, status: &str) -> Value {
        json!({
            "id": id,
            "type": "incident",
            "title": "Checkout returns 500s",
            "status": status,
            "urgency": "high",
            "created_at": "2026-10-18T09:00:00Z",
            "updated_at": "2026-10-18T09:05:00Z",
            "html_url": format!("https://acme.pagerduty.com/incidents/{id}"),
            "service": {"id": "PSVC1", "summary": "checkout"},
            "assignments": [{"assignee": {"id": "PONCALL", "summary": "Grace"}}],
            "first_trigger_log_entry": {"channel": {
                "type": "web_trigger",
                "summary": "Checkout 500s",
                "details": {"error_rate": "12%", "endpoint": "/api/checkout"}
            }}
        })
    }

    async fn mount_incident(server: &MockServer, id: &str, status: &str) {
        Mock::given(method("GET"))
            .and(path(format!("/incidents/{id}")))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({ "incident": incident(id, status) })),
            )
            .mount(server)
            .await;
    }

    #[test]
    fn filters_parse_and_reject_other_syntaxes() {
        let filter =
            parse_filter("service:PSVC1,PSVC2 urgency:high status:triggered assigned:@me team:PT1")
                .unwrap();
        assert_eq!(filter.services, ["PSVC1", "PSVC2"]);
        assert_eq!(filter.urgencies, ["high"]);
        assert_eq!(filter.statuses, ["triggered"]);
        assert_eq!(filter.assigned, ["@me"]);
        assert_eq!(filter.teams, ["PT1"]);
        assert!(parse_filter("urgency:medium").is_err());
        assert!(parse_filter("status:open").is_err());
        assert!(parse_filter("project = BUG").is_err());
    }

    #[tokio::test]
    async fn test_reads_the_configured_user() {
        let server = MockServer::start().await;
        mount_user(&server).await;
        let message = tracker(&server, config()).test().await.unwrap();
        assert_eq!(
            message,
            "Signed in to PagerDuty; notes are written as Prep Cook (prepcook@acme.test), \
             picking up incidents on PSVC1."
        );

        let rejected = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/users/PBOT1"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&rejected)
            .await;
        assert!(matches!(
            tracker(&rejected, config()).test().await,
            Err(TrackerError::AuthFailed)
        ));
    }

    #[tokio::test]
    async fn pickup_lists_open_incidents_on_the_configured_services() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/incidents"))
            .and(query_param("statuses[]", "triggered"))
            .and(query_param("statuses[]", "acknowledged"))
            .and(query_param("service_ids[]", "PSVC1"))
            .and(query_param("urgencies[]", "high"))
            .and(query_param("include[]", "first_trigger_log_entries"))
            .and(query_param("sort_by", "created_at:asc"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "incidents": [incident("Q1ABC", "triggered")],
                "more": false
            })))
            .expect(1)
            .mount(&server)
            .await;
        let pd = tracker(&server, config());
        let found = pd
            .find(Query::Created {
                filter: "urgency:high",
                within: chrono::Duration::hours(24),
            })
            .await
            .unwrap();
        assert_eq!(found.len(), 1);
        let incident = &found[0];
        assert_eq!(incident.key, "Q1ABC");
        assert_eq!(incident.summary, "Checkout returns 500s");
        assert_eq!(incident.issue_type, "Incident");
        assert_eq!(incident.priority_name.as_deref(), Some("P1"));
        assert_eq!(incident.area.as_deref(), Some("checkout"));
        assert_eq!(incident.status_category.as_deref(), Some("new"));
        assert_eq!(incident.assignee_account_id.as_deref(), Some("PONCALL"));
        assert_eq!(
            incident.description_text,
            "error_rate: 12%\nendpoint: /api/checkout"
        );
        // The page the API reported is the card's link and origin.
        assert_eq!(
            pd.issue_url("Q1ABC"),
            "https://acme.pagerduty.com/incidents/Q1ABC"
        );
        assert_eq!(
            pd.issue_url("Q2DEF"),
            "https://acme.pagerduty.com/incidents/Q2DEF"
        );
        // Incidents do not reopen and notes carry no mentions.
        assert!(
            pd.find(Query::Reopened {
                project: "x",
                within: chrono::Duration::days(3)
            })
            .await
            .unwrap()
            .is_empty()
        );
    }

    #[tokio::test]
    async fn notes_are_the_comments_and_are_written_as_the_configured_user() {
        let server = MockServer::start().await;
        mount_user(&server).await;
        Mock::given(method("GET"))
            .and(path("/incidents/Q1ABC/notes"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "notes": [
                {"id": "N2", "user": {"id": "PONCALL", "summary": "Grace"},
                 "content": "RCA: APPROVED", "created_at": "2026-10-18T10:00:00Z"},
                {"id": "N1", "user": {"id": "PBOT1", "summary": "Prep Cook"},
                 "content": "Looking into it.", "created_at": "2026-10-18T09:10:00Z"}
            ]})))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/incidents/Q1ABC/notes"))
            .and(header("From", "prepcook@acme.test"))
            .and(body_json(
                json!({"note": {"content": "TRIAGE REPORT\n@PONCALL see above"}}),
            ))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({"note": {"id": "N3"}})))
            .expect(1)
            .mount(&server)
            .await;
        let pd = tracker(&server, config());
        let notes = pd.comments("Q1ABC").await.unwrap();
        assert_eq!(notes.len(), 2);
        assert_eq!(notes[0].author, "Prep Cook");
        assert_eq!(notes[1].author_account_id.as_deref(), Some("PONCALL"));
        assert_eq!(notes[1].body_text, "RCA: APPROVED");
        pd.post_comment("Q1ABC", "TRIAGE REPORT\n[~accountid:PONCALL] see above")
            .await
            .unwrap();
        assert_eq!(pd.identity().await.unwrap().account_id, "PBOT1");
    }

    #[tokio::test]
    async fn planning_acknowledges_and_never_reassigns() {
        let server = MockServer::start().await;
        mount_user(&server).await;
        mount_incident(&server, "Q1ABC", "triggered").await;
        Mock::given(method("PUT"))
            .and(path("/incidents/Q1ABC"))
            .and(header("From", "prepcook@acme.test"))
            .and(body_json(json!({"incident": {"type": "incident_reference",
                                               "status": "acknowledged"}})))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
            .expect(1)
            .mount(&server)
            .await;
        let pd = tracker(&server, config());
        pd.enter_stage("Q1ABC", TicketStage::Planning)
            .await
            .unwrap();
        // Other stages before the end leave it alone.
        pd.enter_stage("Q1ABC", TicketStage::Executing)
            .await
            .unwrap();
        // So does triage, unless configured to resolve.
        pd.enter_stage("Q1ABC", TicketStage::Triaged).await.unwrap();
        // A name that is not a PagerDuty status is a no-op.
        pd.transition("Q1ABC", "In Progress").await.unwrap();
    }

    #[tokio::test]
    async fn triage_resolves_only_when_configured_and_clean_up_resolves() {
        let server = MockServer::start().await;
        mount_user(&server).await;
        mount_incident(&server, "Q1ABC", "acknowledged").await;
        Mock::given(method("PUT"))
            .and(path("/incidents/Q1ABC"))
            .and(body_json(json!({"incident": {"type": "incident_reference",
                                               "status": "resolved"}})))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
            .expect(2)
            .mount(&server)
            .await;
        let mut resolving = config();
        resolving.incidents.resolve_on_triage = true;
        let pd = tracker(&server, resolving);
        pd.enter_stage("Q1ABC", TicketStage::Triaged).await.unwrap();
        tracker(&server, config())
            .enter_stage("Q1ABC", TicketStage::Done)
            .await
            .unwrap();
        // Acknowledging an incident that is already acknowledged is a no-op.
        pd.transition_to_in_progress("Q1ABC").await.unwrap();
    }

    #[tokio::test]
    async fn assign_title_links_and_create() {
        let server = MockServer::start().await;
        mount_user(&server).await;
        mount_incident(&server, "Q1ABC", "acknowledged").await;
        Mock::given(method("PUT"))
            .and(path("/incidents/Q1ABC"))
            .and(body_json(json!({"incident": {"type": "incident_reference",
                "assignments": [{"assignee": {"id": "PBOT1", "type": "user_reference"}}]}})))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("PUT"))
            .and(path("/incidents/Q1ABC"))
            .and(body_json(json!({"incident": {"type": "incident_reference",
                                               "title": "Checkout 500s: pool exhausted"}})))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/incidents/Q1ABC/notes"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"notes": [
                {"id": "N1", "user": {"id": "PBOT1"}, "content": "RCA: https://kb.example.com/1",
                 "created_at": "2026-10-18T09:10:00Z"}
            ]})))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/incidents/Q1ABC/notes"))
            .and(body_json(json!({"note": {"content": "Blocked by Q9XYZ: \
                https://acme.pagerduty.com/incidents/Q9XYZ"}})))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({})))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/incidents"))
            .and(body_json(json!({"incident": {
                "type": "incident", "title": "Pattern review", "urgency": "low",
                "service": {"id": "PSVC1", "type": "service_reference"},
                "body": {"type": "incident_body", "details": "Five RCAs."}
            }})))
            .respond_with(
                ResponseTemplate::new(201).set_body_json(json!({"incident": {
                    "id": "Q7NEW", "html_url": "https://acme.pagerduty.com/incidents/Q7NEW"
                }})),
            )
            .expect(1)
            .mount(&server)
            .await;

        let pd = tracker(&server, config());
        assert_eq!(
            pd.assignee("Q1ABC").await.unwrap().unwrap().display_name,
            "Grace"
        );
        pd.assign_to_self("Q1ABC").await.unwrap();
        pd.set_summary("Q1ABC", "Checkout 500s: pool exhausted")
            .await
            .unwrap();
        // Already noted: not noted again.
        pd.set_remote_link(
            "Q1ABC",
            RemoteLink {
                id: "rca",
                title: "RCA",
                url: "https://kb.example.com/1",
            },
        )
        .await
        .unwrap();
        pd.link_blocks("Q9XYZ", "Q1ABC").await.unwrap();
        let key = pd
            .create_issue(&NewIssue {
                project_key: None,
                issue_type: "Story".to_string(),
                summary: "Pattern review".to_string(),
                description: "Five RCAs.".to_string(),
                priority: "P2".to_string(),
                labels: Vec::new(),
            })
            .await
            .unwrap();
        assert_eq!(key, "Q7NEW");
        assert_eq!(
            pd.get_issue("Q1ABC").await.unwrap().status_name.as_deref(),
            Some("acknowledged")
        );
    }

    #[tokio::test]
    async fn watchers_sub_tasks_and_attachments() {
        let server = MockServer::start().await;
        let pd = tracker(&server, config());
        assert!(matches!(
            pd.add_watcher("Q1ABC", "PONCALL").await,
            Err(TrackerError::Unsupported {
                kind: TrackerKind::PagerDuty,
                ..
            })
        ));
        assert!(matches!(
            pd.create_subtasks("Q1ABC", &[]).await,
            Err(TrackerError::Unsupported { .. })
        ));
        assert!(
            pd.download_attachments("Q1ABC", Path::new("/nonexistent"))
                .await
                .unwrap()
                .is_empty()
        );
        assert!(pd.is_valid_key("Q1ABC2DEF"));
        assert!(!pd.is_valid_key("12345"));
        assert!(!pd.is_valid_key("Q1-AB"));
    }

    #[tokio::test]
    async fn assignable_users_are_the_escalation_policy_reach() {
        let server = MockServer::start().await;
        mount_user(&server).await;
        Mock::given(method("GET"))
            .and(path("/services/PSVC1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "service": {"id": "PSVC1", "escalation_policy": {"id": "PEP1"}}
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/escalation_policies/PEP1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "escalation_policy": {"escalation_rules": [
                    {"targets": [{"id": "PGRACE", "type": "user_reference", "summary": "Grace"},
                                 {"id": "PSCHED", "type": "schedule_reference"}]},
                    {"targets": [{"id": "PBOT1", "type": "user_reference", "summary": "Prep Cook"}]}
                ]}
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/schedules/PSCHED/users"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "users": [{"id": "PADA", "name": "Ada"}, {"id": "PGRACE", "name": "Grace"}]
            })))
            .mount(&server)
            .await;
        let users = tracker(&server, config())
            .assignable_users("ignored")
            .await
            .unwrap();
        let names: Vec<&str> = users.iter().map(|u| u.display_name.as_str()).collect();
        assert_eq!(names, ["Grace", "Ada"]);
    }

    #[tokio::test]
    async fn fetch_skips_incidents_that_are_gone() {
        let server = MockServer::start().await;
        mount_incident(&server, "Q1ABC", "resolved").await;
        Mock::given(method("GET"))
            .and(path("/incidents/Q404X"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        let found = tracker(&server, config())
            .fetch(&[
                "Q1ABC".to_string(),
                "Q404X".to_string(),
                "bad key!".to_string(),
            ])
            .await
            .unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].status_category.as_deref(), Some("done"));
    }
}
