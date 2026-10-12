//! HTTP client for the board, with automatic retries. Every authenticated
//! request carries a freshly minted owner token signed with the board secret
//! (see `board_secret`), so there is no sign-in, refresh or stored credential.

use std::time::Duration;

use api_types::{
    AddKbDocumentTagsRequest, CreateIssueAssigneeRequest, CreateIssueCommentRequest,
    CreateIssueRelationshipRequest, CreateIssueRequest, CreateIssueTagRequest,
    CreateKbDocumentRequest, CreateKbEdgeRequest, CreateKbNodeRequest, CreateTagRequest,
    CreateWorkspaceRequest, DeleteResponse, DeleteWorkspaceRequest, Issue, IssueAssignee,
    IssueComment, IssueRelationship, IssueTag, KbDocument, KbEdge, KbNode, ListAttachmentsResponse,
    ListIssueAssigneesResponse, ListIssueCommentsResponse, ListIssueRelationshipsResponse,
    ListIssueTagsResponse, ListIssuesResponse, ListKbDocumentsResponse, ListKbEdgesResponse,
    ListKbNodesResponse, ListMembersResponse, ListOrganizationsResponse,
    ListProjectStatusesResponse, ListProjectsResponse, ListPullRequestsResponse, ListTagsResponse,
    MutationResponse, ProfileResponse, PullRequest, SearchIssuesRequest,
    StampKbNodeInvestigationRequest, Tag, UpdateIssueRequest, UpdatePullRequestApiRequest,
    UpdateWorkspaceRequest, UpsertPullRequestRequest, Workspace,
};
use backon::{ExponentialBuilder, Retryable};
use chrono::Utc;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::warn;
use url::Url;
use utils::jwt::{OWNER_TOKEN_TTL_SECONDS, mint_owner_token};
use uuid::Uuid;

use super::board_secret::BoardSecret;

#[derive(Debug, Clone, Error)]
pub enum RemoteClientError {
    #[error("network error: {0}")]
    Transport(String),
    #[error("timeout")]
    Timeout,
    #[error("http {status}: {body}")]
    Http { status: u16, body: String },
    /// The board refused the request's owner token (401/403): this server's
    /// board secret is not the board's.
    #[error("unauthorized")]
    Auth,
    /// No board secret, so no request to the board can be authenticated.
    #[error("no board secret")]
    NoSecret,
    #[error("json error: {0}")]
    Serde(String),
    #[error("url error: {0}")]
    Url(String),
    #[error("could not sign the owner token: {0}")]
    Token(String),
}

impl RemoteClientError {
    pub fn generic_degraded_slug() -> &'static str {
        "remote_auth_unavailable"
    }

    /// Returns true if the error is transient and should be retried.
    fn should_retry(&self) -> bool {
        match self {
            Self::Transport(_) | Self::Timeout => true,
            Self::Http { status, .. } => (500..=599).contains(status),
            _ => false,
        }
    }

    /// Whether the board could not be called at all (`NoSecret`) or refused
    /// this server's secret (`Auth`), as opposed to a failure of one request.
    pub fn is_unauthenticated(&self) -> bool {
        matches!(self, Self::Auth | Self::NoSecret)
    }
}

/// HTTP client for the board with automatic retries.
#[derive(Debug, Clone)]
pub struct RemoteClient {
    base: Url,
    http: Client,
    secret: Option<BoardSecret>,
    /// The board's owner, once the board has answered `/v1/profile`. The
    /// owner of a board never changes, so it is fetched once.
    owner: std::sync::Arc<RwLock<Option<ProfileResponse>>>,
}

impl RemoteClient {
    const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

    pub fn new(base_url: &str, secret: Option<BoardSecret>) -> Result<Self, RemoteClientError> {
        let base = Url::parse(base_url).map_err(|e| RemoteClientError::Url(e.to_string()))?;
        let builder = Client::builder()
            .timeout(Self::REQUEST_TIMEOUT)
            .user_agent(concat!("remote-client/", env!("CARGO_PKG_VERSION")));
        #[cfg(debug_assertions)]
        let builder = builder.danger_accept_invalid_certs(true);

        let http = builder
            .build()
            .map_err(|e| RemoteClientError::Transport(e.to_string()))?;
        Ok(Self {
            base,
            http,
            secret,
            owner: Default::default(),
        })
    }

