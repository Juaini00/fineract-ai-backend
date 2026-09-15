//! Engine: klaim job, lease/fencing, recovery, dan penyelesaian.
//!
//! Satu Engine memiliki orchestration end-to-end. Planner, eksekusi node, dan
//! klarifikasi belum ada — lihat [`worker`] untuk arti penyelesaian saat ini.

pub mod compose;
pub mod executor;
pub mod planner;
pub mod reaper;
pub mod repository;
pub mod worker;

use std::{sync::Arc, time::Duration};

use foundation::state::Foundation;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

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
    pub async fn spawn(foundation: &Foundation) -> anyhow::Result<Self> {
        let shutdown = CancellationToken::new();
        let mut tasks = Vec::new();

        if foundation.config().worker_enabled {
            let checked = crate::catalog::check(foundation, false).await?;
            let status = checked.status();
            let catalog = Arc::new(checked.catalog);

            let catalog_version_id = match repository::catalog_version_id(
                foundation.app_db().pool(),
                &catalog.content_hash,
            )
            .await?
            {
                Some(id) => Some(id),
                None => {
                    // Versi belum tercatat (mis. `catalog --sync` belum pernah
                    // dijalankan). Dicatat sekarang apa adanya — termasuk bila
                    // statusnya `failed`.
                    Some(
                        crate::catalog::repository::upsert_version(
                            foundation.app_db().pool(),
                            &catalog,
                            status,
                            serde_json::json!({ "registered_by": "engine_startup" }),
                        )
                        .await?,
                    )
                }
            };

            if status != "validated" {
                warn!(
                    content_hash = %catalog.content_hash,
                    "katalog berstatus {status}; capability darinya tetap dieksekusi hanya bila planner memilihnya"
                );
            }

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

    /// Minta berhenti lalu tunggu task selesai.
    pub async fn shutdown(self) {
        self.shutdown.cancel();
        for task in self.tasks {
            let _ = task.await;
        }
    }
}
