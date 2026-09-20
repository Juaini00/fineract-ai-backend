//! Pembacaan `job_events`. Satu-satunya sumber isi stream.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

/// Satu event yang sudah durable.
#[derive(Debug, Clone, FromRow, serde::Serialize)]
pub struct JobEvent {
    pub sequence: i64,
    pub schema_version: i32,
    pub event_type: String,
    pub occurred_at: DateTime<Utc>,
    pub plan_version: Option<i32>,
    pub node_id: Option<String>,
    pub node_attempt: Option<i32>,
    pub clarification_id: Option<Uuid>,
    pub clarification_revision: Option<i32>,
    pub response_version: Option<i32>,
    pub payload_json: Option<Value>,
    pub payload_truncated: bool,
}

/// Event setelah `after`, terbatas `limit`.
///
/// `LIMIT` adalah batas buffer keluar: satu job dengan ribuan event tidak boleh
/// dimaterialisasi sekaligus hanya karena klien terlambat menyambung (sse.md —
/// bound outgoing buffers).
pub async fn after(
    pool: &PgPool,
    job_id: Uuid,
    after: i64,
    limit: i64,
) -> sqlx::Result<Vec<JobEvent>> {
    sqlx::query_as::<_, JobEvent>(
        "SELECT sequence, schema_version, event_type, occurred_at, plan_version,
                node_id, node_attempt, clarification_id, clarification_revision,
                response_version, payload_json, payload_truncated
         FROM job_events
         WHERE job_id = $1 AND sequence > $2
         ORDER BY sequence
         LIMIT $3",
    )
    .bind(job_id)
    .bind(after)
    .bind(limit)
    .fetch_all(pool)
    .await
}

/// Sequence terkecil yang masih tersimpan untuk job ini.
///
/// Dipakai untuk membedakan "cursor dari masa depan" dari "riwayat sudah
/// dipurge". Keduanya wajib menjadi kesalahan eksplisit, bukan stream yang
/// diam-diam kehilangan awal riwayatnya (sse.md aturan 5).
pub async fn earliest(pool: &PgPool, job_id: Uuid) -> sqlx::Result<Option<i64>> {
    sqlx::query_scalar::<_, Option<i64>>("SELECT MIN(sequence) FROM job_events WHERE job_id = $1")
        .bind(job_id)
        .fetch_one(pool)
        .await
}