    /// Public base URL of the remote (also serves the board web UI), for
    /// building human-facing links in notifications.
    pub fn base_url(&self) -> String {
        self.base.to_string().trim_end_matches('/').to_string()
    }

    /// Whether this client holds a board secret and can authenticate.
    pub fn has_secret(&self) -> bool {
        self.secret.is_some()
    }

    /// A fresh owner token for the board, valid for
    /// [`OWNER_TOKEN_TTL_SECONDS`]. Minting is a local HMAC, so every request
    /// gets its own.
    pub fn access_token(&self) -> Result<String, RemoteClientError> {
        let secret = self.secret.as_ref().ok_or(RemoteClientError::NoSecret)?;
        mint_owner_token(secret.expose(), Utc::now(), OWNER_TOKEN_TTL_SECONDS)
            .map_err(|e| RemoteClientError::Token(e.to_string()))
    }

    async fn send<B>(
        &self,
        method: reqwest::Method,
        path: &str,
        requires_auth: bool,
        body: Option<&B>,
    ) -> Result<reqwest::Response, RemoteClientError>
    where
        B: Serialize,
    {
        self.send_internal(method, path, requires_auth, body).await
    }

    async fn send_internal<B>(
        &self,
        method: reqwest::Method,
        path: &str,
        requires_auth: bool,
        body: Option<&B>,
    ) -> Result<reqwest::Response, RemoteClientError>
    where
        B: Serialize,
    {
        self.send_internal_with_request(method, path, requires_auth, |req| {
            if let Some(body) = body {
                req.json(body)
            } else {
                req
            }
        })
        .await
    }

    async fn send_internal_with_request<F>(
        &self,
        method: reqwest::Method,
        path: &str,
        requires_auth: bool,
        customize_request: F,
    ) -> Result<reqwest::Response, RemoteClientError>
    where
        F: Fn(reqwest::RequestBuilder) -> reqwest::RequestBuilder,
    {
        let url = self
            .base
            .join(path)
            .map_err(|e| RemoteClientError::Url(e.to_string()))?;

        let operation = || async {
            let mut req = self
                .http
                .request(method.clone(), url.clone())
                .header("X-Client-Version", env!("CARGO_PKG_VERSION"))
                .header("X-Client-Type", "local-backend");

            if requires_auth {
                req = req.bearer_auth(self.access_token()?);
            }

            req = customize_request(req);

            let res = req.send().await.map_err(map_reqwest_error)?;

            match res.status() {
                s if s.is_success() => Ok(res),
                StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Err(RemoteClientError::Auth),
                s => {
                    let status = s.as_u16();
                    let body = res.text().await.unwrap_or_default();
                    Err(RemoteClientError::Http { status, body })
                }
            }
        };

        operation
            .retry(
                &ExponentialBuilder::default()
                    .with_min_delay(Duration::from_millis(500))
                    .with_max_delay(Duration::from_secs(2))
                    .with_max_times(2)
                    .with_jitter(),
            )
            .when(RemoteClientError::should_retry)
            .notify(|e, dur| {
                warn!(
                    "Remote call failed, retrying after {:.2}s: {}",
                    dur.as_secs_f64(),
                    e
                )
            })
            .await
    }

    // Authenticated endpoint helpers (require token)
    async fn get_authed<T>(&self, path: &str) -> Result<T, RemoteClientError>
    where
        T: for<'de> Deserialize<'de>,
    {
        let res = self
            .send(reqwest::Method::GET, path, true, None::<&()>)
            .await?;
        res.json::<T>()
            .await
            .map_err(|e| RemoteClientError::Serde(e.to_string()))
    }

    pub async fn post_authed<T, B>(
        &self,
        path: &str,
        body: Option<&B>,
    ) -> Result<T, RemoteClientError>
    where
        T: for<'de> Deserialize<'de>,
        B: Serialize,
    {
        let res = self.send(reqwest::Method::POST, path, true, body).await?;
        res.json::<T>()
            .await
            .map_err(|e| RemoteClientError::Serde(e.to_string()))
    }

