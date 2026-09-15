//! Worker: mengklaim job, menjaga lease, merencanakan, mengeksekusi, lalu
//! menyelesaikannya.
//!
//! Satu Engine memiliki orchestration end-to-end (PRD §4). Alurnya:
//! klaim (T2) → scope terotorisasi → plan (T3) → eksekusi node di luar
//! transaksi (T4) → komposisi deterministik → commit response (T7).
//!
//! Permintaan yang tidak tercakup capability yang disetujui **tidak dijawab**:
//! hasilnya `Unsupported` dengan response `kind='limitation'` yang menyebut
//! sebabnya (PRD §5). Itu bukan kegagalan operasional, dan bukan pula jawaban
//! kosong yang berpura-pura menjawab.

use std::{sync::Arc, time::Duration};

use foundation::state::Foundation;
use sha2::{Digest, Sha256};
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::{
    catalog::Catalog,
    clarification::repository as clarification_repository,
    engine::{
        compose, executor,
        planner::{self, Plan},
        repository::{self, ClaimedJob, NodeOutcome, SettledResponse},
    },
};

/// Satu-satunya versi plan yang dihasilkan planner deterministik. Re-plan baru
/// relevan ketika klarifikasi atau invalidasi output ada.
const PLAN_VERSION: i32 = 1;

/// Jalankan loop worker sampai `shutdown` dibatalkan.
pub async fn run(
    foundation: Foundation,
    catalog: Arc<Catalog>,
    catalog_version_id: Option<Uuid>,
    shutdown: CancellationToken,
) {
    let config = foundation.config();
    let worker = worker_identity();
    let poll = Duration::from_millis(config.worker_poll_interval_ms);

    info!(%worker, "worker berjalan");

    loop {
        if shutdown.is_cancelled() {
            break;
        }

        match repository::claim_next(
            foundation.app_db().pool(),
            &worker,
            config.worker_lease_duration_secs,
            config.job_ttl_running_secs,
        )
        .await
        {
            Ok(Some(job)) => {
                if let Err(error) =
                    process(&foundation, &catalog, catalog_version_id, &worker, job).await
                {
                    error!(error = %error, "job gagal diproses");
                }
                // Langsung lanjut: mungkin masih ada antrean.
                continue;
            }
            Ok(None) => {}
            Err(error) => {
                // Database bermasalah; jangan memutar loop sekencang mungkin.
                error!(error = %error, "klaim job gagal");
                sleep(poll).await;
                continue;
            }
        }

        tokio::select! {
            _ = shutdown.cancelled() => break,
            _ = sleep(poll) => {}
        }
    }

    info!(%worker, "worker berhenti");
}

async fn process(
    foundation: &Foundation,
    catalog: &Catalog,
    catalog_version_id: Option<Uuid>,
    worker: &str,
    job: ClaimedJob,
) -> anyhow::Result<()> {
    let pool = foundation.app_db().pool().clone();
    let config = foundation.config();

    // K1: heartbeat WAJIB dari task independen. Bila ia hanya dipancarkan di
    // batas node, satu node lambat membuat worker sehat dipagari di tengah kerja.
    let fenced = CancellationToken::new();
    let heartbeat = tokio::spawn(heartbeat_loop(
        pool.clone(),
        job.id,
        job.lease_token,
        config.worker_lease_duration_secs,
        Duration::from_secs(config.worker_lease_heartbeat_interval_secs),
        fenced.clone(),
    ));

    let settled = if repository::cancel_requested(&pool, job.id).await? {
        repository::settle_cancelled(&pool, job.id, job.session_id, job.lease_token).await?
    } else {
        run_job(foundation, catalog, catalog_version_id, &job).await?
    };

    fenced.cancel();
    let _ = heartbeat.await;

    if settled {
        info!(job_id = %job.id, %worker, "job diselesaikan");
    } else {
        // Fencing kalah: worker lain sudah memegang job ini, atau reaper sudah
        // menutupnya. Berhenti tanpa mencoba transisi alternatif (engine.md §5).
        warn!(job_id = %job.id, %worker, "worker dipagari; tidak ada yang ditulis");
    }

    Ok(())
}

