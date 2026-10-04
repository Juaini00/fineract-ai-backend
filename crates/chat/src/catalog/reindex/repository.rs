use anyhow::{Context, ensure};
use sqlx::{Connection, PgConnection, PgPool};
use uuid::Uuid;

use super::model::{IndexDocument, Run};
use crate::audit::{self, AuditEvent};

// Fixed application-wide key. CLI and HTTP must both use this lock.
const LOCK_NAMESPACE: i32 = 0x46494e31;
const LOCK_KEY: i32 = 160;

pub async fn acquire(database_url: &str) -> anyhow::Result<Option<PgConnection>> {
    let mut connection = PgConnection::connect(database_url)
        .await
        .context("catalog maintenance database unavailable")?;
    let acquired: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1, $2)")
        .bind(LOCK_NAMESPACE)
        .bind(LOCK_KEY)
        .fetch_one(&mut connection)
        .await?;
    if acquired {
        Ok(Some(connection))
    } else {
        Ok(None)
    }
}

pub async fn release(mut connection: PgConnection) -> anyhow::Result<()> {
    let released: bool = sqlx::query_scalar("SELECT pg_advisory_unlock($1, $2)")
        .bind(LOCK_NAMESPACE)
        .bind(LOCK_KEY)
        .fetch_one(&mut connection)
        .await?;
    ensure!(released, "catalog maintenance lock was lost");
    connection.close().await?;
    Ok(())
}

pub async fn assert_lock_alive(connection: &mut PgConnection) -> anyhow::Result<()> {
    // PgConnection never transparently reconnects. If the dedicated session
    // died while waiting for the provider, publishing must be refused.
    let _: i32 = sqlx::query_scalar("SELECT 1").fetch_one(connection).await?;
    Ok(())
}

pub struct AdmissionFields<'a> {
    pub hash: &'a str,
    pub running_hash: Option<&'a str>,
    pub rebuild: bool,
    pub owner: Option<Uuid>,
    pub capabilities: usize,
    pub queries: usize,
    pub datasets: usize,
    pub resolver_shapes: usize,
    pub warnings: usize,
    pub document_count: usize,
}

