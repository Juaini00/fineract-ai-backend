//! SQL job, event, idempotency dan pesan.
//!
//! Seluruh T1 (`docs/data/database-design.md` §6) berada dalam satu transaksi
//! di sini. Tidak ada panggilan eksternal di dalamnya (I1).

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{
    audit::{self, AuditEvent},
    session::repository as session_repository,
    settings,
};

/// Snapshot state job. Tiga dimensi status tetap terpisah (#1).
#[derive(Debug, Clone, FromRow, serde::Serialize)]
pub struct Job {
    pub id: Uuid,
    pub session_id: Uuid,
    pub owner_user_id: Uuid,
    pub request_text: String,
    pub lifecycle: String,
    pub outcome: Option<String>,
    pub completeness: Option<String>,
    pub completeness_reason: Option<String>,
    pub failure_code: Option<String>,
    pub plan_version: Option<i32>,
    pub final_response_version: Option<i32>,
    /// Cursor event: klien berlangganan mulai setelah sequence ini.
    pub last_event_sequence: i64,
    pub scope_json: Value,
    pub created_at: DateTime<Utc>,
    pub terminal_at: Option<DateTime<Utc>>,
}

/// Hasil pemeriksaan idempotency sebelum pekerjaan nyata dimulai.
#[derive(Debug)]
pub enum Accepted {
    /// Job baru benar-benar dibuat. Di-box agar varian replay yang kecil
    /// tidak ikut membawa ukuran snapshot job.
    Created(Box<Job>),
    /// Kunci dan payload sama persis: acknowledgement tersimpan dikembalikan
    /// apa adanya, tanpa eksekusi kedua.
    Replayed { status: i32, body: Value },
}

/// T1 — create job.
///
/// Urutan: idempotency → ownership/lifecycle session → snapshot PII → insert
/// job → event → message → audit → tandai idempotency selesai. Commit baru
/// terjadi setelah semuanya durable; 202 tidak pernah mendahului commit.
#[allow(clippy::too_many_arguments)]
pub async fn create_job(
    pool: &PgPool,
    owner_user_id: Uuid,
    session_id: Uuid,
    request_text: &str,
    office_ids: &[i64],
    fineract_tenant: &str,
    idempotency_key: &str,
    fingerprint: &str,
    idempotency_ttl_secs: i64,
) -> Result<Accepted, CreateError> {
    let mut tx = pool.begin().await?;

    // 1. Idempotency lebih dulu: retry tidak boleh sempat menyentuh state lain.
    match claim_idempotency(
        &mut tx,
        owner_user_id,
        "job.create",
        idempotency_key,
        fingerprint,
        idempotency_ttl_secs,
    )
    .await?
    {
        Idempotency::Claimed(id) => {
            let job = insert_job_chain(
                &mut tx,
                owner_user_id,
                session_id,
                request_text,
                office_ids,
                fineract_tenant,
            )
            .await?;

            let acknowledgement = serde_json::json!({
                "job_id": job.id,
                "session_id": job.session_id,
                "lifecycle": job.lifecycle,
            });

            sqlx::query(
                "UPDATE idempotency_keys
                 SET status = 'completed', response_status = 202,
                     response_body_json = $2, target_job_id = $3, completed_at = now()
                 WHERE id = $1",
            )
            .bind(id)
            .bind(&acknowledgement)
            .bind(job.id)
            .execute(&mut *tx)
            .await?;

            tx.commit().await?;
            Ok(Accepted::Created(Box::new(job)))
        }
        Idempotency::Replay { status, body } => {
            tx.commit().await?;
            Ok(Accepted::Replayed { status, body })
        }
        Idempotency::Mismatch => {
            tx.rollback().await?;
            Err(CreateError::KeyReused)
        }
        Idempotency::InProgress => {
            tx.rollback().await?;
            Err(CreateError::InProgress)
        }
    }
}

enum Idempotency {
    Claimed(Uuid),
    Replay { status: i32, body: Value },
    /// Kunci sama, payload berbeda.
    Mismatch,
    /// Permintaan pertama belum selesai.
    InProgress,
}

