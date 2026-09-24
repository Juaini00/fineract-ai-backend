//! Transaksi Engine: klaim (T2), heartbeat, penyelesaian (T7/T8), dan reaper (T11).
//!
//! **Fencing (C16)**: setiap tulisan durable milik worker memakai
//! `AND lease_token = $token`. Update yang menyentuh 0 baris berarti worker
//! sudah dipagari dan wajib berhenti — bukan mencoba transisi lain.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{
    audit::{self, AuditEvent},
    engine::{
        memory::MemoryFact,
        validate::{self, Validated},
    },
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
    let (_window, mut tx) = foundation::commit_isolation::begin(pool).await?;

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
    /// Lineage (#10): rantai finding → metric → operasi → dataset → sumber.
    /// Ia kolomnya sendiri dan **bukan blok** — blok yang tidak dikenal klien
    /// dilewati tanpa merusak render (responses.md §1), dan jejak asal angka
    /// tidak boleh ikut hilang bersamanya.
    pub evidence: Value,
    pub response_hash: String,
}

/// Ledger yang dipakai validator untuk **menghitung ulang** klaim composer
/// (responses.md §3–§5).
///
/// Dibaca kembali dari database dengan sengaja, bukan diambil dari nilai yang
/// baru saja dikirim proses ini: yang diperiksa adalah apakah dokumen cocok
/// dengan apa yang **durable**, dan job yang dilanjutkan worker lain harus
/// menghasilkan pemeriksaan yang sama persis.
///
/// `datasets` belum punya jalur, jadi kontributor hari ini hanya `job_node_runs`.
/// Saat dataset berchunk ada, `completeness`-nya masuk ke daftar yang sama —
/// dan `handle_state` wajib ikut dibaca non-optional (C13), supaya tidak ada
/// jalur baca yang dapat melewatkan status dataset.
pub async fn ledger(
    pool: &PgPool,
    job_id: Uuid,
    plan_version: i32,
) -> sqlx::Result<validate::Ledger> {
    let rows = sqlx::query_as::<_, (Uuid, Option<String>, Option<Value>)>(
        "SELECT id, completeness, input_binding_json
         FROM job_node_runs
         WHERE job_id = $1 AND plan_version = $2 AND status <> 'Pending'",
    )
    .bind(job_id)
    .bind(plan_version)
    .fetch_all(pool)
    .await?;

    let mut contributors = BTreeMap::new();
    let mut auto_bound = BTreeSet::new();
    let mut parameters = Vec::new();

    for (node_run_id, completeness, binding) in rows {
        // Node yang sudah berjalan tanpa `completeness` adalah kontributor yang
        // tidak menyatakan apa pun — `Unknown`, bukan dilewati (I4).
        //
        // Berkunci `id`: D1 dihitung PER BLOK lewat `derived_from`, dan daftar
        // tanpa identitas hanya dapat dihitung di tingkat dokumen.
        contributors.insert(
            node_run_id.to_string(),
            completeness.unwrap_or_else(|| "Unknown".to_string()),
        );

        let slots = binding
            .as_ref()
            .and_then(|binding| binding.get("auto_bound_slots"))
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]);

        auto_bound.extend(slots.iter().filter_map(Value::as_str).map(str::to_string));

        if let Some(bound) = binding
            .as_ref()
            .and_then(|binding| binding.get("parameters"))
            .and_then(Value::as_array)
        {
            parameters.extend(bound.iter().cloned());
        }
    }

    Ok(validate::Ledger { contributors, auto_bound, derivations: Vec::new(), parameters })
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
    validated: Validated,
    facts: &[MemoryFact],
) -> sqlx::Result<bool> {
    let (_window, mut tx) = foundation::commit_isolation::begin(pool).await?;

    let validation_status = validated.status();
    let Validated { served: response, rejected, report } = validated;

    // Dokumen yang ditolak menempati versi 1 dan versi yang disajikan menjadi 2.
    // Versi yang gagal TIDAK dihapus (#10): ia satu-satunya bukti tentang apa
    // yang nyaris disajikan, dan tanpa itu "kenapa jawaban ini konservatif"
    // hanya dapat ditebak.
    let rejected_version: i32 = 1;
    let response_version: i32 = if rejected.is_some() { 2 } else { 1 };

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
    .bind(response_version)
    .execute(&mut *tx)
    .await?
    .rows_affected();

    if updated == 0 {
        tx.rollback().await?;
        return Ok(false);
    }

    if let Some(rejected) = &rejected {
        sqlx::query(
            "INSERT INTO job_responses
                (job_id, response_version, kind, outcome, completeness, completeness_reason,
                 blocks_json, evidence_json, validation_status, validation_report_json,
                 superseded_by_version, response_hash, composed_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'failed', $9, $10, $11, now())",
        )
        .bind(job_id)
        .bind(rejected_version)
        .bind(rejected.kind)
        .bind(rejected.outcome)
        .bind(rejected.completeness)
        .bind(&rejected.completeness_reason)
        .bind(&rejected.blocks)
        .bind(&rejected.evidence)
        .bind(&report)
        .bind(response_version)
        .bind(&rejected.response_hash)
        .execute(&mut *tx)
        .await?;
    }

    sqlx::query(
        "INSERT INTO job_responses
            (job_id, response_version, kind, outcome, completeness, completeness_reason,
             blocks_json, evidence_json, validation_status, validation_report_json,
             response_hash, composed_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, now())",
    )
    .bind(job_id)
    .bind(response_version)
    .bind(response.kind)
    .bind(response.outcome)
    .bind(response.completeness)
    .bind(&response.completeness_reason)
    .bind(&response.blocks)
    .bind(&response.evidence)
    .bind(validation_status)
    .bind(&report)
    .bind(&response.response_hash)
    .execute(&mut *tx)
    .await?;

    // Sesudah `job_responses`, bukan sebelum: FK komposit K4 menunjuk response
    // yang baru saja ditulis, dan urutan sebaliknya gagal di dalam statement
    // yang sama. Versi yang ditunjuk adalah versi yang DISAJIKAN — fakta memori
    // tidak boleh bersumber pada dokumen yang ditolak.
    promote(
        &mut tx,
        session_id,
        owner_user_id,
        job_id,
        response_version,
        facts,
    )
    .await?;

    sqlx::query(
        "INSERT INTO chat_messages (session_id, job_id, role, response_version)
         VALUES ($1, $2, 'assistant', $3)",
    )
    .bind(session_id)
    .bind(job_id)
    .bind(response_version)
    .execute(&mut *tx)
    .await?;

    append_event_ref(
        &mut tx,
        job_id,
        "job.completed",
        EventRef { response_version: Some(response_version), ..Default::default() },
        Some(serde_json::json!({
            "outcome": response.outcome,
            "completeness": response.completeness,
            // I5 — fallback tidak pernah diam. Klien yang hanya mendengarkan
            // SSE tetap tahu bahwa yang dikirim bukan dokumen pertama.
            "validation_status": validation_status,
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
                "response_version": response_version,
                "kind": response.kind,
                "response_hash": response.response_hash,
                "validation_status": validation_status,
                "validation_report": report,
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
    let (_window, mut tx) = foundation::commit_isolation::begin(pool).await?;

    let updated = sqlx::query(
        "UPDATE chat_jobs
         SET lifecycle = 'Cancelled',
             outcome = 'Cancelled',
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
            job_outcome: Some("Cancelled"),
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
    /// Job yang attempt tak pastinya mencapai `NODE_ATTEMPT_CAP` dan karena
    /// itu ditutup `Failed`, bukan diulang lagi.
    pub exhausted: u64,
    pub cancelled: u64,
}

impl ReaperSweep {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// `failure_code` job yang attempt tak pastinya habis (engine.md "Recovery
/// attempt tak pasti" langkah 5).
pub const NODE_ATTEMPT_CAP_REACHED: &str = "node_attempt_cap_reached";

/// T11 — satu putaran recovery.
///
/// Urutannya penting: kedaluwarsa diperiksa **sebelum** requeue, supaya job
/// yang selalu mematikan worker-nya tidak berputar selamanya. Attempt yang
/// sudah menyentuh sumber dibatasi `node_attempt_cap`; TTL (`expires_at`)
/// tetap jaring terakhir untuk sisanya.
///
/// `exhausted` adalah response yang disajikan saat cap tercapai. Ia disusun
/// pemanggil karena komposisi blok bukan urusan repository.
pub async fn sweep(
    pool: &PgPool,
    worker: &str,
    node_attempt_cap: i32,
    exhausted: &SettledResponse,
) -> sqlx::Result<ReaperSweep> {
    // Urutan ini bukan gaya penulisan: kedaluwarsa dulu, baru requeue.
    let expired = settle_expired(pool, worker).await?;
    let cancelled = settle_abandoned_cancelling(pool, worker).await?;
    let (requeued, exhausted) =
        recover_lost_leases(pool, worker, node_attempt_cap, exhausted).await?;

    Ok(ReaperSweep {
        expired,
        requeued,
        exhausted,
        cancelled,
    })
}

async fn settle_expired(pool: &PgPool, worker: &str) -> sqlx::Result<u64> {
    let (_window, mut tx) = foundation::commit_isolation::begin(pool).await?;

    let jobs = sqlx::query_as::<_, (Uuid, Uuid)>(
        "UPDATE chat_jobs
         SET lifecycle = 'Expired',
             outcome = 'Expired',
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
    let (_window, mut tx) = foundation::commit_isolation::begin(pool).await?;

    // `Cancelling` tanpa lease hidup berarti tidak ada worker yang akan
    // menuntaskannya; reaper yang menutupnya.
    let jobs = sqlx::query_as::<_, (Uuid, Uuid)>(
        "UPDATE chat_jobs
         SET lifecycle = 'Cancelled',
             outcome = 'Cancelled',
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

/// Attempt yang ditandai `Abandoned` oleh reaper.
#[derive(Debug, FromRow)]
struct AbandonedAttempt {
    plan_version: i32,
    node_id: String,
    node_kind: String,
    attempt: i32,
}

/// Lease hilang saat `Running` (engine.md, "Recovery attempt tak pasti").
///
/// Hasil external call yang mungkin sudah berjalan TIDAK DIKETAHUI (I4) —
/// bukan gagal, bukan sukses. Attempt `Running` ditutup `Abandoned`, lalu
/// logical node mendapat attempt baru selama cap belum tercapai; baris attempt
/// lama tidak pernah dibuka lagi. Job tanpa attempt `Running` (lease hilang
/// sebelum admisi) dikembalikan ke antrean tanpa menyimpulkan apa pun.
/// `expires_at` sengaja dibiarkan: TTL tetap membatasi seluruh percobaan.
async fn recover_lost_leases(
    pool: &PgPool,
    worker: &str,
    node_attempt_cap: i32,
    exhausted: &SettledResponse,
) -> sqlx::Result<(u64, u64)> {
    let (_window, mut tx) = foundation::commit_isolation::begin(pool).await?;

    // (1) Kunci job dan pastikan lease MASIH kedaluwarsa di bawah kunci itu:
    // renewal yang commit lebih dulu membuat job tidak terpilih, dan renewal
    // yang datang sesudahnya menemukan token yang sudah dikosongkan.
    let jobs = sqlx::query_as::<_, (Uuid, Uuid)>(
        "SELECT id, session_id FROM chat_jobs
         WHERE lifecycle = 'Running'
           AND lease_expires_at IS NOT NULL
           AND lease_expires_at < now()
         FOR UPDATE SKIP LOCKED",
    )
    .fetch_all(&mut *tx)
    .await?;

    let (mut requeued, mut exhausted_jobs) = (0, 0);

    for (job_id, session_id) in jobs {
        // (2)+(3) `Running` → `Abandoned`, tanpa menyimpulkan hasilnya.
        let abandoned = abandon_running_attempts(&mut tx, job_id, session_id, worker).await?;
        // Budget absolut lintas attempt (K9): query yang mungkin sudah
        // berjalan tetap memakai kuota, walau hasilnya tidak pernah durable.
        let spent = abandoned.len() as i32;

        // (5) Cap tercapai: selesaikan menurut fail-policy, jangan ulang lagi.
        if abandoned
            .iter()
            .any(|attempt| attempt.attempt >= node_attempt_cap)
        {
            settle_attempts_exhausted(&mut tx, job_id, session_id, worker, spent, exhausted)
                .await?;
            exhausted_jobs += 1;
            continue;
        }

        // (4) Logical node masih diperlukan: attempt baru. Plan satu node
        // tidak punya fan-in, jadi attempt itu langsung `Runnable`.
        let mut retry = Vec::with_capacity(abandoned.len());
        for attempt in &abandoned {
            sqlx::query(
                "INSERT INTO job_node_runs
                    (job_id, plan_version, node_id, node_kind, attempt, status)
                 VALUES ($1, $2, $3, $4, $5, 'Runnable')",
            )
            .bind(job_id)
            .bind(attempt.plan_version)
            .bind(&attempt.node_id)
            .bind(&attempt.node_kind)
            .bind(attempt.attempt + 1)
            .execute(&mut *tx)
            .await?;
            retry.push(serde_json::json!({
                "node_id": attempt.node_id,
                "attempt": attempt.attempt + 1,
            }));
        }

        sqlx::query(
            "UPDATE chat_jobs
             SET lifecycle = 'Queued',
                 lease_owner = NULL,
                 lease_token = NULL,
                 lease_expires_at = NULL,
                 query_count = query_count + $2,
                 updated_at = now()
             WHERE id = $1",
        )
        .bind(job_id)
        .bind(spent)
        .execute(&mut *tx)
        .await?;

        // `job.notice` = retry (sse.md). `retry` hanya ada bila ada attempt
        // yang diulang; lease yang hilang sebelum admisi tidak mengulang apa pun.
        let mut notice = serde_json::json!({ "by": "reaper" });
        if !retry.is_empty() {
            notice["retry"] = Value::Array(retry.clone());
        }
        append_event(&mut tx, job_id, "job.notice", Some(notice)).await?;

        audit::insert(
            &mut tx,
            AuditEvent {
                actor_kind: "reaper",
                job_id: Some(job_id),
                session_id: Some(session_id),
                stage: "settle",
                action: "job.lease_lost",
                result: "ok",
                detail_json: Some(serde_json::json!({ "reaper": worker, "retry": retry })),
                ..Default::default()
            },
        )
        .await?;

        requeued += 1;
    }

    tx.commit().await?;
    Ok((requeued, exhausted_jobs))
}

/// Tutup setiap attempt `Running` job ini sebagai `Abandoned` + event + audit.
///
/// `completeness = 'Unknown'`: hasilnya tidak diketahui, bukan nol dan bukan
/// gagal. `failure_code` sengaja kosong — tidak ada kegagalan yang diketahui.
async fn abandon_running_attempts(
    tx: &mut Transaction<'_, Postgres>,
    job_id: Uuid,
    session_id: Uuid,
    worker: &str,
) -> sqlx::Result<Vec<AbandonedAttempt>> {
    let abandoned = sqlx::query_as::<_, AbandonedAttempt>(
        "UPDATE job_node_runs
         SET status = 'Abandoned', completeness = 'Unknown', finished_at = now()
         WHERE job_id = $1 AND status = 'Running'
         RETURNING plan_version, node_id, node_kind, attempt",
    )
    .bind(job_id)
    .fetch_all(&mut **tx)
    .await?;

    for attempt in &abandoned {
        append_event_ref(
            tx,
            job_id,
            "node.status_changed",
            EventRef {
                plan_version: Some(attempt.plan_version),
                node_id: Some(&attempt.node_id),
                node_attempt: Some(attempt.attempt),
                ..Default::default()
            },
            Some(serde_json::json!({ "status": "Abandoned" })),
        )
        .await?;

        audit::insert(
            tx,
            AuditEvent {
                actor_kind: "reaper",
                job_id: Some(job_id),
                session_id: Some(session_id),
                stage: "node_execute",
                action: "node.abandoned",
                result: "ok",
                detail_json: Some(serde_json::json!({
                    "reaper": worker,
                    "plan_version": attempt.plan_version,
                    "node_id": attempt.node_id,
                    "attempt": attempt.attempt,
                })),
                ..Default::default()
            },
        )
        .await?;
    }

    Ok(abandoned)
}

/// Cap attempt tak pasti tercapai: `Failed` + `OperationalFailure` +
/// `Unknown` (engine.md: satu-satunya pasangan sah untuk `Failed`), dengan
/// response `limitation` yang sama bentuknya dengan kegagalan operasional
/// worker. Tidak pernah `Completed`: tidak ada hasil yang durable.
async fn settle_attempts_exhausted(
    tx: &mut Transaction<'_, Postgres>,
    job_id: Uuid,
    session_id: Uuid,
    worker: &str,
    spent: i32,
    response: &SettledResponse,
) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE chat_jobs
         SET lifecycle = 'Failed',
             outcome = 'OperationalFailure',
             completeness = $3,
             completeness_reason = $4,
             failure_code = $5,
             final_response_version = $6,
             terminal_at = now(),
             expires_at = NULL,
             lease_owner = NULL,
             lease_token = NULL,
             lease_expires_at = NULL,
             query_count = query_count + $2,
             updated_at = now()
         WHERE id = $1",
    )
    .bind(job_id)
    .bind(spent)
    .bind(response.completeness)
    .bind(&response.completeness_reason)
    .bind(NODE_ATTEMPT_CAP_REACHED)
    .bind(FAILED_RESPONSE_VERSION)
    .execute(&mut **tx)
    .await?;

    record_failure(
        tx,
        job_id,
        session_id,
        FailureActor::Reaper(worker),
        NODE_ATTEMPT_CAP_REACHED,
        response,
    )
    .await
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
    /// Lineage (#10). Dikirim ke klien karena "dari mana angka ini" adalah
    /// bagian dari jawaban, bukan metadata internal — dan sejak ia keluar dari
    /// blok, tidak ada tempat lain untuk membacanya.
    pub evidence_json: Value,
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
                blocks_json, evidence_json, validation_status, response_hash, created_at
         FROM job_responses
         WHERE job_id = $1 AND response_version = $2",
    )
    .bind(job_id)
    .bind(response_version)
    .fetch_optional(pool)
    .await
}

/// Hasil T3 bagi worker.
#[derive(Debug, PartialEq, Eq)]
pub enum PlanPersisted {
    /// Fencing kalah; tidak ada yang ditulis.
    Fenced,
    /// Plan baru beserta attempt pertamanya ditulis.
    Persisted,
    /// Versi plan ini sudah durable dari klaim sebelumnya (job dikembalikan
    /// ke antrean oleh reaper, T11) dan verifikasi ulang menghasilkan plan
    /// yang identik: attempt yang sudah ada di ledger yang dijalankan, bukan
    /// plan kedua dengan versi yang sama.
    Adopted,
    /// Versi plan ini sudah durable, tetapi verifikasi ulang menghasilkan
    /// graph atau kontrak/katalog lain. Node tidak boleh berjalan di bawah plan
    /// yang bukan miliknya (D4), dan re-plan ke `plan_version` baru belum ada.
    Changed,
}

/// T3 — plan terverifikasi dipersist.
///
/// `contract_versions_json` menyimpan `catalog_version_id` + `content_hash`,
/// bukan teks versi: di sistem lama kolom versi literal selalu berisi "local",
/// dan investigasi "prosa kontrak mana yang dilihat planner" karena itu tidak
/// pernah terjawab (migrasi 3).
///
/// Job yang diklaim ulang sesudah lease hilang sudah punya plan versi ini.
/// Menulisnya kedua kali melanggar UNIQUE `(job_id, plan_version)`; menimpanya
/// menghapus bukti plan mana yang dipakai attempt sebelumnya. Karena itu plan
/// yang baru diverifikasi hanya DIADOPSI bila identik ([`PlanPersisted`]).
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
) -> sqlx::Result<PlanPersisted> {
    let (_window, mut tx) = foundation::commit_isolation::begin(pool).await?;

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
        return Ok(PlanPersisted::Fenced);
    }

    let existing = sqlx::query_scalar::<_, bool>(
        "SELECT graph_hash = $3 AND contract_versions_json = $4
         FROM job_plans WHERE job_id = $1 AND plan_version = $2",
    )
    .bind(job_id)
    .bind(plan_version)
    .bind(graph_hash)
    .bind(contract_versions)
    .fetch_optional(&mut *tx)
    .await?;

    let persisted = match existing {
        Some(true) => PlanPersisted::Adopted,
        Some(false) => {
            tx.rollback().await?;
            return Ok(PlanPersisted::Changed);
        }
        None => {
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

            PlanPersisted::Persisted
        }
    };

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
            action: if persisted == PlanPersisted::Adopted {
                "plan.adopted"
            } else {
                "plan.persisted"
            },
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
    Ok(persisted)
}

/// Hasil admisi node.
#[derive(Debug, PartialEq, Eq)]
pub enum Admission {
    /// Attempt ini sekarang `Running`.
    Admitted(i32),
    /// Fencing kalah; tidak ada yang ditulis.
    Fenced,
    /// Lease masih milik worker ini, tetapi tidak ada attempt `Runnable`:
    /// node sudah `Completed` oleh attempt sebelumnya (lease hilang sesudah
    /// T4, sebelum T7).
    NothingRunnable,
}

/// Admisi node: attempt `Runnable` menjadi `Running` TEPAT sebelum external
/// call dikirim.
///
/// Inilah yang membuat ketidakpastian dapat dinyatakan (I4): reaper hanya
/// menandai `Abandoned` attempt yang `Running` — yang mungkin sudah menyentuh
/// sumber. Attempt yang belum diadmisikan tetap `Runnable`; ia tidak pernah
/// berjalan, jadi tidak ada yang tidak pasti tentangnya.
pub async fn admit_node(
    pool: &PgPool,
    job_id: Uuid,
    lease_token: Uuid,
    plan_version: i32,
) -> sqlx::Result<Admission> {
    // Satu statement: pemeriksaan pagar dan admisi melihat snapshot yang sama.
    let (held, attempt) = sqlx::query_as::<_, (bool, Option<i32>)>(
        "WITH held AS (
             SELECT id FROM chat_jobs
             WHERE id = $1 AND lease_token = $3 AND lifecycle = 'Running'
         ), admitted AS (
             UPDATE job_node_runs
             SET status = 'Running', started_at = now()
             WHERE job_id IN (SELECT id FROM held)
               AND plan_version = $2 AND node_id = 'main' AND status = 'Runnable'
             RETURNING attempt
         )
         SELECT EXISTS (SELECT 1 FROM held), (SELECT attempt FROM admitted)",
    )
    .bind(job_id)
    .bind(plan_version)
    .bind(lease_token)
    .fetch_one(pool)
    .await?;

    Ok(match (held, attempt) {
        (_, Some(attempt)) => Admission::Admitted(attempt),
        (false, None) => Admission::Fenced,
        (true, None) => Admission::NothingRunnable,
    })
}

/// Hasil satu node yang akan dipersist lewat T4.
#[derive(Debug)]
pub struct NodeOutcome<'a> {
    pub status: &'a str,
    pub completeness: Option<&'a str>,
    /// Wajib saat `completeness` bukan `Complete`, mis. `row_cap_reached`
    /// (FIN-133): klaim yang turun tanpa alasan adalah penghilangan senyap (I5).
    pub completeness_reason: Option<&'a str>,
    pub failure_code: Option<&'a str>,
    /// Binding yang BENAR-BENAR dikonsumsi node — termasuk slot yang diikat
    /// resolver tanpa bertanya (K5). Ini yang dibaca validator D2, dan itulah
    /// sebabnya ia ditulis di sini alih-alih disimpulkan ulang dari plan:
    /// pemeriksaan yang membaca sumber yang sama dengan yang diperiksa tidak
    /// membuktikan apa pun.
    pub input_binding_json: Value,
    pub output_json: Option<Value>,
    pub provenance_json: Value,
    pub rows_returned: Option<i64>,
    pub duration_ms: Option<i64>,
}

/// T4 — node selesai. Mengembalikan `job_node_runs.id`, atau `None` bila
/// fencing kalah.
///
/// Identitasnya dikembalikan karena blok response merujuknya lewat
/// `derived_from` (responses.md §1): tanpa identitas kontributor, `completeness`
/// hanya dapat dihitung di tingkat dokumen — dan §3 aturan 2 menuntutnya per
/// blok.
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
    attempt: i32,
    outcome: NodeOutcome<'_>,
) -> sqlx::Result<Option<Uuid>> {
    let (_window, mut tx) = foundation::commit_isolation::begin(pool).await?;

    let updated = sqlx::query_as::<_, (Uuid,)>(
        "UPDATE job_node_runs
         SET status = $4,
             completeness = $5,
             completeness_reason = $13,
             failure_code = $6,
             output_json = $7,
             provenance_json = $8,
             rows_returned = $9,
             duration_ms = $10,
             input_binding_json = $11,
             input_binding_hash = $12,
             finished_at = now()
         -- Hanya attempt yang diadmisikan proses ini dan masih `Running`:
         -- attempt terminal (mis. `Abandoned` oleh reaper) tidak dibuka lagi.
         WHERE job_id = $1 AND plan_version = $2 AND node_id = 'main'
           AND attempt = $14 AND status = 'Running'
           AND EXISTS (
               SELECT 1 FROM chat_jobs
               WHERE id = $1 AND lease_token = $3 AND lifecycle = 'Running'
           )
         RETURNING id",
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
    .bind(&outcome.input_binding_json)
    // Hash dihitung di Rust, bukan di SQL: PostgreSQL tidak punya cast
    // `text -> bytea`, dan jalan memutar lewat `convert_to` menaruh aturan
    // kanonikalisasi hash di tempat yang tidak dapat diuji `cargo test`.
    .bind(hex::encode(Sha256::digest(
        outcome.input_binding_json.to_string().as_bytes(),
    )))
    .bind(outcome.completeness_reason)
    .bind(attempt)
    .fetch_optional(&mut *tx)
    .await?;

    let Some((node_run_id,)) = updated else {
        tx.rollback().await?;
        return Ok(None);
    };

    // Budget dihitung pada baris job, bukan disimpulkan dari jumlah baris
    // ledger. Attempt yang `Abandoned` tidak pernah sampai ke sini; kuotanya
    // dicatat reaper saat menandainya (T11), karena query-nya mungkin sudah
    // berjalan.
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
            node_attempt: Some(attempt),
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
    Ok(Some(node_run_id))
}

/// Versi response kegagalan operasional: tidak ada validator, jadi tidak ada
/// versi yang ditolak mendahuluinya.
const FAILED_RESPONSE_VERSION: i32 = 1;

/// Siapa yang memindahkan job ke `Failed` — menentukan baris audit-nya.
enum FailureActor<'a> {
    Worker,
    Reaper(&'a str),
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
    let (_window, mut tx) = foundation::commit_isolation::begin(pool).await?;

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
    .bind(FAILED_RESPONSE_VERSION)
    .execute(&mut *tx)
    .await?
    .rows_affected();

    if updated == 0 {
        tx.rollback().await?;
        return Ok(false);
    }

    record_failure(
        &mut tx,
        job_id,
        session_id,
        FailureActor::Worker,
        failure_code,
        &response,
    )
    .await?;

    tx.commit().await?;
    Ok(true)
}

/// Sisa T7 jalur gagal sesudah job dipindahkan ke `Failed` pada transaksi
/// yang sama: response, pesan assistant, event terminal, dan audit.
async fn record_failure(
    tx: &mut Transaction<'_, Postgres>,
    job_id: Uuid,
    session_id: Uuid,
    actor: FailureActor<'_>,
    failure_code: &str,
    response: &SettledResponse,
) -> sqlx::Result<()> {
    // Response tetap ditulis: pengguna berhak tahu APA yang gagal, dan
    // investigasi berhak melihat dokumen yang dilihat pengguna.
    sqlx::query(
        "INSERT INTO job_responses
            (job_id, response_version, kind, outcome, completeness, completeness_reason,
             blocks_json, evidence_json, validation_status, response_hash, composed_at)
         VALUES ($1, $2, $3, 'OperationalFailure', $4, $5, $6, $7, 'passed', $8, now())",
    )
    .bind(job_id)
    .bind(FAILED_RESPONSE_VERSION)
    .bind(response.kind)
    .bind(response.completeness)
    .bind(&response.completeness_reason)
    .bind(&response.blocks)
    .bind(&response.evidence)
    .bind(&response.response_hash)
    .execute(&mut **tx)
    .await?;

    sqlx::query(
        "INSERT INTO chat_messages (session_id, job_id, role, response_version)
         VALUES ($1, $2, 'assistant', $3)",
    )
    .bind(session_id)
    .bind(job_id)
    .bind(FAILED_RESPONSE_VERSION)
    .execute(&mut **tx)
    .await?;

    append_event_ref(
        tx,
        job_id,
        "job.failed",
        EventRef {
            response_version: Some(FAILED_RESPONSE_VERSION),
            ..Default::default()
        },
        Some(serde_json::json!({ "failure_code": failure_code })),
    )
    .await?;

    let (actor_kind, stage, detail_json) = match actor {
        FailureActor::Worker => ("worker", "commit", None),
        FailureActor::Reaper(reaper) => (
            "reaper",
            "settle",
            Some(serde_json::json!({ "reaper": reaper })),
        ),
    };

    audit::insert(
        tx,
        AuditEvent {
            actor_kind,
            job_id: Some(job_id),
            session_id: Some(session_id),
            stage,
            action: "job.failed",
            result: "failed",
            failure_code: Some(failure_code),
            job_outcome: Some("OperationalFailure"),
            job_completeness: Some(response.completeness),
            detail_json,
            ..Default::default()
        },
    )
    .await
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