pub async fn admit(pool: &PgPool, fields: AdmissionFields<'_>) -> anyhow::Result<Run> {
    let (_window, mut tx) = foundation::commit_isolation::begin(pool).await?;
    let abandoned = sqlx::query_scalar::<_, Uuid>(
        "UPDATE catalog_reindex_runs SET status = 'Abandoned', finished_at = now(), error_code = 'INTERRUPTED'
         WHERE status IN ('Pending','Running') RETURNING id"
    ).fetch_all(&mut *tx).await?;
    for id in abandoned {
        audit::insert(
            &mut tx,
            AuditEvent {
                actor_kind: "system",
                stage: "admin",
                action: "catalog_reindex_abandoned",
                result: "failed",
                failure_code: Some("INTERRUPTED"),
                detail_json: Some(serde_json::json!({"run_id": id})),
                ..Default::default()
            },
        )
        .await?;
    }
    let inserted_version: Option<Uuid> = sqlx::query_scalar(
        "INSERT INTO knowledge_catalog_versions (content_hash,status,document_count,metadata_json,synced_at)
         VALUES ($1,'validated',$2,$3,NULL)
         ON CONFLICT (content_hash) DO NOTHING
         RETURNING id"
    ).bind(fields.hash)
     .bind(fields.document_count as i32)
     .bind(serde_json::json!({"validation": "passed", "warning_count": fields.warnings,
         "dataset_definition_count": fields.datasets, "resolver_shape_count": fields.resolver_shapes}))
     .fetch_optional(&mut *tx).await?;
    let version_id = match inserted_version {
        Some(id) => id,
        None => {
            sqlx::query_scalar::<_, Uuid>(
                "SELECT id FROM knowledge_catalog_versions WHERE content_hash=$1",
            )
            .bind(fields.hash)
            .fetch_one(&mut *tx)
            .await?
        }
    };
    let run: Run = sqlx::query_as(
        "INSERT INTO catalog_reindex_runs
         (status,rebuild_embeddings,requested_by_user_id,catalog_version_id,content_hash,running_content_hash,
          capability_count,query_count,dataset_definition_count,resolver_shape_count,finding_warning_count)
         VALUES ('Pending',$1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
         RETURNING *"
    ).bind(fields.rebuild).bind(fields.owner).bind(version_id).bind(fields.hash)
     .bind(fields.running_hash).bind(fields.capabilities as i32).bind(fields.queries as i32)
     .bind(fields.datasets as i32).bind(fields.resolver_shapes as i32)
     .bind(fields.warnings as i32).fetch_one(&mut *tx).await?;
    audit::insert(
        &mut tx,
        AuditEvent {
            actor_kind: if fields.owner.is_some() {
                "user"
            } else {
                "system"
            },
            actor_user_id: fields.owner,
            stage: "admin",
            action: "catalog_reindex_admitted",
            result: "ok",
            detail_json: Some(serde_json::json!({"run_id": run.id,
            "catalog_version_id": version_id, "content_hash": fields.hash,
            "rebuild_embeddings": fields.rebuild})),
            ..Default::default()
        },
    )
    .await?;
    tx.commit().await?;
    Ok(run)
}

pub async fn mark_running(pool: &PgPool, id: Uuid) -> anyhow::Result<()> {
    let (_window, mut tx) = foundation::commit_isolation::begin(pool).await?;
    let result = sqlx::query(
        "UPDATE catalog_reindex_runs SET status='Running',started_at=now()
        WHERE id=$1 AND status='Pending'",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;
    ensure!(
        result.rows_affected() == 1,
        "catalog run is no longer pending"
    );
    audit::insert(
        &mut tx,
        AuditEvent {
            actor_kind: "worker",
            stage: "admin",
            action: "catalog_reindex_started",
            result: "ok",
            detail_json: Some(serde_json::json!({"run_id": id})),
            ..Default::default()
        },
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn progress(pool: &PgPool, id: Uuid, processed: usize) -> anyhow::Result<()> {
    let (_window, mut tx) = foundation::commit_isolation::begin(pool).await?;
    sqlx::query(
        "UPDATE catalog_reindex_runs SET processed_row_count=$2 WHERE id=$1 AND status='Running'",
    )
    .bind(id)
    .bind(processed as i32)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn fail(pool: &PgPool, id: Uuid, code: &'static str) -> anyhow::Result<()> {
    let (_window, mut tx) = foundation::commit_isolation::begin(pool).await?;
    let result = sqlx::query(
        "UPDATE catalog_reindex_runs SET status='Failed',error_code=$2,finished_at=now()
        WHERE id=$1 AND status IN ('Pending','Running')",
    )
    .bind(id)
    .bind(code)
    .execute(&mut *tx)
    .await?;
    ensure!(result.rows_affected() == 1, "catalog run was not active");
    audit::insert(
        &mut tx,
        AuditEvent {
            actor_kind: "worker",
            stage: "admin",
            action: "catalog_reindex_failed",
            result: "failed",
            failure_code: Some(code),
            detail_json: Some(serde_json::json!({"run_id": id})),
            ..Default::default()
        },
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn publish(
    pool: &PgPool,
    run: &Run,
    documents: &[IndexDocument],
    vectors: Option<&[Vec<f32>]>,
    model: Option<(&str, usize, &str)>,
) -> anyhow::Result<Run> {
    if let Some(vectors) = vectors {
        ensure!(vectors.len() == documents.len(), "vector count mismatch");
    }
    let version = run.catalog_version_id.context("catalog version missing")?;
    let (_window, mut tx) = foundation::commit_isolation::begin(pool).await?;
    let cap_ids: Vec<String> = documents
        .iter()
        .filter(|d| d.source_type == "capability")
        .map(|d| d.source_id.clone())
        .collect();
    let query_ids: Vec<String> = documents
        .iter()
        .filter(|d| d.source_type == "query")
        .map(|d| d.source_id.clone())
        .collect();
    sqlx::query("DELETE FROM knowledge_index WHERE catalog_version_id=$1 AND source_type='capability' AND NOT (source_id = ANY($2))")
        .bind(version).bind(&cap_ids).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM knowledge_index WHERE catalog_version_id=$1 AND source_type='query' AND NOT (source_id = ANY($2))")
        .bind(version).bind(&query_ids).execute(&mut *tx).await?;
    for (index, document) in documents.iter().enumerate() {
        let vector = vectors.map(|all| pgvector::Vector::from(all[index].clone()));
        sqlx::query(
            "INSERT INTO knowledge_index
             (catalog_version_id,source_type,source_id,source_path,title,retrieval_text,metadata_json,embedding,embedding_model,embedded_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,CASE WHEN $8 IS NULL THEN NULL ELSE now() END)
             ON CONFLICT (catalog_version_id,source_type,source_id) DO UPDATE SET
               source_path=EXCLUDED.source_path,title=EXCLUDED.title,
               retrieval_text=EXCLUDED.retrieval_text,metadata_json=EXCLUDED.metadata_json,
               embedding=CASE WHEN $8 IS NOT NULL THEN $8
                 WHEN knowledge_index.retrieval_text=EXCLUDED.retrieval_text THEN knowledge_index.embedding
                 ELSE NULL END,
               embedding_model=CASE WHEN $8 IS NOT NULL THEN $9
                 WHEN knowledge_index.retrieval_text=EXCLUDED.retrieval_text THEN knowledge_index.embedding_model
                 ELSE NULL END,
               embedded_at=CASE WHEN $8 IS NOT NULL THEN now()
                 WHEN knowledge_index.retrieval_text=EXCLUDED.retrieval_text THEN knowledge_index.embedded_at
                 ELSE NULL END"
        ).bind(version).bind(document.source_type).bind(&document.source_id)
         .bind(&document.source_path).bind(&document.title).bind(&document.retrieval_text)
         .bind(&document.metadata).bind(vector).bind(model.map(|m| m.0))
         .execute(&mut *tx).await?;
    }
    let (total, embedded): (i64, i64) = sqlx::query_as(
        "SELECT count(*),count(*) FILTER (WHERE embedding IS NOT NULL) FROM knowledge_index
         WHERE catalog_version_id=$1 AND source_type IN ('capability','query')",
    )
    .bind(version)
    .fetch_one(&mut *tx)
    .await?;
    ensure!(
        total == documents.len() as i64,
        "catalog lexical coverage mismatch"
    );
    let metadata = if let Some((name, dims, input_type)) = model {
        (
            Some(name.to_string()),
            Some(dims as i32),
            Some(input_type.to_string()),
        )
    } else if embedded == total && total > 0 {
        let previous = sqlx::query_as::<_, (Option<String>, Option<i32>, Option<String>)>(
            "SELECT embedding_model,embedding_dimensions,embedding_input_type
             FROM knowledge_catalog_versions WHERE id=$1",
        )
        .bind(version)
        .fetch_one(&mut *tx)
        .await?;
        let mismatched: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM knowledge_index WHERE catalog_version_id=$1
             AND source_type IN ('capability','query')
             AND embedding_model IS DISTINCT FROM $2",
        )
        .bind(version)
        .bind(&previous.0)
        .fetch_one(&mut *tx)
        .await?;
        if mismatched == 0 {
            previous
        } else {
            (None, None, None)
        }
    } else {
        (None, None, None)
    };
    let fully_embedded = total > 0 && embedded == total && metadata.0.is_some();
    sqlx::query("UPDATE knowledge_catalog_versions SET status=$2,
        embedding_model=$3,embedding_dimensions=$4,embedding_input_type=$5,
        metadata_json=$6,synced_at=now() WHERE id=$1")
        .bind(version).bind(if fully_embedded { "embedded" } else { "indexed" })
        .bind(&metadata.0).bind(metadata.1).bind(&metadata.2)
        .bind(serde_json::json!({"validation":"passed", "dataset_definition_count": run.dataset_definition_count,
            "resolver_shape_count":run.resolver_shape_count,"lexical_row_count":total,"embedded_row_count":embedded}))
        .execute(&mut *tx).await?;
    let finished: Run = sqlx::query_as(
        "UPDATE catalog_reindex_runs SET status='Completed',
        lexical_row_count=$2,embedded_row_count=$3,processed_row_count=$4,finished_at=now()
        WHERE id=$1 AND status='Running' RETURNING *",
    )
    .bind(run.id)
    .bind(total as i32)
    .bind(embedded as i32)
    .bind(vectors.map_or(0, |all| all.len()) as i32)
    .fetch_one(&mut *tx)
    .await?;
    audit::insert(&mut tx, AuditEvent { actor_kind: "worker", stage: "admin",
        action: "catalog_reindex_completed", result: "ok",
        detail_json: Some(serde_json::json!({"run_id": run.id,"catalog_version_id":version,
            "content_hash":run.content_hash,"lexical_row_count":total,"embedded_row_count":embedded})),
        ..Default::default() }).await?;
    tx.commit().await?;
    Ok(finished)
}

pub async fn get(pool: &PgPool, id: Uuid) -> sqlx::Result<Option<Run>> {
    sqlx::query_as("SELECT * FROM catalog_reindex_runs WHERE id=$1")
        .bind(id)
        .fetch_optional(pool)
        .await
}

pub async fn recent(pool: &PgPool) -> sqlx::Result<Vec<Run>> {
    sqlx::query_as("SELECT * FROM catalog_reindex_runs ORDER BY created_at DESC,id DESC LIMIT 25")
        .fetch_all(pool)
        .await
}

pub async fn version_id(pool: &PgPool, hash: &str) -> sqlx::Result<Option<Uuid>> {
    sqlx::query_scalar("SELECT id FROM knowledge_catalog_versions WHERE content_hash=$1")
        .bind(hash)
        .fetch_optional(pool)
        .await
}

pub async fn coverage(pool: &PgPool, version: Uuid) -> sqlx::Result<(i64, i64)> {
    sqlx::query_as(
        "SELECT count(*), count(*) FILTER (WHERE embedding IS NOT NULL)
        FROM knowledge_index WHERE catalog_version_id=$1 AND source_type IN ('capability','query')",
    )
    .bind(version)
    .fetch_one(pool)
    .await
}