async fn claim_idempotency(
    tx: &mut Transaction<'_, Postgres>,
    owner_user_id: Uuid,
    operation: &str,
    key: &str,
    fingerprint: &str,
    ttl_secs: i64,
) -> sqlx::Result<Idempotency> {
    let claimed = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO idempotency_keys
            (owner_user_id, operation, idempotency_key, request_fingerprint, status, expires_at)
         VALUES ($1, $2, $3, $4, 'in_progress', now() + make_interval(secs => $5))
         ON CONFLICT (owner_user_id, operation, idempotency_key) DO NOTHING
         RETURNING id",
    )
    .bind(owner_user_id)
    .bind(operation)
    .bind(key)
    .bind(fingerprint)
    .bind(ttl_secs as f64)
    .fetch_optional(&mut **tx)
    .await?;

    if let Some(id) = claimed {
        return Ok(Idempotency::Claimed(id));
    }

    // Baris sudah ada. `FOR UPDATE` menahan dua retry bersamaan agar tidak
    // sama-sama menyimpulkan "masih in_progress" lalu saling menimpa.
    let existing = sqlx::query_as::<_, (String, String, Option<i32>, Option<Value>)>(
        "SELECT request_fingerprint, status, response_status, response_body_json
         FROM idempotency_keys
         WHERE owner_user_id = $1 AND operation = $2 AND idempotency_key = $3
         FOR UPDATE",
    )
    .bind(owner_user_id)
    .bind(operation)
    .bind(key)
    .fetch_one(&mut **tx)
    .await?;

    let (stored_fingerprint, status, response_status, response_body) = existing;

    if stored_fingerprint != fingerprint {
        return Ok(Idempotency::Mismatch);
    }

    match (status.as_str(), response_status, response_body) {
        ("completed", Some(status_code), Some(body)) => Ok(Idempotency::Replay {
            status: status_code,
            body,
        }),
        _ => Ok(Idempotency::InProgress),
    }
}

async fn insert_job_chain(
    tx: &mut Transaction<'_, Postgres>,
    owner_user_id: Uuid,
    session_id: Uuid,
    request_text: &str,
    office_ids: &[i64],
    fineract_tenant: &str,
) -> Result<Job, CreateError> {
    let session = session_repository::find_in_tx(tx, session_id)
        .await?
        .ok_or(CreateError::SessionNotFound)?;

    if session.owner_user_id != owner_user_id {
        return Err(CreateError::SessionNotFound);
    }

    if session.status != "active" {
        return Err(CreateError::SessionNotActive(session.status));
    }

    // Snapshot scope + PII efektif saat accept (#15 aturan 1). Tanpa ini,
    // laporan lama berubah makna ketika konfigurasi diubah.
    let pii = settings::current_pii(tx).await?;
    let scope = serde_json::json!({
        "pii": { "enabled": pii.enabled, "mode": pii.mode, "setting_version": pii.setting_version },
        "source": "admin_projection",
        "office_ids": office_ids,
        "fineract_tenant": fineract_tenant,
    });

    let job = sqlx::query_as::<_, Job>("INSERT INTO chat_jobs (session_id, owner_user_id, request_text, scope_json, lifecycle)
         VALUES ($1, $2, $3, $4, 'Queued')
         RETURNING id, session_id, owner_user_id, request_text, lifecycle, outcome,
     completeness, completeness_reason, failure_code, plan_version, final_response_version,
     last_event_sequence, scope_json, created_at, terminal_at"
    )
    .bind(session_id)
    .bind(owner_user_id)
    .bind(request_text)
    .bind(&scope)
    .fetch_one(&mut **tx)
    .await
    .map_err(|error| match constraint_of(&error) {
        // Satu job nonterminal per session (#13) ditegakkan unique index,
        // bukan pemeriksaan aplikasi yang bisa kalah balapan.
        Some("chat_jobs_one_active_per_session") => CreateError::SessionBusy,
        _ => CreateError::Database(error),
    })?;

    let sequence = append_event(
        tx,
        job.id,
        "job.accepted",
        Some(serde_json::json!({ "lifecycle": "Queued" })),
    )
    .await?;

    sqlx::query("INSERT INTO chat_messages (session_id, job_id, role) VALUES ($1, $2, 'user')")
        .bind(session_id)
        .bind(job.id)
        .execute(&mut **tx)
        .await?;

    audit::insert(
        tx,
        AuditEvent {
            actor_kind: "user",
            actor_user_id: Some(owner_user_id),
            session_id: Some(session_id),
            job_id: Some(job.id),
            stage: "accept",
            action: "job.create",
            result: "ok",
            // Metadata scope, bukan nilai filter.
            scope_json: Some(serde_json::json!({
                "pii_enabled": pii.enabled,
                "pii_setting_version": pii.setting_version,
                "office_id_count": office_ids.len(),
                "fineract_tenant": fineract_tenant,
            })),
            detail_json: Some(serde_json::json!({ "request_chars": request_text.chars().count() })),
            ..Default::default()
        },
    )
    .await?;

    session_repository::touch(tx, session_id).await?;

    Ok(Job {
        last_event_sequence: sequence,
        ..job
    })
}

/// Referensi bertipe pada sebuah event.
///
/// Ini **kolom**, bukan isi payload (migrasi 4): kolom selalu tersedia berapa
/// pun ambang inline payload, sehingga ambang itu dapat diubah tanpa membuat
/// klien kehilangan rujukan yang ia pakai untuk menyusun tampilan.
#[derive(Debug, Default, Clone, Copy)]
pub struct EventRef<'a> {
    pub plan_version: Option<i32>,
    pub node_id: Option<&'a str>,
    pub node_attempt: Option<i32>,
    pub clarification_id: Option<Uuid>,
    pub clarification_revision: Option<i32>,
    pub response_version: Option<i32>,
}

