//! Settings queries. No audit evidence or unbounded detail columns are read.

use chrono::{DateTime, Utc};
use foundation::db::FineractDb;
use sqlx::PgPool;
use uuid::Uuid;

use super::model::{AuditEvent, CatalogVersion, IndexCounts, IndexSourceCount, PiiRow, Principal};

pub async fn current_principal(
    pool: &PgPool,
    user_id: Uuid,
    auth_session_id: Uuid,
) -> sqlx::Result<Option<Principal>> {
    sqlx::query_as::<_, Principal>(
        "SELECT u.id AS user_id, u.username, u.role, s.id AS auth_session_id,
                s.expires_at AS session_expires_at, s.entitlements_source, s.entitlements_json
         FROM users u JOIN auth_sessions s ON s.user_id = u.id
         WHERE u.id = $1 AND s.id = $2 AND u.is_active AND u.role = 'admin'
           AND s.revoked_at IS NULL AND s.expires_at > now()",
    )
    .bind(user_id)
    .bind(auth_session_id)
    .fetch_optional(pool)
    .await
}

pub async fn current_pii(pool: &PgPool) -> sqlx::Result<Vec<PiiRow>> {
    sqlx::query_as::<_, PiiRow>(
        "SELECT DISTINCT ON (key) key, value_json, version FROM system_settings
         WHERE key IN ('pii.enabled', 'pii.mode') AND effective_from <= now()
         ORDER BY key, version DESC",
    )
    .fetch_all(pool)
    .await
}

pub async fn office_ids(fineract: &FineractDb) -> sqlx::Result<Vec<i64>> {
    sqlx::query_scalar("SELECT id FROM m_office ORDER BY id")
        .fetch_all(fineract.pool())
        .await
}

pub async fn catalog_version(pool: &PgPool, hash: &str) -> sqlx::Result<Option<CatalogVersion>> {
    sqlx::query_as::<_, CatalogVersion>(
        "SELECT id, status, document_count, embedding_model, embedding_dimensions,
                embedding_input_type, synced_at
         FROM knowledge_catalog_versions WHERE content_hash = $1",
    )
    .bind(hash)
    .fetch_optional(pool)
    .await
}

pub async fn latest_catalog_hash(pool: &PgPool) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar(
        "SELECT v.content_hash FROM knowledge_catalog_versions v
        WHERE EXISTS (SELECT 1 FROM knowledge_index i WHERE i.catalog_version_id = v.id)
        ORDER BY v.synced_at DESC NULLS LAST, v.created_at DESC, v.id DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await
}

pub async fn index_counts(pool: &PgPool, catalog_version_id: Uuid) -> sqlx::Result<IndexCounts> {
    sqlx::query_as::<_, IndexCounts>(
        "SELECT count(*) AS total,
                count(*) FILTER (WHERE source_type = 'capability') AS capabilities,
                count(*) FILTER (WHERE embedding IS NOT NULL) AS embedded,
                count(*) FILTER (WHERE embedding IS NULL) AS missing,
                count(*) FILTER (WHERE source_type = 'capability' AND embedding IS NOT NULL) AS embedded_capabilities
         FROM knowledge_index WHERE catalog_version_id = $1"
    ).bind(catalog_version_id).fetch_one(pool).await
}

pub async fn index_source_counts(
    pool: &PgPool,
    catalog_version_id: Uuid,
) -> sqlx::Result<Vec<IndexSourceCount>> {
    sqlx::query_as::<_, IndexSourceCount>(
        "SELECT source_type, count(*) AS count FROM knowledge_index
         WHERE catalog_version_id = $1 GROUP BY source_type ORDER BY source_type",
    )
    .bind(catalog_version_id)
    .fetch_all(pool)
    .await
}

pub async fn vector_info(pool: &PgPool) -> sqlx::Result<(bool, Option<i32>)> {
    let installed: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'vector')")
            .fetch_one(pool)
            .await?;
    let dimensions: Option<i32> = sqlx::query_scalar(
        "SELECT CASE WHEN atttypmod > 0 THEN atttypmod ELSE NULL END
         FROM pg_attribute WHERE attrelid = 'knowledge_index'::regclass AND attname = 'embedding' AND NOT attisdropped"
    ).fetch_optional(pool).await?.flatten();
    Ok((installed, dimensions))
}

pub async fn audit_page(
    pool: &PgPool,
    limit: i64,
    before: Option<(DateTime<Utc>, Uuid)>,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
    stage: Option<&str>,
    result: Option<&str>,
    job_id: Option<Uuid>,
) -> sqlx::Result<Vec<AuditEvent>> {
    sqlx::query_as::<_, AuditEvent>(
        "SELECT id, occurred_at, duration_ms, stage, action, result, actor_kind,
                actor_user_id, job_id, session_id, node_id, request_id, model_call_id,
                catalog_version_id, catalog_content_hash, has_evidence
         FROM audit_events
         WHERE ($1::timestamptz IS NULL OR occurred_at >= $1)
           AND ($2::timestamptz IS NULL OR occurred_at <= $2)
           AND ($3::text IS NULL OR stage = $3)
           AND ($4::text IS NULL OR result = $4)
           AND ($5::uuid IS NULL OR job_id = $5)
           AND ($6::timestamptz IS NULL OR (occurred_at, id) < ($6, $7))
         ORDER BY occurred_at DESC, id DESC LIMIT $8",
    )
    .bind(from)
    .bind(to)
    .bind(stage)
    .bind(result)
    .bind(job_id)
    .bind(before.map(|b| b.0))
    .bind(before.map(|b| b.1))
    .bind(limit)
    .fetch_all(pool)
    .await
}

pub async fn audit_event(pool: &PgPool, id: Uuid) -> sqlx::Result<Option<AuditEvent>> {
    sqlx::query_as::<_, AuditEvent>(
        "SELECT id, occurred_at, duration_ms, stage, action, result, actor_kind,
                actor_user_id, job_id, session_id, node_id, request_id, model_call_id,
                catalog_version_id, catalog_content_hash, has_evidence
         FROM audit_events WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}
