//! Jira Cloud behind [`Tracker`]. Auth is Basic (base64 of
//! `email:api_token`) against `https://{site}`; every Jira-specific shape —
//! Atlassian Document Format, `[~accountid:…]` mentions, transition lookup,
//! JQL — stays in this module.
//!
//! Filters (import, pickup, assigned) are JQL. Stages move the ticket to the
//! status `status_mapping` names for them; Planning also assigns the ticket
//! to the agent's account.

use std::path::Path;

use anyhow::{Context, anyhow};
use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use serde_json::Value;

use super::{
    CreatedSubtask, FetchedIssue, IssueFields, NewIssue, NewSubtask, Query, RemoteLink,
    TicketStage, Tracker, TrackerAttachment, TrackerComment, TrackerError, TrackerIdentity,
    TrackerResult, TrackerUser, is_project_key,
};
use crate::services::{
    config::{TrackerConfig, TrackerKind},
    integrations::{Descriptor, Field, FieldKind},
};

/// Accepts "example.atlassian.net", "https://example.atlassian.net/", etc. and
/// reduces to a bare hostname, since callers prepend the scheme themselves.
fn normalize_site(site: &str) -> String {
    site.trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_matches('/')
        .to_string()
}

fn parse_timestamp(raw: Option<&str>) -> Option<chrono::DateTime<chrono::FixedOffset>> {
    raw.and_then(|raw| chrono::DateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S%.3f%z").ok())
}

fn labels_of(fields: &Value) -> Vec<String> {
    fields["labels"]
        .as_array()
        .map(|labels| {
            labels
                .iter()
                .filter_map(|label| label.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Turns a non-success response into an error, telling a credentials
/// problem apart from everything else.
async fn rejected(response: reqwest::Response, what: &str) -> TrackerError {
    let status = response.status();
    if status.as_u16() == 401 || status.as_u16() == 403 {
        return TrackerError::AuthFailed;
    }
    let text = response.text().await.unwrap_or_default();
    TrackerError::Other(anyhow!(
        "Jira {what} rejected (HTTP {}): {text}",
        status.as_u16()
    ))
}

const MAX_SEARCH_PAGES: usize = 5;
const MAX_COMMENT_PAGES: usize = 10;
/// Keys per `key in (...)` search: the JQL travels as a query parameter, so
/// a batch has to stay well inside URL limits.
const KEY_BATCH: usize = 100;

/// A JQL relative date for a window back from now: hours under two days,
/// whole days beyond (`-24h`, `-3d`).
fn jql_window(within: chrono::Duration) -> String {
    let hours = within.num_hours().max(1);
    if hours >= 48 && hours % 24 == 0 {
        format!("-{}d", hours / 24)
    } else {
        format!("-{hours}h")
    }
}

/// The JQL one of the watcher's or orchestrator's questions becomes. `None`
/// when the question cannot match anything (no statuses to look for).
pub(crate) fn jql_for(query: Query<'_>) -> Option<String> {
    Some(match query {
        Query::Created { filter, within } => format!(
            "({filter}) AND created >= {} ORDER BY created ASC",
            jql_window(within)
        ),
        Query::Updated { filter, within } => format!(
            "({filter}) AND updated >= {} ORDER BY updated ASC",
            jql_window(within)
        ),
        // `statusCategory = "To Do"`, not `!= Done`: a reopen puts the ticket
        // back at the start, while `!= Done` also matches every forward move
        // between working statuses.
        Query::Reopened { project, within } => format!(
            "project = {project} AND statusCategory = \"To Do\" AND status CHANGED AFTER {} \
             ORDER BY updated ASC",
            jql_window(within)
        ),
        Query::Active { project, within } => format!(
            "project = {project} AND updated >= {} ORDER BY updated ASC",
            jql_window(within)
        ),
        Query::InStatuses {
            project,
            statuses,
            within,
        } => {
            if statuses.is_empty() {
                return None;
            }
            let clause = statuses
                .iter()
                .map(|name| format!("\"{name}\""))
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "project = \"{project}\" AND status IN ({clause}) AND updated >= {}",
                jql_window(within)
            )
        }
    })
}

pub struct JiraTracker {
    config: TrackerConfig,
    /// `https://{site}`; a test points it at a mock server.
    base: String,
    http: reqwest::Client,
    /// The account the API token belongs to, resolved once per tracker. The
    /// tracker is rebuilt when its config changes, so new credentials get a
    /// fresh lookup.
    identity: tokio::sync::OnceCell<TrackerIdentity>,
}

impl JiraTracker {
    pub fn new(config: TrackerConfig) -> Self {
        let base = format!("https://{}", normalize_site(config.site()));
        Self::with_base(config, base)
    }

    pub(crate) fn with_base(config: TrackerConfig, base: String) -> Self {
        Self {
            config,
            base,
            http: reqwest::Client::new(),
            identity: tokio::sync::OnceCell::new(),
        }
    }

    pub fn descriptor() -> Descriptor {
        Descriptor::new(
            TrackerKind::Jira.as_str(),
            "Jira",
            "Jira Cloud: tickets come from a project, filters are JQL, and each stage moves \
             the ticket to the status the workflow mapping names.",
            vec![
                Field::new(
                    "site",
                    "Site",
                    FieldKind::Text,
                    true,
                    "Your Atlassian site's host name, e.g. example.atlassian.net.",
                ),
                Field::new(
                    "email",
                    "Email",
                    FieldKind::Text,
                    true,
                    "The email of the agent's Jira account, the one the API token belongs to.",
                ),
                Field::new(
                    "api_token",
                    "API token",
                    FieldKind::Secret,
                    true,
                    "An API token for that account \
                     (https://id.atlassian.com/manage-profile/security/api-tokens).",
                ),
                Field::new(
                    "bot_identity",
                    "Agent account id",
                    FieldKind::Text,
                    false,
                    "The account id the agent's own comments are recognised by. Blank uses the \
                     account the token belongs to.",
                ),
                Field::new(
                    "project_key",
                    "Project key",
                    FieldKind::Text,
                    false,
                    "The project tickets are imported from and filed into (default BUG).",
                ),
                Field::new(
                    "import_filter",
                    "Import filter",
                    FieldKind::Text,
                    false,
                    "JQL the import dialog runs; {project_key} is substituted. Default: To Do \
                     tickets in open sprints assigned to you, by priority.",
                ),
                Field::new(
                    "pickup_filter",
                    "Pickup filter",
                    FieldKind::Text,
                    false,
                    "JQL selecting new tickets to pick up; a created-recently window is added. \
                     Default: project = {project_key} AND priority in (P0, P1) AND issuetype = Bug.",
                ),
                Field::new(
                    "assigned_filter",
                    "Assigned pickup filter",
                    FieldKind::Text,
                    false,
                    "JQL for tickets assigned to the agent, started whatever their priority, \
                     e.g. project = {project_key} AND assignee = currentUser(). Blank turns it off.",
                ),
            ],
            "https://developer.atlassian.com/cloud/jira/platform/rest/v3/",
        )
    }

    fn site(&self) -> String {
        normalize_site(self.config.site())
    }

    fn api(&self, path: &str) -> String {
        format!("{}/rest/api/3/{path}", self.base)
    }

    fn auth(&self) -> TrackerResult<String> {
        let (Some(email), Some(api_token)) = (self.config.email(), self.config.api_token()) else {
            return Err(TrackerError::NotConfigured(
                "Jira email and API token are not set".to_string(),
            ));
        };
        Ok(format!(
            "Basic {}",
            BASE64_STANDARD.encode(format!("{email}:{api_token}"))
        ))
    }

    async fn get_json(&self, url: &str, what: &str) -> TrackerResult<Value> {
        let response = self
            .http
            .get(url)
            .header(reqwest::header::AUTHORIZATION, self.auth()?)
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await
            .with_context(|| format!("failed to reach Jira ({what})"))?;
        if !response.status().is_success() {
            return Err(rejected(response, what).await);
        }
        Ok(response
            .json()
            .await
            .with_context(|| format!("failed to parse Jira {what} response"))?)
    }

    /// Sends a JSON body and checks the status; the response body is
    /// returned for the callers that need it.
    async fn send_json(
        &self,
        request: reqwest::RequestBuilder,
        body: &Value,
        what: &str,
    ) -> TrackerResult<reqwest::Response> {
        let response = request
            .header(reqwest::header::AUTHORIZATION, self.auth()?)
            .header(reqwest::header::ACCEPT, "application/json")
            .json(body)
            .send()
            .await
            .with_context(|| format!("failed to reach Jira ({what})"))?;
        if !response.status().is_success() {
            return Err(rejected(response, what).await);
        }
        Ok(response)
    }

    async fn create_with_fields(&self, fields: Value) -> TrackerResult<String> {
        let response = self
            .send_json(
                self.http.post(self.api("issue")),
                &serde_json::json!({ "fields": fields }),
                "issue create",
            )
            .await?;
        let value: Value = response
            .json()
            .await
            .context("failed to parse Jira create response")?;
        value["key"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| TrackerError::Other(anyhow!("Jira create response missing issue key")))
    }

    /// Applies the transition whose target status is named `target_name`,
    /// falling back to any transition into the `target_category` status
    /// category; `None` means the name must match.
    async fn transition_to(
        &self,
        key: &str,
        target_name: &str,
        target_category: Option<&str>,
    ) -> TrackerResult<()> {
        let transitions_url = self.api(&format!("issue/{key}/transitions"));
        let response = self.get_json(&transitions_url, "transitions").await?;
        let transitions = response["transitions"]
            .as_array()
            .cloned()
            .unwrap_or_default();

        let is_target_category = |transition: &Value| {
            target_category.is_some_and(|target| {
                transition["to"]["statusCategory"]["key"]
                    .as_str()
                    .is_some_and(|key| key.eq_ignore_ascii_case(target))
            })
        };
        let is_target_name = |transition: &Value| {
            transition["to"]["name"]
                .as_str()
                .is_some_and(|name| name.eq_ignore_ascii_case(target_name))
        };

        let chosen = transitions
            .iter()
            .find(|t| is_target_name(t))
            .or_else(|| transitions.iter().find(|t| is_target_category(t)))
            .ok_or_else(|| {
                anyhow!("no transition to a {target_name} status available for {key}")
            })?;
        let transition_id = chosen["id"]
            .as_str()
            .ok_or_else(|| anyhow!("Jira transition has no id"))?;

        self.send_json(
            self.http.post(&transitions_url),
            &serde_json::json!({ "transition": { "id": transition_id } }),
            "transition",
        )
        .await?;
        Ok(())
    }
}

#[async_trait]
impl Tracker for JiraTracker {
    fn kind(&self) -> TrackerKind {
        TrackerKind::Jira
    }

    fn is_configured(&self) -> bool {
        self.config.has_credentials() && self.config.mode_mismatch().is_none()
    }

    fn issue_url(&self, key: &str) -> String {
        format!("https://{}/browse/{key}", self.site())
    }

    fn is_valid_key(&self, key: &str) -> bool {
        is_project_key(key)
    }

    /// `/myself`: proves the site, email and token in one read.
    async fn test(&self) -> TrackerResult<String> {
        let myself = self.get_json(&self.api("myself"), "/myself").await?;
        Ok(format!(
            "Signed in to {} as {}.",
            self.site(),
            myself["displayName"]
                .as_str()
                .or(myself["emailAddress"].as_str())
                .unwrap_or("the token's account")
        ))
    }

    async fn find(&self, query: Query<'_>) -> TrackerResult<Vec<FetchedIssue>> {
        match jql_for(query) {
            Some(jql) => self.search(&jql).await,
            None => Ok(Vec::new()),
        }
    }

    async fn fetch(&self, keys: &[String]) -> TrackerResult<Vec<FetchedIssue>> {
        let keys: Vec<&str> = keys
            .iter()
            .map(String::as_str)
            .filter(|key| is_project_key(key))
            .collect();
        let mut out = Vec::new();
        for batch in keys.chunks(KEY_BATCH) {
            out.extend(
                self.search(&format!("key in ({})", batch.join(",")))
                    .await?,
            );
        }
        Ok(out)
    }

    /// Planning moves the ticket to the planning status and assigns it to the
    /// agent, so a picked-up ticket does not sit unassigned; the other stages
    /// move it only when the mapping names a status.
    async fn enter_stage(&self, key: &str, stage: TicketStage) -> TrackerResult<()> {
        if stage == TicketStage::Planning {
            let moved = self.transition_to_in_progress(key).await;
            let assigned = self.assign_to_self(key).await;
            return moved.and(assigned);
        }
        match stage.mapped(&self.config.status_mapping) {
            Some(status) => self.transition(key, &status).await,
            None => Ok(()),
        }
    }

    /// Runs a JQL search and returns the matching issues (up to
    /// `MAX_SEARCH_PAGES` × 100) from `/rest/api/3/search/jql`.
    async fn search(&self, query: &str) -> TrackerResult<Vec<FetchedIssue>> {
        let url = self.api("search/jql");
        let auth = self.auth()?;
        let mut fetched = Vec::new();
        let mut next_page_token: Option<String> = None;

        for _ in 0..MAX_SEARCH_PAGES {
            let mut params: Vec<(&str, String)> = vec![
                ("jql", query.to_string()),
                (
                    "fields",
                    "summary,issuetype,description,status,parent,priority,created,updated,assignee,labels"
                        .to_string(),
                ),
                ("maxResults", "100".to_string()),
            ];
            if let Some(token) = &next_page_token {
                params.push(("nextPageToken", token.clone()));
            }

            let response = self
                .http
                .get(&url)
                .query(&params)
                .header(reqwest::header::AUTHORIZATION, &auth)
                .header(reqwest::header::ACCEPT, "application/json")
                .send()
                .await
                .context("failed to reach Jira")?;
            if !response.status().is_success() {
                return Err(rejected(response, "search").await);
            }
            let page: Value = response
                .json()
                .await
                .context("failed to parse Jira search response")?;

            for issue in page["issues"].as_array().cloned().unwrap_or_default() {
                let Some(key) = issue["key"].as_str().map(str::to_string) else {
                    continue;
                };
                let fields = &issue["fields"];
                fetched.push(FetchedIssue {
                    key,
                    summary: fields["summary"].as_str().unwrap_or_default().to_string(),
                    issue_type: fields["issuetype"]["name"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string(),
                    description_text: adf_to_text(&fields["description"]),
                    parent_key: fields["parent"]["key"].as_str().map(str::to_string),
                    parent_type: fields["parent"]["fields"]["issuetype"]["name"]
                        .as_str()
                        .map(str::to_string),
                    priority_name: fields["priority"]["name"].as_str().map(str::to_string),
                    created: parse_timestamp(fields["created"].as_str()),
                    updated: parse_timestamp(fields["updated"].as_str()),
                    assignee_account_id: fields["assignee"]["accountId"]
                        .as_str()
                        .map(str::to_string),
                    status_category: fields["status"]["statusCategory"]["key"]
                        .as_str()
                        .map(str::to_string),
                    status_name: fields["status"]["name"].as_str().map(str::to_string),
                    labels: labels_of(fields),
                    area: None,
                });
            }

            next_page_token = page["nextPageToken"].as_str().map(str::to_string);
            if next_page_token.is_none() {
                break;
            }
        }
        Ok(fetched)
    }

    /// Reads summary, type, priority, status, assignee, reporter and labels
    /// in one call: the reopen scan needs all of them, and a request per
    /// field would multiply the Jira traffic.
    async fn get_issue(&self, key: &str) -> TrackerResult<IssueFields> {
        let issue = self
            .get_json(
                &self.api(&format!(
                    "issue/{key}?fields=summary,issuetype,priority,status,assignee,reporter,labels"
                )),
                "issue read",
            )
            .await?;
        let fields = &issue["fields"];
        Ok(IssueFields {
            summary: fields["summary"].as_str().unwrap_or_default().to_string(),
            issue_type: fields["issuetype"]["name"].as_str().map(str::to_string),
            priority: fields["priority"]["name"].as_str().map(str::to_string),
            status_category: fields["status"]["statusCategory"]["key"]
                .as_str()
                .map(str::to_string),
            assignee_account_id: fields["assignee"]["accountId"].as_str().map(str::to_string),
            reporter_account_id: fields["reporter"]["accountId"].as_str().map(str::to_string),
            status_name: fields["status"]["name"].as_str().map(str::to_string),
            labels: labels_of(fields),
        })
    }

    /// Fetches every comment (paginated, oldest first) from
    /// `/rest/api/3/issue/{key}/comment`. The search endpoint returns only a
    /// truncated comment set, so comments are fetched separately.
    async fn comments(&self, key: &str) -> TrackerResult<Vec<TrackerComment>> {
        let url = self.api(&format!("issue/{key}/comment"));
        let auth = self.auth()?;
        let mut out = Vec::new();
        let mut start_at: u64 = 0;
        for _ in 0..MAX_COMMENT_PAGES {
            let response = self
                .http
                .get(&url)
                .query(&[
                    ("startAt", start_at.to_string()),
                    ("maxResults", "100".to_string()),
                    ("orderBy", "created".to_string()),
                ])
                .header(reqwest::header::AUTHORIZATION, &auth)
                .header(reqwest::header::ACCEPT, "application/json")
                .send()
                .await
                .context("failed to reach Jira")?;
            if !response.status().is_success() {
                return Err(rejected(response, "comments").await);
            }
            let page: Value = response
                .json()
                .await
                .context("failed to parse Jira comments response")?;
            let comments = page["comments"].as_array().cloned().unwrap_or_default();
            let page_len = comments.len() as u64;
            for comment in &comments {
                out.push(TrackerComment {
                    author: comment["author"]["displayName"]
                        .as_str()
                        .unwrap_or("Unknown")
                        .to_string(),
                    author_account_id: comment["author"]["accountId"].as_str().map(str::to_string),
                    mentions: collect_adf_mentions(&comment["body"]),
                    created: parse_timestamp(comment["created"].as_str()),
                    body_text: adf_to_text(&comment["body"]),
                });
            }
            let total = page["total"].as_u64().unwrap_or(0);
            start_at += page_len;
            if page_len == 0 || start_at >= total {
                break;
            }
        }
        Ok(out)
    }

    /// Posts a plain-text comment (one ADF paragraph per line).
    async fn post_comment(&self, key: &str, text: &str) -> TrackerResult<()> {
        self.send_json(
            self.http.post(self.api(&format!("issue/{key}/comment"))),
            &serde_json::json!({ "body": text_to_adf(text) }),
            "comment",
        )
        .await?;
        Ok(())
    }

    /// Used where the destination is the point — putting a ticket back where
    /// it was before it was parked. A category fallback would land it in a
    /// status that happens to share a category, and shipped statuses sit in
    /// categories with several members.
    async fn transition(&self, key: &str, status_name: &str) -> TrackerResult<()> {
        self.transition_to(key, status_name, None).await
    }

    async fn transition_to_in_progress(&self, key: &str) -> TrackerResult<()> {
        // "indeterminate" is Jira's status-category key for In Progress.
        let status = self.config.status_mapping.planning_status().to_string();
        self.transition_to(key, &status, Some("indeterminate"))
            .await
    }

    async fn assignee(&self, key: &str) -> TrackerResult<Option<TrackerUser>> {
        let issue = self
            .get_json(
                &self.api(&format!("issue/{key}?fields=assignee")),
                "assignee read",
            )
            .await?;
        let assignee = &issue["fields"]["assignee"];
        Ok(assignee["accountId"].as_str().map(|id| TrackerUser {
            account_id: id.to_string(),
            display_name: assignee["displayName"].as_str().unwrap_or("").to_string(),
        }))
    }

    /// There is no "assign to me" shortcut in the assignee API, so this
    /// resolves the agent's account id and PUTs it.
    async fn assign_to_self(&self, key: &str) -> TrackerResult<()> {
        let identity = self.identity().await?;
        self.send_json(
            self.http.put(self.api(&format!("issue/{key}/assignee"))),
            &serde_json::json!({ "accountId": identity.account_id }),
            "assignee update",
        )
        .await?;
        Ok(())
    }

    async fn set_summary(&self, key: &str, summary: &str) -> TrackerResult<()> {
        self.send_json(
            self.http.put(self.api(&format!("issue/{key}"))),
            &serde_json::json!({ "fields": { "summary": summary } }),
            "summary update",
        )
        .await?;
        Ok(())
    }

    /// Direction matters and is easy to get backwards: Jira's API names the
    /// *blocker* as `inwardIssue`.
    async fn link_blocks(&self, blocker: &str, blocked: &str) -> TrackerResult<()> {
        self.send_json(
            self.http.post(self.api("issueLink")),
            &serde_json::json!({
                "type": { "name": "Blocks" },
                "inwardIssue": { "key": blocker },
                "outwardIssue": { "key": blocked },
            }),
            "issue link",
        )
        .await?;
        Ok(())
    }

    /// Upserts a remote link; Jira dedupes on `globalId`, which is the link's
    /// id, so pinning again is idempotent.
    async fn set_remote_link(&self, key: &str, link: RemoteLink<'_>) -> TrackerResult<()> {
        self.send_json(
            self.http.post(self.api(&format!("issue/{key}/remotelink"))),
            &serde_json::json!({
                "globalId": link.id,
                "object": { "title": link.title, "url": link.url }
            }),
            "remote link",
        )
        .await?;
        Ok(())
    }

    /// The body is a bare JSON string — the account id on its own — which is
    /// what this endpoint expects. Requires the *Manage watchers* permission;
    /// without it Jira answers 403 and the caller is expected to carry on.
    async fn add_watcher(&self, key: &str, account_id: &str) -> TrackerResult<()> {
        self.send_json(
            self.http.post(self.api(&format!("issue/{key}/watchers"))),
            &serde_json::json!(account_id),
            "watcher add",
        )
        .await?;
        Ok(())
    }

    /// The description is plain text rendered as a minimal ADF document (one
    /// paragraph per line).
    async fn create_issue(&self, issue: &NewIssue) -> TrackerResult<String> {
        let project_key = issue
            .project_key
            .as_deref()
            .map(str::trim)
            .filter(|key| !key.is_empty())
            .unwrap_or_else(|| self.config.project_key());
        self.create_with_fields(serde_json::json!({
            "project": { "key": project_key },
            "issuetype": { "name": issue.issue_type },
            "summary": issue.summary,
            "priority": { "name": issue.priority },
            "labels": issue.labels,
            "description": text_to_adf(&issue.description),
        }))
        .await
    }

    /// The project is the parent key's prefix; the sub-task issue type is
    /// whatever the type mapping maps to `WorkClass::Subtask`. A credentials
    /// failure stops the batch; any other failure skips that one item.
    async fn create_subtasks(
        &self,
        parent: &str,
        items: &[NewSubtask],
    ) -> TrackerResult<Vec<CreatedSubtask>> {
        let project_key = parent
            .split_once('-')
            .map(|(prefix, _)| prefix.to_string())
            .unwrap_or_default();
        let subtask_type = self
            .config
            .type_mapping
            .iter()
            .find(|(_, work_class)| **work_class == api_types::WorkClass::Subtask)
            .map(|(name, _)| name.clone())
            .unwrap_or_else(|| "Sub-task".to_string());

        let mut out = Vec::with_capacity(items.len());
        for item in items {
            let mut fields = serde_json::json!({
                "project": { "key": project_key },
                "parent": { "key": parent },
                "issuetype": { "name": subtask_type },
                "summary": item.title,
            });
            if let Some(description) = item
                .description
                .as_deref()
                .filter(|text| !text.trim().is_empty())
            {
                fields["description"] = text_to_adf(description);
            }
            let key = match self.create_with_fields(fields).await {
                Ok(key) => Some(key),
                Err(TrackerError::AuthFailed) => return Err(TrackerError::AuthFailed),
                Err(error) => {
                    tracing::warn!("Jira sub-task create failed for '{}': {error}", item.title);
                    None
                }
            };
            out.push(CreatedSubtask {
                title: item.title.clone(),
                key,
            });
        }
        Ok(out)
    }

    /// Filenames are reduced to their basename (they are user-supplied and
    /// must not traverse out of `dir`); collisions get a numeric prefix.
    async fn download_attachments(
        &self,
        key: &str,
        dir: &Path,
    ) -> TrackerResult<Vec<TrackerAttachment>> {
        let auth = self.auth()?;
        let value = self
            .get_json(
                &self.api(&format!("issue/{key}?fields=attachment")),
                "issue fetch",
            )
            .await?;
        let attachments = value["fields"]["attachment"]
            .as_array()
            .cloned()
            .unwrap_or_default();

        tokio::fs::create_dir_all(dir)
            .await
            .context("failed to create attachment directory")?;

        let mut downloaded = Vec::new();
        for (index, attachment) in attachments.iter().enumerate() {
            let Some(content_url) = attachment["content"].as_str() else {
                continue;
            };
            let raw_name = attachment["filename"].as_str().unwrap_or("attachment");
            let base_name = Path::new(raw_name)
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| format!("attachment-{index}"));
            let mut target = dir.join(&base_name);
            if target.exists() {
                target = dir.join(format!("{index}-{base_name}"));
            }

            let file_response = self
                .http
                .get(content_url)
                .header(reqwest::header::AUTHORIZATION, &auth)
                .send()
                .await
                .context("failed to download attachment")?;
            if !file_response.status().is_success() {
                tracing::warn!(
                    "Jira attachment download failed (HTTP {}): {base_name}",
                    file_response.status().as_u16()
                );
                continue;
            }
            let bytes = file_response
                .bytes()
                .await
                .context("failed to read attachment body")?;
            tokio::fs::write(&target, &bytes)
                .await
                .context("failed to write attachment")?;

            downloaded.push(TrackerAttachment {
                filename: base_name,
                path: target.to_string_lossy().to_string(),
                size_bytes: bytes.len() as u64,
                mime_type: attachment["mimeType"].as_str().map(str::to_string),
            });
        }
        Ok(downloaded)
    }

    /// The API token's own account (`/myself`), with `bot_identity` standing
    /// in for its id when configured. Resolved once per tracker.
    async fn identity(&self) -> TrackerResult<TrackerIdentity> {
        self.identity
            .get_or_try_init(|| async {
                let myself = self.get_json(&self.api("myself"), "/myself").await?;
                let account_id = match self.config.bot_identity() {
                    Some(configured) => configured.to_string(),
                    None => myself["accountId"]
                        .as_str()
                        .map(str::to_string)
                        .ok_or_else(|| anyhow!("Jira /myself returned no accountId"))?,
                };
                Ok::<_, TrackerError>(TrackerIdentity {
                    display_name: myself["displayName"]
                        .as_str()
                        .unwrap_or(&account_id)
                        .to_string(),
                    account_id,
                })
            })
            .await
            .cloned()
    }

    /// Everyone who can be assigned an issue in `project`.
    ///
    /// Deliberately a broad pool that a person then narrows: it includes
    /// account managers and anyone else with a seat on the project, because
    /// Jira will not say who is an engineer.
    async fn assignable_users(&self, project: &str) -> TrackerResult<Vec<TrackerUser>> {
        const MAX_USER_PAGES: usize = 10;
        const USER_PAGE_SIZE: usize = 50;

        let url = self.api("user/assignable/search");
        let auth = self.auth()?;
        // The agent is assignable on its own project, so without this it
        // would appear in the picker and could be ticked by mistake. The
        // approval check excludes it again at use time, and a failure to
        // resolve the agent's identity must not empty the picker, so the
        // exclusion is simply skipped then.
        let agent = self
            .identity()
            .await
            .ok()
            .map(|identity| identity.account_id);

        let mut users: Vec<TrackerUser> = Vec::new();
        for page in 0..MAX_USER_PAGES {
            let response = self
                .http
                .get(&url)
                .query(&[
                    ("project", project.to_string()),
                    ("startAt", (page * USER_PAGE_SIZE).to_string()),
                    ("maxResults", USER_PAGE_SIZE.to_string()),
                ])
                .header(reqwest::header::AUTHORIZATION, &auth)
                .header(reqwest::header::ACCEPT, "application/json")
                .send()
                .await
                .context("failed to reach Jira")?;
            if !response.status().is_success() {
                return Err(rejected(response, "user search").await);
            }
            let page_users: Vec<Value> = response
                .json()
                .await
                .context("failed to parse Jira user search response")?;
            let count = page_users.len();
            for user in page_users {
                // `atlassian` is a person with a seat. The same endpoint
                // returns app and customer accounts too, and none of them is
                // someone who can approve anything.
                if user["accountType"].as_str() != Some("atlassian")
                    || user["active"].as_bool() == Some(false)
                {
                    continue;
                }
                let Some(account_id) = user["accountId"].as_str() else {
                    continue;
                };
                if agent.as_deref() == Some(account_id)
                    || users.iter().any(|known| known.account_id == account_id)
                {
                    continue;
                }
                users.push(TrackerUser {
                    account_id: account_id.to_string(),
                    display_name: user["displayName"]
                        .as_str()
                        .unwrap_or(account_id)
                        .to_string(),
                });
            }
            if count < USER_PAGE_SIZE {
                break;
            }
        }
        Ok(users)
    }
}

/// Extracts plain text from an Atlassian Document Format tree by collecting
/// every "text" value in document order. Block-level nodes are separated by
/// newlines so paragraphs stay readable.
fn adf_to_text(node: &Value) -> String {
    let mut out = String::new();
    collect_adf_text(node, &mut out);
    out.trim().to_string()
}

fn collect_adf_text(node: &Value, out: &mut String) {
    match node {
        Value::Object(map) => {
            if let Some(Value::String(text)) = map.get("text") {
                out.push_str(text);
            }
            // A mention is `{type:"mention", attrs:{id, text:"@Name"}}` with no
            // `text` field and no children: without this branch every
            // @-mention vanished from the flattened body, so nobody could see
            // who a comment was addressed to.
            if map.get("type").and_then(Value::as_str) == Some("mention")
                && let Some(attrs) = map.get("attrs")
            {
                match attrs.get("text").and_then(Value::as_str) {
                    Some(label) if !label.is_empty() => out.push_str(label),
                    // Older payloads omit the label; the display name is the
                    // best available stand-in.
                    _ => {
                        if let Some(name) = attrs.get("displayName").and_then(Value::as_str) {
                            out.push('@');
                            out.push_str(name);
                        }
                    }
                }
            }
            if let Some(content) = map.get("content") {
                collect_adf_text(content, out);
            }
            let is_block = matches!(
                map.get("type").and_then(Value::as_str),
                Some("paragraph" | "heading" | "listItem" | "codeBlock" | "blockquote")
            );
            if is_block && !out.is_empty() && !out.ends_with('\n') {
                out.push('\n');
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_adf_text(item, out);
            }
        }
        _ => {}
    }
}

/// accountIds of every `mention` node in an ADF tree, in document order.
fn collect_adf_mentions(node: &Value) -> Vec<String> {
    let mut out = Vec::new();
    walk_adf_mentions(node, &mut out);
    out
}

fn walk_adf_mentions(node: &Value, out: &mut Vec<String>) {
    match node {
        Value::Object(map) => {
            if map.get("type").and_then(Value::as_str) == Some("mention")
                && let Some(id) = map
                    .get("attrs")
                    .and_then(|attrs| attrs.get("id"))
                    .and_then(Value::as_str)
                && !out.iter().any(|seen| seen == id)
            {
                out.push(id.to_string());
            }
            if let Some(content) = map.get("content") {
                walk_adf_mentions(content, out);
            }
        }
        Value::Array(items) => {
            for item in items {
                walk_adf_mentions(item, out);
            }
        }
        _ => {}
    }
}

/// Jira rejects a description or comment whose text exceeds this with
/// HTTP 400 ("The entered text is too long. It exceeds the allowed limit of
/// 32767 characters").
const JIRA_TEXT_LIMIT: usize = 32_767;

/// Budget for the caller's text. The gap to [`JIRA_TEXT_LIMIT`] is headroom for
/// anything Jira counts that this code does not model — notably the ADF
/// paragraph markup wrapped around each line, which may or may not be included
/// in its tally.
const JIRA_TEXT_BUDGET: usize = 30_000;

// Ties the two together: the budget is meaningless if it ever exceeds the limit,
// and this also keeps JIRA_TEXT_LIMIT used outside `cfg(test)` (the workspace
// builds with `-D warnings`, so an unused const fails the lint gate).
const _: () = assert!(JIRA_TEXT_BUDGET < JIRA_TEXT_LIMIT);

/// Clips text to something Jira will accept, marking where it was cut.
///
/// Every string this module sends is assembled from agent output or
/// knowledge-base records, and neither has a natural upper bound: the
/// knowledge scan's investigation story grows one line per linked RCA and
/// silently began failing on every sweep once a node passed the limit.
/// Truncating loses detail; a rejected write loses the whole story.
fn clip_for_jira(text: &str) -> String {
    // Jira runs on the JVM and counts UTF-16 code units, so an emoji counts as
    // two where `chars()` counts one. Measuring the same way it does keeps the
    // budget honest for text that is mostly non-BMP; slicing still happens on
    // char boundaries, so a multi-byte character can never be split.
    let total: usize = text.chars().map(char::len_utf16).sum();
    if total <= JIRA_TEXT_BUDGET {
        return text.to_string();
    }
    let notice = format!(
        "\n\n[Truncated for Jira: {total} characters exceeded the field limit. \
         The full text is in the orchestrator's records.]"
    );
    let budget = JIRA_TEXT_BUDGET.saturating_sub(notice.chars().map(char::len_utf16).sum());
    let mut clipped = String::new();
    let mut used = 0usize;
    for ch in text.chars() {
        let width = ch.len_utf16();
        if used + width > budget {
            break;
        }
        clipped.push(ch);
        used += width;
    }
    clipped.push_str(&notice);
    clipped
}

/// Plain text -> ADF paragraph list (one paragraph per line, blank lines
/// preserved as empty paragraphs).
///
/// Clips to Jira's field limit here rather than at each call site: this
/// function exists only to build Jira payloads, so the destination's limit is
/// part of the conversion, and a future writer cannot forget it.
/// Splits one line into ADF inline nodes, turning `[~accountid:xxx]` into a
/// real mention.
///
/// ADF has no markup: a mention is a node type, so the wiki-markup form a
/// person would type posts as literal characters unless it is converted here.
/// That is the difference between a comment the reporter happens to read and
/// one that notifies them — which is the whole point of naming them.
fn inline_nodes(line: &str) -> Vec<Value> {
    const OPEN: &str = "[~accountid:";
    let mut nodes = Vec::new();
    let mut rest = line;
    while let Some(start) = rest.find(OPEN) {
        let after = &rest[start + OPEN.len()..];
        let Some(end) = after.find(']') else {
            break;
        };
        let id = &after[..end];
        // An empty or spaced id is not an account id; leave the text alone
        // rather than posting a mention node Jira will reject.
        if id.is_empty() || id.contains(char::is_whitespace) {
            break;
        }
        if start > 0 {
            nodes.push(serde_json::json!({ "type": "text", "text": &rest[..start] }));
        }
        nodes.push(serde_json::json!({
            "type": "mention",
            "attrs": { "id": id }
        }));
        rest = &after[end + 1..];
    }
    if !rest.is_empty() {
        nodes.push(serde_json::json!({ "type": "text", "text": rest }));
    }
    nodes
}

fn text_to_adf_content(text: &str) -> Vec<Value> {
    let text = clip_for_jira(text);
    text.lines()
        .map(|line| {
            if line.trim().is_empty() {
                serde_json::json!({ "type": "paragraph", "content": [] })
            } else {
                serde_json::json!({
                    "type": "paragraph",
                    "content": inline_nodes(line)
                })
            }
        })
        .collect()
}

#[cfg(test)]
mod adf_tests {
    use super::inline_nodes;

    #[test]
    fn an_account_id_becomes_a_mention_node() {
        let nodes = inline_nodes("[~accountid:5b10a2] this needs you.");
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0]["type"], "mention");
        assert_eq!(nodes[0]["attrs"]["id"], "5b10a2");
        assert_eq!(nodes[1]["text"], " this needs you.");
    }

    #[test]
    fn text_without_a_mention_is_one_node() {
        let nodes = inline_nodes("MR #9241 merged.");
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0]["type"], "text");
    }

    #[test]
    fn a_malformed_marker_stays_literal() {
        // Better a comment that reads oddly than one Jira rejects outright,
        // which would lose the update entirely.
        for line in [
            "[~accountid:] hello",
            "[~accountid:a b] hello",
            "[~accountid:oops",
        ] {
            let nodes = inline_nodes(line);
            assert_eq!(nodes.len(), 1, "{line}");
            assert_eq!(nodes[0]["type"], "text", "{line}");
        }
    }
}

