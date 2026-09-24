//! Persistence handle dataset dan chunk-nya.
//!
//! Satu-satunya tempat `sqlx` untuk L3. Tiga aturan yang tidak boleh
//! dilonggarkan di sini:
//!
//! 1. **Handle + chunk lahir dalam satu transaksi** dan baru kemudian menjadi
//!    `ready` (§3). Handle `ready` tanpa chunk-nya adalah snapshot yang bohong.
//! 2. **Tulisan worker dipagari** `lease_token` (C16): worker basi tidak
//!    meninggalkan handle yatim.
//! 3. **Purge tidak menghapus baris handle** — hanya chunk-nya (§6, C13).

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::engine::dataset::{Candidate, Chunk, ENCODING_VERSION, FORMAT_JSON, releasable};

/// Handle yang akan ditulis, beserta apa yang dinyatakan tentangnya.
#[derive(Debug)]
pub struct NewDataset<'a> {
    pub job_id: Uuid,
    pub session_id: Uuid,
    pub owner_user_id: Uuid,
    pub lease_token: Uuid,
    pub node_id: &'a str,
    pub plan_version: i32,
    pub schema_json: Value,
    pub grain_json: Value,
    pub scope_json: Value,
    pub provenance_json: Value,
    pub sort_key_json: Value,
    pub completeness: &'a str,
    pub completeness_reason: Option<&'a str>,
    pub truncated: bool,
    pub row_count_available: i64,
    pub row_count_total: Option<i64>,
    pub byte_size: i64,
    pub ttl_secs: i64,
    pub chunks: Vec<Chunk>,
}

/// Baris handle apa adanya. `handle_state` diturunkan di service dari `status`
/// (C13) — bukan disalin ke kolom, supaya `datasets.status` tetap otoritatif.
#[derive(Debug, Clone, FromRow)]
pub struct Handle {
    pub id: Uuid,
    pub job_id: Uuid,
    pub session_id: Uuid,
    pub owner_user_id: Uuid,
    pub node_id: Option<String>,
    pub plan_version: Option<i32>,
    pub schema_json: Value,
    pub grain_json: Value,
    pub scope_json: Value,
    pub provenance_json: Value,
    pub sort_key_json: Value,
    pub completeness: String,
    pub completeness_reason: Option<String>,
    pub truncated: bool,
    pub row_count_available: Option<i64>,
    pub row_count_total: Option<i64>,
    pub byte_size: Option<i64>,
    pub chunk_count: i32,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub purged_at: Option<DateTime<Utc>>,
}

/// Satu chunk sebagaimana tersimpan, lengkap dengan diskriminator encoding-nya.
///
/// `payload` (JSONB) `NULL` berarti chunk ini menyimpan isinya di
/// `payload_bytes` — encoding yang belum dibaca runtime ini (DS-8.5). Kolom
/// BYTEA itu sengaja tidak di-SELECT: tidak ada decoder yang memakainya, dan
/// membacanya hanya menambah de-TOAST tanpa guna.
#[derive(Debug, Clone, FromRow)]
pub struct StoredChunk {
    pub chunk_index: i32,
    pub row_from: i64,
    pub row_to: i64,
    pub payload: Option<Value>,
    pub format: String,
    pub encoding_version: i32,
}

