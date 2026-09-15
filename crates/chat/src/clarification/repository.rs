//! SQL klarifikasi: T5 (form dibuat, job ditangguhkan) dan T6 (jawaban
//! diterima, job dilanjutkan).
//!
//! Form **immutable per revision**: form adalah apa yang dilihat pengguna saat
//! menjawab, dan update in-place menghapus bukti tampilan sekaligus membuat cek
//! stale-revision menjadi tebakan (migrasi 5).

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::{
    audit::{self, AuditEvent},
    job::repository::append_event,
};

/// Form yang sedang terbuka untuk sebuah job.
#[derive(Debug, Clone, FromRow, serde::Serialize)]
pub struct Form {
    pub id: Uuid,
    pub job_id: Uuid,
    pub clarification_id: Uuid,
    pub revision: i32,
    pub schema_version: i32,
    pub purpose: Option<String>,
    /// Label deskriptif, bukan "step 2 of 5": jumlah tahap tidak dikarang.
    pub stage_label: Option<String>,
    pub fields_json: Value,
    pub state: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// T5 — klarifikasi dibutuhkan.
///
/// Job ditangguhkan ke `WaitingForUser` dan `expires_at` **dihitung ulang** dari
/// `waiting_since + CLARIFICATION_WAIT_LIMIT` (amandemen K3): memakai TTL job
/// yang sedang berjalan akan meng-`Expired` percakapan sah yang dijawab lebih
/// lama daripada TTL itu.
#[allow(clippy::too_many_arguments)]
pub async fn open_form(
    pool: &PgPool,
    job_id: Uuid,
    session_id: Uuid,
    lease_token: Uuid,
    plan_version: Option<i32>,
    purpose: &str,
    stage_label: &str,
    fields: &Value,
    wait_limit_secs: i64,
) -> sqlx::Result<Option<Form>> {
    let mut tx = pool.begin().await?;

    let updated = sqlx::query(
        "UPDATE chat_jobs
         SET lifecycle = 'WaitingForUser',
             waiting_since = now(),
             expires_at = now() + make_interval(secs => $3),
             -- Lease DILEPAS: tidak ada worker yang sedang mengerjakan job yang
             -- menunggu manusia. Membiarkannya terisi membuat job yang sudah
             -- dijawab menganggur sampai lease lama kedaluwarsa — penundaan
             -- yang tidak terlihat sebagai kesalahan apa pun.
             lease_owner = NULL,
             lease_token = NULL,
             lease_expires_at = NULL,
             updated_at = now()
         WHERE id = $1 AND lease_token = $2 AND lifecycle = 'Running'",
    )
    .bind(job_id)
    .bind(lease_token)
    .bind(wait_limit_secs as f64)
    .execute(&mut *tx)
    .await?
    .rows_affected();

    if updated == 0 {
        tx.rollback().await?;
        return Ok(None);
    }

    let clarification_id = Uuid::new_v4();
    let form = sqlx::query_as::<_, Form>(
        "INSERT INTO clarification_forms
            (job_id, clarification_id, revision, plan_version, purpose, stage_label,
             fields_json, state, expires_at)
         VALUES ($1, $2, 1, $3, $4, $5, $6, 'open', now() + make_interval(secs => $7))
         RETURNING id, job_id, clarification_id, revision, schema_version, purpose,
     stage_label, fields_json, state, expires_at, created_at",
    )
    .bind(job_id)
    .bind(clarification_id)
    .bind(plan_version)
    .bind(purpose)
    .bind(stage_label)
    .bind(fields)
    .bind(wait_limit_secs as f64)
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO chat_messages
            (session_id, job_id, role, clarification_id, clarification_revision)
         VALUES ($1, $2, 'clarification', $3, 1)",
    )
    .bind(session_id)
    .bind(job_id)
    .bind(clarification_id)
    .execute(&mut *tx)
    .await?;

    append_event(
        &mut tx,
        job_id,
        "clarification.required",
        Some(serde_json::json!({
            "clarification_id": clarification_id,
            "revision": 1,
            "fields": fields,
        })),
    )
    .await?;

    audit::insert(
        &mut tx,
        AuditEvent {
            actor_kind: "worker",
            job_id: Some(job_id),
            session_id: Some(session_id),
            stage: "clarify",
            action: "clarification.opened",
            result: "deferred",
            detail_json: Some(serde_json::json!({ "clarification_id": clarification_id })),
            ..Default::default()
        },
    )
    .await?;

