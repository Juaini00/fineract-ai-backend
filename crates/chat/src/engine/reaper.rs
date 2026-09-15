//! Reaper (T11): idempoten dan berulang.
//!
//! Ia tidak pernah menyimpulkan panggilan eksternal berhasil atau gagal —
//! lease yang hilang berarti **tidak diketahui** (I4). Yang dilakukan hanya
//! mengembalikan job ke antrean, menutup pembatalan yang tidak bertuan, dan
//! menghormati batas waktu.

use std::time::Duration;

use foundation::state::Foundation;
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use crate::engine::{dataset, repository};

pub async fn run(foundation: Foundation, interval: Duration, shutdown: CancellationToken) {
    let identity = format!("reaper/{}", std::process::id());
    info!(%identity, ?interval, "reaper berjalan");

    loop {
        tokio::select! {
            _ = shutdown.cancelled() => break,
            _ = sleep(interval) => {}
        }

        match repository::sweep(foundation.app_db().pool(), &identity).await {
            Ok(sweep) if sweep.is_empty() => {}
            Ok(sweep) => info!(
                expired = sweep.expired,
                requeued = sweep.requeued,
                cancelled = sweep.cancelled,
                "reaper menyelesaikan job tertinggal"
            ),
            Err(error) => error!(error = %error, "sapuan reaper gagal"),
        }

        // Purge dataset kedaluwarsa (§6): chunk dihapus, BARIS HANDLE
        // DIPERTAHANKAN supaya statusnya tetap dapat dinyatakan (C13). Dataset
        // milik job nonterminal tidak ikut, berapa pun umurnya (#11).
        match dataset::repository::purge_expired(foundation.app_db().pool()).await {
            Ok(0) => {}
            Ok(purged) => info!(purged, "dataset kedaluwarsa dipurge"),
            Err(error) => error!(error = %error, "purge dataset gagal"),
        }
    }

    info!(%identity, "reaper berhenti");
}