/// Buat handle beserta chunk-nya. `None` berarti **fencing kalah** dan tidak
/// ada apa pun yang ditulis.
pub async fn create(pool: &PgPool, dataset: NewDataset<'_>) -> sqlx::Result<Option<Uuid>> {
    let (_window, mut tx) = foundation::commit_isolation::begin(pool).await?;

    // `SELECT … WHERE EXISTS` alih-alih `VALUES`: pemegang lease basi tidak
    // boleh meninggalkan handle yang tidak pernah dirujuk node mana pun.
    let id: Option<Uuid> = sqlx::query_scalar(
        "INSERT INTO datasets
            (job_id, node_id, plan_version, session_id, owner_user_id, schema_json, grain_json,
             scope_json, provenance_json, sort_key_json, completeness, completeness_reason,
             truncated, row_count_available, row_count_total, status)
         SELECT $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, 'building'
         WHERE EXISTS (
             SELECT 1 FROM chat_jobs
             WHERE id = $1 AND lease_token = $16 AND lifecycle = 'Running'
         )
         RETURNING id",
    )
    .bind(dataset.job_id)
    .bind(dataset.node_id)
    .bind(dataset.plan_version)
    .bind(dataset.session_id)
    .bind(dataset.owner_user_id)
    .bind(&dataset.schema_json)
    .bind(&dataset.grain_json)
    .bind(&dataset.scope_json)
    .bind(&dataset.provenance_json)
    .bind(&dataset.sort_key_json)
    .bind(dataset.completeness)
    .bind(dataset.completeness_reason)
    .bind(dataset.truncated)
    .bind(dataset.row_count_available)
    .bind(dataset.row_count_total)
    .bind(dataset.lease_token)
    .fetch_optional(&mut *tx)
    .await?;

    let Some(id) = id else {
        tx.rollback().await?;
        return Ok(None);
    };

    for chunk in &dataset.chunks {
        sqlx::query(
            "INSERT INTO dataset_chunks
                (dataset_id, chunk_index, row_from, row_to, payload, row_count, byte_size,
                 format, encoding_version)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(id)
        .bind(chunk.chunk_index)
        .bind(chunk.row_from)
        .bind(chunk.row_to)
        .bind(&chunk.payload)
        .bind(chunk.row_count)
        .bind(chunk.byte_size)
        .bind(FORMAT_JSON)
        .bind(ENCODING_VERSION)
        .execute(&mut *tx)
        .await?;
    }

    // `ready` ditulis TERAKHIR: sesudah baris itu, handle immutable (§1) dan
    // `expires_at` mulai berjalan (§6).
    sqlx::query(
        "UPDATE datasets
         SET status = 'ready',
             byte_size = $2,
             chunk_count = $3,
             expires_at = now() + make_interval(secs => $4)
         WHERE id = $1",
    )
    .bind(id)
    .bind(dataset.byte_size)
    .bind(dataset.chunks.len() as i32)
    .bind(dataset.ttl_secs as f64)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(Some(id))
}

/// **C5** — hubungkan ledger node ke handle-nya.
///
/// Ditulis terpisah dari `complete_node` dan tetap dipagari `lease_token`
/// (C16): worker yang sudah direbut tidak boleh menautkan apa pun. `false`
/// berarti fencing kalah.
pub async fn link_node_run(
    pool: &PgPool,
    node_run_id: Uuid,
    dataset_id: Uuid,
    job_id: Uuid,
    lease_token: Uuid,
) -> sqlx::Result<bool> {
    let updated = sqlx::query(
        "UPDATE job_node_runs
         SET dataset_id = $2
         WHERE id = $1
           AND EXISTS (
               SELECT 1 FROM chat_jobs WHERE id = $3 AND lease_token = $4
           )",
    )
    .bind(node_run_id)
    .bind(dataset_id)
    .bind(job_id)
    .bind(lease_token)
    .execute(pool)
    .await?
    .rows_affected();

    Ok(updated == 1)
}

/// Baca handle. Otorisasi TIDAK dilakukan di sini — ia milik service (I7),
/// supaya tidak ada dua tempat yang masing-masing mengira yang lain memeriksanya.
pub async fn find(pool: &PgPool, dataset_id: Uuid) -> sqlx::Result<Option<Handle>> {
    sqlx::query_as::<_, Handle>(
        "SELECT id, job_id, session_id, owner_user_id, node_id, plan_version, schema_json,
                grain_json, scope_json, provenance_json, sort_key_json, completeness,
                completeness_reason, truncated, row_count_available, row_count_total, byte_size,
                chunk_count, status, created_at, expires_at, purged_at
         FROM datasets WHERE id = $1",
    )
    .bind(dataset_id)
    .fetch_optional(pool)
    .await
}

/// Chunk yang memuat baris `row`, dan seterusnya — keyset atas
/// `(dataset_id, chunk_index)`.
///
/// Dibaca **utuh per chunk**: engine tidak pernah menyaring ulang salinan
/// retained-nya (#11). Yang dipotong hanyalah berapa baris yang dikembalikan
/// ke pemanggil, bukan baris mana yang dianggap ada.
pub async fn chunks_from(
    pool: &PgPool,
    dataset_id: Uuid,
    chunk_index: i32,
    limit: i64,
) -> sqlx::Result<Vec<StoredChunk>> {
    sqlx::query_as::<_, StoredChunk>(
        "SELECT chunk_index, row_from, row_to, payload, format, encoding_version
         FROM dataset_chunks
         WHERE dataset_id = $1 AND chunk_index >= $2
         ORDER BY chunk_index
         LIMIT $3",
    )
    .bind(dataset_id)
    .bind(chunk_index)
    .bind(limit)
    .fetch_all(pool)
    .await
}

/// Satu putaran purge TTL (T11, idempoten).
///
/// Kandidat diambil bersama lifecycle job pemiliknya, lalu **disaring di Rust**
/// oleh [`releasable`]: aturan "jangan sentuh dataset job nonterminal" (#11)
/// hidup di satu tempat yang dapat diuji `cargo test`, bukan tersembunyi di
/// dalam predikat SQL yang hanya dapat dibuktikan dengan database berjalan.
pub async fn purge_expired(pool: &PgPool) -> sqlx::Result<u64> {
    let candidates = sqlx::query_as::<_, (Uuid, String)>(
        "SELECT d.id, j.lifecycle
         FROM datasets d
         JOIN chat_jobs j ON j.id = d.job_id
         WHERE d.status = 'ready'
           AND d.expires_at IS NOT NULL
           AND d.expires_at < now()",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|(dataset_id, job_lifecycle)| Candidate { dataset_id, job_lifecycle })
    .collect::<Vec<_>>();

    purge(pool, &releasable(&candidates)).await
}

/// Seam lokal (FIN-44): purge SATU handle sekarang, tanpa menunggu TTL.
///
/// Aturan yang sama dengan reaper, bukan jalur kedua: kandidatnya disaring
/// [`releasable`] — dataset milik job nonterminal tidak disentuh (#11) — dan
/// purge-nya adalah [`purge`] yang juga dipakai T11. Yang dilewati hanya
/// `expires_at`. `false` berarti tidak ada yang dipurge (bukan `ready`, atau
/// job-nya belum terminal).
pub async fn purge_now(pool: &PgPool, dataset_id: Uuid) -> sqlx::Result<bool> {
    let candidates = sqlx::query_as::<_, (Uuid, String)>(
        "SELECT d.id, j.lifecycle
         FROM datasets d
         JOIN chat_jobs j ON j.id = d.job_id
         WHERE d.id = $1 AND d.status = 'ready'",
    )
    .bind(dataset_id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|(dataset_id, job_lifecycle)| Candidate { dataset_id, job_lifecycle })
    .collect::<Vec<_>>();

    Ok(purge(pool, &releasable(&candidates)).await? == 1)
}

async fn purge(pool: &PgPool, ids: &[Uuid]) -> sqlx::Result<u64> {
    if ids.is_empty() {
        return Ok(0);
    }

    let (_window, mut tx) = foundation::commit_isolation::begin(pool).await?;

    sqlx::query("DELETE FROM dataset_chunks WHERE dataset_id = ANY($1)")
        .bind(ids)
        .execute(&mut *tx)
        .await?;

    // Baris handle DIPERTAHANKAN: `session_memory` dan `job_node_runs`
    // merujuknya, dan `handle_state` hanya dapat dinyatakan bila barisnya ada.
    let purged = sqlx::query(
        "UPDATE datasets
         SET status = 'purged', purged_at = now(), chunk_count = 0, byte_size = 0
         WHERE id = ANY($1) AND status = 'ready'",
    )
    .bind(ids)
    .execute(&mut *tx)
    .await?
    .rows_affected();

    tx.commit().await?;
    Ok(purged)
}