    tx.commit().await?;
    Ok(Some(form))
}

/// Form terbuka milik sebuah job, bila ada.
pub async fn open_form_of(pool: &PgPool, job_id: Uuid) -> sqlx::Result<Option<Form>> {
    sqlx::query_as::<_, Form>(
        "SELECT id, job_id, clarification_id, revision, schema_version, purpose,
     stage_label, fields_json, state, expires_at, created_at
         FROM clarification_forms
         WHERE job_id = $1 AND state = 'open'",
    )
    .bind(job_id)
    .fetch_optional(pool)
    .await
}

/// Jawaban yang sudah diterima untuk sebuah job, dikunci `field_id`.
///
/// Dibaca saat worker melanjutkan job: jawaban yang sudah durable tidak pernah
/// ditanyakan ulang.
pub async fn accepted_answers(pool: &PgPool, job_id: Uuid) -> sqlx::Result<BTreeMap<String, String>> {
    let rows = sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT a.field_id, a.raw_text
         FROM clarification_answers a
         JOIN clarification_forms f ON f.id = a.form_id
         WHERE f.job_id = $1
         ORDER BY a.answered_at",
    )
    .bind(job_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .filter_map(|(field, value)| value.map(|value| (field, value)))
        .collect())
}

/// T6 — jawaban diterima.
///
/// Urutan pemeriksaan sudah dilakukan service (idempotency → ownership →
/// lifecycle → revision → validasi field); di sini hanya penulisan atomiknya.
/// Job kembali ke `Queued` dengan `expires_at` dihitung ulang (K3); `job.resumed`
/// dipancarkan saat worker benar-benar melanjutkan, bukan di sini.
pub async fn accept_answers(
    pool: &PgPool,
    form: &Form,
    session_id: Uuid,
    answered_by: Uuid,
    answers: &BTreeMap<String, String>,
    job_ttl_running_secs: i64,
) -> sqlx::Result<bool> {
    let mut tx = pool.begin().await?;

    // `WHERE state = 'open'` adalah penegaknya: dua pengiriman yang berlomba
    // hanya menghasilkan satu penerimaan, dan yang kalah menyentuh 0 baris.
    let closed = sqlx::query(
        "UPDATE clarification_forms
         SET state = 'answered', resolved_by_user_id = $2, resolved_at = now(),
             resolution_reason = 'answered'
         WHERE id = $1 AND state = 'open'",
    )
    .bind(form.id)
    .bind(answered_by)
    .execute(&mut *tx)
    .await?
    .rows_affected();

    if closed == 0 {
        tx.rollback().await?;
        return Ok(false);
    }

    for (field_id, value) in answers {
        sqlx::query(
            "INSERT INTO clarification_answers
                (form_id, field_id, answer_kind, raw_text, binding_json, provenance,
                 answered_by_user_id)
             VALUES ($1, $2, 'typed_value', $3, $4, 'user_confirmed', $5)",
        )
        .bind(form.id)
        .bind(field_id)
        .bind(value)
        .bind(serde_json::json!({ "value": value }))
        .bind(answered_by)
        .execute(&mut *tx)
        .await?;
    }

    sqlx::query(
        "UPDATE chat_jobs
         SET lifecycle = 'Queued',
             waiting_since = NULL,
             expires_at = now() + make_interval(secs => $2),
             updated_at = now()
         WHERE id = $1 AND lifecycle = 'WaitingForUser'",
    )
    .bind(form.job_id)
    .bind(job_ttl_running_secs as f64)
    .execute(&mut *tx)
    .await?;

    append_event(
        &mut tx,
        form.job_id,
        "clarification.accepted",
        Some(serde_json::json!({
            "clarification_id": form.clarification_id,
            "revision": form.revision,
            "answered_fields": answers.keys().collect::<Vec<_>>(),
        })),
    )
    .await?;

    audit::insert(
        &mut tx,
        AuditEvent {
            actor_kind: "user",
            actor_user_id: Some(answered_by),
            job_id: Some(form.job_id),
            session_id: Some(session_id),
            stage: "clarify",
            action: "clarification.answered",
            result: "ok",
            // Nilai jawaban TIDAK ikut: ia dapat memuat PII. Yang dicatat
            // hanya field mana yang dijawab.
            detail_json: Some(serde_json::json!({
                "clarification_id": form.clarification_id,
                "fields": answers.keys().collect::<Vec<_>>(),
            })),
            ..Default::default()
        },
    )
    .await?;

    tx.commit().await?;
    Ok(true)
}
