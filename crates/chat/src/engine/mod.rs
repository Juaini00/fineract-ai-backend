//! Engine: klaim job, lease/fencing, recovery, dan penyelesaian.
//!
//! Satu Engine memiliki orchestration end-to-end. Planner, eksekusi node, dan
//! klarifikasi belum ada — lihat [`worker`] untuk arti penyelesaian saat ini.

pub mod compose;
pub mod executor;
pub mod memory;
pub mod planner;
pub mod reaper;
pub mod repository;
pub mod resolver;
pub mod validate;
pub mod worker;

use std::{sync::Arc, time::Duration};

use foundation::state::Foundation;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::info;
use uuid::Uuid;

/// Task latar Engine, dimatikan bersama proses.
pub struct Background {
    shutdown: CancellationToken,
    tasks: Vec<JoinHandle<()>>,
}

impl Background {
    /// Jalankan worker dan reaper bila diaktifkan.
    ///
    /// Katalog dimuat SEKALI di sini, bukan per job: isinya tetap selama proses
    /// hidup, dan `content_hash`-nya yang menjadi identitas plan. Versinya
    /// didaftarkan supaya plan punya sesuatu untuk dirujuk — tanpa itu Engine
    /// menolak mengeksekusi capability apa pun.
    pub async fn spawn(
        foundation: &Foundation,
        catalog: Arc<crate::catalog::Catalog>,
        catalog_version_id: Uuid,
    ) -> anyhow::Result<Self> {
        let shutdown = CancellationToken::new();
        let mut tasks = Vec::new();

        if foundation.config().worker_enabled {
            tasks.push(tokio::spawn(worker::run(
                foundation.clone(),
                catalog,
                catalog_version_id,
                shutdown.clone(),
            )));
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

        Ok(Self { shutdown, tasks })
    }

    /// Token yang ikut dibatalkan saat proses berhenti. Dibagikan supaya task
    /// latar lain (mis. hub notifikasi) mati bersama, bukan menggantung.
    pub fn shutdown_token(&self) -> CancellationToken {
        self.shutdown.clone()
    }

    /// Minta berhenti lalu tunggu task selesai.
    pub async fn shutdown(self) {
        self.shutdown.cancel();
        for task in self.tasks {
            let _ = task.await;
        }
    }
}
