use std::collections::HashMap;

use api_types::{
    ConfidenceLevel, CreateIssueCommentRequest, CreateIssueRequest, FindingClass, FindingSeverity,
    Issue, IssueComment, IssueExtMetadata, IssuePriority, IssueRelationshipType, IssueSortField,
    KbDocument, KbNode, ListIssueCommentsResponse, ListIssueRelationshipsResponse,
    ListIssueTagsResponse, ListIssuesResponse, ListKbDocumentsResponse, ListKbNodesResponse,
    ListPullRequestsResponse, ListTagsResponse, MutationResponse, PlanningVerdict,
    PullRequestStatus, ReviewFinding, ReviewVerdict, SearchIssuesRequest, SortDirection,
    UpdateIssueRequest,
};
use rmcp::{
    ErrorData, handler::server::wrapper::Parameters, model::CallToolResult, schemars, tool,
    tool_router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{McpServer, ToolError};

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct McpTrackerAttachmentsRequest {
    #[schemars(
        description = "Tracker ticket key, e.g. BUG-1234 (also accepted as `ticket_key`). Optional when the workspace is linked to an issue imported from the tracker (resolved from the issue's metadata)."
    )]
    #[serde(alias = "ticket_key")]
    jira_key: Option<String>,
    #[schemars(
        description = "Board issue id to resolve the ticket key from. Optional; defaults to the workspace's linked issue."
    )]
    issue_id: Option<Uuid>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct McpTrackerAddCommentRequest {
    #[schemars(
        description = "Tracker ticket key, e.g. BUG-1234 (also accepted as `ticket_key`). Optional when the workspace is linked to an issue imported from the tracker (resolved from the issue's metadata)."
    )]
    #[serde(alias = "ticket_key")]
    jira_key: Option<String>,
    #[schemars(
        description = "Board issue id to resolve the ticket key from. Optional; defaults to the workspace's linked issue."
    )]
    issue_id: Option<Uuid>,
    #[schemars(
        description = "Plain-text comment body. Keep it skimmable; the audience often includes non-engineers."
    )]
    body: String,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
struct McpTrackerAttachmentFile {
    filename: String,
    path: String,
    size_bytes: u64,
    mime_type: Option<String>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct McpTrackerAttachmentsResponse {
    downloaded_to: String,
    files: Vec<McpTrackerAttachmentFile>,
}

/// Why a `tracker_*` tool had no ticket to act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NoTicket {
    /// No tracker is configured: every card is board-only.
    NoTracker,
    /// A tracker is configured, but this card was made on the board.
    BoardOnlyCard,
}

impl NoTicket {
    /// What the agent is told. `nothing_done` says what did not happen
    /// ("nothing posted").
    fn note(self, nothing_done: &str) -> String {
        match self {
            Self::NoTracker => {
                format!("no tracker is configured; the card is the only record — {nothing_done}")
            }
            Self::BoardOnlyCard => format!(
                "this card has no tracker ticket; the card is the only record — {nothing_done}"
            ),
        }
    }
}

/// A `tracker_*` tool's answer when there is no ticket: a success, not an
/// error. A playbook step such as "post the reproduction on the ticket too"
/// has nothing to do without a ticket, and must not fail the session.
/// `fields` is the tool's usual result with nothing in it, so a caller that
/// reads it finds the shape it expects.
fn no_ticket_result(
    reason: NoTicket,
    nothing_done: &str,
    fields: serde_json::Value,
) -> serde_json::Value {
    let mut result = match fields {
        serde_json::Value::Object(map) => map,
        _ => serde_json::Map::new(),
    };
    result.insert(
        "note".to_string(),
        serde_json::Value::String(reason.note(nothing_done)),
    );
    serde_json::Value::Object(result)
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct McpSlackThreadRequest {
    #[schemars(
        description = "Slack channel id (e.g. C0123456). Optional; defaults to the team's configured escalation channel."
    )]
    channel: Option<String>,
    #[schemars(
        description = "Thread root timestamp, e.g. 1700000000.000100 (the ts of the first message in the thread)."
    )]
    thread_ts: String,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
