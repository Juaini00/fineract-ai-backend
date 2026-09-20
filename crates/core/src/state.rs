//! Fondasi bersama yang dirakit `app` dan dipakai `chat`.

use std::sync::Arc;

use crate::{
    config::Config,
    db::{AppDb, FineractDb},
    redis::Notifier,
};

/// Dependency fondasi, murah untuk di-clone (satu `Arc`).
#[derive(Debug, Clone)]
pub struct Foundation(Arc<Inner>);

#[derive(Debug)]
struct Inner {
    config: Config,
    app_db: AppDb,
    fineract_db: FineractDb,
    notifier: Notifier,
}

impl Foundation {
    /// Hubungkan seluruh dependency. Gagal hanya bila PostgreSQL aplikasi tidak
    /// tersedia — ia satu-satunya sumber kebenaran durable. Fineract (lazy) dan
    /// Redis (opsional) boleh absen saat boot dan dilaporkan lewat `/health`.
    pub async fn connect(config: Config) -> anyhow::Result<Self> {
        let app_db = AppDb::connect(&config).await?;
        let fineract_db = FineractDb::connect(&config)?;
        let notifier = Notifier::connect(&config).await;

        Ok(Self(Arc::new(Inner {
            config,
            app_db,
            fineract_db,
            notifier,
        })))
    }

    pub fn config(&self) -> &Config {
        &self.0.config
    }

    pub fn app_db(&self) -> &AppDb {
        &self.0.app_db
    }

    pub fn fineract_db(&self) -> &FineractDb {
        &self.0.fineract_db
    }

    pub fn notifier(&self) -> &Notifier {
        &self.0.notifier
    }
}
