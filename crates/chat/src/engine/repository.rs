//! Transaksi Engine: klaim (T2), heartbeat, penyelesaian (T7/T8), dan reaper (T11).
//!
//! **Fencing (C16)**: setiap tulisan durable milik worker memakai
//! `AND lease_token = $token`. Update yang menyentuh 0 baris berarti worker
//! sudah dipagari dan wajib berhenti — bukan mencoba transisi lain.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{
    audit::{self, AuditEvent},
    engine::memory::MemoryFact,
    job::repository::{EventRef, append_event, append_event_ref},
};

/// Job yang berhasil diklaim, beserta token pagarnya.
#[derive(Debug, Clone, FromRow)]
pub struct ClaimedJob {
    pub id: Uuid,
    pub session_id: Uuid,
    pub owner_user_id: Uuid,
    pub request_text: String,
    pub scope_json: Value,
    pub lease_token: Uuid,
}

/// T2 — klaim satu job `Queued`.
///
/// `FOR UPDATE SKIP LOCKED` membuat dua worker tidak pernah memperebutkan baris
/// yang sama, dan `lease_expires_at` yang kedaluwarsa membuat job milik worker
/// mati dapat diambil alih tanpa koordinasi tambahan.
pub async fn claim_next(
    pool: &PgPool,
    worker: &str,
    lease_duration_secs: i64,
    job_ttl_running_secs: i64,
) -> sqlx::Result<Option<ClaimedJob>> {
    let mut tx = pool.begin().await?;

    let claimed = sqlx::query_as::<_, ClaimedJob>(
        "UPDATE chat_jobs
         SET lifecycle = 'Running',
             lease_owner = $1,
             lease_token = gen_random_uuid(),
             lease_claimed_at = now(),
             heartbeat_at = now(),
             lease_expires_at = now() + make_interval(secs => $2),
             started_at = COALESCE(started_at, now()),
             -- K3: expires_at dihitung ULANG pada setiap transisi fase. Tanpa
             -- ini, job yang sempat menunggu klarifikasi lama akan langsung
             -- kedaluwarsa begitu dilanjutkan.
             expires_at = now() + make_interval(secs => $3),
             updated_at = now()
         WHERE id = (
             SELECT id FROM chat_jobs
             WHERE lifecycle = 'Queued'
               AND (lease_expires_at IS NULL OR lease_expires_at < now())
               -- Job yang sudah melewati batas waktunya BUKAN milik worker:
               -- klaim menghitung ulang expires_at (K3), sehingga memungutnya
               -- akan memberinya umur baru dan menjadikan kedaluwarsa sekadar
               -- balapan melawan interval polling. Reaper yang menutupnya.
               AND (expires_at IS NULL OR expires_at > now())
             ORDER BY created_at
             FOR UPDATE SKIP LOCKED
             LIMIT 1
         )
         RETURNING id, session_id, owner_user_id, request_text, scope_json, lease_token",
    )
    .bind(worker)
    .bind(lease_duration_secs as f64)
    .bind(job_ttl_running_secs as f64)
    .fetch_optional(&mut *tx)
    .await?;

    let Some(job) = claimed else {
        tx.rollback().await?;
        return Ok(None);
    };

    // `job.resumed` mendahului fase, karena ia menandai APA yang dimulai:
    // kelanjutan, bukan pengerjaan pertama. Urutan terbalik akan membuat klien
    // menampilkan "Understanding your request" untuk job yang sebenarnya sedang
    // melanjutkan pekerjaan yang sudah berjalan sebelumnya.
    let resumed = was_suspended(&mut tx, job.id).await?;
    if resumed {
        append_event(&mut tx, job.id, "job.resumed", None).await?;
    }

    append_event(
        &mut tx,
        job.id,
        "job.phase_changed",
        // Kosakata fase PUBLIK (sse.md), bukan nama lifecycle internal: fase
        // adalah proyeksi pengalaman, dan membocorkan lifecycle memberi klien
        // state machine kedua untuk diikuti.
        Some(serde_json::json!({ "phase": "understanding", "message": "Understanding your request." })),
    )
    .await?;

    audit::insert(
        &mut tx,
        AuditEvent {
            actor_kind: "worker",
            job_id: Some(job.id),
            session_id: Some(job.session_id),
            stage: "accept",
            action: "job.claimed",
            result: "ok",
            detail_json: Some(serde_json::json!({ "worker": worker, "resumed": resumed })),
            ..Default::default()
        },
    )
    .await?;

    tx.commit().await?;
    Ok(Some(job))
}

