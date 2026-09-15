//! Engine: klaim job, lease/fencing, recovery, dan penyelesaian.
//!
//! Satu Engine memiliki orchestration end-to-end. Planner, eksekusi node, dan
//! klarifikasi belum ada — lihat [`worker`] untuk arti penyelesaian saat ini.

pub mod repository;
pub mod reaper;
pub mod worker;

use std::time::Duration;

use foundation::state::Foundation;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::info;

/// Task latar Engine, dimatikan bersama proses.
pub struct Background {
    shutdown: CancellationToken,
    tasks: Vec<JoinHandle<()>>,
}

impl Background {
    /// Jalankan worker dan reaper bila diaktifkan.
    pub fn spawn(foundation: &Foundation) -> Self {
        let shutdown = CancellationToken::new();
        let mut tasks = Vec::new();

        if foundation.config().worker_enabled {
            tasks.push(tokio::spawn(worker::run(foundation.clone(), shutdown.clone())));
            tasks.push(tokio::spawn(reaper::run(
                foundation.clone(),
                Duration::from_secs(foundation.config().reaper_interval_secs),
                shutdown.clone(),
            )));
        } else {
            // Dinyatakan, bukan didiamkan: proses tanpa worker menerima job dan
            // tidak pernah menjalankannya, dan itu wajib terlihat di log.
            info!("worker dimatikan (WORKER_ENABLED=false); job akan tetap Queued");
        }

        Self { shutdown, tasks }
    }

    /// Minta berhenti lalu tunggu task selesai.
    pub async fn shutdown(self) {
        self.shutdown.cancel();
        for task in self.tasks {
            let _ = task.await;
        }
    }
}
