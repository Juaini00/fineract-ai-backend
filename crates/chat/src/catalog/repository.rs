//! Persistensi versi katalog dan indeks retrieval.
//!
//! `knowledge_catalog_versions` bersifat append-only: baris yang dirujuk
//! `job_plans.contract_versions_json` tidak boleh hilang, karena pertanyaan
//! investigasi "prosa kontrak mana yang dilihat planner" hanya terjawab lewat
//! baris itu (migrasi 3).

use sqlx::PgPool;
use uuid::Uuid;

use crate::catalog::loader::Catalog;

#[derive(Debug, sqlx::FromRow)]
pub struct PendingEmbedding {
    pub id: Uuid,
    pub retrieval_text: String,
}

#[derive(Debug, sqlx::FromRow)]
pub struct EmbeddingMetadata {
    pub embedding_model: Option<String>,
    pub embedding_dimensions: Option<i32>,
    pub embedding_input_type: Option<String>,
}

const MARK_COMPLETE_EMBEDDINGS_SQL: &str = "UPDATE knowledge_catalog_versions v
     SET embedding_model = $1, embedding_dimensions = $2,
         embedding_input_type = $3,
         status = CASE WHEN status = 'failed' THEN status ELSE 'embedded' END,
         synced_at = now()
     WHERE EXISTS (SELECT 1 FROM knowledge_index i WHERE i.catalog_version_id = v.id)
       AND NOT EXISTS (
           SELECT 1 FROM knowledge_index i
           WHERE i.catalog_version_id = v.id
             AND (i.embedding IS NULL OR i.embedding_model IS DISTINCT FROM $1)
       )";

/// Id versi yang sudah tercatat untuk sebuah `content_hash`.
pub async fn version_id(pool: &PgPool, content_hash: &str) -> sqlx::Result<Option<Uuid>> {
    sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM knowledge_catalog_versions WHERE content_hash = $1",
    )
    .bind(content_hash)
    .fetch_optional(pool)
    .await
}