/// Apakah job ini pernah benar-benar ditangguhkan menunggu manusia.
///
/// Dibaca dari bukti durable: ada form yang DIJAWAB manusia. Form yang tertutup
/// karena `resolver_unique` tidak dihitung — job itu tidak pernah menunggu
/// siapa pun, jadi tidak ada yang dilanjutkan.
///
/// Inilah sebabnya `job.resumed` lahir di sini dan bukan saat jawaban diterima:
/// jawaban hanya mengembalikan job ke antrean, dan di antara itu dan pengerjaan
/// berikutnya job masih dapat kedaluwarsa atau dibatalkan. Memancarkannya lebih
/// awal berarti memberi tahu pengguna bahwa pekerjaan dilanjutkan pada saat
/// belum ada satu pun worker yang menyentuhnya.
async fn was_suspended(tx: &mut Transaction<'_, Postgres>, job_id: Uuid) -> sqlx::Result<bool> {
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (
             SELECT 1 FROM clarification_forms
             WHERE job_id = $1 AND state = 'answered' AND resolution_reason = 'answered'
         )",
    )
    .bind(job_id)
    .fetch_one(&mut **tx)
    .await
}

/// Perpanjang lease. `false` berarti worker sudah dipagari (token tidak cocok,
/// lifecycle berpindah, atau job terhapus) dan wajib berhenti.
pub async fn renew_lease(
    pool: &PgPool,
    job_id: Uuid,
    lease_token: Uuid,
    lease_duration_secs: i64,
) -> sqlx::Result<bool> {
    let affected = sqlx::query(
        "UPDATE chat_jobs
         SET heartbeat_at = now(),
             lease_expires_at = now() + make_interval(secs => $3)
         WHERE id = $1 AND lease_token = $2 AND lifecycle = 'Running'",
    )
    .bind(job_id)
    .bind(lease_token)
    .bind(lease_duration_secs as f64)
    .execute(pool)
    .await?
    .rows_affected();

    Ok(affected == 1)
}

/// Apakah pembatalan sudah diminta untuk job ini.
pub async fn cancel_requested(pool: &PgPool, job_id: Uuid) -> sqlx::Result<bool> {
    sqlx::query_scalar::<_, bool>(
        "SELECT cancel_requested_at IS NOT NULL OR lifecycle = 'Cancelling'
         FROM chat_jobs WHERE id = $1",
    )
    .bind(job_id)
    .fetch_optional(pool)
    .await
    .map(|found| found.unwrap_or(false))
}

/// Response yang akan dipersist bersama penyelesaian job.
#[derive(Debug)]
pub struct SettledResponse {
    pub kind: &'static str,
    pub outcome: &'static str,
    pub completeness: &'static str,
    pub completeness_reason: String,
    pub blocks: Value,
    pub response_hash: String,
}

