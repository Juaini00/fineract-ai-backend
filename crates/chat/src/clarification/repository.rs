//! SQL klarifikasi: T5 (form dibuat, job ditangguhkan) dan T6 (jawaban
//! diterima, job dilanjutkan).
//!
//! Form **immutable per revision**: form adalah apa yang dilihat pengguna saat
//! menjawab, dan update in-place menghapus bukti tampilan sekaligus membuat cek
//! stale-revision menjadi tebakan (migrasi 5).

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{
    audit::{self, AuditEvent},
    engine::compose::AutoBound,
    job::repository::{EventRef, append_event_ref},
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

/// Satu jawaban yang sudah lolos kebijakan dan siap ditulis.
///
/// `answer_kind` / `raw_text` / `binding_json` sengaja terpisah (C10): teks
/// bebas tidak pernah menjadi binding identitas, dan `provenance` membedakan
/// "pengguna mengonfirmasi" dari "kebetulan hanya ada satu" (K5).
#[derive(Debug, Clone)]
pub struct AcceptedAnswer {
    pub field_id: String,
    pub answer_kind: &'static str,
    pub raw_text: Option<String>,
    pub binding_json: Value,
    pub provenance: &'static str,
    pub resolver_ref: Option<String>,
    pub option_set_ref: Option<String>,
}

/// T5 — klarifikasi dibutuhkan.
///
/// Job ditangguhkan ke `WaitingForUser` dan `expires_at` **dihitung ulang** dari
/// `waiting_since + CLARIFICATION_WAIT_LIMIT` (amandemen K3): memakai TTL job
/// yang sedang berjalan akan meng-`Expired` percakapan sah yang dijawab lebih
/// lama daripada TTL itu.
///
/// `auto_bound` adalah slot yang resolver-nya hanya mengembalikan satu kandidat
/// dalam scope. Bila ia menutupi **seluruh** field, job tidak pernah masuk
/// `WaitingForUser`: tidak ada yang ditanyakan, jadi menyatakan job menunggu
/// manusia adalah kebohongan kecil yang membuat metrik waktu tunggu salah.
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
    auto_bound: &[AcceptedAnswer],
    wait_limit_secs: i64,
    job_ttl_running_secs: i64,
) -> sqlx::Result<Option<Form>> {
    let field_count = fields.as_array().map_or(0, Vec::len);
    let fully_resolved = field_count > 0 && auto_bound.len() == field_count;

    let mut tx = pool.begin().await?;

    let updated = if fully_resolved {
        // Langsung kembali mengantre: worker berikutnya merencanakan ulang
        // dengan binding yang sudah durable.
        sqlx::query(
            "UPDATE chat_jobs
             SET lifecycle = 'Queued',
                 waiting_since = NULL,
                 expires_at = now() + make_interval(secs => $3),
                 lease_owner = NULL,
                 lease_token = NULL,
                 lease_expires_at = NULL,
                 updated_at = now()
             WHERE id = $1 AND lease_token = $2 AND lifecycle = 'Running'",
        )
        .bind(job_id)
        .bind(lease_token)
        .bind(job_ttl_running_secs as f64)
    } else {
        sqlx::query(
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
    }
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
             fields_json, state, resolved_at, resolution_reason, expires_at)
         VALUES ($1, $2, 1, $3, $4, $5, $6,
                 CASE WHEN $8 THEN 'answered' ELSE 'open' END,
                 CASE WHEN $8 THEN now() END,
                 CASE WHEN $8 THEN 'resolver_unique' END,
                 now() + make_interval(secs => $7))
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
    .bind(fully_resolved)
    .fetch_one(&mut *tx)
    .await?;

    for answer in auto_bound {
        insert_answer(&mut tx, form.id, answer, None).await?;
    }

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

    let auto_bound_fields: Vec<&str> = auto_bound
        .iter()
        .map(|answer| answer.field_id.as_str())
        .collect();

    append_event_ref(
        &mut tx,
        job_id,
        if fully_resolved {
            "clarification.auto_resolved"
        } else {
            "clarification.required"
        },
        EventRef {
            clarification_id: Some(clarification_id),
            clarification_revision: Some(1),
            plan_version,
            ..Default::default()
        },
        Some(serde_json::json!({
            "clarification_id": clarification_id,
            "revision": 1,
            "fields": fields,
            // K5 — auto-bind terlihat di stream, bukan hanya di response.
            "auto_bound_fields": auto_bound_fields,
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
            result: if fully_resolved { "ok" } else { "deferred" },
            detail_json: Some(serde_json::json!({
                "clarification_id": clarification_id,
                "auto_bound_fields": auto_bound_fields,
            })),
            ..Default::default()
        },
    )
    .await?;

    tx.commit().await?;
    Ok(Some(form))
}

/// Node yang outputnya sudah durable dan sah untuk job ini.
///
/// Menentukan `completeness` skip: ada output valid berarti `Partial`, tidak ada
/// berarti `Unknown`. `Complete` **dilarang** (engine.md) — pengguna berhenti di
/// tengah, jadi tidak ada klaim kelengkapan yang dapat dibuat.
pub async fn completed_node_count(pool: &PgPool, job_id: Uuid) -> sqlx::Result<i64> {
    sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM job_node_runs WHERE job_id = $1 AND status = 'Completed'",
    )
    .bind(job_id)
    .fetch_one(pool)
    .await
}

/// T8 — pengguna memilih tidak melanjutkan.
///
/// Skip **bukan abort**: ia memakai jalur commit yang sama dengan jawaban
/// normal — response document, pesan assistant, event terminal dan audit dalam
/// satu transaksi. Job yang berhenti tanpa dokumen apa pun akan meninggalkan
/// pengguna tanpa penjelasan dan investigasi tanpa jejak.
///
/// Tidak ada `lease_token` di sini karena tidak ada worker yang memegang job
/// yang menunggu manusia — lease dilepas saat T5. Yang memagari jalur ini adalah
/// `WHERE lifecycle = 'WaitingForUser'`: skip kedua menyentuh 0 baris.
pub async fn skip(
    pool: &PgPool,
    form: &Form,
    session_id: Uuid,
    actor_user_id: Uuid,
    response: SkippedResponse,
) -> sqlx::Result<bool> {
    let mut tx = pool.begin().await?;

    const RESPONSE_VERSION: i32 = 1;

    let closed = sqlx::query(
        "UPDATE clarification_forms
         SET state = 'skipped', resolved_by_user_id = $2, resolved_at = now(),
             resolution_reason = 'skipped_by_user'
         WHERE id = $1 AND state = 'open'",
    )
    .bind(form.id)
    .bind(actor_user_id)
    .execute(&mut *tx)
    .await?
    .rows_affected();

    if closed == 0 {
        tx.rollback().await?;
        return Ok(false);
    }

    let settled = sqlx::query(
        "UPDATE chat_jobs
         SET lifecycle = 'Completed',
             outcome = 'SkippedByUser',
             completeness = $2,
             completeness_reason = $3,
             final_response_version = $4,
             waiting_since = NULL,
             terminal_at = now(),
             expires_at = NULL,
             lease_owner = NULL,
             lease_token = NULL,
             lease_expires_at = NULL,
             updated_at = now()
         WHERE id = $1 AND lifecycle = 'WaitingForUser'",
    )
    .bind(form.job_id)
    .bind(response.completeness)
    .bind(&response.completeness_reason)
    .bind(RESPONSE_VERSION)
    .execute(&mut *tx)
    .await?
    .rows_affected();

    if settled == 0 {
        // Lifecycle berpindah di antara pemeriksaan service dan transaksi ini
        // (mis. reaper meng-Expired). Terminal bersifat immutable: berhenti.
        tx.rollback().await?;
        return Ok(false);
    }

    sqlx::query(
        "INSERT INTO job_responses
            (job_id, response_version, kind, outcome, completeness, completeness_reason,
             blocks_json, validation_status, response_hash, composed_at)
         VALUES ($1, $2, 'skipped', 'SkippedByUser', $3, $4, $5, 'passed', $6, now())",
    )
    .bind(form.job_id)
    .bind(RESPONSE_VERSION)
    .bind(response.completeness)
    .bind(&response.completeness_reason)
    .bind(&response.blocks)
    .bind(&response.response_hash)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO chat_messages (session_id, job_id, role, response_version)
         VALUES ($1, $2, 'assistant', $3)",
    )
    .bind(session_id)
    .bind(form.job_id)
    .bind(RESPONSE_VERSION)
    .execute(&mut *tx)
    .await?;

    append_event_ref(
        &mut tx,
        form.job_id,
        "job.completed",
        EventRef {
            clarification_id: Some(form.clarification_id),
            clarification_revision: Some(form.revision),
            response_version: Some(RESPONSE_VERSION),
            ..Default::default()
        },
        Some(serde_json::json!({
            "outcome": "SkippedByUser",
            "completeness": response.completeness,
        })),
    )
    .await?;

    audit::insert(
        &mut tx,
        AuditEvent {
            actor_kind: "user",
            actor_user_id: Some(actor_user_id),
            job_id: Some(form.job_id),
            session_id: Some(session_id),
            // `commit`, bukan `clarify`: skip MENULIS response document, dan
            // CHECK `audit_events_job_result_scope` hanya mengizinkan
            // outcome/completeness pada stage yang memang menyelesaikan job.
            // Constraint itu menangkap kekeliruan ini sebagai 500 sebelum satu
            // pun baris tersimpan — persis gunanya.
            stage: "commit",
            action: "job.skipped",
            result: "ok",
            job_outcome: Some("SkippedByUser"),
            job_completeness: Some(response.completeness),
            detail_json: Some(serde_json::json!({
                "clarification_id": form.clarification_id,
                "unanswered_fields": response.unanswered_fields,
            })),
            ..Default::default()
        },
    )
    .await?;

    tx.commit().await?;
    Ok(true)
}

