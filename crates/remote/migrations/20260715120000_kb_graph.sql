-- The RCA knowledge graph: concept nodes and their links to documents.
-- Nodes of kind product_area/component are SEEDED by the team; the analysis
-- agent may only invent root_cause_class nodes (enforced at the MCP layer).
CREATE TABLE kb_nodes (
    id UUID PRIMARY KEY,
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    -- product_area | component | root_cause_class | data_model | infra
    kind TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    seeded BOOLEAN NOT NULL DEFAULT false,
    -- Set when the investigation threshold fired for this node (K3).
    investigation_jira_key TEXT,
    investigation_edge_count INTEGER,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX kb_nodes_project_kind_name
    ON kb_nodes (project_id, kind, lower(name));

CREATE TABLE kb_edges (
    id UUID PRIMARY KEY,
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    node_id UUID NOT NULL REFERENCES kb_nodes(id) ON DELETE CASCADE,
    document_id UUID NOT NULL REFERENCES kb_documents(id) ON DELETE CASCADE,
    rationale TEXT,
    -- low | medium | high
    confidence TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- One edge per node/document pair; re-analysis refreshes instead of stacking.
CREATE UNIQUE INDEX kb_edges_node_document ON kb_edges (node_id, document_id);
CREATE INDEX kb_edges_document ON kb_edges (document_id);