    async fn patch_authed<T, B>(&self, path: &str, body: &B) -> Result<T, RemoteClientError>
    where
        T: for<'de> Deserialize<'de>,
        B: Serialize,
    {
        let res = self
            .send(reqwest::Method::PATCH, path, true, Some(body))
            .await?;
        res.json::<T>()
            .await
            .map_err(|e| RemoteClientError::Serde(e.to_string()))
    }

    async fn delete_authed(&self, path: &str) -> Result<(), RemoteClientError> {
        self.send(reqwest::Method::DELETE, path, true, None::<&()>)
            .await?;
        Ok(())
    }

    async fn delete_authed_with_body<B>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<(), RemoteClientError>
    where
        B: Serialize,
    {
        self.send(reqwest::Method::DELETE, path, true, Some(body))
            .await?;
        Ok(())
    }

    /// The board's owner (the user this server acts as), fetched once and
    /// then kept. Answering also proves the board accepts this server's
    /// secret.
    pub async fn profile(&self) -> Result<ProfileResponse, RemoteClientError> {
        if let Some(profile) = self.owner.read().await.clone() {
            return Ok(profile);
        }
        let profile: ProfileResponse = self.get_authed("/v1/profile").await?;
        *self.owner.write().await = Some(profile.clone());
        Ok(profile)
    }

    /// Lists organizations for the authenticated user.
    pub async fn list_organizations(&self) -> Result<ListOrganizationsResponse, RemoteClientError> {
        self.get_authed("/v1/organizations").await
    }

    /// The server's organization: every signed-in user is a member of it, so
    /// the listing has exactly one entry.
    pub async fn default_organization_id(&self) -> Result<Uuid, RemoteClientError> {
        self.list_organizations()
            .await?
            .organizations
            .into_iter()
            .next()
            .map(|organization| organization.id)
            .ok_or_else(|| RemoteClientError::Http {
                status: 404,
                body: "no organization for this user".to_string(),
            })
    }

    /// Lists members of an organization.
    pub async fn list_members(
        &self,
        org_id: Uuid,
    ) -> Result<ListMembersResponse, RemoteClientError> {
        self.get_authed(&format!("/v1/organizations/{org_id}/members"))
            .await
    }

    /// Deletes a workspace on the remote server by its local workspace ID.
    pub async fn delete_workspace(
        &self,
        local_workspace_id: Uuid,
    ) -> Result<(), RemoteClientError> {
        self.delete_authed_with_body(
            "/v1/workspaces",
            &DeleteWorkspaceRequest { local_workspace_id },
        )
        .await
    }

    /// Gets a workspace from the remote server by its local workspace ID.
    /// Lists remote workspaces linked to an issue (newest first).
    pub async fn list_workspaces_by_issue(
        &self,
        issue_id: Uuid,
    ) -> Result<Vec<Workspace>, RemoteClientError> {
        self.get_authed(&format!("/v1/workspaces/by-issue/{issue_id}"))
            .await
    }

    pub async fn get_workspace_by_local_id(
        &self,
        local_workspace_id: Uuid,
    ) -> Result<Workspace, RemoteClientError> {
        self.get_authed(&format!("/v1/workspaces/by-local-id/{local_workspace_id}"))
            .await
    }