/// Alokasikan sequence lalu sisipkan event, atomik dengan transisi state (C14).
///
/// Alokator adalah `UPDATE ... RETURNING` pada baris job — bukan sequence
/// PostgreSQL, yang meninggalkan lubang saat rollback (I3).
pub async fn append_event(
    tx: &mut Transaction<'_, Postgres>,
    job_id: Uuid,
    event_type: &str,
    payload: Option<Value>,
) -> sqlx::Result<i64> {
    append_event_ref(tx, job_id, event_type, EventRef::default(), payload).await
}

/// Seperti [`append_event`], dengan referensi bertipe yang ikut sebagai kolom.
pub async fn append_event_ref(
    tx: &mut Transaction<'_, Postgres>,
    job_id: Uuid,
    event_type: &str,
    references: EventRef<'_>,
    payload: Option<Value>,
) -> sqlx::Result<i64> {
    let sequence = sqlx::query_scalar::<_, i64>(
        "UPDATE chat_jobs SET last_event_sequence = last_event_sequence + 1, updated_at = now()
         WHERE id = $1 RETURNING last_event_sequence",
    )
    .bind(job_id)
    .fetch_one(&mut **tx)
    .await?;

    sqlx::query(
        "INSERT INTO job_events
            (job_id, sequence, event_type, plan_version, node_id, node_attempt,
             clarification_id, clarification_revision, response_version, payload_json)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
    )
    .bind(job_id)
    .bind(sequence)
    .bind(event_type)
    .bind(references.plan_version)
    .bind(references.node_id)
    .bind(references.node_attempt)
    .bind(references.clarification_id)
    .bind(references.clarification_revision)
    .bind(references.response_version)
    .bind(payload)
    .execute(&mut **tx)
    .await?;

    // Notifikasi dipancarkan DI SINI, satu-satunya tempat sequence dialokasikan,
    // dan sebagai `pg_notify` — bukan panggilan jaringan. Ia tetap tulisan lokal
    // PostgreSQL (I1) dan baru terkirim saat transaksi commit, sehingga mustahil
    // memberi tahu subscriber tentang event yang kemudian di-rollback.
    //
    // Memancarkannya dari tiap pemanggil akan mengulang kesalahan yang
    // AGENTS.md peringatkan: satu jalur transisi lupa memancarkan, dan tidak ada
    // yang gagal — stream hanya terlambat sampai fallback polling menutupinya.
    sqlx::query("SELECT pg_notify('job_events', $1)")
        .bind(job_id.to_string())
        .execute(&mut **tx)
        .await?;

    Ok(sequence)
}

pub async fn find(pool: &PgPool, job_id: Uuid) -> sqlx::Result<Option<Job>> {
    sqlx::query_as::<_, Job>("SELECT id, session_id, owner_user_id, request_text, lifecycle, outcome,
     completeness, completeness_reason, failure_code, plan_version, final_response_version,
     last_event_sequence, scope_json, created_at, terminal_at FROM chat_jobs WHERE id = $1")
        .bind(job_id)
        .fetch_optional(pool)
        .await
}

/// T9 — cancel. Menandai saja; worker yang menuntaskan dan menutup job.
///
/// `WHERE lifecycle IN (...)` membuat job terminal tidak pernah dibuka kembali,
/// dan cancel berulang menjadi no-op alih-alih transisi kedua.
pub async fn request_cancel(pool: &PgPool, job_id: Uuid, actor_user_id: Uuid) -> sqlx::Result<bool> {
    let mut tx = pool.begin().await?;

    let updated = sqlx::query(
        "UPDATE chat_jobs
         SET lifecycle = 'Cancelling', cancel_requested_at = now(), updated_at = now()
         WHERE id = $1 AND lifecycle IN ('Queued','Running','WaitingForUser')",
    )
    .bind(job_id)
    .execute(&mut *tx)
    .await?
    .rows_affected();

    if updated == 0 {
        tx.rollback().await?;
        return Ok(false);
    }

    append_event(&mut tx, job_id, "job.cancelling", None).await?;

    audit::insert(
        &mut tx,
        AuditEvent {
            actor_kind: "user",
            actor_user_id: Some(actor_user_id),
            job_id: Some(job_id),
            stage: "settle",
            action: "job.cancel_requested",
            result: "ok",
            ..Default::default()
        },
    )
    .await?;

    tx.commit().await?;
    Ok(true)
}

/// Kegagalan T1 yang punya arti berbeda bagi klien.
#[derive(Debug)]
pub enum CreateError {
    SessionNotFound,
    SessionNotActive(String),
    /// Sudah ada job nonterminal pada session ini (#13).
    SessionBusy,
    /// Kunci idempotency dipakai ulang dengan payload berbeda.
    KeyReused,
    /// Permintaan dengan kunci sama masih diproses.
    InProgress,
    Database(sqlx::Error),
}

impl From<sqlx::Error> for CreateError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

fn constraint_of(error: &sqlx::Error) -> Option<&str> {
    match error {
        sqlx::Error::Database(database_error) => database_error.constraint(),
        _ => None,
    }
}