/// Promosikan fakta session di dalam transaksi commit (memory-context.md §3).
///
/// Tiga hal yang tidak boleh dipisahkan dari sini:
///
/// - **`session_seq` dialokasikan lewat row lock induknya** (I3):
///   `UPDATE chat_sessions … RETURNING` adalah alokatornya. Sequence PostgreSQL
///   meninggalkan lubang saat rollback, dan lubang membuat watermark ringkasan
///   tidak dapat dipercaya.
/// - **Fakta lama di-supersede, tidak dihapus.** Baris yang hilang menghapus
///   penjelasan "kenapa jawaban berubah antar-turn". Ia juga wajib: partial
///   unique index menolak dua `ActiveScope`/`ResolvedEntity` valid, jadi tanpa
///   supersede seluruh commit gagal.
/// - **Ringkasan menjadi `stale`.** Watermark tidak bergerak di sini; ringkasan
///   dihitung di luar transaksi karena butuh LLM (I1). Membiarkan statusnya
///   `current` berarti call berikutnya memakai ringkasan yang belum memuat
///   fakta ini.
async fn promote(
    tx: &mut Transaction<'_, Postgres>,
    session_id: Uuid,
    owner_user_id: Uuid,
    job_id: Uuid,
    response_version: i32,
    facts: &[MemoryFact],
) -> sqlx::Result<()> {
    for fact in facts {
        let session_seq: i64 = sqlx::query_scalar(
            "UPDATE chat_sessions
             SET memory_seq_last = memory_seq_last + 1,
                 memory_summary_status = 'stale'
             WHERE id = $1
             RETURNING memory_seq_last",
        )
        .bind(session_id)
        .fetch_one(&mut **tx)
        .await?;

        let id = Uuid::new_v4();

        // `entity_key IS NOT DISTINCT FROM $3` menyatukan dua aturan keunikan
        // dalam satu statement: `ActiveScope` (entity_key NULL, satu per
        // session) dan `ResolvedEntity` (satu per entity_key).
        sqlx::query(
            "UPDATE session_memory
             SET status = 'superseded',
                 superseded_by_id = $4,
                 invalidation_reason = 'superseded_by_newer',
                 invalidated_at = now()
             WHERE session_id = $1 AND kind = $2 AND entity_key IS NOT DISTINCT FROM $3
               AND status = 'valid' AND kind <> 'PriorResult'",
        )
        .bind(session_id)
        .bind(fact.kind)
        .bind(fact.entity_key.as_deref())
        .bind(id)
        .execute(&mut **tx)
        .await?;

        sqlx::query(
            "INSERT INTO session_memory
                (id, session_id, owner_user_id, session_seq, kind, entity_key, label, fact_json,
                 source_job_id, source_plan_version, source_response_version,
                 provenance_json, completeness, completeness_reason)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9,
                     -- Plan aktif menurut C3, bukan angka yang dibawa pemanggil:
                     -- satu sumber untuk 'plan mana yang menghasilkan fakta ini'.
                     (SELECT plan_version FROM chat_jobs WHERE id = $9),
                     $10, $11, $12, $13)",
        )
        .bind(id)
        .bind(session_id)
        .bind(owner_user_id)
        .bind(session_seq)
        .bind(fact.kind)
        .bind(fact.entity_key.as_deref())
        .bind(fact.label.as_deref())
        .bind(&fact.fact_json)
        .bind(job_id)
        .bind(response_version)
        .bind(&fact.provenance_json)
        .bind(&fact.completeness)
        .bind(&fact.completeness_reason)
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}

