//! Persistensi versi katalog dan indeks retrieval.
//!
//! `knowledge_catalog_versions` bersifat append-only: baris yang dirujuk
//! `job_plans.contract_versions_json` tidak boleh hilang, karena pertanyaan
//! investigasi "prosa kontrak mana yang dilihat planner" hanya terjawab lewat
//! baris itu (migrasi 3).

use sqlx::PgPool;
use uuid::Uuid;

use crate::catalog::loader::Catalog;

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
    let mut tx = pool.begin().await?;

    let existing = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM knowledge_catalog_versions WHERE content_hash = $1",
    )
    .bind(&catalog.content_hash)
    .fetch_optional(&mut *tx)
    .await?;

    let version_id = match existing {
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
            id
        }
        None => {
            sqlx::query_scalar::<_, Uuid>(
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
            .await?
        }
    };

    // Indeks retrieval ditulis ulang untuk versi ini. Aman karena baris indeks
    // adalah turunan isi katalog, bukan bukti audit — buktinya ada pada baris
    // versi dan content_hash-nya.
    sqlx::query("DELETE FROM knowledge_index WHERE catalog_version_id = $1")
        .bind(version_id)
        .execute(&mut *tx)
        .await?;

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