async fn heartbeat_loop(
    pool: sqlx::PgPool,
    job_id: Uuid,
    lease_token: Uuid,
    lease_duration_secs: i64,
    interval: Duration,
    fenced: CancellationToken,
) {
    loop {
        tokio::select! {
            _ = fenced.cancelled() => break,
            _ = sleep(interval) => {}
        }

        match repository::renew_lease(&pool, job_id, lease_token, lease_duration_secs).await {
            Ok(true) => {}
            Ok(false) => {
                warn!(%job_id, "lease tidak dapat diperpanjang; worker dipagari");
                fenced.cancel();
                break;
            }
            Err(error) => {
                error!(%job_id, error = %error, "renewal lease gagal");
            }
        }
    }
}

/// Jalankan satu job: plan → eksekusi → komposisi → commit.
async fn run_job(
    foundation: &Foundation,
    catalog: &Catalog,
    catalog_version_id: Option<Uuid>,
    job: &ClaimedJob,
) -> anyhow::Result<bool> {
    let pool = foundation.app_db().pool();

    // Tanpa versi katalog tercatat, tidak ada yang dapat dirujuk plan sebagai
    // "kontrak yang dilihat planner" — dan plan tanpa rujukan itu tidak dapat
    // diinvestigasi (migrasi 3).
    let Some(catalog_version_id) = catalog_version_id else {
        return repository::settle_with_response(
            pool,
            job.id,
            job.session_id,
            job.lease_token,
            limitation_response(
                "catalog_version_unavailable",
                "The approved catalog version is not registered, so no capability can be executed.",
                &job.request_text,
            ),
        )
        .await
        .map_err(Into::into);
    };

    // Scope dari otorisasi, dipersempit oleh permintaan — tidak pernah
    // diperlebar olehnya (I7).
    let requested_offices = requested_office_ids(&job.scope_json);
    let authorized = match executor::authorized_office_ids(
        foundation.fineract_db(),
        &requested_offices,
    )
    .await
    {
        Ok(offices) => offices,
        Err(error) => {
            warn!(job_id = %job.id, code = error.failure_code(), "scope tidak dapat diturunkan");
            return settle_operational_failure(foundation, job, error.failure_code()).await;
        }
    };

    // Jawaban klarifikasi yang sudah diterima dibaca lebih dulu: slot yang
    // sudah dijawab tidak pernah ditanyakan ulang (clarifications.md).
    let supplied = clarification_repository::accepted_answers(pool, job.id).await?;

    let planned = planner::plan(
        pool,
        catalog,
        catalog_version_id,
        &job.request_text,
        &authorized,
        &supplied,
    )
    .await?;

    let plan = match planned {
        Ok(plan) => plan,
        // Parameter yang kurang dan dapat dijawab pengguna → tanyakan (T5),
        // jangan tolak. Job yang sama ditangguhkan; tidak ada job pengganti.
        Err(planner::Unplannable::NeedsClarification { capability, missing })
            if missing.iter().all(|item| !item.identity) =>
        {
            return open_clarification(foundation, job, &capability, &missing).await;
        }
        Err(problem) => {
            return repository::settle_with_response(
                pool,
                job.id,
                job.session_id,
                job.lease_token,
                limitation_response(&problem.reason(), &problem.explain(), &job.request_text),
            )
            .await
            .map_err(Into::into);
        }
    };

    let contract_versions = serde_json::json!({
        "catalog_version_id": plan.catalog_version_id,
        "catalog_content_hash": plan.catalog_content_hash,
        "capability_id": plan.capability_id,
        "query_id": plan.query_id,
    });

    let persisted = repository::persist_plan(
        pool,
        job.id,
        job.session_id,
        job.lease_token,
        PLAN_VERSION,
        &plan.graph_json,
        &plan.graph_hash,
        &contract_versions,
        &plan.capability_id,
    )
    .await?;

    if !persisted {
        return Ok(false);
    }

    // Di luar transaksi mana pun (I1).
    let executed = executor::execute(foundation.fineract_db(), &plan).await;

    match executed {
        Ok(result) => {
            // Sakelar PII disnapshot saat job diterima (#15), bukan dibaca ulang
            // sekarang: laporan tidak boleh berubah makna karena konfigurasi
            // berubah di tengah eksekusi.
            let pii_enabled = job
                .scope_json
                .get("pii")
                .and_then(|pii| pii.get("enabled"))
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);

            let response = compose::analysis(&plan, &result.rows, result.duration_ms, pii_enabled);
            let (_, withheld) = compose::visible_fields(&plan, pii_enabled);

            let node_persisted = repository::complete_node(
                pool,
                job.id,
                job.session_id,
                job.lease_token,
                PLAN_VERSION,
                NodeOutcome {
                    status: "Completed",
                    completeness: Some("Complete"),
                    failure_code: None,
                    // Hasil kecil disimpan inline; dataset berchunk baru
                    // diperlukan saat hasil besar, dan belum ada konsumennya.
                    // Kolom yang ditahan dibuang SEBELUM disimpan: PII yang
                    // hanya disembunyikan dari response tetap tersimpan, dan
                    // yang tersimpan cepat atau lambat terbaca.
                    output_json: Some(serde_json::json!({
                        "rows": compose::redact(&result.rows, &withheld),
                        "withheld_columns": withheld,
                    })),
                    provenance_json: node_provenance(&plan, result.rows.len()),
                    rows_returned: Some(result.rows.len() as i64),
                    duration_ms: Some(result.duration_ms),
                },
            )
            .await?;

            if !node_persisted {
                return Ok(false);
            }

            repository::settle_with_response(
                pool,
                job.id,
                job.session_id,
                job.lease_token,
                response,
            )
            .await
            .map_err(Into::into)
        }
        Err(error) => {
            let failure_code = error.failure_code();
            warn!(job_id = %job.id, code = failure_code, "eksekusi query gagal");

            // Kegagalan node tetap dicatat di ledger: tanpa ini, investigasi
            // hanya melihat job gagal tanpa tahu operasi mana yang gagal.
            repository::complete_node(
                pool,
                job.id,
                job.session_id,
                job.lease_token,
                PLAN_VERSION,
                NodeOutcome {
                    status: "Failed",
                    // Hasilnya TIDAK DIKETAHUI, bukan nol (I4).
                    completeness: Some("Unknown"),
                    failure_code: Some(failure_code),
                    output_json: None,
                    provenance_json: node_provenance(&plan, 0),
                    rows_returned: None,
                    duration_ms: None,
                },
            )
            .await?;

            settle_operational_failure(foundation, job, failure_code).await
        }
    }
}

