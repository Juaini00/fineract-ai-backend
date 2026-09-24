//! Pool PostgreSQL.
//!
//! Dua pool terpisah (overview §2): pool aplikasi bersifat writable dan
//! authoritative; pool Fineract **read-only** dan memakai kredensial berbeda.
//! Keduanya tidak pernah ditukar — karena itu tipenya dibedakan, bukan sekadar
//! dua `PgPool` yang bentuknya identik.

use std::str::FromStr;

use sqlx::postgres::{PgConnectOptions, PgPool, PgPoolOptions};

use crate::commit_isolation::{self, ExternalCall};
use crate::config::Config;

/// Pool ke PostgreSQL aplikasi: writable, sumber kebenaran durable.
#[derive(Debug, Clone)]
pub struct AppDb(PgPool);

/// Pool ke Fineract: **SELECT saja**. Tidak pernah menulis, tidak pernah
/// mengubah schema (AGENTS.md).
#[derive(Debug, Clone)]
pub struct FineractDb(PgPool);

impl AppDb {
    pub fn pool(&self) -> &PgPool {
        &self.0
    }

    pub async fn connect(config: &Config) -> anyhow::Result<Self> {
        Ok(Self(
            connect(
                &config.app_database_url,
                config.app_database_max_connections,
            )
            .await?,
        ))
    }

    /// Pasang schema dari `migrations/`. Hanya dipanggil bila
    /// [`Config::may_migrate_on_startup`] mengizinkan.
    pub async fn migrate(&self) -> anyhow::Result<()> {
        sqlx::migrate!("../../migrations").run(&self.0).await?;
        Ok(())
    }

    /// Probe koneksi untuk health check.
    pub async fn ping(&self) -> anyhow::Result<()> {
        sqlx::query("SELECT 1").execute(&self.0).await?;
        Ok(())
    }
}

impl FineractDb {
    /// Satu-satunya jalan menuju pool Fineract, karena itu juga titik cek I1:
    /// query sumber tidak pernah boleh berjalan saat transaksi commit terbuka.
    pub fn pool(&self) -> &PgPool {
        commit_isolation::guard(ExternalCall::Fineract);
        &self.0
    }

    /// Lazy: koneksi pertama dibuka saat query pertama.
    ///
    /// Fineract adalah sumber data, bukan sumber kebenaran aplikasi. Replika
    /// yang sedang mati tidak boleh menahan boot — job yang membutuhkannya akan
    /// gagal dengan sebab yang jelas dan `/health` melaporkannya, sedangkan
    /// gagal boot hanya menyembunyikan seluruh sistem di balik satu dependency.
    ///
    /// Setiap sesi dibuka dengan `default_transaction_read_only=on`: read-only
    /// ditegakkan server, bukan hanya diandalkan pada kredensial terpisah —
    /// kredensial lokal (`root`) writable, dan statement tulis apa pun kini
    /// gagal dengan `cannot execute … in a read-only transaction` (OVR-6.1).
    pub fn connect(config: &Config) -> anyhow::Result<Self> {
        let options = PgConnectOptions::from_str(&config.fineract_database_url)?
            .options([("default_transaction_read_only", "on")]);
        Ok(Self(
            PgPoolOptions::new()
                .max_connections(config.fineract_database_max_connections)
                .connect_lazy_with(options),
        ))
    }

    pub async fn ping(&self) -> anyhow::Result<()> {
        sqlx::query("SELECT 1").execute(self.pool()).await?;
        Ok(())
    }
}

async fn connect(url: &str, max_connections: u32) -> anyhow::Result<PgPool> {
    Ok(PgPoolOptions::new()
        .max_connections(max_connections)
        .connect(url)
        .await?)
}
