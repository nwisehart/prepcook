use api_types::{CreateKbDocumentRequest, KbDocument, KbDocumentSummary};
use sqlx::PgPool;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum KbDocumentError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

pub struct KbDocumentRepository;

impl KbDocumentRepository {
    pub async fn find_by_id(
        pool: &PgPool,
        id: Uuid,
    ) -> Result<Option<KbDocument>, KbDocumentError> {
        let record = sqlx::query_as!(
            KbDocument,
            r#"
            SELECT
                id          AS "id!: Uuid",
                project_id  AS "project_id!: Uuid",
                kind        AS "kind!",
                title       AS "title!",
                body        AS "body!",
                issue_id    AS "issue_id: Uuid",
                jira_key,
                work_class,
                summary,
                tags        AS "tags!",
                created_at  AS "created_at!",
                updated_at  AS "updated_at!"
            FROM kb_documents
            WHERE id = $1
            "#,
            id
        )
        .fetch_optional(pool)
        .await?;

        Ok(record)
    }

    /// List without bodies (RCA documents are large); optional kind filter.
    /// `unlinked_only` restricts to documents with no graph edges yet
    /// (pending analysis).
    pub async fn list_by_project(
        pool: &PgPool,
        project_id: Uuid,
        kind: Option<&str>,
        unlinked_only: bool,
    ) -> Result<Vec<KbDocumentSummary>, KbDocumentError> {
        let records = sqlx::query_as!(
            KbDocumentSummary,
            r#"
            SELECT
                id          AS "id!: Uuid",
                project_id  AS "project_id!: Uuid",
                kind        AS "kind!",
                title       AS "title!",
                issue_id    AS "issue_id: Uuid",
                jira_key,
                work_class,
                summary,
                tags        AS "tags!",
                created_at  AS "created_at!",
                updated_at  AS "updated_at!"
            FROM kb_documents
            WHERE project_id = $1
              AND ($2::text IS NULL OR kind = $2)
              AND (NOT $3::bool OR NOT EXISTS (
                    SELECT 1 FROM kb_edges e WHERE e.document_id = kb_documents.id
              ))
            ORDER BY created_at DESC
            "#,
            project_id,
            kind,
            unlinked_only
        )
        .fetch_all(pool)
        .await?;

        Ok(records)
    }

    /// Idempotent create: a document sourced from an issue upserts on that
    /// issue (re-approval after review feedback refreshes the stored body
    /// instead of duplicating). Issue-less documents always insert.
    pub async fn upsert(
        pool: &PgPool,
        request: &CreateKbDocumentRequest,
    ) -> Result<KbDocument, KbDocumentError> {
        let id = Uuid::new_v4();
        let record = if request.issue_id.is_some() {
            sqlx::query_as!(
                KbDocument,
                r#"
                INSERT INTO kb_documents
                    (id, project_id, kind, title, body, issue_id, jira_key, work_class, summary, tags)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                ON CONFLICT (issue_id) WHERE issue_id IS NOT NULL DO UPDATE SET
                    title = EXCLUDED.title,
                    body = EXCLUDED.body,
                    jira_key = EXCLUDED.jira_key,
                    work_class = EXCLUDED.work_class,
                    summary = EXCLUDED.summary,
                    -- tags deliberately preserved: refiling must not wipe
                    -- team classification
                    updated_at = now()
                RETURNING
                    id          AS "id!: Uuid",
                    project_id  AS "project_id!: Uuid",
                    kind        AS "kind!",
                    title       AS "title!",
                    body        AS "body!",
                    issue_id    AS "issue_id: Uuid",
                    jira_key,
                    work_class,
                    summary,
                    tags        AS "tags!",
                    created_at  AS "created_at!",
                    updated_at  AS "updated_at!"
                "#,
                id,
                request.project_id,
                request.kind,
                request.title,
                request.body,
                request.issue_id,
                request.jira_key.as_deref(),
                request.work_class.as_deref(),
                request.summary.as_deref(),
                &request.tags
            )
            .fetch_one(pool)
            .await?
        } else {
            sqlx::query_as!(
                KbDocument,
                r#"
                INSERT INTO kb_documents
                    (id, project_id, kind, title, body, issue_id, jira_key, work_class, summary, tags)
                VALUES ($1, $2, $3, $4, $5, NULL, $6, $7, $8, $9)
                RETURNING
                    id          AS "id!: Uuid",
                    project_id  AS "project_id!: Uuid",
                    kind        AS "kind!",
                    title       AS "title!",
                    body        AS "body!",
                    issue_id    AS "issue_id: Uuid",
                    jira_key,
                    work_class,
                    summary,
                    tags        AS "tags!",
                    created_at  AS "created_at!",
                    updated_at  AS "updated_at!"
                "#,
                id,
                request.project_id,
                request.kind,
                request.title,
                request.body,
                request.jira_key.as_deref(),
                request.work_class.as_deref(),
                request.summary.as_deref(),
                &request.tags
            )
            .fetch_one(pool)
            .await?
        };

        Ok(record)
    }

    /// Unions `tags` into the document's tag set (idempotent).
    pub async fn add_tags(
        pool: &PgPool,
        id: Uuid,
        tags: &[String],
    ) -> Result<Option<KbDocument>, KbDocumentError> {
        let record = sqlx::query_as!(
            KbDocument,
            r#"
            UPDATE kb_documents
            SET tags = ARRAY(SELECT DISTINCT unnest(tags || $2::text[]) ORDER BY 1),
                updated_at = now()
            WHERE id = $1
            RETURNING
                id          AS "id!: Uuid",
                project_id  AS "project_id!: Uuid",
                kind        AS "kind!",
                title       AS "title!",
                body        AS "body!",
                issue_id    AS "issue_id: Uuid",
                jira_key,
                work_class,
                summary,
                tags        AS "tags!",
                created_at  AS "created_at!",
                updated_at  AS "updated_at!"
            "#,
            id,
            tags
        )
        .fetch_optional(pool)
        .await?;

        Ok(record)
    }
}