struct McpSlackThreadMessage {
    ts: String,
    text: String,
    is_bot: bool,
    user: Option<String>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct McpSlackThreadResponse {
    message_count: usize,
    transcript: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct McpCreateIssueRequest {
    #[schemars(
        description = "The ID of the project to create the issue in. Optional if running inside a workspace linked to a remote project."
    )]
    project_id: Option<Uuid>,
    #[schemars(description = "The title of the issue")]
    title: String,
    #[schemars(description = "Optional description of the issue")]
    description: Option<String>,
    #[schemars(
        description = "Optional priority of the issue. Allowed values: 'urgent', 'high', 'medium', 'low'."
    )]
    priority: Option<String>,
    #[schemars(description = "Optional parent issue ID to create a subissue")]
    parent_issue_id: Option<Uuid>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct McpCreateIssueResponse {
    issue_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct McpListIssuesRequest {
    #[schemars(
        description = "The ID of the project to list issues from. Optional if running inside a workspace linked to a remote project."
    )]
    project_id: Option<Uuid>,
    #[schemars(description = "Maximum number of issues to return (default: 50)")]
    limit: Option<i32>,
    #[schemars(description = "Number of results to skip before returning rows (default: 0)")]
    offset: Option<i32>,
    #[schemars(description = "Filter by status name (case-insensitive)")]
    status: Option<String>,
    #[schemars(
        description = "Filter by priority. Allowed values: 'urgent', 'high', 'medium', 'low'."
    )]
    priority: Option<String>,
    #[schemars(description = "Filter by parent issue ID (subissues of this issue)")]
    parent_issue_id: Option<Uuid>,
    #[schemars(description = "Case-insensitive substring match against title and description")]
    search: Option<String>,
    #[schemars(description = "Filter by issue simple ID (case-insensitive exact match)")]
    simple_id: Option<String>,
    #[schemars(description = "Filter to issues assigned to this user ID")]
    assignee_user_id: Option<Uuid>,
    #[schemars(description = "Filter to issues having this tag ID")]
    tag_id: Option<Uuid>,
    #[schemars(description = "Filter to issues having a tag with this name (case-insensitive)")]
    tag_name: Option<String>,
    #[schemars(
        description = "Field to sort by. Allowed values: 'sort_order', 'priority', 'created_at', 'updated_at', 'title'. Default: 'sort_order'."
    )]
    sort_field: Option<String>,
    #[schemars(description = "Sort direction. Allowed values: 'asc', 'desc'. Default: 'asc'.")]
    sort_direction: Option<String>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct IssueSummary {
    #[schemars(description = "The unique identifier of the issue")]
    id: String,
    #[schemars(description = "The title of the issue")]
    title: String,
    #[schemars(description = "The human-readable issue simple ID")]
    simple_id: String,
    #[schemars(description = "Current status of the issue")]
    status: String,
    #[schemars(description = "Current priority of the issue")]
    priority: Option<String>,
    #[schemars(description = "Parent issue ID if this is a subissue")]
    parent_issue_id: Option<String>,
    #[schemars(description = "When the issue was created")]
    created_at: String,
    #[schemars(description = "When the issue was last updated")]
    updated_at: String,
    #[schemars(description = "Number of pull requests linked to this issue")]
    pull_request_count: usize,
    #[schemars(description = "URL of the most recent pull request, if any")]
    latest_pr_url: Option<String>,
    #[schemars(
        description = "Status of the most recent pull request: 'open', 'merged', or 'closed'"
    )]
    latest_pr_status: Option<PullRequestStatus>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct PullRequestSummary {
    #[schemars(description = "PR number")]
    number: i32,
    #[schemars(description = "URL of the pull request")]
    url: String,
    #[schemars(description = "Status of the pull request: 'open', 'merged', or 'closed'")]
    status: PullRequestStatus,
    #[schemars(description = "When the PR was merged, if applicable")]
    merged_at: Option<String>,
    #[schemars(description = "Target branch for the PR")]
    target_branch_name: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct McpTagSummary {
    #[schemars(description = "The tag ID")]
    id: String,
    #[schemars(description = "The tag name")]
    name: String,
    #[schemars(description = "The tag color")]
    color: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct McpRelationshipSummary {
    #[schemars(description = "The relationship ID (use this to delete)")]
    id: String,
    #[schemars(description = "The related issue ID")]
    related_issue_id: String,
    #[schemars(description = "The related issue's simple ID (e.g. 'PROJ-42')")]
    related_simple_id: String,
    #[schemars(description = "Relationship type: blocking, related, or has_duplicate")]
    relationship_type: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct McpSubIssueSummary {
    #[schemars(description = "The sub-issue ID")]
    id: String,
    #[schemars(description = "Short human-readable identifier (e.g. 'PROJ-43')")]
    simple_id: String,
    #[schemars(description = "The sub-issue title")]
    title: String,
    #[schemars(description = "Current status of the sub-issue")]
    status: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct IssueDetails {
    #[schemars(description = "The unique identifier of the issue")]
    id: String,
    #[schemars(description = "The title of the issue")]
    title: String,
    #[schemars(description = "The human-readable issue simple ID")]
    simple_id: String,
    #[schemars(description = "Optional description of the issue")]
    description: Option<String>,
    #[schemars(description = "Current status of the issue")]
    status: String,
    #[schemars(description = "The status ID (UUID)")]
    status_id: String,
    #[schemars(description = "Current priority of the issue")]
    priority: Option<String>,
    #[schemars(description = "Parent issue ID if this is a subissue")]
    parent_issue_id: Option<String>,
    #[schemars(description = "Optional planned start date")]
    start_date: Option<String>,
    #[schemars(description = "Optional planned target date")]
    target_date: Option<String>,
    #[schemars(description = "Optional completion date")]
    completed_at: Option<String>,
    #[schemars(description = "When the issue was created")]
    created_at: String,
    #[schemars(description = "When the issue was last updated")]
    updated_at: String,
    #[schemars(description = "Pull requests linked to this issue")]
    pull_requests: Vec<PullRequestSummary>,
    #[schemars(description = "Tags attached to this issue")]
    tags: Vec<McpTagSummary>,
    #[schemars(description = "Relationships to other issues")]
    relationships: Vec<McpRelationshipSummary>,
    #[schemars(description = "Sub-issues under this issue")]
    sub_issues: Vec<McpSubIssueSummary>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct McpListIssuesResponse {
    issues: Vec<IssueSummary>,
    total_count: usize,
    returned_count: usize,
    limit: usize,
    offset: usize,
    project_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct McpUpdateIssueRequest {
    #[schemars(
        description = "The ID of the issue to update. Optional in a workspace launched for an issue — omit it to update that issue."
    )]
    issue_id: Option<Uuid>,
    #[schemars(description = "New title for the issue")]
    title: Option<String>,
    #[schemars(description = "New description for the issue")]
    description: Option<String>,
    #[schemars(description = "New status name for the issue (must match a project status name)")]
    status: Option<String>,
    #[schemars(
        description = "New priority for the issue. Allowed values: 'urgent', 'high', 'medium', 'low'."
    )]
    priority: Option<String>,
    #[schemars(
        description = "Parent issue ID to set this as a subissue. Pass null to un-nest from parent."
    )]
    parent_issue_id: Option<Option<Uuid>>,
    #[schemars(
        description = "Planning verdict kind to record on the issue's extension_metadata. Allowed values: 'spec' (story), 'rca' (bug). Required when any other planning_* field is set."
    )]
    planning_kind: Option<String>,
    #[schemars(
        description = "Number of unresolved open questions in the spec/RCA. Specs cannot advance to Executing until this is 0."
    )]
    planning_open_questions: Option<u32>,
    #[schemars(
        description = "Confidence in the spec/RCA. Allowed values: 'low', 'medium', 'high'. RCAs cannot advance to Executing until this is 'high'."
    )]
    planning_confidence: Option<String>,
    #[schemars(
        description = "When planning_confidence is 'low' or 'medium': one sentence naming the missing confirmation that keeps it from 'high' (e.g. 'no runtime repro — needs the CloudWatch trace for generation 3775956'). Surfaced on the Slack escalation card."
    )]
    planning_confidence_reason: Option<String>,
    #[schemars(description = "One-paragraph summary of the planning outcome.")]
    planning_summary: Option<String>,
    #[schemars(
        description = "Who can answer the open questions: 'reporter' or 'engineer'. Required whenever planning_open_questions is above 0, because the ticket's daily update addresses whoever this names. 'reporter' means the person who filed it or whoever is fielding the customer, and fits a detail about their data, an account, a screenshot, a decision only they can make. 'engineer' means a root cause, a scope decision, a question about the code. Choose by asking who could actually act on the request — a reporter handed an engineering question cannot answer it, and the ticket sits still while they work out it was never theirs."
    )]
    planning_ask_audience: Option<String>,
    #[schemars(
        description = "The request itself, addressed to planning_ask_audience and phrased as a request: what to do or supply. Not a description of what is unknown — 'the root cause of the nightly failure is still unidentified' tells the reader nothing they can act on, while 'please confirm which timezone the customer's schedule is set to' does. One or two sentences, in their words, not yours."
    )]
    planning_ask_request: Option<String>,
    #[schemars(
        description = "The external system the failure path runs through, if it runs through one — 'Salesforce', 'the customer's SSO provider', 'the HubSpot API'. Record it whatever you conclude the cause is, including when you conclude the cause is our own code. It is a fact about how far your reproduction could reach, not a verdict: the development tenant cannot hold the customer's credentials, permissions or data, so an expired token or a revoked permission on their side looks from here exactly like a defect in ours. Leave unset only when nothing external is involved at all."
    )]
    planning_integration: Option<String>,
    #[schemars(
        description = "Plain-language version of planning_summary for account managers and support: what broke, who was affected, what the fix does, in 3-4 short sentences with no jargon, file paths, or class names. Posted as a Jira comment when the RCA is filed."
    )]
    planning_client_summary: Option<String>,
    #[schemars(
        description = "Path of the spec/RCA HTML artifact relative to the workspace root (e.g. 'docs/specs/BUG-1234.html'). Recorded on the issue's extension_metadata."
    )]
    spec_path: Option<String>,
    #[schemars(
        description = "Set true ONLY after the user has explicitly approved the spec/RCA in the Lavish review gate (clicked Approve). Records spec.approved_at; pass false to clear a previous approval."
    )]
    spec_approved: Option<bool>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct McpUpdateIssueResponse {
    issue: IssueDetails,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct McpAddIssueCommentRequest {
    #[schemars(description = "The ID of the issue to comment on")]
    issue_id: Uuid,
    #[schemars(description = "The comment body (markdown supported)")]
    message: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct McpAddIssueCommentResponse {
    comment_id: String,
    issue_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct McpListIssueCommentsRequest {
    #[schemars(
        description = "The issue whose comments to read. Optional; defaults to the issue this workspace was launched for."
    )]
    issue_id: Option<Uuid>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct McpIssueCommentSummary {
    comment_id: String,
    created_at: String,
    message: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct McpListIssueCommentsResponse {
    issue_id: String,
    issue_comments: Vec<McpIssueCommentSummary>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct McpReviewFindingInput {
    #[schemars(
        description = "Who the finding is for: 'systematic' (a rule the codebase already answers — duplicated helper, diverged pattern, broken convention, unreadable name, a comment that narrates the change, missing test), 'correctness' (a bug with a stated failure scenario, or a security/data-loss risk), or 'product' (a question only a human can answer)."
    )]
    class: String,
    #[schemars(
        description = "'blocker', 'major', 'minor' or 'nit'. Only blocker and major send the work back for a fix round."
    )]
    severity: String,
    #[schemars(
        description = "Repo-relative path. Omit for a finding about the change as a whole."
    )]
    path: Option<String>,
    #[schemars(
        description = "Line the finding anchors to: '118', a range '10:15', or 'old:7' for a line the branch deleted."
    )]
    line: Option<String>,
    #[schemars(description = "One-line claim.")]
    title: String,
    #[schemars(
        description = "The argument: what it is, why it matters, what to do instead. Written to be read on its own."
    )]
    body: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct McpRecordReviewRequest {
    #[schemars(
        description = "The issue reviewed. Defaults to the issue this workspace was launched for."
    )]
    issue_id: Option<Uuid>,
    #[schemars(description = "True when the change is approved to advance.")]
    approved: bool,
    #[schemars(description = "Every finding, in the order they should be read.")]
    findings: Vec<McpReviewFindingInput>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct McpRecordReviewResponse {
    issue_id: String,
    approved: bool,
    findings: usize,
    /// Findings that will start a fix round: systematic or correctness, at
    /// blocker or major. Echoed so the reviewer can see what its own severities
    /// just set in motion.
    fix_round_findings: usize,
    product_questions: usize,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct McpListKbDocumentsRequest {
    #[schemars(description = "Optional document kind filter, e.g. 'rca'. Omit for all kinds.")]
    kind: Option<String>,
    #[schemars(
        description = "The ID of the VK project. Optional if running inside a workspace linked to a remote project."
    )]
    project_id: Option<Uuid>,
    #[schemars(
        description = "When true, only documents with no graph links yet (pending analysis)."
    )]
    unlinked: Option<bool>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct McpKbDocumentSummary {
    document_id: String,
    kind: String,
    title: String,
    jira_key: Option<String>,
    summary: Option<String>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct McpListKbDocumentsResponse {
    documents: Vec<McpKbDocumentSummary>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct McpGetKbDocumentRequest {
    #[schemars(description = "The knowledge document ID (from kb_list_documents)")]
    document_id: Uuid,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct McpKbFileDocumentRequest {
    #[schemars(
        description = "Document kind. 'rca' for a root-cause analysis — the kind the knowledge graph links to product areas and root-cause classes."
    )]
    kind: String,
    #[schemars(description = "Document title; for an RCA, the ticket key and the one-line cause.")]
    title: String,
    #[schemars(
        description = "Full document content as authored. For an RCA, the self-contained HTML written to docs/specs/<KEY>.html — read it from disk and pass it whole."
    )]
    body: String,
    #[schemars(
        description = "The Jira ticket the document is about, e.g. BUG-1234. The document is pinned to it as a remote link. Filing the same ticket again returns the document already filed for it instead of a duplicate."
    )]
    jira_key: String,
    #[schemars(description = "'bug' for an RCA. Optional.")]
    work_class: Option<String>,
    #[schemars(description = "One-line summary: the verdict summary for an RCA. Optional.")]
    summary: Option<String>,
    #[schemars(
        description = "Classification tags ('namespace:value'), e.g. 'severity:p2'. Optional; kb_tag_document adds more later."
    )]
    tags: Option<Vec<String>>,
    #[schemars(
        description = "The ID of the VK project. Optional if running inside a workspace linked to a remote project."
    )]
    project_id: Option<Uuid>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct McpKbTagDocumentRequest {
    #[schemars(description = "The knowledge document ID (from kb_list_documents)")]
    document_id: Uuid,
    #[schemars(
        description = "Classification tags to add ('namespace:value' convention, e.g. 'origin:regression', 'origin:dormant', 'detectable-by:code-review'). Idempotent — existing tags are kept."
    )]
    tags: Vec<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct McpGraphNodesRequest {
    #[schemars(
        description = "The ID of the VK project. Optional if running inside a workspace linked to a remote project."
    )]
    project_id: Option<Uuid>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct McpGraphNode {
    node_id: String,
    kind: String,
    name: String,
    description: Option<String>,
    /// Documents currently linked to this node.
    edge_count: i64,
    /// Set when an investigation story was already created for this node.
    investigation_jira_key: Option<String>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct McpGraphNodesResponse {
    nodes: Vec<McpGraphNode>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct McpGraphCreateNodeRequest {
    #[schemars(
        description = "Name of the new root-cause-class node. Make it generic enough to attract future edges (e.g. 'stale cache invalidation', not 'dashboard cache bug'). ALWAYS check graph_nodes for an existing fit first."
    )]
    name: String,
    #[schemars(description = "One or two sentences describing the recurring cause pattern")]
    description: Option<String>,
    #[schemars(
        description = "The ID of the VK project. Optional if running inside a workspace linked to a remote project."
    )]
    project_id: Option<Uuid>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct McpGraphLinkRequest {
    #[schemars(description = "Node ID (from graph_nodes or graph_create_node)")]
    node_id: Uuid,
    #[schemars(description = "Knowledge document ID (from kb_list_documents)")]
    document_id: Uuid,
    #[schemars(description = "One line: why this document exhibits this node")]
    rationale: String,
    #[schemars(description = "Confidence in the link: 'low', 'medium', or 'high'")]
    confidence: String,
    #[schemars(
        description = "The ID of the VK project. Optional if running inside a workspace linked to a remote project."
    )]
    project_id: Option<Uuid>,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