/// Plain text -> a full ADF document.
fn text_to_adf(text: &str) -> Value {
    serde_json::json!({ "type": "doc", "version": 1, "content": text_to_adf_content(text) })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn comment_body() -> Value {
        serde_json::json!({
            "type": "doc",
            "content": [{
                "type": "paragraph",
                "content": [
                    {"type": "mention", "attrs": {"id": "agent-123", "text": "@Prep Cook"}},
                    {"type": "text", "text": " any update on this?"}
                ]
            }]
        })
    }

    /// The regression this guards: a mention carries its label in `attrs`, so
    /// a flattener that only collects `text` nodes drops it silently and
    /// nobody can see who a comment was addressed to.
    #[test]
    fn mentions_survive_the_flattener() {
        assert_eq!(
            adf_to_text(&comment_body()),
            "@Prep Cook any update on this?"
        );
    }

    #[test]
    fn mention_account_ids_are_collected_once() {
        let body = serde_json::json!({
            "type": "doc",
            "content": [
                {"type": "paragraph", "content": [
                    {"type": "mention", "attrs": {"id": "agent-123", "text": "@Prep Cook"}},
                    {"type": "mention", "attrs": {"id": "human-9", "text": "@Jack"}}
                ]},
                {"type": "paragraph", "content": [
                    {"type": "mention", "attrs": {"id": "agent-123", "text": "@Prep Cook"}}
                ]}
            ]
        });
        assert_eq!(collect_adf_mentions(&body), vec!["agent-123", "human-9"]);
    }

    /// Older payloads omit the rendered label; the display name stands in so
    /// the body still reads as a mention.
    #[test]
    fn mentions_without_a_label_fall_back_to_the_display_name() {
        let body = serde_json::json!({
            "type": "paragraph",
            "content": [{
                "type": "mention",
                "attrs": {"id": "agent-123", "displayName": "Prep Cook"}
            }]
        });
        assert_eq!(adf_to_text(&body), "@Prep Cook");
    }

    #[test]
    fn the_watcher_questions_become_the_jql_it_has_always_run() {
        let filter = "project = BUG AND priority in (P0, P1)";
        assert_eq!(
            jql_for(Query::Created {
                filter,
                within: chrono::Duration::hours(24)
            })
            .unwrap(),
            "(project = BUG AND priority in (P0, P1)) AND created >= -24h ORDER BY created ASC"
        );
        assert_eq!(
            jql_for(Query::Updated {
                filter: "assignee = currentUser()",
                within: chrono::Duration::hours(24)
            })
            .unwrap(),
            "(assignee = currentUser()) AND updated >= -24h ORDER BY updated ASC"
        );
        assert_eq!(
            jql_for(Query::Reopened {
                project: "BUG",
                within: chrono::Duration::days(3)
            })
            .unwrap(),
            "project = BUG AND statusCategory = \"To Do\" AND status CHANGED AFTER -3d ORDER BY \
             updated ASC"
        );
        assert_eq!(
            jql_for(Query::Active {
                project: "BUG",
                within: chrono::Duration::days(3)
            })
            .unwrap(),
            "project = BUG AND updated >= -3d ORDER BY updated ASC"
        );
        let shipped = vec!["Merged".to_string(), "Done".to_string()];
        assert_eq!(
            jql_for(Query::InStatuses {
                project: "BUG",
                statuses: &shipped,
                within: chrono::Duration::days(30)
            })
            .unwrap(),
            "project = \"BUG\" AND status IN (\"Merged\", \"Done\") AND updated >= -30d"
        );
        assert!(
            jql_for(Query::InStatuses {
                project: "BUG",
                statuses: &[],
                within: chrono::Duration::days(30)
            })
            .is_none()
        );
    }

    #[test]
    fn text_within_the_limit_is_untouched() {
        let text = "Linked RCAs:\n- BUG-1: a summary\n\nInvestigate.";
        assert_eq!(clip_for_jira(text), text);
    }

    /// Jira measures in UTF-16 code units, so that is what the budget has to
    /// hold — the same yardstick the server rejects on.
    fn utf16_len(text: &str) -> usize {
        text.chars().map(char::len_utf16).sum()
    }

    #[test]
    fn oversized_text_is_clipped_under_the_limit_and_says_so() {
        // The shape that failed on every knowledge scan: a description grown
        // past Jira's field limit one linked RCA at a time.
        let text = "- BUG-1234: recurring template resolution failure\n".repeat(2_000);
        assert!(utf16_len(&text) > JIRA_TEXT_LIMIT);

        let clipped = clip_for_jira(&text);
        assert!(utf16_len(&clipped) <= JIRA_TEXT_BUDGET);
        assert!(clipped.contains("Truncated for Jira"));
        assert!(clipped.starts_with("- BUG-1234"));
    }

    #[test]
    fn clipping_never_splits_a_multibyte_character() {
        // 3 bytes each in UTF-8, 1 UTF-16 unit each: a byte-based clip would
        // produce invalid UTF-8 or panic on a slice boundary.
        let text = "壊".repeat(JIRA_TEXT_BUDGET + 500);
        let clipped = clip_for_jira(&text);
        assert!(utf16_len(&clipped) <= JIRA_TEXT_BUDGET);
        assert!(clipped.starts_with('壊'));
    }

    #[test]
    fn non_bmp_characters_count_double_as_jira_counts_them() {
        // Emoji are surrogate pairs: 2 UTF-16 units each. Counting `chars()`
        // would let ~2x the real budget through and still earn an HTTP 400.
        let text = "🚀".repeat(JIRA_TEXT_BUDGET);
        assert_eq!(utf16_len(&text), JIRA_TEXT_BUDGET * 2);

        let clipped = clip_for_jira(&text);
        assert!(utf16_len(&clipped) <= JIRA_TEXT_BUDGET);
        assert!(clipped.starts_with('🚀'));
    }
}

