-- Knowledge documents: the permanent, team-shared context store (first
-- consumer: RCA documents filed automatically at spec approval). Generic by
-- design — `kind` distinguishes rca / doc / decision / prompt so later
-- Knowledge-layer work reuses the table.
CREATE TABLE kb_documents (
    id UUID PRIMARY KEY,
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    title TEXT NOT NULL,
    body TEXT NOT NULL,
    -- Source linkage for documents born from board issues (RCAs).
    issue_id UUID REFERENCES issues(id) ON DELETE SET NULL,
    jira_key TEXT,
    work_class TEXT,
    summary TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- One document per source issue: filing is idempotent (re-approval after
-- feedback updates the body instead of duplicating).
CREATE UNIQUE INDEX kb_documents_issue_unique
    ON kb_documents (issue_id)
    WHERE issue_id IS NOT NULL;

CREATE INDEX kb_documents_project_kind ON kb_documents (project_id, kind);