/// T5 — tangguhkan job dan terbitkan satu form berisi seluruh slot yang kurang.
async fn open_clarification(
    foundation: &Foundation,
    job: &ClaimedJob,
    capability: &str,
    missing: &[planner::Missing],
) -> anyhow::Result<bool> {
    let fields: Vec<serde_json::Value> = missing
        .iter()
        .map(|item| {
            serde_json::json!({
                "field_id": item.name,
                "type": item.field_type(),
                // Tipe parameter ikut dibawa supaya validasi jawaban memakai
                // kontrak yang sama dengan pengikatan parameter — bukan dua
                // aturan yang dapat menyimpang.
                "parameter_kind": item.kind,
                "label": item.name.replace('_', " "),
                "required": true,
            })
        })
        .collect();

    let form = clarification_repository::open_form(
        foundation.app_db().pool(),
        job.id,
        job.session_id,
        job.lease_token,
        None,
        &format!("Missing input for capability '{capability}'"),
        "Additional input needed",
        &serde_json::Value::Array(fields),
        foundation.config().clarification_wait_limit_secs,
    )
    .await?;

    match form {
        Some(form) => {
            info!(job_id = %job.id, clarification_id = %form.clarification_id, "klarifikasi dibuka");
            Ok(true)
        }
        // Fencing kalah.
        None => Ok(false),
    }
}

