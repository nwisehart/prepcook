use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

/// A knowledge document: permanent, team-shared context. First consumer is
/// the RCA store (kind = "rca", filed automatically when a bug's RCA is
/// approved); the shape is generic so runbooks/decisions/prompts can reuse
/// it (see docs/plans/rca-knowledge-graph-plan.md).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct KbDocument {
    pub id: Uuid,
    pub project_id: Uuid,
    /// Document kind: "rca" today; "doc" | "decision" | "prompt" later.
    pub kind: String,
    pub title: String,
    /// Full document content as authored (RCAs are self-contained HTML).
    pub body: String,
    /// Board issue this document was filed from, when applicable.
    pub issue_id: Option<Uuid>,
    pub jira_key: Option<String>,
    pub work_class: Option<String>,
    /// One-line summary (the planning verdict summary for RCAs).
    pub summary: Option<String>,
    /// Classification tags ("namespace:value" convention) — e.g.
    /// origin:regression vs origin:dormant, and culprit:orchestrator vs
    /// culprit:human (who authored the offending commit) on regression RCAs.
    /// Team process metrics, not graph analysis.
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CreateKbDocumentRequest {
    pub project_id: Uuid,
    pub kind: String,
    pub title: String,
    pub body: String,
    #[ts(optional)]
    pub issue_id: Option<Uuid>,
    #[ts(optional)]
    pub jira_key: Option<String>,
    #[ts(optional)]
    pub work_class: Option<String>,
    #[ts(optional)]
    pub summary: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct AddKbDocumentTagsRequest {
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListKbDocumentsQuery {
    pub project_id: Uuid,
    pub kind: Option<String>,
    /// When true, only documents with no graph edges yet (pending analysis).
    #[serde(default)]
    pub unlinked: Option<bool>,
}

/// Stamps a node with its investigation story (threshold fired). Records
/// the edge count at fire time so the re-arm math has a baseline.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct StampKbNodeInvestigationRequest {
    pub jira_key: String,
    pub edge_count: i32,
}

/// List entries deliberately exclude `body` (RCA documents are large);
/// fetch a single document for content.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct KbDocumentSummary {
    pub id: Uuid,
    pub project_id: Uuid,
    pub kind: String,
    pub title: String,
    pub issue_id: Option<Uuid>,
    pub jira_key: Option<String>,
    pub work_class: Option<String>,
    pub summary: Option<String>,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ListKbDocumentsResponse {
    pub documents: Vec<KbDocumentSummary>,
}

/// A concept node in the RCA graph. `product_area`/`component` nodes are
/// seeded by the team; `root_cause_class` nodes are invented by the
/// analysis agent when no existing node fits.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct KbNode {
    pub id: Uuid,
    pub project_id: Uuid,
    /// product_area | component | root_cause_class | data_model | infra
    pub kind: String,
    pub name: String,
    pub description: Option<String>,
    pub seeded: bool,
    /// Jira key of the investigation story, once the threshold fired (K3).
    pub investigation_jira_key: Option<String>,
    pub investigation_edge_count: Option<i32>,
    /// Number of documents currently linked to this node.
    pub edge_count: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CreateKbNodeRequest {
    pub project_id: Uuid,
    pub kind: String,
    pub name: String,
    #[ts(optional)]
    pub description: Option<String>,
    #[serde(default)]
    pub seeded: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListKbNodesQuery {
    pub project_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ListKbNodesResponse {
    pub nodes: Vec<KbNode>,
}

/// A link between a graph node and a document (an RCA exhibiting that
/// area/cause). Unique per node/document pair — re-linking refreshes.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct KbEdge {
    pub id: Uuid,
    pub project_id: Uuid,
    pub node_id: Uuid,
    pub document_id: Uuid,
    pub rationale: Option<String>,
    /// low | medium | high
    pub confidence: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CreateKbEdgeRequest {
    pub project_id: Uuid,
    pub node_id: Uuid,
    pub document_id: Uuid,
    #[ts(optional)]
    pub rationale: Option<String>,
    #[ts(optional)]
    pub confidence: Option<String>,
}

/// Edge listing supports either side: all links of a document, or all
/// documents of a node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListKbEdgesQuery {
    pub project_id: Uuid,
    pub node_id: Option<Uuid>,
    pub document_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ListKbEdgesResponse {
    pub edges: Vec<KbEdge>,
}

/// Files a document authored outside the board — an engineer's local RCA —
/// the way the board files its own: into the store, pinned to its Jira
/// ticket. Not exported to the web client; the app server and the MCP are
/// its only speakers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileKbDocumentRequest {
    pub project_id: Uuid,
    /// "rca" is the kind the knowledge graph links.
    pub kind: String,
    pub title: String,
    /// Full document content as authored (an RCA is self-contained HTML).
    pub body: String,
    /// The ticket the document is about. Required: it is the pin, and it is
    /// how a second filing of the same ticket is recognised.
    pub jira_key: String,
    pub work_class: Option<String>,
    pub summary: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileKbDocumentResponse {
    pub document: KbDocument,
    /// False when a document for this ticket was already in the store and
    /// came back unchanged: a document with no source issue cannot be
    /// refreshed in place, so the first filing stands.
    pub filed: bool,
    /// Whether the Jira remote link was set on this call.
    pub jira_linked: bool,
    /// Where the document is read: the board's HTML view of it.
    pub url: String,
}