struct McpTrackerSubtaskSpec {
    #[schemars(description = "Sub-task summary (one line)")]
    title: String,
    #[schemars(description = "Optional sub-task description (plain text)")]
    description: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct McpCreateTrackerSubtasksRequest {
    #[schemars(
        description = "Ticket key of the parent epic, e.g. 'BUG-8127'. Sub-tasks are created under this epic."
    )]
    epic_key: String,
    #[schemars(description = "Sub-tasks to create, in execution order")]
    subtasks: Vec<McpTrackerSubtaskSpec>,
    #[schemars(
        description = "The ID of the VK project to import the sub-tasks into. Optional if running inside a workspace linked to a remote project."
    )]
    project_id: Option<Uuid>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct McpCreateTrackerSubtasksResponse {
    created_jira_keys: Vec<String>,
    imported_vk_issues: u32,
    failed_titles: Vec<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct McpDeleteIssueRequest {
    #[schemars(description = "The ID of the issue to delete")]
    issue_id: Uuid,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct McpDeleteIssueResponse {
    deleted_issue_id: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct McpGetIssueRequest {
    #[schemars(description = "The ID of the issue to retrieve")]
    issue_id: Uuid,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct McpGetIssueResponse {
    issue: IssueDetails,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct McpListIssuePrioritiesResponse {
    priorities: Vec<String>,
}

#[tool_router(router = remote_issues_tools_router, vis = "pub")]
impl McpServer {
    #[tool(
        description = "Create a new issue in a project. `project_id` is optional if running inside a workspace linked to a remote project."
    )]
    async fn create_issue(
        &self,
        Parameters(McpCreateIssueRequest {
            project_id,
            title,
            description,
            priority,
            parent_issue_id,
        }): Parameters<McpCreateIssueRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let project_id = match self.resolve_project_id(project_id).await {
            Ok(id) => id,
            Err(e) => return Ok(McpServer::tool_error(e)),
        };

        let expanded_description = match description {
            Some(desc) => Some(self.expand_tags(&desc).await),
            None => None,
        };

        let status_id = match self.default_status_id(project_id).await {
            Ok(id) => id,
            Err(e) => return Ok(McpServer::tool_error(e)),
        };

        let priority = match priority {
            Some(p) => match Self::parse_issue_priority(&p) {
                Ok(priority) => Some(priority),
                Err(e) => return Ok(McpServer::tool_error(e)),
            },
            None => None,
        };

        let payload = CreateIssueRequest {
            id: None,
            project_id,
            status_id,
            title,
            description: expanded_description,
            priority,
            start_date: None,
            target_date: None,
            completed_at: None,
            sort_order: 0.0,
            parent_issue_id,
            parent_issue_sort_order: None,
            extension_metadata: serde_json::json!({}),
        };

        let url = self.url("/api/remote/issues");
        let response: MutationResponse<Issue> =
            match self.send_json(self.client.post(&url).json(&payload)).await {
                Ok(r) => r,
                Err(e) => return Ok(McpServer::tool_error(e)),
            };

        McpServer::success(&McpCreateIssueResponse {
            issue_id: response.data.id.to_string(),
        })
    }

    #[tool(
        description = "List all the issues in a project. `project_id` is optional if running inside a workspace linked to a remote project."
    )]
    async fn list_issues(
        &self,
        Parameters(McpListIssuesRequest {
            project_id,
            limit,
            offset,
            status,
            priority,
            parent_issue_id,
            search,
            simple_id,
            assignee_user_id,
            tag_id,
            tag_name,
            sort_field,
            sort_direction,
        }): Parameters<McpListIssuesRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let project_id = match self.resolve_project_id(project_id).await {
            Ok(id) => id,
            Err(e) => return Ok(McpServer::tool_error(e)),
        };

        let project_statuses = match self.fetch_project_statuses(project_id).await {
            Ok(statuses) => Some(statuses),
            Err(e) => {
                if status.is_some() {
                    return Ok(McpServer::tool_error(e));
                }
                None
            }
        };
        let status_names_by_id = project_statuses.as_ref().map(|statuses| {
            statuses
                .iter()
                .map(|status| (status.id, status.name.clone()))
                .collect::<HashMap<_, _>>()
        });

        let (status_id, status_ids, missing_status_name_match) = match status.as_deref() {
            Some(status) => match Uuid::parse_str(status) {
                Ok(status_id) => (Some(status_id), None, false),
                Err(_) => {
                    let matching_status_ids = project_statuses
                        .as_deref()
                        .map(|statuses| {
                            Self::matching_ids_by_name(
                                statuses
                                    .iter()
                                    .map(|status| (status.id, status.name.as_str())),
                                status,
                            )
                        })
                        .unwrap_or_default();
                    let missing_status_name_match = matching_status_ids.is_empty();
                    (
                        None,
                        (!missing_status_name_match).then_some(matching_status_ids),
                        missing_status_name_match,
                    )
                }
            },
            None => (None, None, false),
        };

        let priority = match priority {
            Some(priority) => match Self::parse_issue_priority(&priority) {
                Ok(priority) => Some(priority),
                Err(e) => return Ok(McpServer::tool_error(e)),
            },
            None => None,
        };

        let sort_field = match Self::parse_issue_sort_field(sort_field.as_deref()) {
            Ok(value) => Some(value),
            Err(e) => return Ok(McpServer::tool_error(e)),
        };
        let sort_direction = match Self::parse_sort_direction(sort_direction.as_deref()) {
            Ok(value) => Some(value),
            Err(e) => return Ok(McpServer::tool_error(e)),
        };

        let matching_tag_ids = match tag_name.as_deref() {
            Some(tag_name) => match self.find_tag_ids_by_name(project_id, tag_name).await {
                Ok(tag_ids) => Some(tag_ids),
                Err(e) => return Ok(McpServer::tool_error(e)),
            },
            None => None,
        };
        let (tag_id, tag_ids, missing_tag_name_match) =
            Self::resolve_tag_filters(tag_id, matching_tag_ids);

        let response = if missing_status_name_match || missing_tag_name_match {
            ListIssuesResponse {
                issues: Vec::new(),
                total_count: 0,
                limit: limit.unwrap_or(50).max(0) as usize,
                offset: offset.unwrap_or(0).max(0) as usize,
            }
        } else {
            let query = SearchIssuesRequest {
                project_id,
                status_id,
                status_ids,
                priority,
                parent_issue_id,
                search,
                simple_id,
                assignee_user_id,
                tag_id,
                tag_ids,
                sort_field,
                sort_direction,
                limit: Some(limit.unwrap_or(50).max(0)),
                offset: Some(offset.unwrap_or(0).max(0)),
            };
            let url = self.url("/api/remote/issues/search");
            match self.send_json(self.client.post(&url).json(&query)).await {
                Ok(r) => r,
                Err(e) => return Ok(McpServer::tool_error(e)),
            }
        };

        let mut summaries = Vec::with_capacity(response.issues.len());
        for issue in &response.issues {
            let pull_requests = self.fetch_pull_requests(issue.id).await;
            summaries.push(self.issue_to_summary(
                issue,
                status_names_by_id.as_ref(),
                &pull_requests,
            ));
        }

        McpServer::success(&McpListIssuesResponse {
            total_count: response.total_count,
            returned_count: summaries.len(),
            limit: response.limit,
            offset: response.offset,
            issues: summaries,
            project_id: project_id.to_string(),
        })
    }

    #[tool(
        description = "Get detailed information about a specific issue. You can use `list_issues` to find issue IDs. `issue_id` is required."
    )]
    async fn get_issue(
        &self,
        Parameters(McpGetIssueRequest { issue_id }): Parameters<McpGetIssueRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let url = self.url(&format!("/api/remote/issues/{}", issue_id));
        let issue: Issue = match self.send_json(self.client.get(&url)).await {
            Ok(i) => i,
            Err(e) => return Ok(McpServer::tool_error(e)),
        };

        let pull_requests = self.fetch_pull_requests(issue_id).await;
        let details = self.issue_to_details(&issue, pull_requests).await;
        McpServer::success(&McpGetIssueResponse { issue: details })
    }

    #[tool(
        description = "Update an existing issue's title, description, or status, and/or record a planning verdict (spec/RCA outcome) via the planning_* fields, and/or record the spec artifact path / Lavish approval via spec_path / spec_approved. Every field is optional: `issue_id` defaults to the issue this workspace was launched for."
    )]
    async fn update_issue(
        &self,
        Parameters(McpUpdateIssueRequest {
            issue_id,
            title,
            description,
            status,
            priority,
            parent_issue_id,
            planning_kind,
            planning_open_questions,
            planning_ask_audience,
            planning_ask_request,
            planning_confidence,
            planning_confidence_reason,
            planning_summary,
            planning_client_summary,
            planning_integration,
            spec_path,
            spec_approved,
        }): Parameters<McpUpdateIssueRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let issue_id = match self.resolve_issue_id(issue_id).await {
            Ok(id) => id,
            Err(e) => return Ok(McpServer::tool_error(e)),
        };

        // First get the issue to know its project_id for status resolution
        let get_url = self.url(&format!("/api/remote/issues/{}", issue_id));
        let existing_issue: Issue = match self.send_json(self.client.get(&get_url)).await {
            Ok(i) => i,
            Err(e) => return Ok(McpServer::tool_error(e)),
        };

        // Resolve status name to status_id if provided
        let status_id = if let Some(ref status_name) = status {
            match self
                .resolve_status_id(existing_issue.project_id, status_name)
                .await
            {
                Ok(id) => Some(id),
                Err(e) => return Ok(McpServer::tool_error(e)),
            }
        } else {
            None
        };

        // Expand @tagname references in description
        let expanded_description = match description {
            Some(desc) => Some(Some(self.expand_tags(&desc).await)),
            None => None,
        };

        let priority = if let Some(priority) = priority {
            match Self::parse_issue_priority(&priority) {
                Ok(parsed) => Some(Some(parsed)),
                Err(e) => return Ok(McpServer::tool_error(e)),
            }
        } else {
            None
        };

        // Planning verdict: merged into the existing extension_metadata blob
        // (IssueExtMetadata::merge_into) so keys written by other tools are
        // preserved.
        let has_planning_params = planning_kind.is_some()
            || planning_open_questions.is_some()
            || planning_ask_audience.is_some()
            || planning_ask_request.is_some()
            || planning_confidence.is_some()
            || planning_confidence_reason.is_some()
            || planning_summary.is_some()
            || planning_client_summary.is_some()
            || planning_integration.is_some();
        let has_spec_params = spec_path.is_some() || spec_approved.is_some();
        let planning_verdict = if has_planning_params {
            let kind = match planning_kind.as_deref().map(str::trim) {
                Some(kind)
                    if kind.eq_ignore_ascii_case("spec") || kind.eq_ignore_ascii_case("rca") =>
                {
                    kind.to_ascii_lowercase()
                }
                Some(other) => {
                    return Ok(McpServer::tool_error(ToolError::message(format!(
                        "Unknown planning_kind '{}'. Allowed values: ['spec', 'rca']",
                        other
                    ))));
                }
                None => {
                    return Ok(McpServer::tool_error(ToolError::message(
                        "planning_kind is required when recording a planning verdict. Allowed values: ['spec', 'rca']",
                    )));
                }
            };
            let confidence = match planning_confidence.as_deref() {
                Some(confidence) => match Self::parse_confidence_level(confidence) {
                    Ok(parsed) => Some(parsed),
                    Err(e) => return Ok(McpServer::tool_error(e)),
                },
                None => None,
            };
            Some(PlanningVerdict {
                kind: Some(kind),
                open_questions: planning_open_questions,
                confidence,
                // Sanitize at ingress: these three are free text an agent
                // sometimes writes with its own tool syntax still attached.
                confidence_reason: planning_confidence_reason
                    .as_deref()
                    .map(api_types::strip_agent_scaffolding),
                summary: planning_summary
                    .as_deref()
                    .map(api_types::strip_agent_scaffolding),
                client_summary: planning_client_summary
                    .as_deref()
                    .map(api_types::strip_agent_scaffolding),
                recorded_at: Some(chrono::Utc::now()),
                integration: planning_integration
                    .as_deref()
                    .map(api_types::strip_agent_scaffolding)
                    .map(|system| system.trim().to_string())
                    .filter(|system| !system.is_empty()),
                // Both halves or neither: an audience with no request names a
                // reader and tells them nothing, and a request with no
                // audience is the field this ticket was raised about.
                ask: match (
                    planning_ask_audience
                        .as_deref()
                        .map(str::trim)
                        .and_then(api_types::AskAudience::parse),
                    planning_ask_request
                        .as_deref()
                        .map(api_types::strip_agent_scaffolding)
                        .map(|request| request.trim().to_string())
                        .filter(|request| !request.is_empty()),
                ) {
                    (Some(audience), Some(request)) => {
                        Some(api_types::PlanningAsk { audience, request })
                    }
                    _ => None,
                },
            })
        } else {
            None
        };

        // Spec artifact path / approval: merged with the existing SpecRef so
        // setting approval alone doesn't drop a previously recorded path.
        let spec = if has_spec_params {
            let existing_meta = IssueExtMetadata::from_value(&existing_issue.extension_metadata);
            let mut spec = existing_meta.spec.unwrap_or_default();
            if let Some(path) = spec_path {
                spec.path = Some(path);
            }
            match spec_approved {
                // An approval Prep Cook already recorded from the channel itself
                // (Lavish, a comment, the API) says more about who approved
                // than the agent can; keep it rather than overwrite it.
                Some(true)
                    if spec.approved_at.is_some()
                        && spec
                            .approved_via
                            .is_some_and(|via| via != api_types::ApprovalSource::Agent) => {}
                Some(true) => {
                    spec.approved_at = Some(chrono::Utc::now());
                    // Recorded as agent-reported, because that is all this is.
                    // The tool's description asks the model to set it only
                    // after a person clicked Approve in the Lavish gate, and
                    // an instruction to a model is not evidence about a human.
                    spec.approved_via = Some(api_types::ApprovalSource::Agent);
                    spec.approved_by = None;
                }
                Some(false) => {
                    spec.approved_at = None;
                    spec.approved_via = None;
                    spec.approved_by = None;
                }
                None => {}
            }
            Some(spec)
        } else {
            None
        };

        let extension_metadata = if planning_verdict.is_some() || spec.is_some() {
            let update = IssueExtMetadata {
                planning: planning_verdict,
                spec,
                ..Default::default()
            };
            Some(update.merge_into(&existing_issue.extension_metadata))
        } else {
            None
        };

        let payload = UpdateIssueRequest {
            status_id,
            title,
            description: expanded_description,
            priority,
            start_date: None,
            target_date: None,
            completed_at: None,
            sort_order: None,
            parent_issue_id,
            parent_issue_sort_order: None,
            extension_metadata,
        };

        let url = self.url(&format!("/api/remote/issues/{}", issue_id));
        let response: MutationResponse<Issue> =
            match self.send_json(self.client.patch(&url).json(&payload)).await {
                Ok(r) => r,
                Err(e) => return Ok(McpServer::tool_error(e)),
            };
        let mut final_issue = response.data;

        // Recording a spec approval IS the human gate decision: advance the
        // ticket Planning → Executing in a second update (the remote's gate
        // validates against the pre-update metadata, so approval and status
        // cannot land in one PATCH). Best-effort: if the verdict gate still
        // rejects the move, the approval stays recorded and the ticket
        // remains in Planning.
        let takeover_paused = IssueExtMetadata::from_value(&final_issue.extension_metadata)
            .human_takeover
            .is_some_and(|takeover| takeover.detected_at.is_some());
        if spec_approved == Some(true)
            && !takeover_paused
            && let Ok(planning_id) = self
                .resolve_status_id(existing_issue.project_id, "Planning")
                .await
            && final_issue.status_id == planning_id
            && let Ok(executing_id) = self
                .resolve_status_id(existing_issue.project_id, "Executing")
                .await
        {
            let advance = UpdateIssueRequest {
                status_id: Some(executing_id),
                title: None,
                description: None,
                priority: None,
                start_date: None,
                target_date: None,
                completed_at: None,
                sort_order: None,
                parent_issue_id: None,
                parent_issue_sort_order: None,
                extension_metadata: None,
            };
            match self
                .send_json::<MutationResponse<Issue>>(self.client.patch(&url).json(&advance))
                .await
            {
                Ok(advanced) => final_issue = advanced.data,
                Err(e) => {
                    tracing::warn!(
                        "Approval recorded but auto-advance to Executing was rejected: {e:?}"
                    );
                }
            }
        }

        let pull_requests = self.fetch_pull_requests(issue_id).await;
        let details = self.issue_to_details(&final_issue, pull_requests).await;
        McpServer::success(&McpUpdateIssueResponse { issue: details })
    }

    #[tool(description = "List allowed issue priority values.")]
    async fn list_issue_priorities(&self) -> Result<CallToolResult, ErrorData> {
        McpServer::success(&McpListIssuePrioritiesResponse {
            priorities: ["urgent", "high", "medium", "low"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
        })
    }

    #[tool(
        description = "Add a comment to an issue (e.g. review findings, progress notes). `issue_id` and `message` are required."
    )]
    async fn add_issue_comment(
        &self,
        Parameters(McpAddIssueCommentRequest { issue_id, message }): Parameters<
            McpAddIssueCommentRequest,
        >,
    ) -> Result<CallToolResult, ErrorData> {
        // A card comment whose first line is a decision verb is a person
        // deciding on the plan, and the card cannot tell the agent's comments
        // from a person's by author. So the agent may not write one.
        if api_types::parse_card_decision(&message).is_some() {
            return Ok(McpServer::tool_error(ToolError::message(
                "A comment opening with APPROVE, REQUEST CHANGES or REPLAN is a person's decision \
                 on the plan, not something an agent can post. Reword the first line.",
            )));
        }
        let payload = CreateIssueCommentRequest {
            id: None,
            issue_id,
            message: self.expand_tags(&message).await,
            parent_id: None,
        };
        let url = self.url("/api/remote/issue_comments");
        let response: MutationResponse<IssueComment> =
            match self.send_json(self.client.post(&url).json(&payload)).await {
                Ok(r) => r,
                Err(e) => return Ok(McpServer::tool_error(e)),
            };

        McpServer::success(&McpAddIssueCommentResponse {
            comment_id: response.data.id.to_string(),
            issue_id: issue_id.to_string(),
        })
    }

    #[tool(
        description = "Read an issue's comments, oldest first. The board is where the work talks to itself: `REPRODUCTION:` carries the harness's cases with expected and observed values, `REVIEW: APPROVE` / `REVIEW: REQUEST CHANGES` a review verdict, `REVIEW-REJECTED: <file>:<line>` a fix round's argument that a finding does not hold. `issue_id` defaults to the issue this workspace was launched for."
    )]
    async fn list_issue_comments(
        &self,
        Parameters(McpListIssueCommentsRequest { issue_id }): Parameters<
            McpListIssueCommentsRequest,
        >,
    ) -> Result<CallToolResult, ErrorData> {
        let issue_id = match self.resolve_issue_id(issue_id).await {
            Ok(id) => id,
            Err(e) => return Ok(McpServer::tool_error(e)),
        };

        let url = self.url(&format!("/api/remote/issue_comments?issue_id={}", issue_id));
        let response: ListIssueCommentsResponse = match self.send_json(self.client.get(&url)).await
        {
            Ok(r) => r,
            Err(e) => return Ok(McpServer::tool_error(e)),
        };

        // Oldest first: a reader following a review loop wants the rounds in
        // the order they happened, and the API does not promise an order.
        let mut comments = response.issue_comments;
        comments.sort_by_key(|comment| comment.created_at);

        McpServer::success(&McpListIssueCommentsResponse {
            issue_id: issue_id.to_string(),
            issue_comments: comments
                .into_iter()
                .map(|comment| McpIssueCommentSummary {
                    comment_id: comment.id.to_string(),
                    created_at: comment.created_at.to_rfc3339(),
                    message: comment.message,
                })
                .collect(),
        })
    }

    #[tool(
        description = "Record an independent review's outcome as structured findings, so the orchestrator can route each one: systematic and correctness findings at blocker/major start a fix round, product questions are held for a human, nits are listed and never send work back. Call this BEFORE posting the review comment — the comment is what people read, this is what the automation acts on."
    )]
    async fn record_review(
        &self,
        Parameters(McpRecordReviewRequest {
            issue_id,
            approved,
            findings,
        }): Parameters<McpRecordReviewRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let issue_id = match self.resolve_issue_id(issue_id).await {
            Ok(id) => id,
            Err(e) => return Ok(McpServer::tool_error(e)),
        };

        let mut parsed = Vec::with_capacity(findings.len());
        for (index, finding) in findings.iter().enumerate() {
            // Reject the whole call rather than dropping one finding: a review
            // recorded with a finding silently missing routes as though the
            // reviewer never raised it.
            let class = match finding.class.trim().to_ascii_lowercase().as_str() {
                "systematic" => FindingClass::Systematic,
                "correctness" => FindingClass::Correctness,
                "product" => FindingClass::Product,
                other => {
                    return Ok(McpServer::tool_error(ToolError::message(format!(
                        "finding {}: unknown class '{other}'. Allowed values: ['systematic', 'correctness', 'product']",
                        index + 1
                    ))));
                }
            };
            let severity = match finding.severity.trim().to_ascii_lowercase().as_str() {
                "blocker" => FindingSeverity::Blocker,
                "major" => FindingSeverity::Major,
                "minor" => FindingSeverity::Minor,
                "nit" => FindingSeverity::Nit,
                other => {
                    return Ok(McpServer::tool_error(ToolError::message(format!(
                        "finding {}: unknown severity '{other}'. Allowed values: ['blocker', 'major', 'minor', 'nit']",
                        index + 1
                    ))));
                }
            };
            parsed.push(ReviewFinding {
                class,
                severity,
                path: finding.path.clone().filter(|p| !p.trim().is_empty()),
                line: finding.line.clone().filter(|l| !l.trim().is_empty()),
                // Free text an agent sometimes writes with its own tool syntax
                // still attached, same as the planning fields.
                title: api_types::strip_agent_scaffolding(&finding.title),
                body: api_types::strip_agent_scaffolding(&finding.body),
            });
        }

        let url = self.url(&format!("/api/remote/issues/{}", issue_id));
        let existing: Issue = match self.send_json(self.client.get(&url)).await {
            Ok(issue) => issue,
            Err(e) => return Ok(McpServer::tool_error(e)),
        };

        let verdict = ReviewVerdict {
            approved: Some(approved),
            findings: parsed,
            recorded_at: Some(chrono::Utc::now()),
        };
        let fix_round_findings = verdict.fixable().count();
        let product_questions = verdict.product_questions().count();
        let findings_recorded = verdict.findings.len();

        // Write only the `review` key: extension_metadata carries keys other
        // tools own, and a typed round-trip of the whole blob would drop them.
        let update = UpdateIssueRequest {
            extension_metadata: Some(
                IssueExtMetadata {
                    review: Some(verdict),
                    ..Default::default()
                }
                .merge_into(&existing.extension_metadata),
            ),
            status_id: None,
            title: None,
            description: None,
            priority: None,
            start_date: None,
            target_date: None,
            completed_at: None,
            sort_order: None,
            parent_issue_id: None,
            parent_issue_sort_order: None,
        };
        if let Err(e) = self
            .send_json::<MutationResponse<Issue>>(self.client.patch(&url).json(&update))
            .await
        {
            return Ok(McpServer::tool_error(e));
        }

        McpServer::success(&McpRecordReviewResponse {
            issue_id: issue_id.to_string(),
            approved,
            findings: findings_recorded,
            fix_round_findings,
            product_questions,
        })
    }

    #[tool(
        description = "List knowledge documents for the project (permanent team-shared store; kind 'rca' = approved root-cause analyses). Returns metadata only — use kb_get_document for content."
    )]
    async fn kb_list_documents(
        &self,
        Parameters(McpListKbDocumentsRequest {
            kind,
            project_id,
            unlinked,
        }): Parameters<McpListKbDocumentsRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let project_id = match self.resolve_project_id(project_id).await {
            Ok(id) => id,
            Err(e) => return Ok(McpServer::tool_error(e)),
        };
        let mut url = self.url(&format!("/api/remote/kb_documents?project_id={project_id}"));
        if let Some(kind) = &kind {
            url.push_str(&format!("&kind={kind}"));
        }
        if unlinked == Some(true) {
            url.push_str("&unlinked=true");
        }
        let response: ListKbDocumentsResponse = match self.send_json(self.client.get(&url)).await {
            Ok(r) => r,
            Err(e) => return Ok(McpServer::tool_error(e)),
        };

        McpServer::success(&McpListKbDocumentsResponse {
            documents: response
                .documents
                .into_iter()
                .map(|doc| McpKbDocumentSummary {
                    document_id: doc.id.to_string(),
                    kind: doc.kind,
                    title: doc.title,
                    jira_key: doc.jira_key,
                    summary: doc.summary,
                })
                .collect(),
        })
    }

    #[tool(
        description = "Fetch one knowledge document with its full content (e.g. an approved RCA). `document_id` comes from kb_list_documents."
    )]
    async fn kb_get_document(
        &self,
        Parameters(McpGetKbDocumentRequest { document_id }): Parameters<McpGetKbDocumentRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let url = self.url(&format!("/api/remote/kb_documents/{document_id}"));
        let response: KbDocument = match self.send_json(self.client.get(&url)).await {
            Ok(r) => r,
            Err(e) => return Ok(McpServer::tool_error(e)),
        };
        McpServer::success(&response)
    }

    #[tool(
        description = "File a document written outside the orchestrator — an RCA from a local session — into the project's knowledge store and pin it to its Jira ticket, exactly as the orchestrator files its own RCAs. The orchestrator's graph analyst links it on its next run. Returns the document id and its URL. Filing the same ticket twice returns the first filing unchanged (filed: false): the store cannot replace a document that has no source issue, so post updates on the ticket instead."
    )]
    async fn kb_file_document(
        &self,
        Parameters(McpKbFileDocumentRequest {
            kind,
            title,
            body,
            jira_key,
            work_class,
            summary,
            tags,
            project_id,
        }): Parameters<McpKbFileDocumentRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let project_id = match self.resolve_project_id(project_id).await {
            Ok(id) => id,
            Err(e) => return Ok(McpServer::tool_error(e)),
        };
        let payload = api_types::FileKbDocumentRequest {
            project_id,
            kind,
            title,
            body,
            jira_key,
            work_class,
            summary,
            tags: tags.unwrap_or_default(),
        };
        let url = self.url("/api/remote/kb_documents/file");
        let response: api_types::FileKbDocumentResponse =
            match self.send_json(self.client.post(&url).json(&payload)).await {
                Ok(r) => r,
                Err(e) => return Ok(McpServer::tool_error(e)),
            };
        McpServer::success(&serde_json::json!({
            "document_id": response.document.id.to_string(),
            "filed": response.filed,
            "jira_linked": response.jira_linked,
            "url": response.url,
            "title": response.document.title,
            "tags": response.document.tags,
        }))
    }

    #[tool(
        description = "Add classification tags to a knowledge document (process metrics the team counts, e.g. origin:regression vs origin:dormant on RCAs). Idempotent; existing tags are kept."
    )]
    async fn kb_tag_document(
        &self,
        Parameters(McpKbTagDocumentRequest { document_id, tags }): Parameters<
            McpKbTagDocumentRequest,
        >,
    ) -> Result<CallToolResult, ErrorData> {
        if tags.is_empty() {
            return Ok(McpServer::tool_error(ToolError::message(
                "tags must not be empty",
            )));
        }
        let payload = api_types::AddKbDocumentTagsRequest { tags };
        let url = self.url(&format!("/api/remote/kb_documents/{document_id}/tags"));
        let response: KbDocument = match self.send_json(self.client.post(&url).json(&payload)).await
        {
            Ok(r) => r,
            Err(e) => return Ok(McpServer::tool_error(e)),
        };
        McpServer::success(&serde_json::json!({
            "document_id": response.id.to_string(),
            "tags": response.tags,
        }))
    }

    #[tool(
        description = "List the RCA-graph concept nodes for the project with their document counts (product areas, components, root-cause classes). Use before linking or creating nodes — attach to an existing node whenever plausible."
    )]
    async fn graph_nodes(
        &self,
        Parameters(McpGraphNodesRequest { project_id }): Parameters<McpGraphNodesRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let project_id = match self.resolve_project_id(project_id).await {
            Ok(id) => id,
            Err(e) => return Ok(McpServer::tool_error(e)),
        };
        let url = self.url(&format!("/api/remote/kb_nodes?project_id={project_id}"));
        let response: ListKbNodesResponse = match self.send_json(self.client.get(&url)).await {
            Ok(r) => r,
            Err(e) => return Ok(McpServer::tool_error(e)),
        };
        McpServer::success(&McpGraphNodesResponse {
            nodes: response
                .nodes
                .into_iter()
                .map(|node| McpGraphNode {
                    node_id: node.id.to_string(),
                    kind: node.kind,
                    name: node.name,
                    description: node.description,
                    edge_count: node.edge_count,
                    investigation_jira_key: node.investigation_jira_key,
                })
                .collect(),
        })
    }

    #[tool(
        description = "Create a new ROOT-CAUSE-CLASS node in the RCA graph. Only use when no existing node fits (check graph_nodes first) — node sprawl destroys the graph's value. Product-area and component nodes are seeded by the team and cannot be created here."
    )]
    async fn graph_create_node(
        &self,
        Parameters(McpGraphCreateNodeRequest {
            name,
            description,
            project_id,
        }): Parameters<McpGraphCreateNodeRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let project_id = match self.resolve_project_id(project_id).await {
            Ok(id) => id,
            Err(e) => return Ok(McpServer::tool_error(e)),
        };
        let payload = api_types::CreateKbNodeRequest {
            project_id,
            kind: "root_cause_class".to_string(),
            name,
            description,
            seeded: false,
        };
        let url = self.url("/api/remote/kb_nodes");
        let response: KbNode = match self.send_json(self.client.post(&url).json(&payload)).await {
            Ok(r) => r,
            Err(e) => return Ok(McpServer::tool_error(e)),
        };
        McpServer::success(&McpGraphNode {
            node_id: response.id.to_string(),
            kind: response.kind,
            name: response.name,
            description: response.description,
            edge_count: response.edge_count,
            investigation_jira_key: response.investigation_jira_key,
        })
    }

    #[tool(
        description = "Link an RCA document to a graph node (this RCA exhibits that area/cause). Idempotent per node+document. `rationale` (one line) and `confidence` (low|medium|high) are required."
    )]
    async fn graph_link(
        &self,
        Parameters(McpGraphLinkRequest {
            node_id,
            document_id,
            rationale,
            confidence,
            project_id,
        }): Parameters<McpGraphLinkRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let project_id = match self.resolve_project_id(project_id).await {
            Ok(id) => id,
            Err(e) => return Ok(McpServer::tool_error(e)),
        };
        if !["low", "medium", "high"].contains(&confidence.as_str()) {
            return Ok(McpServer::tool_error(ToolError::message(
                "confidence must be 'low', 'medium', or 'high'",
            )));
        }
        let payload = api_types::CreateKbEdgeRequest {
            project_id,
            node_id,
            document_id,
            rationale: Some(rationale),
            confidence: Some(confidence),
        };
        let url = self.url("/api/remote/kb_edges");
        let response: api_types::KbEdge =
            match self.send_json(self.client.post(&url).json(&payload)).await {
                Ok(r) => r,
                Err(e) => return Ok(McpServer::tool_error(e)),
            };
        McpServer::success(&serde_json::json!({
            "edge_id": response.id.to_string(),
            "node_id": response.node_id.to_string(),
            "document_id": response.document_id.to_string(),
        }))
    }

    #[tool(
        description = "Break an epic down: create tracker sub-tasks under the given epic and import each as a prepcook ticket (work_class subtask, linked via epic_key). Use during epic execution after the plan is approved. `epic_key` and `subtasks` are required."
    )]
    async fn tracker_create_subtasks(
        &self,
        Parameters(McpCreateTrackerSubtasksRequest {
            epic_key,
            subtasks,
            project_id,
        }): Parameters<McpCreateTrackerSubtasksRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let project_id = match self.resolve_project_id(project_id).await {
            Ok(id) => id,
            Err(e) => return Ok(McpServer::tool_error(e)),
        };
        if subtasks.is_empty() {
            return Ok(McpServer::tool_error(ToolError::message(
                "subtasks must not be empty",
            )));
        }
        if !self.tracker_configured().await {
            return McpServer::success(&no_ticket_result(
                NoTicket::NoTracker,
                "nothing created; make each sub-task a card with `create_issue` instead",
                serde_json::json!({
                    "created_jira_keys": [],
                    "imported_vk_issues": 0,
                    "failed_titles": [],
                }),
            ));
        }

        let payload = serde_json::json!({
            "project_id": project_id,
            "epic_key": epic_key,
            "subtasks": subtasks,
        });

        #[derive(Deserialize)]
        struct CreateSubtasksData {
            created_keys: Vec<String>,
            imported: u32,
            failed: Vec<String>,
        }

        let url = self.url("/api/tracker/create-subtasks");
        let response: CreateSubtasksData =
            match self.send_json(self.client.post(&url).json(&payload)).await {
                Ok(r) => r,
                Err(e) => return Ok(McpServer::tool_error(e)),
            };

        McpServer::success(&McpCreateTrackerSubtasksResponse {
            created_jira_keys: response.created_keys,
            imported_vk_issues: response.imported,
            failed_titles: response.failed,
        })
    }

    #[tool(
        description = "Deprecated name for `tracker_create_subtasks`; same parameters, same behaviour."
    )]
    async fn create_jira_subtasks(
        &self,
        params: Parameters<McpCreateTrackerSubtasksRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        self.tracker_create_subtasks(params).await
    }

    #[tool(
        description = "Download the tracker attachments of a ticket (screenshots, logs, HAR files) into ./jira-attachments/ in the current workspace so they can be read as local files. Uses the app's tracker credentials — no extra auth needed. Use this during bug investigation instead of reporting that attachments are inaccessible."
    )]
    async fn tracker_get_attachments(
        &self,
        Parameters(McpTrackerAttachmentsRequest { jira_key, issue_id }): Parameters<
            McpTrackerAttachmentsRequest,
        >,
    ) -> Result<CallToolResult, ErrorData> {
        let nothing = |reason| {
            McpServer::success(&no_ticket_result(
                reason,
                "nothing downloaded",
                serde_json::json!({ "downloaded_to": null, "files": [] }),
            ))
        };
        if !self.tracker_configured().await {
            return nothing(NoTicket::NoTracker);
        }
        // Resolve the Jira key: explicit param, else the linked issue's
        // imported-from-Jira metadata.
        let jira_key = match jira_key
            .map(|k| k.trim().to_string())
            .filter(|k| !k.is_empty())
        {
            Some(key) => key,
            None => {
                let issue_id = match self.resolve_issue_id(issue_id).await {
                    Ok(id) => id,
                    // Keep the resolver's wording: "not linked" and "board
                    // unreachable" call for different reactions from the agent.
                    Err(e) => return Ok(McpServer::tool_error(e)),
                };
                let url = self.url(&format!("/api/remote/issues/{}", issue_id));
                let issue: Issue = match self.send_json(self.client.get(&url)).await {
                    Ok(issue) => issue,
                    Err(e) => return Ok(McpServer::tool_error(e)),
                };
                match IssueExtMetadata::from_value(&issue.extension_metadata)
                    .jira
                    .map(|jira| jira.key)
                {
                    Some(key) => key,
                    None => return nothing(NoTicket::BoardOnlyCard),
                }
            }
        };

        // Files land in the agent's own worktree: the MCP server runs with
        // the workspace as cwd, and the app server shares its filesystem.
        let dest_dir = match std::env::current_dir() {
            Ok(cwd) => cwd.join("jira-attachments"),
            Err(e) => {
                return Ok(McpServer::tool_error(ToolError::new(
                    "Could not resolve the workspace directory",
                    Some(e.to_string()),
                )));
            }
        };

        let url = self.url("/api/tracker/attachments");
        let payload = serde_json::json!({
            "issue_key": jira_key,
            "dest_dir": dest_dir.to_string_lossy(),
        });
        let files: Vec<McpTrackerAttachmentFile> =
            match self.send_json(self.client.post(&url).json(&payload)).await {
                Ok(files) => files,
                Err(e) => return Ok(McpServer::tool_error(e)),
            };

        McpServer::success(&McpTrackerAttachmentsResponse {
            downloaded_to: dest_dir.to_string_lossy().to_string(),
            files,
        })
    }

    #[tool(
        description = "Deprecated name for `tracker_get_attachments`; same parameters, same behaviour."
    )]
    async fn jira_get_attachments(
        &self,
        params: Parameters<McpTrackerAttachmentsRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        self.tracker_get_attachments(params).await
    }

    #[tool(
        description = "Post a plain-text comment on a tracker ticket with the app's tracker credentials — answer people where they asked instead of only on the board. Use for triage verdicts, RCA summaries for non-engineers, and status answers on reopened tickets. Write it for the people who read Jira: whoever filed the ticket, their account manager, support. Say what happened, who it was visible to, what changes, and — when something is needed from them — exactly what, in one sentence, first. No file or class names, no branch names, no merge request numbers, no ticket keys but this one, and a product word wherever one exists: a template's dynamic content, not the DC resolver. If you cannot say it without them, say it more simply. The board is where you write for engineers."
    )]
    async fn tracker_add_comment(
        &self,
        Parameters(McpTrackerAddCommentRequest {
            jira_key,
            issue_id,
            body,
        }): Parameters<McpTrackerAddCommentRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        if body.trim().is_empty() {
            return Ok(McpServer::tool_error(ToolError::message(
                "Comment body is empty",
            )));
        }
        let nothing = |reason| {
            McpServer::success(&no_ticket_result(
                reason,
                "nothing posted",
                serde_json::json!({ "posted": false }),
            ))
        };
        if !self.tracker_configured().await {
            return nothing(NoTicket::NoTracker);
        }
        // Resolve the key exactly like tracker_get_attachments: explicit
        // param, else the linked issue's imported-from-Jira metadata.
        let jira_key = match jira_key
            .map(|k| k.trim().to_string())
            .filter(|k| !k.is_empty())
        {
            Some(key) => key,
            None => {
                let issue_id = match self.resolve_issue_id(issue_id).await {
                    Ok(id) => id,
                    Err(e) => return Ok(McpServer::tool_error(e)),
                };
                let url = self.url(&format!("/api/remote/issues/{}", issue_id));
                let issue: Issue = match self.send_json(self.client.get(&url)).await {
                    Ok(issue) => issue,
                    Err(e) => return Ok(McpServer::tool_error(e)),
                };
                match IssueExtMetadata::from_value(&issue.extension_metadata)
                    .jira
                    .map(|jira| jira.key)
                {
                    Some(key) => key,
                    None => return nothing(NoTicket::BoardOnlyCard),
                }
            }
        };

        let url = self.url("/api/tracker/comment");
        let payload = serde_json::json!({ "issue_key": jira_key, "body": body });
        // `send_empty_json`, not `send_json`: this route answers
        // `{"success":true,"data":null}`, and serde reads a null `data` as
        // `None`, which `send_json` rejects as "missing data field". Every
        // successful post therefore reported failure, and agents — reasonably —
        // retried, so a single answer landed on the ticket four times.
        match self
            .send_empty_json(self.client.post(&url).json(&payload))
            .await
        {
            Ok(()) => McpServer::success(&serde_json::json!({
                "posted": true,
                "issue_key": jira_key,
            })),
            Err(e) => Ok(McpServer::tool_error(e)),
        }
    }

    #[tool(
        description = "Deprecated name for `tracker_add_comment`; same parameters, same behaviour."
    )]
    async fn jira_add_comment(
        &self,
        params: Parameters<McpTrackerAddCommentRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        self.tracker_add_comment(params).await
    }

    #[tool(
        description = "Read a Slack thread as a transcript (uses the app's bot token — no extra auth needed). Useful when a ticket or escalation references a Slack discussion. channel defaults to the team's escalation channel."
    )]
    async fn slack_read_thread(
        &self,
        Parameters(McpSlackThreadRequest { channel, thread_ts }): Parameters<McpSlackThreadRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let url = self.url("/api/slack/thread");
        let mut request = self
            .client
            .get(&url)
            .query(&[("thread_ts", thread_ts.as_str())]);
        if let Some(channel) = channel.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
            request = request.query(&[("channel", channel)]);
        }
        let messages: Vec<McpSlackThreadMessage> = match self.send_json(request).await {
            Ok(messages) => messages,
            Err(e) => return Ok(McpServer::tool_error(e)),
        };

        let transcript = messages
            .iter()
            .map(|msg| {
                let author = if msg.is_bot {
                    "bot".to_string()
                } else {
                    msg.user.clone().unwrap_or_else(|| "user".to_string())
                };
                format!("[{}] {}: {}", msg.ts, author, msg.text)
            })
            .collect::<Vec<_>>()
            .join("\n");

        McpServer::success(&McpSlackThreadResponse {
            message_count: messages.len(),
            transcript,
        })
    }

    #[tool(description = "Delete an issue. `issue_id` is required.")]
    async fn delete_issue(
        &self,
        Parameters(McpDeleteIssueRequest { issue_id }): Parameters<McpDeleteIssueRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let url = self.url(&format!("/api/remote/issues/{}", issue_id));
        if let Err(e) = self.send_empty_json(self.client.delete(&url)).await {
            return Ok(McpServer::tool_error(e));
        }

        McpServer::success(&McpDeleteIssueResponse {
            deleted_issue_id: Some(issue_id.to_string()),
        })
    }
}