async fn settle_operational_failure(
    foundation: &Foundation,
    job: &ClaimedJob,
    failure_code: &str,
) -> anyhow::Result<bool> {
    let blocks = serde_json::json!([
        {
            "type": "limitation",
            "id": failure_code,
            "title": "Request not answered",
            "body": "The approved source query did not complete, so no figure is reported. \
                     The outcome of the attempt is unknown, not zero.",
        }
    ]);

    repository::settle_failed(
        foundation.app_db().pool(),
        job.id,
        job.session_id,
        job.lease_token,
        failure_code,
        SettledResponse {
            kind: "limitation",
            outcome: "OperationalFailure",
            completeness: "Unknown",
            completeness_reason: failure_code.to_string(),
            response_hash: hash_blocks(&blocks),
            blocks,
        },
    )
    .await
    .map_err(Into::into)
}

/// Penyempitan office yang diminta saat job diterima (snapshot `scope_json`).
fn requested_office_ids(scope_json: &serde_json::Value) -> Vec<i64> {
    scope_json
        .get("office_ids")
        .and_then(|value| value.as_array())
        .map(|ids| ids.iter().filter_map(serde_json::Value::as_i64).collect())
        .unwrap_or_default()
}

fn node_provenance(plan: &Plan, row_count: usize) -> serde_json::Value {
    serde_json::json!({
        "capability_id": plan.capability_id,
        "query_id": plan.query_id,
        "sql_file": plan.sql_file,
        "catalog_version_id": plan.catalog_version_id,
        "catalog_content_hash": plan.catalog_content_hash,
        "retrieval_score": plan.retrieval_score,
        "timeout_ms": plan.timeout_ms,
        "row_count": row_count,
    })
}

/// Response `limitation` yang menyatakan sebabnya, bukan dokumen kosong
/// (responses.md: hasilnya response `kind='limitation'`, bukan job yang gagal
/// diam-diam).
fn limitation_response(reason: &str, explanation: &str, request_text: &str) -> SettledResponse {
    let blocks = serde_json::json!([
        {
            "type": "limitation",
            "id": reason,
            "title": "Request not answered",
            "body": explanation,
            "request_echo": request_text,
        }
    ]);

    SettledResponse {
        kind: "limitation",
        // engine.md: Completed + Unsupported wajib berpasangan dengan
        // completeness Unknown — tidak ada klaim kelengkapan atas data sumber.
        outcome: "Unsupported",
        completeness: "Unknown",
        completeness_reason: reason.to_string(),
        response_hash: hash_blocks(&blocks),
        blocks,
    }
}

/// Hash isi response. Audit menyimpan hash, bukan isinya (migrasi 6).
fn hash_blocks(blocks: &serde_json::Value) -> String {
    hex::encode(Sha256::digest(blocks.to_string().as_bytes()))
}

/// Identitas worker untuk `lease_owner`: cukup untuk menjawab "proses mana yang
/// memegang job ini" saat investigasi.
fn worker_identity() -> String {
    let host = std::env::var("HOSTNAME").unwrap_or_else(|_| "unknown-host".to_string());
    format!("{host}/{}", std::process::id())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_response_matches_engine_matrix() {
        let response = limitation_response("no_capability_matched", "tidak tercakup", "berapa total portfolio?");

        assert_eq!(response.kind, "limitation");
        assert_eq!(response.outcome, "Unsupported");
        // Completed + Unsupported dengan completeness selain Unknown dilarang.
        assert_eq!(response.completeness, "Unknown");
    }

    #[test]
    fn response_hash_follows_content() {
        let first = limitation_response("r", "penjelasan", "pertanyaan a");
        let second = limitation_response("r", "penjelasan", "pertanyaan b");

        assert_ne!(first.response_hash, second.response_hash);
        assert_eq!(first.response_hash.len(), 64);
    }

    #[test]
    fn limitation_block_states_the_reason() {
        let response = limitation_response("r", "penjelasan", "apa pun");
        let blocks = response.blocks.as_array().unwrap();

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0]["type"], "limitation");
        assert!(!blocks[0]["body"].as_str().unwrap().is_empty());
    }
}
