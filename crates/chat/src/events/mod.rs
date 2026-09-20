//! Progress publik sebagai stream, diproyeksikan dari `job_events`.
//!
//! Tiga aturan yang menentukan seluruh bentuk modul ini:
//!
//! 1. **PostgreSQL adalah sumbernya.** `job_events.sequence` strictly increasing
//!    per job (I3) dan ditulis satu transaksi dengan perubahan state (C14).
//!    Notifikasi hanya memberi tahu "ada yang baru"; ia tidak pernah membawa
//!    isinya dan tidak pernah menjadi alasan sebuah event dianggap ada.
//! 2. **Kehilangan notifikasi wajib pulih.** Karena itu setiap stream tetap
//!    membaca PostgreSQL secara berkala walau tidak ada notifikasi sama sekali —
//!    dengan Redis mati, dengan Redis dimatikan, dan dengan notifikasi yang
//!    jatuh saat subscriber sedang sibuk.
//! 3. **Duplikat aman, kehilangan tidak.** Stream boleh mengirim ulang sebuah
//!    sequence; ia tidak boleh melewatinya.

pub mod repository;
pub mod route;
pub mod service;

use std::sync::Arc;

use foundation::state::Foundation;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};
use uuid::Uuid;

/// Channel `pg_notify` yang dipancarkan `job::repository::append_event`.
const PG_CHANNEL: &str = "job_events";
/// Channel Redis untuk menyebarkan notifikasi antar instance.
const REDIS_CHANNEL: &str = "jarvis:job_events";
/// Kapasitas fan-out in-process. Subscriber yang tertinggal menerima `Lagged`,
/// dan itu **bukan** kehilangan event: ia hanya kehilangan sinyal bangun, lalu
/// membaca PostgreSQL seperti biasa.
const FANOUT_CAPACITY: usize = 256;

/// Fan-out notifikasi ke seluruh stream yang sedang terbuka.
#[derive(Debug)]
pub struct Hub {
    sender: broadcast::Sender<Uuid>,
}

impl Hub {
    /// Jalankan pendengar notifikasi.
    ///
    /// `pg_notify` adalah sumber notifikasi utama karena ia transaksional:
    /// ia terkirim tepat saat event commit, tidak lebih awal. Redis dipakai
    /// untuk menyebarkannya ke instance lain — dan bila Redis mati, yang hilang
    /// hanyalah latensi, bukan event.
    pub fn spawn(foundation: &Foundation, shutdown: CancellationToken) -> Arc<Self> {
        let (sender, _) = broadcast::channel(FANOUT_CAPACITY);
        let hub = Arc::new(Self { sender });

        if !foundation.config().sse_notifications_enabled {
            // Mode degradasi yang didukung: tidak ada notifikasi sama sekali.
            // Stream tetap lengkap karena kebenarannya ada di PostgreSQL; yang
            // berubah hanya latensi, dan itu terlihat sebagai jeda polling.
            info!("notifikasi event dimatikan; stream sepenuhnya memakai fallback polling");
            return hub;
        }

        tokio::spawn(listen_postgres(
            foundation.clone(),
            sender_of(&hub),
            shutdown.clone(),
        ));

        if foundation.notifier().is_live() {
            tokio::spawn(listen_redis(
                foundation.clone(),
                sender_of(&hub),
                shutdown,
            ));
        } else {
            info!(
                status = foundation.notifier().status(),
                "redis tidak dipakai untuk notifikasi; stream bergantung pada pg_notify dan fallback polling"
            );
        }

        hub
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Uuid> {
        self.sender.subscribe()
    }
}

fn sender_of(hub: &Arc<Hub>) -> broadcast::Sender<Uuid> {
    hub.sender.clone()
}

async fn listen_postgres(
    foundation: Foundation,
    sender: broadcast::Sender<Uuid>,
    shutdown: CancellationToken,
) {
    loop {
        if shutdown.is_cancelled() {
            break;
        }

        let mut listener = match sqlx::postgres::PgListener::connect_with(foundation.app_db().pool())
            .await
        {
            Ok(listener) => listener,
            Err(error) => {
                warn!(error = %error, "listener pg_notify gagal dibuka; mencoba lagi");
                if wait(&shutdown).await {
                    break;
                }
                continue;
            }
        };

        if let Err(error) = listener.listen(PG_CHANNEL).await {
            warn!(error = %error, "LISTEN gagal; mencoba lagi");
            if wait(&shutdown).await {
                break;
            }
            continue;
        }

        info!(channel = PG_CHANNEL, "mendengarkan notifikasi event");

        loop {
            let notification = tokio::select! {
                _ = shutdown.cancelled() => return,
                received = listener.recv() => received,
            };

            match notification {
                Ok(notification) => {
                    let Ok(job_id) = notification.payload().parse::<Uuid>() else {
                        warn!(payload = notification.payload(), "payload notifikasi bukan job id");
                        continue;
                    };
                    // Tidak ada subscriber = tidak ada stream terbuka. Bukan
                    // kesalahan: event tetap durable di PostgreSQL.
                    let _ = sender.send(job_id);
                    foundation
                        .notifier()
                        .publish(REDIS_CHANNEL, notification.payload())
                        .await;
                }
                Err(error) => {
                    // Koneksi putus. Yang hilang adalah notifikasi, bukan event;
                    // stream yang sedang terbuka tetap menemukannya lewat
                    // fallback polling sementara listener tersambung ulang.
                    warn!(error = %error, "koneksi listener putus; tersambung ulang");
                    break;
                }
            }
        }

        if wait(&shutdown).await {
            break;
        }
    }
}

async fn listen_redis(
    foundation: Foundation,
    sender: broadcast::Sender<Uuid>,
    shutdown: CancellationToken,
) {
    use futures::StreamExt;

    loop {
        if shutdown.is_cancelled() {
            break;
        }

        let mut pubsub = match foundation::redis::subscriber(&foundation.config().redis_url).await {
            Ok(pubsub) => pubsub,
            Err(error) => {
                warn!(error = %error, "subscriber redis gagal dibuka; stream tetap jalan tanpa notifikasi lintas instance");
                if wait(&shutdown).await {
                    break;
                }
                continue;
            }
        };

        if let Err(error) = pubsub.subscribe(REDIS_CHANNEL).await {
            warn!(error = %error, "SUBSCRIBE redis gagal");
            if wait(&shutdown).await {
                break;
            }
            continue;
        }

        let mut stream = pubsub.on_message();
        loop {
            let message = tokio::select! {
                _ = shutdown.cancelled() => return,
                message = stream.next() => message,
            };

            let Some(message) = message else {
                break;
            };

            if let Ok(payload) = message.get_payload::<String>()
                && let Ok(job_id) = payload.parse::<Uuid>()
            {
                let _ = sender.send(job_id);
            }
        }

        if wait(&shutdown).await {
            break;
        }
    }
}

/// Jeda sebelum mencoba lagi. `true` berarti shutdown diminta.
async fn wait(shutdown: &CancellationToken) -> bool {
    tokio::select! {
        _ = shutdown.cancelled() => true,
        _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => false,
    }
}