    /// Checks if a workspace exists on the remote server.
    pub async fn workspace_exists(
        &self,
        local_workspace_id: Uuid,
    ) -> Result<bool, RemoteClientError> {
        match self
            .send(
                reqwest::Method::HEAD,
                &format!("/v1/workspaces/exists/{local_workspace_id}"),
                true,
                None::<&()>,
            )
            .await
        {
            Ok(_) => Ok(true),
            Err(RemoteClientError::Http { status: 404, .. }) => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Updates a workspace on the remote server.
    pub async fn update_workspace(
        &self,
        local_workspace_id: Uuid,
        name: Option<Option<String>>,
        archived: Option<bool>,
        files_changed: Option<i32>,
        lines_added: Option<i32>,
        lines_removed: Option<i32>,
    ) -> Result<(), RemoteClientError> {
        self.send(
            reqwest::Method::PATCH,
            "/v1/workspaces",
            true,
            Some(&UpdateWorkspaceRequest {
                local_workspace_id,
                name,
                archived,
                files_changed: files_changed.map(Some),
                lines_added: lines_added.map(Some),
                lines_removed: lines_removed.map(Some),
            }),
        )
        .await?;
        Ok(())
    }

    /// Triggers issue-status sync for a workspace that was merged locally without a PR.
    pub async fn sync_issue_status_from_local_workspace_merge(
        &self,
        local_workspace_id: Uuid,
    ) -> Result<(), RemoteClientError> {
        self.send(
            reqwest::Method::POST,
            &format!("/v1/workspaces/{local_workspace_id}/sync_issue_status_from_local_merge"),
            true,
            None::<&()>,
        )
        .await?;
        Ok(())
    }

    /// Creates a workspace on the remote server, linking it to a local workspace and an issue.
    pub async fn create_workspace(
        &self,
        request: CreateWorkspaceRequest,
    ) -> Result<(), RemoteClientError> {
        self.send(
            reqwest::Method::POST,
            "/v1/workspaces",
            true,
            Some(&request),
        )
        .await?;
        Ok(())
    }

    // ── Issues ──────────────────────────────────────────────────────────

    /// Lists issues for a project.
    pub async fn list_issues(
        &self,
        project_id: Uuid,
    ) -> Result<ListIssuesResponse, RemoteClientError> {
        self.get_authed(&format!("/v1/issues?project_id={project_id}"))
            .await
    }

    /// Searches issues for a project using the canonical JSON request shape.
    pub async fn search_issues(
        &self,
        request: &SearchIssuesRequest,
    ) -> Result<ListIssuesResponse, RemoteClientError> {
        self.post_authed("/v1/issues/search", Some(request)).await
    }

    /// Gets a single issue by ID.
    pub async fn get_issue(&self, issue_id: Uuid) -> Result<Issue, RemoteClientError> {
        self.get_authed(&format!("/v1/issues/{issue_id}")).await
    }

    /// Creates a new issue.
    pub async fn create_issue(
        &self,
        request: &CreateIssueRequest,
    ) -> Result<MutationResponse<Issue>, RemoteClientError> {
        self.post_authed("/v1/issues", Some(request)).await
    }

    /// Updates an existing issue.
    pub async fn update_issue(
        &self,
        issue_id: Uuid,
        request: &UpdateIssueRequest,
    ) -> Result<MutationResponse<Issue>, RemoteClientError> {
        self.patch_authed(&format!("/v1/issues/{issue_id}"), request)
            .await
    }

    /// Deletes an issue.
    pub async fn delete_issue(&self, issue_id: Uuid) -> Result<DeleteResponse, RemoteClientError> {
        let res = self
            .send(
                reqwest::Method::DELETE,
                &format!("/v1/issues/{issue_id}"),
                true,
                None::<&()>,
            )
            .await?;
        res.json::<DeleteResponse>()
            .await
            .map_err(|e| RemoteClientError::Serde(e.to_string()))
    }

    // ── Issue Comments ─────────────────────────────────────────────────

    /// Lists comments for an issue.
    pub async fn list_issue_comments(
        &self,
        issue_id: Uuid,
    ) -> Result<ListIssueCommentsResponse, RemoteClientError> {
        self.get_authed(&format!("/v1/issue_comments?issue_id={issue_id}"))
            .await
    }

    /// Creates a comment on an issue.
    pub async fn create_issue_comment(
        &self,
        request: &CreateIssueCommentRequest,
    ) -> Result<MutationResponse<IssueComment>, RemoteClientError> {
        self.post_authed("/v1/issue_comments", Some(request)).await
    }

    // ── Knowledge documents ────────────────────────────────────────────

    /// Lists knowledge documents for a project (no bodies), optionally
    /// filtered by kind (e.g. "rca").
    pub async fn list_kb_documents(
        &self,
        project_id: Uuid,
        kind: Option<&str>,
        unlinked_only: bool,
    ) -> Result<ListKbDocumentsResponse, RemoteClientError> {
        let mut url = format!("/v1/kb_documents?project_id={project_id}");
        if let Some(kind) = kind {
            url.push_str(&format!("&kind={kind}"));
        }
        if unlinked_only {
            url.push_str("&unlinked=true");
        }
        self.get_authed(&url).await
    }

    /// Fetches one knowledge document with its full body.
    pub async fn get_kb_document(&self, id: Uuid) -> Result<KbDocument, RemoteClientError> {
        self.get_authed(&format!("/v1/kb_documents/{id}")).await
    }

    /// Files (or refreshes, keyed by issue) a knowledge document.
    pub async fn create_kb_document(
        &self,
        request: &CreateKbDocumentRequest,
    ) -> Result<KbDocument, RemoteClientError> {
        self.post_authed("/v1/kb_documents", Some(request)).await
    }

    /// Unions classification tags into a knowledge document (idempotent).
    pub async fn add_kb_document_tags(
        &self,
        id: Uuid,
        request: &AddKbDocumentTagsRequest,
    ) -> Result<KbDocument, RemoteClientError> {
        self.post_authed(&format!("/v1/kb_documents/{id}/tags"), Some(request))
            .await
    }

    /// Lists RCA-graph nodes with live edge counts, largest threads first.
    pub async fn list_kb_nodes(
        &self,
        project_id: Uuid,
    ) -> Result<ListKbNodesResponse, RemoteClientError> {
        self.get_authed(&format!("/v1/kb_nodes?project_id={project_id}"))
            .await
    }

    /// Creates (or returns, keyed by kind+name) a graph node.
    pub async fn create_kb_node(
        &self,
        request: &CreateKbNodeRequest,
    ) -> Result<KbNode, RemoteClientError> {
        self.post_authed("/v1/kb_nodes", Some(request)).await
    }

    /// Lists graph edges, filterable by node or document.
    pub async fn list_kb_edges(
        &self,
        project_id: Uuid,
        node_id: Option<Uuid>,
        document_id: Option<Uuid>,
    ) -> Result<ListKbEdgesResponse, RemoteClientError> {
        let mut url = format!("/v1/kb_edges?project_id={project_id}");
        if let Some(node_id) = node_id {
            url.push_str(&format!("&node_id={node_id}"));
        }
        if let Some(document_id) = document_id {
            url.push_str(&format!("&document_id={document_id}"));
        }
        self.get_authed(&url).await
    }

    /// Stamps a node with its investigation story (threshold fired).
    pub async fn stamp_kb_node_investigation(
        &self,
        node_id: Uuid,
        request: &StampKbNodeInvestigationRequest,
    ) -> Result<KbNode, RemoteClientError> {
        self.post_authed(
            &format!("/v1/kb_nodes/{node_id}/investigation"),
            Some(request),
        )
        .await
    }

    /// Links a node to a document (idempotent per pair).
    pub async fn create_kb_edge(
        &self,
        request: &CreateKbEdgeRequest,
    ) -> Result<KbEdge, RemoteClientError> {
        self.post_authed("/v1/kb_edges", Some(request)).await
    }

    // ── Issue Assignees ────────────────────────────────────────────────

    /// Lists assignees for an issue.
    pub async fn list_issue_assignees(
        &self,
        issue_id: Uuid,
    ) -> Result<ListIssueAssigneesResponse, RemoteClientError> {
        self.get_authed(&format!("/v1/issue_assignees?issue_id={issue_id}"))
            .await
    }

    /// Gets a single issue assignee by ID.
    pub async fn get_issue_assignee(
        &self,
        issue_assignee_id: Uuid,
    ) -> Result<IssueAssignee, RemoteClientError> {
        self.get_authed(&format!("/v1/issue_assignees/{issue_assignee_id}"))
            .await
    }

    /// Creates a new issue assignee.
    pub async fn create_issue_assignee(
        &self,
        request: &CreateIssueAssigneeRequest,
    ) -> Result<MutationResponse<IssueAssignee>, RemoteClientError> {
        self.post_authed("/v1/issue_assignees", Some(request)).await
    }

    /// Deletes an issue assignee.
    pub async fn delete_issue_assignee(
        &self,
        issue_assignee_id: Uuid,
    ) -> Result<DeleteResponse, RemoteClientError> {
        let res = self
            .send(
                reqwest::Method::DELETE,
                &format!("/v1/issue_assignees/{issue_assignee_id}"),
                true,
                None::<&()>,
            )
            .await?;
        res.json::<DeleteResponse>()
            .await
            .map_err(|e| RemoteClientError::Serde(e.to_string()))
    }

    // ── Tags ───────────────────────────────────────────────────────────

    /// Lists tags for a project.
    pub async fn list_tags(&self, project_id: Uuid) -> Result<ListTagsResponse, RemoteClientError> {
        self.get_authed(&format!("/v1/tags?project_id={project_id}"))
            .await
    }

    /// Creates a project tag.
    pub async fn create_tag(
        &self,
        request: &CreateTagRequest,
    ) -> Result<MutationResponse<Tag>, RemoteClientError> {
        self.post_authed("/v1/tags", Some(request)).await
    }

    /// Gets a single tag by ID.
    pub async fn get_tag(&self, tag_id: Uuid) -> Result<Tag, RemoteClientError> {
        self.get_authed(&format!("/v1/tags/{tag_id}")).await
    }

    // ── Issue Tags ─────────────────────────────────────────────────────

    /// Lists tags attached to an issue.
    pub async fn list_issue_tags(
        &self,
        issue_id: Uuid,
    ) -> Result<ListIssueTagsResponse, RemoteClientError> {
        self.get_authed(&format!("/v1/issue_tags?issue_id={issue_id}"))
            .await
    }

    /// Gets a single issue-tag relation by ID.
    pub async fn get_issue_tag(&self, issue_tag_id: Uuid) -> Result<IssueTag, RemoteClientError> {
        self.get_authed(&format!("/v1/issue_tags/{issue_tag_id}"))
            .await
    }

    /// Attaches a tag to an issue.
    pub async fn create_issue_tag(
        &self,
        request: &CreateIssueTagRequest,
    ) -> Result<MutationResponse<IssueTag>, RemoteClientError> {
        self.post_authed("/v1/issue_tags", Some(request)).await
    }

    /// Removes a tag from an issue.
    pub async fn delete_issue_tag(
        &self,
        issue_tag_id: Uuid,
    ) -> Result<DeleteResponse, RemoteClientError> {
        let res = self
            .send(
                reqwest::Method::DELETE,
                &format!("/v1/issue_tags/{issue_tag_id}"),
                true,
                None::<&()>,
            )
            .await?;
        res.json::<DeleteResponse>()
            .await
            .map_err(|e| RemoteClientError::Serde(e.to_string()))
    }

    // ── Issue Relationships ────────────────────────────────────────────

    /// Lists relationships for an issue.
    pub async fn list_issue_relationships(
        &self,
        issue_id: Uuid,
    ) -> Result<ListIssueRelationshipsResponse, RemoteClientError> {
        self.get_authed(&format!("/v1/issue_relationships?issue_id={issue_id}"))
            .await
    }

    /// Creates a new issue relationship.
    pub async fn create_issue_relationship(
        &self,
        request: &CreateIssueRelationshipRequest,
    ) -> Result<MutationResponse<IssueRelationship>, RemoteClientError> {
        self.post_authed("/v1/issue_relationships", Some(request))
            .await
    }

    /// Deletes an issue relationship.
    pub async fn delete_issue_relationship(
        &self,
        relationship_id: Uuid,
    ) -> Result<(), RemoteClientError> {
        self.delete_authed(&format!("/v1/issue_relationships/{relationship_id}"))
            .await
    }

    // ── Remote Projects ─────────────────────────────────────────────────

    /// Gets a single remote project by ID.
    pub async fn get_remote_project(
        &self,
        project_id: Uuid,
    ) -> Result<api_types::Project, RemoteClientError> {
        self.get_authed(&format!("/v1/projects/{project_id}")).await
    }

    /// Lists projects for an organization.
    pub async fn list_remote_projects(
        &self,
        organization_id: Uuid,
    ) -> Result<ListProjectsResponse, RemoteClientError> {
        self.get_authed(&format!("/v1/projects?organization_id={organization_id}"))
            .await
    }

    // ── Project Statuses ────────────────────────────────────────────────

    /// Lists project statuses for a project (used for status name ↔ UUID mapping).
    pub async fn list_project_statuses(
        &self,
        project_id: Uuid,
    ) -> Result<ListProjectStatusesResponse, RemoteClientError> {
        self.get_authed(&format!("/v1/project_statuses?project_id={project_id}"))
            .await
    }

    // ── Pull Requests ───────────────────────────────────────────────────

    /// Upserts a pull request on the remote server.
    /// Creates if not exists, updates if exists.
    pub async fn upsert_pull_request(
        &self,
        request: UpsertPullRequestRequest,
    ) -> Result<(), RemoteClientError> {
        self.send(
            reqwest::Method::PUT,
            "/v1/pull_requests",
            true,
            Some(&request),
        )
        .await?;
        Ok(())
    }

    /// Updates a pull request status on the remote server.
    pub async fn update_pull_request(
        &self,
        request: UpdatePullRequestApiRequest,
    ) -> Result<PullRequest, RemoteClientError> {
        let response: MutationResponse<PullRequest> =
            self.patch_authed("/v1/pull_requests", &request).await?;
        Ok(response.data)
    }

    /// Lists pull requests linked to an issue.
    pub async fn list_pull_requests(
        &self,
        issue_id: Uuid,
    ) -> Result<ListPullRequestsResponse, RemoteClientError> {
        self.get_authed(&format!("/v1/pull_requests?issue_id={issue_id}"))
            .await
    }

    /// Lists attachments for an issue on the remote server.
    pub async fn list_issue_attachments(
        &self,
        issue_id: Uuid,
    ) -> Result<ListAttachmentsResponse, RemoteClientError> {
        self.get_authed(&format!("/v1/issues/{issue_id}/attachments"))
            .await
    }

    /// Used for fetching from presigned Azure SAS URLs.
    pub async fn download_from_url(&self, url: &str) -> Result<Vec<u8>, RemoteClientError> {
        let res = self.http.get(url).send().await.map_err(map_reqwest_error)?;
        if !res.status().is_success() {
            return Err(RemoteClientError::Http {
                status: res.status().as_u16(),
                body: res.text().await.unwrap_or_default(),
            });
        }
        let bytes = res
            .bytes()
            .await
            .map_err(|e| RemoteClientError::Transport(e.to_string()))?;
        Ok(bytes.to_vec())
    }
}

fn map_reqwest_error(e: reqwest::Error) -> RemoteClientError {
    if e.is_timeout() {
        RemoteClientError::Timeout
    } else {
        RemoteClientError::Transport(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };
    use utils::jwt::verify_owner_token;

    use super::*;

    // Standard base64 of 32 bytes, the shape `prepcook up` generates.
    const SECRET: &str = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=";

    /// Answers one request with `body` and returns the request's
    /// `Authorization` header.
    async fn serve_once(body: String) -> (String, tokio::task::JoinHandle<Option<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let handle = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut buffer = [0u8; 4096];
            while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                let read = stream.read(&mut buffer).await.unwrap();
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..read]);
            }
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
            String::from_utf8_lossy(&request).lines().find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("authorization")
                    .then(|| value.trim().to_string())
            })
        });
        (url, handle)
    }

    #[tokio::test]
    async fn a_client_with_a_secret_sends_an_owner_token_the_board_accepts() {
        let owner = ProfileResponse {
            user_id: Uuid::new_v4(),
            username: None,
            email: "owner@localhost".to_string(),
        };
        let (url, request) = serve_once(serde_json::to_string(&owner).unwrap()).await;
        let client = RemoteClient::new(&url, BoardSecret::new(SECRET)).unwrap();

        let profile = client.profile().await.unwrap();
        assert_eq!(profile.user_id, owner.user_id);

        let authorization = request.await.unwrap().expect("an Authorization header");
        let token = authorization
            .strip_prefix("Bearer ")
            .expect("a bearer token");
        let expires_at = verify_owner_token(SECRET, token)
            .expect("the board's decoder accepts it")
            .expect("an owner token");
        assert!(expires_at > Utc::now());

        // The owner is kept: no second request (the server above answers once).
        assert_eq!(client.profile().await.unwrap().user_id, owner.user_id);
    }

    #[tokio::test]
    async fn a_client_without_a_secret_sends_nothing() {
        let client = RemoteClient::new("http://127.0.0.1:1", None).unwrap();
        assert!(!client.has_secret());
        assert!(matches!(
            client.access_token(),
            Err(RemoteClientError::NoSecret)
        ));
        assert!(matches!(
            client.list_organizations().await,
            Err(RemoteClientError::NoSecret)
        ));
    }
}
