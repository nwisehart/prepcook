use api_types::{CreateKbEdgeRequest, CreateKbNodeRequest, KbEdge, KbNode};
use sqlx::PgPool;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum KbGraphError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

pub struct KbGraphRepository;

impl KbGraphRepository {
    /// Nodes with their live document-edge counts, largest threads first.
    pub async fn list_nodes(pool: &PgPool, project_id: Uuid) -> Result<Vec<KbNode>, KbGraphError> {
        let records = sqlx::query_as!(
            KbNode,
            r#"
            SELECT
                n.id          AS "id!: Uuid",
                n.project_id  AS "project_id!: Uuid",
                n.kind        AS "kind!",
                n.name        AS "name!",
                n.description,
                n.seeded      AS "seeded!",
                n.investigation_jira_key,
                n.investigation_edge_count,
                COUNT(e.id)   AS "edge_count!",
                n.created_at  AS "created_at!"
            FROM kb_nodes n
            LEFT JOIN kb_edges e ON e.node_id = n.id
            WHERE n.project_id = $1
            GROUP BY n.id
            ORDER BY COUNT(e.id) DESC, n.name
            "#,
            project_id
        )
        .fetch_all(pool)
        .await?;

        Ok(records)
    }

    /// Idempotent per (project, kind, lower(name)): re-creating an existing
    /// node returns it (description refreshed when provided).
    pub async fn upsert_node(
        pool: &PgPool,
        request: &CreateKbNodeRequest,
    ) -> Result<KbNode, KbGraphError> {
        let id = Uuid::new_v4();
        let record = sqlx::query_as!(
            KbNode,
            r#"
            INSERT INTO kb_nodes (id, project_id, kind, name, description, seeded)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (project_id, kind, lower(name)) DO UPDATE SET
                description = COALESCE(EXCLUDED.description, kb_nodes.description)
            RETURNING
                id          AS "id!: Uuid",
                project_id  AS "project_id!: Uuid",
                kind        AS "kind!",
                name        AS "name!",
                description,
                seeded      AS "seeded!",
                investigation_jira_key,
                investigation_edge_count,
                0::bigint   AS "edge_count!",
                created_at  AS "created_at!"
            "#,
            id,
            request.project_id,
            request.kind,
            request.name,
            request.description.as_deref(),
            request.seeded
        )
        .fetch_one(pool)
        .await?;

        Ok(record)
    }

    /// Stamps a node with its investigation story; returns None when the
    /// node doesn't exist.
    pub async fn stamp_investigation(
        pool: &PgPool,
        id: Uuid,
        jira_key: &str,
        edge_count: i32,
    ) -> Result<Option<KbNode>, KbGraphError> {
        let record = sqlx::query_as!(
            KbNode,
            r#"
            UPDATE kb_nodes
            SET investigation_jira_key = $2,
                investigation_edge_count = $3
            WHERE id = $1
            RETURNING
                id          AS "id!: Uuid",
                project_id  AS "project_id!: Uuid",
                kind        AS "kind!",
                name        AS "name!",
                description,
                seeded      AS "seeded!",
                investigation_jira_key,
                investigation_edge_count,
                (SELECT COUNT(*) FROM kb_edges e WHERE e.node_id = kb_nodes.id) AS "edge_count!",
                created_at  AS "created_at!"
            "#,
            id,
            jira_key,
            edge_count
        )
        .fetch_optional(pool)
        .await?;

        Ok(record)
    }

    pub async fn list_edges(
        pool: &PgPool,
        project_id: Uuid,
        node_id: Option<Uuid>,
        document_id: Option<Uuid>,
    ) -> Result<Vec<KbEdge>, KbGraphError> {
        let records = sqlx::query_as!(
            KbEdge,
            r#"
            SELECT
                id          AS "id!: Uuid",
                project_id  AS "project_id!: Uuid",
                node_id     AS "node_id!: Uuid",
                document_id AS "document_id!: Uuid",
                rationale,
                confidence,
                created_at  AS "created_at!"
            FROM kb_edges
            WHERE project_id = $1
              AND ($2::uuid IS NULL OR node_id = $2)
              AND ($3::uuid IS NULL OR document_id = $3)
            ORDER BY created_at DESC
            "#,
            project_id,
            node_id,
            document_id
        )
        .fetch_all(pool)
        .await?;

        Ok(records)
    }

    /// Idempotent per (node, document): re-linking refreshes the rationale
    /// and confidence instead of duplicating.
    pub async fn upsert_edge(
        pool: &PgPool,
        request: &CreateKbEdgeRequest,
    ) -> Result<KbEdge, KbGraphError> {
        let id = Uuid::new_v4();
        let record = sqlx::query_as!(
            KbEdge,
            r#"
            INSERT INTO kb_edges (id, project_id, node_id, document_id, rationale, confidence)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (node_id, document_id) DO UPDATE SET
                rationale = COALESCE(EXCLUDED.rationale, kb_edges.rationale),
                confidence = COALESCE(EXCLUDED.confidence, kb_edges.confidence)
            RETURNING
                id          AS "id!: Uuid",
                project_id  AS "project_id!: Uuid",
                node_id     AS "node_id!: Uuid",
                document_id AS "document_id!: Uuid",
                rationale,
                confidence,
                created_at  AS "created_at!"
            "#,
            id,
            request.project_id,
            request.node_id,
            request.document_id,
            request.rationale.as_deref(),
            request.confidence.as_deref()
        )
        .fetch_one(pool)
        .await?;

        Ok(record)
    }
}