/// T7 — commit response dan tutup job.
///
/// Semua di dalam satu transaksi: response, lifecycle terminal, fakta session,
/// pesan assistant, event terminal, dan audit. `false` berarti fencing kalah
/// dan tidak ada apa pun yang ditulis.
///
/// `facts` kosong adalah keadaan sah dan sering: response `limitation` tidak
/// mempromosikan apa pun (memory-context.md §3).
pub async fn settle_with_response(
    pool: &PgPool,
    job_id: Uuid,
    session_id: Uuid,
    owner_user_id: Uuid,
    lease_token: Uuid,
    response: SettledResponse,
    facts: &[MemoryFact],
) -> sqlx::Result<bool> {
    let mut tx = pool.begin().await?;

    const RESPONSE_VERSION: i32 = 1;

    let updated = sqlx::query(
        "UPDATE chat_jobs
         SET lifecycle = 'Completed',
             outcome = $3,
             completeness = $4,
             completeness_reason = $5,
             final_response_version = $6,
             terminal_at = now(),
             -- Job terminal tidak punya masa berlaku dan tidak lagi dipegang
             -- worker; membiarkan lease terisi membuat reaper melihat pekerjaan
             -- yang sudah tidak ada.
             expires_at = NULL,
             lease_owner = NULL,
             lease_token = NULL,
             lease_expires_at = NULL,
             updated_at = now()
         WHERE id = $1 AND lease_token = $2 AND lifecycle = 'Running'",
    )
    .bind(job_id)
    .bind(lease_token)
    .bind(response.outcome)
    .bind(response.completeness)
    .bind(&response.completeness_reason)
    .bind(RESPONSE_VERSION)
    .execute(&mut *tx)
    .await?
    .rows_affected();

    if updated == 0 {
        tx.rollback().await?;
        return Ok(false);
    }

    sqlx::query(
        "INSERT INTO job_responses
            (job_id, response_version, kind, outcome, completeness, completeness_reason,
             blocks_json, validation_status, response_hash, composed_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, 'passed', $8, now())",
    )
    .bind(job_id)
    .bind(RESPONSE_VERSION)
    .bind(response.kind)
    .bind(response.outcome)
    .bind(response.completeness)
    .bind(&response.completeness_reason)
    .bind(&response.blocks)
    .bind(&response.response_hash)
    .execute(&mut *tx)
    .await?;

    // Sesudah `job_responses`, bukan sebelum: FK komposit K4 menunjuk response
    // yang baru saja ditulis, dan urutan sebaliknya gagal di dalam statement
    // yang sama.
    promote(
        &mut tx,
        session_id,
        owner_user_id,
        job_id,
        RESPONSE_VERSION,
        facts,
    )
    .await?;

    sqlx::query(
        "INSERT INTO chat_messages (session_id, job_id, role, response_version)
         VALUES ($1, $2, 'assistant', $3)",
    )
    .bind(session_id)
    .bind(job_id)
    .bind(RESPONSE_VERSION)
    .execute(&mut *tx)
    .await?;

    append_event_ref(
        &mut tx,
        job_id,
        "job.completed",
        EventRef { response_version: Some(RESPONSE_VERSION), ..Default::default() },
        Some(serde_json::json!({
            "outcome": response.outcome,
            "completeness": response.completeness,
        })),
    )
    .await?;

    audit::insert(
        &mut tx,
        AuditEvent {
            actor_kind: "worker",
            job_id: Some(job_id),
            session_id: Some(session_id),
            stage: "commit",
            action: "job.settled",
            result: "ok",
            // Wajib ikut pada INSERT: audit append-only tidak dapat ditambal,
            // dan mencoba menambalnya akan membatalkan seluruh transaksi ini.
            job_outcome: Some(response.outcome),
            job_completeness: Some(response.completeness),
            detail_json: Some(serde_json::json!({
                "response_version": RESPONSE_VERSION,
                "kind": response.kind,
                "response_hash": response.response_hash,
                // Fakta apa yang dipromosikan commit ini: tanpa ini, investigasi
                // "kenapa turn berikutnya membawa konteks itu" hanya dapat
                // menebak dari timestamp.
                "promoted_memory": facts.iter().map(|fact| fact.kind).collect::<Vec<_>>(),
            })),
            ..Default::default()
        },
    )
    .await?;

    tx.commit().await?;
    Ok(true)
}

/// Selesaikan job yang pembatalannya diminta. Tanpa response dan tanpa promosi
/// memori (engine.md: `Cancelled` tidak menghasilkan response).
pub async fn settle_cancelled(
    pool: &PgPool,
    job_id: Uuid,
    session_id: Uuid,
    lease_token: Uuid,
) -> sqlx::Result<bool> {
    let mut tx = pool.begin().await?;

    let updated = sqlx::query(
        "UPDATE chat_jobs
         SET lifecycle = 'Cancelled',
             outcome = 'OperationalFailure',
             completeness = 'Unknown',
             completeness_reason = 'cancelled_by_user',
             terminal_at = now(),
             expires_at = NULL,
             lease_owner = NULL,
             lease_token = NULL,
             lease_expires_at = NULL,
             updated_at = now()
         WHERE id = $1 AND lease_token = $2 AND lifecycle IN ('Running', 'Cancelling')",
    )
    .bind(job_id)
    .bind(lease_token)
    .execute(&mut *tx)
    .await?
    .rows_affected();

    if updated == 0 {
        tx.rollback().await?;
        return Ok(false);
    }

    append_event(&mut tx, job_id, "job.cancelled", None).await?;

    audit::insert(
        &mut tx,
        AuditEvent {
            actor_kind: "worker",
            job_id: Some(job_id),
            session_id: Some(session_id),
            stage: "settle",
            action: "job.cancelled",
            result: "ok",
            job_outcome: Some("OperationalFailure"),
            job_completeness: Some("Unknown"),
            ..Default::default()
        },
    )
    .await?;

    tx.commit().await?;
    Ok(true)
}

