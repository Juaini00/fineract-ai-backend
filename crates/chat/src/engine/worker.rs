//! Worker: mengklaim job, menjaga lease, lalu menyelesaikannya.
//!
//! Satu Engine memiliki orchestration end-to-end (PRD §4). Sampai planner dan
//! eksekusi node ada, penyelesaian yang jujur untuk setiap permintaan adalah
//! `Completed` + `Unsupported` + `Unknown` dengan response `kind='limitation'`:
//! tidak ada capability yang disetujui yang dapat dipilih, dan itu justru arti
//! `Unsupported` menurut PRD §5 — bukan kegagalan operasional, bukan pula
//! jawaban kosong yang berpura-pura menjawab.

use std::time::Duration;

use foundation::state::Foundation;
use sha2::{Digest, Sha256};
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::engine::repository::{self, ClaimedJob, SettledResponse};

/// Jalankan loop worker sampai `shutdown` dibatalkan.
pub async fn run(foundation: Foundation, shutdown: CancellationToken) {
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
                if let Err(error) = process(&foundation, &worker, job).await {
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

async fn process(foundation: &Foundation, worker: &str, job: ClaimedJob) -> anyhow::Result<()> {
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
        repository::settle_with_response(
            &pool,
            job.id,
            job.session_id,
            job.lease_token,
            unsupported_response(&job.request_text),
        )
        .await?
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

/// Response `limitation` yang menyatakan sebabnya, bukan dokumen kosong
/// (responses.md: hasilnya response `kind='limitation'`, bukan job yang gagal
/// diam-diam).
fn unsupported_response(request_text: &str) -> SettledResponse {
    let blocks = serde_json::json!([
        {
            "type": "limitation",
            "id": "planner_absent",
            "title": "Request not answered",
            "body": "No approved capability was selected for this request. \
                     Planning and query execution are not implemented yet, so Jarvis \
                     cannot claim any figure for it.",
            "request_echo": request_text,
        }
    ]);

    SettledResponse {
        kind: "limitation",
        // engine.md: Completed + Unsupported wajib berpasangan dengan
        // completeness Unknown — tidak ada klaim kelengkapan atas data sumber.
        outcome: "Unsupported",
        completeness: "Unknown",
        completeness_reason: "planner_not_implemented".to_string(),
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
        let response = unsupported_response("berapa total portfolio?");

        assert_eq!(response.kind, "limitation");
        assert_eq!(response.outcome, "Unsupported");
        // Completed + Unsupported dengan completeness selain Unknown dilarang.
        assert_eq!(response.completeness, "Unknown");
    }

    #[test]
    fn response_hash_follows_content() {
        let first = unsupported_response("pertanyaan a");
        let second = unsupported_response("pertanyaan b");

        assert_ne!(first.response_hash, second.response_hash);
        assert_eq!(first.response_hash.len(), 64);
    }

    #[test]
    fn limitation_block_states_the_reason() {
        let response = unsupported_response("apa pun");
        let blocks = response.blocks.as_array().unwrap();

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0]["type"], "limitation");
        assert!(!blocks[0]["body"].as_str().unwrap().is_empty());
    }
}