/// Simpan (atau temukan kembali) versi katalog untuk `content_hash` ini.
///
/// Hash yang sama berarti isi yang sama, jadi baris lama dipakai ulang —
/// termasuk id-nya, supaya plan lama tetap menunjuk baris yang sama.
pub async fn upsert_version(
    pool: &PgPool,
    catalog: &Catalog,
    status: &str,
    metadata: serde_json::Value,
) -> sqlx::Result<Uuid> {
    let (_window, mut tx) = foundation::commit_isolation::begin(pool).await?;

    let existing = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM knowledge_catalog_versions WHERE content_hash = $1",
    )
    .bind(&catalog.content_hash)
    .fetch_optional(&mut *tx)
    .await?;

    let (version_id, is_new) = match existing {
        Some(id) => {
            // Status boleh berubah (validated -> failed setelah schema Fineract
            // berubah, misalnya); isinya tidak, karena hash-nya sama.
            sqlx::query(
                "UPDATE knowledge_catalog_versions
                 SET status = $2, document_count = $3, metadata_json = $4, synced_at = now()
                 WHERE id = $1",
            )
            .bind(id)
            .bind(status)
            .bind(catalog.document_count() as i32)
            .bind(&metadata)
            .execute(&mut *tx)
            .await?;
            (id, false)
        }
        None => {
            let id = sqlx::query_scalar::<_, Uuid>(
                "INSERT INTO knowledge_catalog_versions
                    (content_hash, status, document_count, metadata_json, synced_at)
                 VALUES ($1, $2, $3, $4, now())
                 RETURNING id",
            )
            .bind(&catalog.content_hash)
            .bind(status)
            .bind(catalog.document_count() as i32)
            .bind(&metadata)
            .fetch_one(&mut *tx)
            .await?;
            (id, true)
        }
    };

    // Baris indeks ditulis hanya untuk versi yang BARU. Sama content_hash berarti
    // sama isi, jadi menulis ulang baris versi yang sudah ada tidak menambah apa
    // pun selain membuang embedding yang sudah dihitung (FIN-149) — backfill
    // eksternal (mahal, per API call) tidak boleh dibatalkan oleh sync berikutnya
    // atas versi yang tidak berubah.
    if !is_new {
        tx.commit().await?;
        return Ok(version_id);
    }

    for loaded in &catalog.capabilities {
        let capability = &loaded.entry;
        let retrieval_text = retrieval_text_for_capability(capability);
        if retrieval_text.trim().is_empty() {
            // Kolom menolak teks kosong; entri tanpa prosa sudah dilaporkan
            // validator sebagai temuan, bukan disisipkan dengan teks karangan.
            continue;
        }

        sqlx::query(
            "INSERT INTO knowledge_index
                (catalog_version_id, source_type, source_id, source_path, title,
                 retrieval_text, metadata_json)
             VALUES ($1, 'capability', $2, $3, $4, $5, $6)",
        )
        .bind(version_id)
        .bind(&capability.id)
        .bind(&loaded.path)
        .bind(capability.display_name.as_deref())
        .bind(&retrieval_text)
        .bind(serde_json::json!({
            "domain": capability.domain,
            "status": capability.status,
            "query_id": capability.query_id,
        }))
        .execute(&mut *tx)
        .await?;
    }

    for loaded in &catalog.queries {
        let query = &loaded.entry;
        sqlx::query(
            "INSERT INTO knowledge_index
                (catalog_version_id, source_type, source_id, source_path, title,
                 retrieval_text, metadata_json)
             VALUES ($1, 'query', $2, $3, $4, $5, $6)",
        )
        .bind(version_id)
        .bind(&query.id)
        .bind(&loaded.path)
        .bind(&query.id)
        .bind(format!(
            "{} parameter: {}",
            query.id,
            query
                .parameters
                .iter()
                .map(|parameter| parameter.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ))
        .bind(serde_json::json!({
            "sql_file": query.sql_file,
            "timeout_ms": query.timeout_ms,
        }))
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(version_id)
}

/// Semua baris historis yang belum punya vector; tidak dibatasi versi terkini.
pub async fn pending_embeddings(pool: &PgPool) -> sqlx::Result<Vec<PendingEmbedding>> {
    sqlx::query_as(
        "SELECT id, retrieval_text FROM knowledge_index WHERE embedding IS NULL ORDER BY catalog_version_id, id",
    )
    .fetch_all(pool)
    .await
}

/// Simpan satu batch lokal setelah seluruh HTTP provider selesai (I1).
pub async fn persist_embeddings(
    pool: &PgPool,
    rows: &[(Uuid, Vec<f32>)],
    model: &str,
    dimensions: usize,
    document_input_type: &str,
) -> sqlx::Result<()> {
    let (_window, mut tx) = foundation::commit_isolation::begin(pool).await?;
    for (id, embedding) in rows {
        sqlx::query("UPDATE knowledge_index SET embedding = $2, embedding_model = $3, embedded_at = now() WHERE id = $1 AND embedding IS NULL")
            .bind(id)
            .bind(pgvector::Vector::from(embedding.clone()))
            .bind(model)
            .execute(&mut *tx).await?;
    }
    sqlx::query(MARK_COMPLETE_EMBEDDINGS_SQL)
        .bind(model)
        .bind(dimensions as i32)
        .bind(document_input_type)
        .execute(&mut *tx)
        .await?;
    tx.commit().await
}

pub async fn embedding_metadata(
    pool: &PgPool,
    version_id: Uuid,
) -> sqlx::Result<Option<EmbeddingMetadata>> {
    sqlx::query_as("SELECT embedding_model, embedding_dimensions, embedding_input_type FROM knowledge_catalog_versions WHERE id = $1")
        .bind(version_id).fetch_optional(pool).await
}

pub async fn best_vector_capability(
    pool: &PgPool,
    version_id: Uuid,
    vector: Vec<f32>,
    cutoff: f32,
) -> sqlx::Result<Option<(String, f32)>> {
    sqlx::query_as(
        "SELECT source_id, (1 - (embedding <=> $2))::real AS similarity
         FROM knowledge_index
         WHERE catalog_version_id = $1 AND source_type = 'capability' AND embedding IS NOT NULL
           AND embedding_model = (SELECT embedding_model FROM knowledge_catalog_versions WHERE id = $1)
           AND 1 - (embedding <=> $2) >= $3
         ORDER BY embedding <=> $2, source_id ASC LIMIT 1",
    )
    .bind(version_id).bind(pgvector::Vector::from(vector)).bind(cutoff)
    .fetch_optional(pool).await
}

/// Teks yang dilihat retrieval. Sengaja menggabungkan prosa **dan** contoh:
/// `CARRY-OVER.md` mencatat capability yang judulnya mengklaim lebih luas
/// daripada query-nya menjadi tidak terjangkau tanpa satu pun kegagalan.
fn retrieval_text_for_capability(capability: &crate::catalog::model::Capability) -> String {
    let mut parts = Vec::new();

    if let Some(name) = &capability.display_name {
        parts.push(name.clone());
    }
    if let Some(description) = &capability.description {
        parts.push(description.clone());
    }
    parts.extend(capability.supported_intents.iter().cloned());
    parts.extend(capability.examples.iter().cloned());

    parts.join("\n")
}