/// Hasil satu putaran reaper. Semuanya idempoten: putaran kedua atas state yang
/// sama tidak mengubah apa pun.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ReaperSweep {
    pub expired: u64,
    pub requeued: u64,
    pub cancelled: u64,
}

impl ReaperSweep {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// T11 — satu putaran recovery.
///
/// Urutannya penting: kedaluwarsa diperiksa **sebelum** requeue, supaya job
/// yang selalu mematikan worker-nya tidak berputar selamanya. Batasnya adalah
/// `expires_at` yang ditetapkan saat klaim.
pub async fn sweep(pool: &PgPool, worker: &str) -> sqlx::Result<ReaperSweep> {
    // Urutan ini bukan gaya penulisan: kedaluwarsa dulu, baru requeue.
    let expired = settle_expired(pool, worker).await?;
    let cancelled = settle_abandoned_cancelling(pool, worker).await?;
    let requeued = requeue_lost_leases(pool, worker).await?;

    Ok(ReaperSweep {
        expired,
        requeued,
        cancelled,
    })
}

async fn settle_expired(pool: &PgPool, worker: &str) -> sqlx::Result<u64> {
    let mut tx = pool.begin().await?;

    let jobs = sqlx::query_as::<_, (Uuid, Uuid)>(
        "UPDATE chat_jobs
         SET lifecycle = 'Expired',
             outcome = 'OperationalFailure',
             completeness = 'Unknown',
             completeness_reason = 'job_ttl_exceeded',
             terminal_at = now(),
             expires_at = NULL,
             lease_owner = NULL,
             lease_token = NULL,
             lease_expires_at = NULL,
             updated_at = now()
         WHERE lifecycle IN ('Queued', 'Running', 'WaitingForUser', 'Cancelling')
           AND expires_at IS NOT NULL
           AND expires_at < now()
         RETURNING id, session_id",
    )
    .fetch_all(&mut *tx)
    .await?;

    for (job_id, session_id) in &jobs {
        finish_sweep_row(&mut tx, *job_id, *session_id, worker, "job.expired", "job.expired").await?;
    }

    tx.commit().await?;
    Ok(jobs.len() as u64)
}

async fn settle_abandoned_cancelling(pool: &PgPool, worker: &str) -> sqlx::Result<u64> {
    let mut tx = pool.begin().await?;

    // `Cancelling` tanpa lease hidup berarti tidak ada worker yang akan
    // menuntaskannya; reaper yang menutupnya.
    let jobs = sqlx::query_as::<_, (Uuid, Uuid)>(
        "UPDATE chat_jobs
         SET lifecycle = 'Cancelled',
             outcome = 'OperationalFailure',
             completeness = 'Unknown',
             completeness_reason = 'cancelled_by_user',
             terminal_at = now(),
             expires_at = NULL,
             lease_owner = NULL,
             lease_token = NULL,
             lease_expires_at = NULL,
             updated_at = now()
         WHERE lifecycle = 'Cancelling'
           AND (lease_expires_at IS NULL OR lease_expires_at < now())
         RETURNING id, session_id",
    )
    .fetch_all(&mut *tx)
    .await?;

    for (job_id, session_id) in &jobs {
        finish_sweep_row(&mut tx, *job_id, *session_id, worker, "job.cancelled", "job.cancelled").await?;
    }

    tx.commit().await?;
    Ok(jobs.len() as u64)
}

async fn requeue_lost_leases(pool: &PgPool, worker: &str) -> sqlx::Result<u64> {
    let mut tx = pool.begin().await?;

    // Lease hilang saat `Running`: hasil eksekusi eksternalnya TIDAK DIKETAHUI
    // (I4) — bukan gagal, bukan sukses. Job dikembalikan ke antrean dengan
    // lease dikosongkan; `expires_at` sengaja dibiarkan apa adanya supaya
    // percobaan ulang tetap terbatas oleh TTL yang sama.
    let jobs = sqlx::query_as::<_, (Uuid, Uuid)>(
        "UPDATE chat_jobs
         SET lifecycle = 'Queued',
             lease_owner = NULL,
             lease_token = NULL,
             lease_expires_at = NULL,
             updated_at = now()
         WHERE lifecycle = 'Running'
           AND lease_expires_at IS NOT NULL
           AND lease_expires_at < now()
         RETURNING id, session_id",
    )
    .fetch_all(&mut *tx)
    .await?;

    for (job_id, session_id) in &jobs {
        finish_sweep_row(
            &mut tx,
            *job_id,
            *session_id,
            worker,
            "job.notice",
            "job.lease_lost",
        )
        .await?;
    }

    tx.commit().await?;
    Ok(jobs.len() as u64)
}

async fn finish_sweep_row(
    tx: &mut Transaction<'_, Postgres>,
    job_id: Uuid,
    session_id: Uuid,
    worker: &str,
    event_type: &str,
    action: &str,
) -> sqlx::Result<()> {
    append_event(tx, job_id, event_type, Some(serde_json::json!({ "by": "reaper" }))).await?;

    audit::insert(
        tx,
        AuditEvent {
            actor_kind: "reaper",
            job_id: Some(job_id),
            session_id: Some(session_id),
            stage: "settle",
            action,
            result: "ok",
            detail_json: Some(serde_json::json!({ "reaper": worker })),
            ..Default::default()
        },
    )
    .await?;

    Ok(())
}

/// Response dokumen untuk dibaca klien.
#[derive(Debug, Clone, FromRow, serde::Serialize)]
pub struct ResponseDocument {
    pub response_version: i32,
    pub kind: String,
    pub outcome: Option<String>,
    pub completeness: String,
    pub completeness_reason: Option<String>,
    pub blocks_json: Value,
    pub validation_status: String,
    pub response_hash: String,
    pub created_at: DateTime<Utc>,
}

pub async fn find_response(
    pool: &PgPool,
    job_id: Uuid,
    response_version: i32,
) -> sqlx::Result<Option<ResponseDocument>> {
    sqlx::query_as::<_, ResponseDocument>(
        "SELECT response_version, kind, outcome, completeness, completeness_reason,
                blocks_json, validation_status, response_hash, created_at
         FROM job_responses
         WHERE job_id = $1 AND response_version = $2",
    )
    .bind(job_id)
    .bind(response_version)
    .fetch_optional(pool)
    .await
}

/// T3 — plan terverifikasi dipersist.
///
/// `contract_versions_json` menyimpan `catalog_version_id` + `content_hash`,
/// bukan teks versi: di sistem lama kolom versi literal selalu berisi "local",
/// dan investigasi "prosa kontrak mana yang dilihat planner" karena itu tidak
/// pernah terjawab (migrasi 3).
pub async fn persist_plan(
    pool: &PgPool,
    job_id: Uuid,
    session_id: Uuid,
    lease_token: Uuid,
    plan_version: i32,
    graph_json: &Value,
    graph_hash: &str,
    contract_versions: &Value,
    capability_id: &str,
) -> sqlx::Result<bool> {
    let mut tx = pool.begin().await?;

    let updated = sqlx::query(
        "UPDATE chat_jobs SET plan_version = $3, updated_at = now()
         WHERE id = $1 AND lease_token = $2 AND lifecycle = 'Running'",
    )
    .bind(job_id)
    .bind(lease_token)
    .bind(plan_version)
    .execute(&mut *tx)
    .await?
    .rows_affected();

    if updated == 0 {
        tx.rollback().await?;
        return Ok(false);
    }

    sqlx::query(
        "INSERT INTO job_plans
            (job_id, plan_version, graph_json, graph_hash, contract_versions_json, verified_at)
         VALUES ($1, $2, $3, $4, $5, now())",
    )
    .bind(job_id)
    .bind(plan_version)
    .bind(graph_json)
    .bind(graph_hash)
    .bind(contract_versions)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO job_node_runs
            (job_id, plan_version, node_id, node_kind, attempt, status, started_at)
         VALUES ($1, $2, 'main', 'CuratedQuery', 1, 'Runnable', now())",
    )
    .bind(job_id)
    .bind(plan_version)
    .execute(&mut *tx)
    .await?;