impl McpServer {
    fn parse_issue_sort_field(sort_field: Option<&str>) -> Result<IssueSortField, ToolError> {
        match sort_field
            .unwrap_or("sort_order")
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "sort_order" => Ok(IssueSortField::SortOrder),
            "priority" => Ok(IssueSortField::Priority),
            "created_at" => Ok(IssueSortField::CreatedAt),
            "updated_at" => Ok(IssueSortField::UpdatedAt),
            "title" => Ok(IssueSortField::Title),
            other => Err(ToolError::message(format!(
                "Unknown sort_field '{}'. Allowed values: ['sort_order', 'priority', 'created_at', 'updated_at', 'title']",
                other
            ))),
        }
    }

    fn parse_sort_direction(sort_direction: Option<&str>) -> Result<SortDirection, ToolError> {
        match sort_direction
            .unwrap_or("asc")
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "asc" => Ok(SortDirection::Asc),
            "desc" => Ok(SortDirection::Desc),
            other => Err(ToolError::message(format!(
                "Unknown sort_direction '{}'. Allowed values: ['asc', 'desc']",
                other
            ))),
        }
    }

    fn issue_to_summary(
        &self,
        issue: &Issue,
        status_names_by_id: Option<&HashMap<Uuid, String>>,
        pull_requests: &ListPullRequestsResponse,
    ) -> IssueSummary {
        let status = status_names_by_id
            .and_then(|status_map| status_map.get(&issue.status_id).cloned())
            .unwrap_or_else(|| issue.status_id.to_string());
        let latest_pr = pull_requests.pull_requests.first();
        IssueSummary {
            id: issue.id.to_string(),
            title: issue.title.clone(),
            simple_id: issue.simple_id.clone(),
            status,
            priority: issue
                .priority
                .map(Self::issue_priority_label)
                .map(str::to_string),
            parent_issue_id: issue.parent_issue_id.map(|id| id.to_string()),
            created_at: issue.created_at.to_rfc3339(),
            updated_at: issue.updated_at.to_rfc3339(),
            pull_request_count: pull_requests.pull_requests.len(),
            latest_pr_url: latest_pr.map(|pr| pr.url.clone()),
            latest_pr_status: latest_pr.map(|pr| pr.status),
        }
    }

    async fn issue_to_details(
        &self,
        issue: &Issue,
        pull_requests: ListPullRequestsResponse,
    ) -> IssueDetails {
        let status = self
            .resolve_status_name(issue.project_id, issue.status_id)
            .await;

        let tags = self
            .fetch_issue_tags_resolved(issue.project_id, issue.id)
            .await;

        let relationships = self
            .fetch_issue_relationships_resolved(issue.project_id, issue.id)
            .await;

        let sub_issues = self.fetch_sub_issues(issue.project_id, issue.id).await;

        IssueDetails {
            id: issue.id.to_string(),
            title: issue.title.clone(),
            simple_id: issue.simple_id.clone(),
            description: issue.description.clone(),
            status,
            status_id: issue.status_id.to_string(),
            priority: issue
                .priority
                .map(Self::issue_priority_label)
                .map(str::to_string),
            parent_issue_id: issue.parent_issue_id.map(|id| id.to_string()),
            start_date: issue.start_date.map(|date| date.to_rfc3339()),
            target_date: issue.target_date.map(|date| date.to_rfc3339()),
            completed_at: issue.completed_at.map(|date| date.to_rfc3339()),
            created_at: issue.created_at.to_rfc3339(),
            updated_at: issue.updated_at.to_rfc3339(),
            pull_requests: pull_requests
                .pull_requests
                .into_iter()
                .map(|pr| PullRequestSummary {
                    number: pr.number,
                    url: pr.url,
                    status: pr.status,
                    merged_at: pr.merged_at.map(|dt| dt.to_rfc3339()),
                    target_branch_name: pr.target_branch_name,
                })
                .collect(),
            tags,
            relationships,
            sub_issues,
        }
    }

    async fn fetch_pull_requests(&self, issue_id: Uuid) -> ListPullRequestsResponse {
        let url = self.url(&format!("/api/remote/pull-requests?issue_id={}", issue_id));
        match self
            .send_json::<ListPullRequestsResponse>(self.client.get(&url))
            .await
        {
            Ok(response) => response,
            Err(_) => ListPullRequestsResponse {
                pull_requests: vec![],
            },
        }
    }

    /// Fetches tags for an issue, resolving tag_ids to names via project tags.
    async fn fetch_issue_tags_resolved(
        &self,
        project_id: Uuid,
        issue_id: Uuid,
    ) -> Vec<McpTagSummary> {
        let tags_url = self.url(&format!("/api/remote/tags?project_id={}", project_id));
        let project_tags: ListTagsResponse = match self.send_json(self.client.get(&tags_url)).await
        {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };
        let tag_map: HashMap<Uuid, &api_types::Tag> =
            project_tags.tags.iter().map(|t| (t.id, t)).collect();

        let url = self.url(&format!("/api/remote/issue-tags?issue_id={}", issue_id));
        let response: ListIssueTagsResponse = match self.send_json(self.client.get(&url)).await {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };

        response
            .issue_tags
            .iter()
            .filter_map(|it| {
                tag_map.get(&it.tag_id).map(|tag| McpTagSummary {
                    id: tag.id.to_string(),
                    name: tag.name.clone(),
                    color: tag.color.clone(),
                })
            })
            .collect()
    }

    /// Fetches relationships for an issue, resolving related issue simple_ids.
    async fn fetch_issue_relationships_resolved(
        &self,
        project_id: Uuid,
        issue_id: Uuid,
    ) -> Vec<McpRelationshipSummary> {
        let rel_url = self.url(&format!(
            "/api/remote/issue-relationships?issue_id={}",
            issue_id
        ));
        let response: ListIssueRelationshipsResponse =
            match self.send_json(self.client.get(&rel_url)).await {
                Ok(r) => r,
                Err(_) => return Vec::new(),
            };

        if response.issue_relationships.is_empty() {
            return Vec::new();
        }

        let issues_url = self.url(&format!("/api/remote/issues?project_id={}", project_id));
        let issues_response: api_types::ListIssuesResponse = self
            .send_json(self.client.get(&issues_url))
            .await
            .unwrap_or(api_types::ListIssuesResponse {
                issues: Vec::new(),
                total_count: 0,
                limit: 0,
                offset: 0,
            });
        let simple_id_map: HashMap<Uuid, &str> = issues_response
            .issues
            .iter()
            .map(|i| (i.id, i.simple_id.as_str()))
            .collect();

        response
            .issue_relationships
            .into_iter()
            .map(|r| {
                let related_simple_id = simple_id_map
                    .get(&r.related_issue_id)
                    .unwrap_or(&"")
                    .to_string();
                McpRelationshipSummary {
                    id: r.id.to_string(),
                    related_issue_id: r.related_issue_id.to_string(),
                    related_simple_id,
                    relationship_type: match r.relationship_type {
                        IssueRelationshipType::Blocking => "blocking".to_string(),
                        IssueRelationshipType::Related => "related".to_string(),
                        IssueRelationshipType::HasDuplicate => "has_duplicate".to_string(),
                    },
                }
            })
            .collect()
    }

    /// Fetches sub-issues for a given parent issue.
    async fn fetch_sub_issues(
        &self,
        project_id: Uuid,
        parent_issue_id: Uuid,
    ) -> Vec<McpSubIssueSummary> {
        let url = self.url(&format!("/api/remote/issues?project_id={}", project_id));
        let response: api_types::ListIssuesResponse =
            match self.send_json(self.client.get(&url)).await {
                Ok(r) => r,
                Err(_) => return Vec::new(),
            };

        let status_names = self
            .fetch_project_statuses(project_id)
            .await
            .ok()
            .map(|statuses| {
                statuses
                    .into_iter()
                    .map(|s| (s.id, s.name))
                    .collect::<HashMap<_, _>>()
            });

        response
            .issues
            .iter()
            .filter(|i| i.parent_issue_id == Some(parent_issue_id))
            .map(|i| {
                let status = status_names
                    .as_ref()
                    .and_then(|m| m.get(&i.status_id).cloned())
                    .unwrap_or_else(|| i.status_id.to_string());
                McpSubIssueSummary {
                    id: i.id.to_string(),
                    simple_id: i.simple_id.clone(),
                    title: i.title.clone(),
                    status,
                }
            })
            .collect()
    }

    fn parse_confidence_level(confidence: &str) -> Result<ConfidenceLevel, ToolError> {
        match confidence.trim().to_ascii_lowercase().as_str() {
            "low" => Ok(ConfidenceLevel::Low),
            "medium" => Ok(ConfidenceLevel::Medium),
            "high" => Ok(ConfidenceLevel::High),
            _ => Err(ToolError::message(format!(
                "Unknown planning_confidence '{}'. Allowed values: ['low', 'medium', 'high']",
                confidence
            ))),
        }
    }

    fn parse_issue_priority(priority: &str) -> Result<IssuePriority, ToolError> {
        match priority.trim().to_ascii_lowercase().as_str() {
            "urgent" => Ok(IssuePriority::Urgent),
            "high" => Ok(IssuePriority::High),
            "medium" => Ok(IssuePriority::Medium),
            "low" => Ok(IssuePriority::Low),
            _ => Err(ToolError::message(format!(
                "Unknown priority '{}'. Allowed values: ['urgent', 'high', 'medium', 'low']",
                priority
            ))),
        }
    }

    fn issue_priority_label(priority: IssuePriority) -> &'static str {
        match priority {
            IssuePriority::Urgent => "urgent",
            IssuePriority::High => "high",
            IssuePriority::Medium => "medium",
            IssuePriority::Low => "low",
        }
    }

    async fn find_tag_ids_by_name(
        &self,
        project_id: Uuid,
        tag_name: &str,
    ) -> Result<Vec<Uuid>, ToolError> {
        let url = self.url(&format!("/api/remote/tags?project_id={}", project_id));
        let tags: ListTagsResponse = self.send_json(self.client.get(&url)).await?;
        Ok(Self::matching_ids_by_name(
            tags.tags.iter().map(|tag| (tag.id, tag.name.as_str())),
            tag_name,
        ))
    }

    fn matching_ids_by_name<'a>(
        items: impl IntoIterator<Item = (Uuid, &'a str)>,
        name: &str,
    ) -> Vec<Uuid> {
        items
            .into_iter()
            .filter(|(_, item_name)| item_name.eq_ignore_ascii_case(name))
            .map(|(id, _)| id)
            .collect()
    }

    fn resolve_tag_filters(
        tag_id: Option<Uuid>,
        matching_tag_ids: Option<Vec<Uuid>>,
    ) -> (Option<Uuid>, Option<Vec<Uuid>>, bool) {
        match (tag_id, matching_tag_ids) {
            (Some(tag_id), Some(matching_tag_ids)) => {
                if matching_tag_ids.contains(&tag_id) {
                    (Some(tag_id), None, false)
                } else {
                    (None, None, true)
                }
            }
            (None, Some(matching_tag_ids)) => {
                let missing_tag_name_match = matching_tag_ids.is_empty();
                (
                    None,
                    (!missing_tag_name_match).then_some(matching_tag_ids),
                    missing_tag_name_match,
                )
            }
            (Some(tag_id), None) => (Some(tag_id), None, false),
            (None, None) => (None, None, false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Answers every request with `body` as a 200 JSON response, the way the
    /// app server answers `GET /api/tracker/status`.
    async fn serve_json(body: &'static str) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let mut request = [0u8; 4096];
                let _ = socket.read(&mut request).await;
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\
                     content-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = socket.write_all(response.as_bytes()).await;
            }
        });
        format!("http://{address}")
    }

    fn result_text(result: &CallToolResult) -> String {
        serde_json::to_string(&result.content).unwrap()
    }

    #[test]
    fn a_missing_ticket_is_a_success_that_says_why() {
        let result = no_ticket_result(
            NoTicket::NoTracker,
            "nothing posted",
            serde_json::json!({ "posted": false }),
        );
        assert_eq!(result["posted"], false);
        assert_eq!(
            result["note"],
            "no tracker is configured; the card is the only record — nothing posted"
        );
        let board_only = no_ticket_result(
            NoTicket::BoardOnlyCard,
            "nothing downloaded",
            serde_json::json!({ "files": [] }),
        );
        assert!(
            board_only["note"]
                .as_str()
                .unwrap()
                .starts_with("this card has no tracker ticket")
        );
    }

    /// The tools ask the app first, so a session told to "post the
    /// reproduction on the ticket too" carries on when there is no tracker.
    #[tokio::test]
    async fn tracker_tools_answer_plainly_when_no_tracker_is_configured() {
        super::super::tests::install_rustls_provider();
        let base =
            serve_json(r#"{"success":true,"data":{"configured":false,"kind":"Jira"}}"#).await;
        let server = McpServer::new_global(&base);

        let comment = server
            .tracker_add_comment(Parameters(McpTrackerAddCommentRequest {
                jira_key: Some("BUG-1".to_string()),
                issue_id: None,
                body: "Reproduced on the reported version.".to_string(),
            }))
            .await
            .unwrap();
        assert_ne!(comment.is_error, Some(true));
        let text = result_text(&comment);
        assert!(text.contains("no tracker is configured"), "{text}");
        assert!(text.contains("nothing posted"), "{text}");

        let attachments = server
            .tracker_get_attachments(Parameters(McpTrackerAttachmentsRequest {
                jira_key: Some("BUG-1".to_string()),
                issue_id: None,
            }))
            .await
            .unwrap();
        assert_ne!(attachments.is_error, Some(true));
        assert!(result_text(&attachments).contains("nothing downloaded"));
    }

    #[test]
    fn collects_all_matching_status_ids_case_insensitively() {
        let first_id = Uuid::new_v4();
        let second_id = Uuid::new_v4();
        let statuses = [
            (first_id, "In Progress"),
            (second_id, "in progress"),
            (Uuid::new_v4(), "Todo"),
        ];

        assert_eq!(
            McpServer::matching_ids_by_name(statuses, "IN PROGRESS"),
            vec![first_id, second_id]
        );
    }

    #[test]
    fn collects_all_matching_tag_ids_case_insensitively() {
        let first_id = Uuid::new_v4();
        let second_id = Uuid::new_v4();
        let tags = [
            (first_id, "bug"),
            (second_id, "Bug"),
            (Uuid::new_v4(), "feature"),
        ];

        assert_eq!(
            McpServer::matching_ids_by_name(tags, "BUG"),
            vec![first_id, second_id]
        );
    }

    #[test]
    fn resolve_tag_filters_requires_explicit_tag_id_to_match_tag_name() {
        let tag_id = Uuid::new_v4();
        let other_tag_id = Uuid::new_v4();

        assert_eq!(
            McpServer::resolve_tag_filters(Some(tag_id), Some(vec![other_tag_id])),
            (None, None, true)
        );
    }

    #[test]
    fn resolve_tag_filters_preserves_exact_tag_id_intersection() {
        let tag_id = Uuid::new_v4();
        let other_tag_id = Uuid::new_v4();

        assert_eq!(
            McpServer::resolve_tag_filters(Some(tag_id), Some(vec![other_tag_id, tag_id])),
            (Some(tag_id), None, false)
        );
    }
}