#[cfg(test)]
mod fixture_tests {
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{body_json, method, path, query_param},
    };

    use super::*;

    fn tracker(server: &MockServer) -> JiraTracker {
        JiraTracker::with_base(
            TrackerConfig {
                site: Some("acme.atlassian.net".to_string()),
                email: Some("agent@acme.test".to_string()),
                api_token: Some("token".to_string()),
                ..Default::default()
            },
            server.uri(),
        )
    }

    fn myself() -> serde_json::Value {
        serde_json::json!({
            "accountId": "agent-1",
            "displayName": "Prep Cook",
            "emailAddress": "agent@acme.test"
        })
    }

    #[tokio::test]
    async fn test_reads_myself() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/rest/api/3/myself"))
            .respond_with(ResponseTemplate::new(200).set_body_json(myself()))
            .expect(1)
            .mount(&server)
            .await;
        let message = tracker(&server).test().await.unwrap();
        assert_eq!(message, "Signed in to acme.atlassian.net as Prep Cook.");
    }

    #[tokio::test]
    async fn a_rejected_token_is_an_auth_failure() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/rest/api/3/myself"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;
        assert!(matches!(
            tracker(&server).test().await,
            Err(TrackerError::AuthFailed)
        ));
    }

    #[tokio::test]
    async fn planning_moves_the_ticket_and_assigns_the_agent() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/rest/api/3/issue/BUG-7/transitions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "transitions": [
                    {"id": "11", "to": {"name": "To Do", "statusCategory": {"key": "new"}}},
                    {"id": "21", "to": {"name": "In Progress",
                                        "statusCategory": {"key": "indeterminate"}}}
                ]
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/rest/api/3/issue/BUG-7/transitions"))
            .and(body_json(serde_json::json!({"transition": {"id": "21"}})))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/rest/api/3/myself"))
            .respond_with(ResponseTemplate::new(200).set_body_json(myself()))
            .mount(&server)
            .await;
        Mock::given(method("PUT"))
            .and(path("/rest/api/3/issue/BUG-7/assignee"))
            .and(body_json(serde_json::json!({"accountId": "agent-1"})))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;
        let jira = tracker(&server);
        jira.enter_stage("BUG-7", TicketStage::Planning)
            .await
            .unwrap();
        // Unmapped stages leave the ticket alone (no further mocks match).
        jira.enter_stage("BUG-7", TicketStage::Triaged)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn fetch_searches_valid_keys_only() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/rest/api/3/search/jql"))
            .and(query_param("jql", "key in (BUG-1,BUG-2)"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "issues": [{
                    "key": "BUG-1",
                    "fields": {
                        "summary": "Export drops a row",
                        "issuetype": {"name": "Bug"},
                        "status": {"name": "Done", "statusCategory": {"key": "done"}},
                        "priority": {"name": "P1"},
                        "labels": ["customer"]
                    }
                }]
            })))
            .expect(1)
            .mount(&server)
            .await;
        let fetched = tracker(&server)
            .fetch(&[
                "BUG-1".to_string(),
                "not a key".to_string(),
                "BUG-2".to_string(),
            ])
            .await
            .unwrap();
        assert_eq!(fetched.len(), 1);
        assert_eq!(fetched[0].key, "BUG-1");
        assert_eq!(fetched[0].status_category.as_deref(), Some("done"));
        assert_eq!(fetched[0].priority_name.as_deref(), Some("P1"));
    }
}