    append_event_ref(
        &mut tx,
        job_id,
        "job.phase_changed",
        EventRef { plan_version: Some(plan_version), ..Default::default() },
        Some(serde_json::json!({ "phase": "planning", "message": "Preparing the analysis." })),
    )
    .await?;

    audit::insert(
        &mut tx,
        AuditEvent {
            actor_kind: "worker",
            job_id: Some(job_id),
            session_id: Some(session_id),
            stage: "plan_verify",
            action: "plan.persisted",
            result: "ok",
            detail_json: Some(serde_json::json!({
                "plan_version": plan_version,
                "graph_hash": graph_hash,
                "capability_id": capability_id,
            })),
            ..Default::default()
        },
    )
    .await?;

    tx.commit().await?;
    Ok(true)
}

/// Hasil satu node yang akan dipersist lewat T4.
#[derive(Debug)]
pub struct NodeOutcome<'a> {
    pub status: &'a str,
    pub completeness: Option<&'a str>,
    pub failure_code: Option<&'a str>,
    pub output_json: Option<Value>,
    pub provenance_json: Value,
    pub rows_returned: Option<i64>,
    pub duration_ms: Option<i64>,
}

/// T4 — node selesai.
///
/// Eksekusi query terjadi **di luar** transaksi ini (I1). Fencing dilakukan
/// lewat `EXISTS` terhadap token job, karena baris node tidak menyimpan token
/// sendiri: pemegang token basi tidak boleh mempersist hasilnya.
pub async fn complete_node(
    pool: &PgPool,
    job_id: Uuid,
    session_id: Uuid,
    lease_token: Uuid,
    plan_version: i32,
    outcome: NodeOutcome<'_>,
) -> sqlx::Result<bool> {
    let mut tx = pool.begin().await?;

    let updated = sqlx::query(
        "UPDATE job_node_runs
         SET status = $4,
             completeness = $5,
             failure_code = $6,
             output_json = $7,
             provenance_json = $8,
             rows_returned = $9,
             duration_ms = $10,
             finished_at = now()
         WHERE job_id = $1 AND plan_version = $2 AND node_id = 'main' AND attempt = 1
           AND EXISTS (
               SELECT 1 FROM chat_jobs
               WHERE id = $1 AND lease_token = $3 AND lifecycle = 'Running'
           )",
    )
    .bind(job_id)
    .bind(plan_version)
    .bind(lease_token)
    .bind(outcome.status)
    .bind(outcome.completeness)
    .bind(outcome.failure_code)
    .bind(&outcome.output_json)
    .bind(&outcome.provenance_json)
    .bind(outcome.rows_returned)
    .bind(outcome.duration_ms)
    .execute(&mut *tx)
    .await?
    .rows_affected();

    if updated == 0 {
        tx.rollback().await?;
        return Ok(false);
    }

    // Budget dihitung pada baris job, bukan disimpulkan dari jumlah baris
    // ledger: attempt yang Abandoned tetap memakai kuota.
    sqlx::query(
        "UPDATE chat_jobs SET query_count = query_count + 1, updated_at = now() WHERE id = $1",
    )
    .bind(job_id)
    .execute(&mut *tx)
    .await?;

    append_event_ref(
        &mut tx,
        job_id,
        "node.status_changed",
        EventRef {
            plan_version: Some(plan_version),
            node_id: Some("main"),
            node_attempt: Some(1),
            ..Default::default()
        },
        Some(serde_json::json!({
            "status": outcome.status,
            "rows_returned": outcome.rows_returned,
        })),
    )
    .await?;

    audit::insert(
        &mut tx,
        AuditEvent {
            actor_kind: "worker",
            job_id: Some(job_id),
            session_id: Some(session_id),
            stage: "source_query",
            action: "node.completed",
            result: if outcome.status == "Completed" { "ok" } else { "failed" },
            failure_code: outcome.failure_code,
            detail_json: Some(outcome.provenance_json.clone()),
            ..Default::default()
        },
    )
    .await?;

    tx.commit().await?;
    Ok(true)
}