/// Dokumen yang ditulis saat skip.
#[derive(Debug)]
pub struct SkippedResponse {
    /// `Partial` atau `Unknown`; `Complete` dilarang.
    pub completeness: &'static str,
    pub completeness_reason: String,
    pub blocks: Value,
    pub response_hash: String,
    pub unanswered_fields: Vec<String>,
}

/// Hanya NAMA field yang dicatat — nilainya dapat memuat PII.
fn answered_fields(answers: &[AcceptedAnswer]) -> Vec<&str> {
    answers
        .iter()
        .map(|answer| answer.field_id.as_str())
        .collect()
}

async fn insert_answer(
    tx: &mut Transaction<'_, Postgres>,
    form_id: Uuid,
    answer: &AcceptedAnswer,
    answered_by: Option<Uuid>,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO clarification_answers
            (form_id, field_id, answer_kind, raw_text, binding_json, provenance,
             resolver_ref, option_set_ref, answered_by_user_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(form_id)
    .bind(&answer.field_id)
    .bind(answer.answer_kind)
    .bind(&answer.raw_text)
    .bind(&answer.binding_json)
    .bind(answer.provenance)
    .bind(&answer.resolver_ref)
    .bind(&answer.option_set_ref)
    .bind(answered_by)
    .execute(&mut **tx)
    .await?;

    Ok(())
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
/// ditanyakan ulang. Jawaban `option_id` tidak punya `raw_text` sama sekali —
/// yang mengikat adalah `binding_json`, dan itulah maksud pemisahan kolomnya
/// (C10/K1).
pub async fn accepted_answers(pool: &PgPool, job_id: Uuid) -> sqlx::Result<BTreeMap<String, String>> {
    let rows = sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT a.field_id, COALESCE(a.binding_json->>'value', a.raw_text)
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

/// Slot yang diikat resolver tanpa bertanya (K5). Dibaca saat komposisi supaya
/// pengungkapannya berasal dari yang benar-benar tersimpan, bukan dari ingatan
/// worker.
pub async fn auto_bound_slots(pool: &PgPool, job_id: Uuid) -> sqlx::Result<Vec<AutoBound>> {
    let rows = sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT a.field_id, a.binding_json->>'label'
         FROM clarification_answers a
         JOIN clarification_forms f ON f.id = a.form_id
         WHERE f.job_id = $1 AND a.provenance = 'resolver_unique'
         ORDER BY a.field_id",
    )
    .bind(job_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(field_id, label)| AutoBound { field_id, label })
        .collect())
}

/// Opsi yang **benar-benar diterbitkan** untuk sebuah halaman.
///
/// `ON CONFLICT DO NOTHING`: mengambil ulang halaman yang sama bukan penerbitan
/// kedua, dan `issued_at` yang pertama adalah yang benar. `expires_at`
/// diturunkan dari form (K4) — opsi tidak boleh kedaluwarsa lebih dulu daripada
/// form yang memuatnya.
pub async fn issue_options(
    pool: &PgPool,
    form: &Form,
    field_id: &str,
    resolver_ref: &str,
    page_cursor: &str,
    options: &[(String, Value, String, Value)],
) -> sqlx::Result<()> {
    let mut tx = pool.begin().await?;

    for (option_id, binding, label, attributes) in options {
        sqlx::query(
            "INSERT INTO clarification_options
                (form_id, field_id, option_id, binding_json, label, attributes_json,
                 resolver_ref, page_cursor, expires_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
             ON CONFLICT (form_id, field_id, option_id) DO NOTHING",
        )
        .bind(form.id)
        .bind(field_id)
        .bind(option_id)
        .bind(binding)
        .bind(label)
        .bind(attributes)
        .bind(resolver_ref)
        .bind(page_cursor)
        .bind(form.expires_at)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(())
}

/// Binding sebuah opsi yang pernah diterbitkan untuk form+field ini.
///
/// `None` berarti opsi itu tidak pernah dikirim: id opsi dari form lain, dari
/// job lain, atau dikarang klien (C9). Keanggotaan **bukan** otorisasi — scope
/// tetap diperiksa ulang ke sumber saat submit.
pub async fn issued_option(
    pool: &PgPool,
    form_id: Uuid,
    field_id: &str,
    option_id: &str,
) -> sqlx::Result<Option<(Value, Option<String>, Option<String>)>> {
    sqlx::query_as::<_, (Value, Option<String>, Option<String>)>(
        "SELECT binding_json, label, resolver_ref
         FROM clarification_options
         WHERE form_id = $1 AND field_id = $2 AND option_id = $3",
    )
    .bind(form_id)
    .bind(field_id)
    .bind(option_id)
    .fetch_optional(pool)
    .await
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
    answers: &[AcceptedAnswer],
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

    for answer in answers {
        insert_answer(&mut tx, form.id, answer, Some(answered_by)).await?;
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

    append_event_ref(
        &mut tx,
        form.job_id,
        "clarification.accepted",
        EventRef {
            clarification_id: Some(form.clarification_id),
            clarification_revision: Some(form.revision),
            ..Default::default()
        },
        Some(serde_json::json!({
            "clarification_id": form.clarification_id,
            "revision": form.revision,
            "answered_fields": answered_fields(answers),
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
                "fields": answered_fields(answers),
            })),
            ..Default::default()
        },
    )
    .await?;

    tx.commit().await?;
    Ok(true)
}
