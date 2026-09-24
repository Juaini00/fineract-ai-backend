//! Redis — **hanya koordinasi live** (overview §5).
//!
//! Bukan job ledger, bukan event history, bukan cursor authority, dan bukan
//! tempat recovery. Karena itu Redis boleh mati: startup tidak gagal, health
//! melaporkannya sebagai degraded, dan state tetap dibaca dari PostgreSQL.

use redis::aio::ConnectionManager;
use tracing::warn;

use crate::commit_isolation::{self, ExternalCall};
use crate::config::Config;

/// Koneksi Redis opsional.
///
/// `Disabled` = sengaja dimatikan lewat `REDIS_ENABLED=false`.
/// `Unavailable` = dinyalakan tetapi tidak dapat dihubungi saat startup;
/// keduanya dibedakan supaya health tidak melaporkan konfigurasi sebagai insiden.
#[derive(Clone)]
pub enum Notifier {
    Live(ConnectionManager),
    Unavailable,
    Disabled,
}

impl std::fmt::Debug for Notifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.status())
    }
}

impl Notifier {
    /// Hubungkan bila diaktifkan. Kegagalan koneksi **tidak** menggagalkan
    /// startup: notifikasi yang hilang selalu dapat dipulihkan dari PostgreSQL.
    pub async fn connect(config: &Config) -> Self {
        if !config.redis_enabled {
            return Self::Disabled;
        }

        match connect(&config.redis_url).await {
            Ok(manager) => Self::Live(manager),
            Err(error) => {
                warn!(error = %error, "redis tidak tersedia saat startup; berjalan tanpa notifikasi live");
                Self::Unavailable
            }
        }
    }

    /// Label status untuk health/log: `live`, `unavailable`, atau `disabled`.
    pub fn status(&self) -> &'static str {
        match self {
            Self::Live(_) => "live",
            Self::Unavailable => "unavailable",
            Self::Disabled => "disabled",
        }
    }

    pub fn is_live(&self) -> bool {
        matches!(self, Self::Live(_))
    }

    /// Pancarkan notifikasi. Kegagalan **tidak** dinaikkan sebagai error:
    /// notifikasi bukan sumber kebenaran, dan subscriber yang kehilangannya
    /// tetap menemukan event itu lewat PostgreSQL. Yang dilarang adalah
    /// kehilangan yang tidak terlihat — karena itu ia tetap di-log.
    pub async fn publish(&self, channel: &str, payload: &str) {
        commit_isolation::guard(ExternalCall::Redis);
        let Self::Live(manager) = self else {
            return;
        };

        let mut manager = manager.clone();
        if let Err(error) = redis::cmd("PUBLISH")
            .arg(channel)
            .arg(payload)
            .exec_async(&mut manager)
            .await
        {
            warn!(error = %error, channel, "notifikasi redis gagal dipancarkan");
        }
    }
}

/// Buka koneksi subscribe tersendiri.
///
/// Pub/sub tidak dapat berbagi koneksi dengan perintah biasa: koneksi yang
/// sudah `SUBSCRIBE` hanya menerima perintah pub/sub.
pub async fn subscriber(url: &str) -> anyhow::Result<redis::aio::PubSub> {
    Ok(redis::Client::open(url)?.get_async_pubsub().await?)
}

async fn connect(url: &str) -> anyhow::Result<ConnectionManager> {
    Ok(redis::Client::open(url)?
        .get_connection_manager()
        .await?)
}