/// Selesaikan job sebagai kegagalan operasional (T7 jalur gagal).
pub async fn settle_failed(
    pool: &PgPool,
    job_id: Uuid,
    session_id: Uuid,
    lease_token: Uuid,
    failure_code: &str,
    response: SettledResponse,
) -> sqlx::Result<bool> {
    let mut tx = pool.begin().await?;

    const RESPONSE_VERSION: i32 = 1;

    let updated = sqlx::query(
        "UPDATE chat_jobs
         SET lifecycle = 'Failed',
             outcome = 'OperationalFailure',
             completeness = $4,
             completeness_reason = $5,
             failure_code = $3,
             final_response_version = $6,
             terminal_at = now(),
             expires_at = NULL,
             lease_owner = NULL,
             lease_token = NULL,
             lease_expires_at = NULL,
             updated_at = now()
         WHERE id = $1 AND lease_token = $2 AND lifecycle = 'Running'",
    )
    .bind(job_id)
    .bind(lease_token)
    .bind(failure_code)
    .bind(response.completeness)
    .bind(&response.completeness_reason)
    .bind(RESPONSE_VERSION)
    .execute(&mut *tx)
    .await?
    .rows_affected();

    if updated == 0 {
        tx.rollback().await?;
        return Ok(false);
    }

    // Response tetap ditulis: pengguna berhak tahu APA yang gagal, dan
    // investigasi berhak melihat dokumen yang dilihat pengguna.
    sqlx::query(
        "INSERT INTO job_responses
            (job_id, response_version, kind, outcome, completeness, completeness_reason,
             blocks_json, validation_status, response_hash, composed_at)
         VALUES ($1, $2, $3, 'OperationalFailure', $4, $5, $6, 'passed', $7, now())",
    )
    .bind(job_id)
    .bind(RESPONSE_VERSION)
    .bind(response.kind)
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
    .bind(job_id)
    .bind(RESPONSE_VERSION)
    .execute(&mut *tx)
    .await?;

    append_event_ref(
        &mut tx,
        job_id,
        "job.failed",
        EventRef { response_version: Some(RESPONSE_VERSION), ..Default::default() },
        Some(serde_json::json!({ "failure_code": failure_code })),
    )
    .await?;

    audit::insert(
        &mut tx,
        AuditEvent {
            actor_kind: "worker",
            job_id: Some(job_id),
            session_id: Some(session_id),
            stage: "commit",
            action: "job.failed",
            result: "failed",
            failure_code: Some(failure_code),
            job_outcome: Some("OperationalFailure"),
            job_completeness: Some(response.completeness),
            ..Default::default()
        },
    )
    .await?;

    tx.commit().await?;
    Ok(true)
}

/// Id versi katalog yang tercatat untuk sebuah `content_hash`.
pub async fn catalog_version_id(pool: &PgPool, content_hash: &str) -> sqlx::Result<Option<Uuid>> {
    sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM knowledge_catalog_versions WHERE content_hash = $1",
    )
    .bind(content_hash)
    .fetch_optional(pool)
    .await
}
