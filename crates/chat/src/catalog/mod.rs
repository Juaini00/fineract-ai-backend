//! Katalog capability: pemuatan, validasi, dan pencatatan versinya.
//!
//! Eksekusi hanya boleh memakai capability yang disetujui di `knowledge/` dan
//! `queries/` (AGENTS.md) — modul ini yang menentukan apa arti "disetujui"
//! secara mekanis, dan yang memberi `catalog_version_id` + `content_hash` yang
//! wajib dicatat plan (T3) dan node run (T4).

pub mod loader;
pub mod model;
pub mod probe;
pub mod repository;
pub mod validate;

use std::path::Path;

use foundation::state::Foundation;
use tracing::{info, warn};
use uuid::Uuid;

pub use loader::Catalog;
pub use validate::{Finding, Report, Severity};

/// Hasil pemeriksaan lengkap: katalog, temuan, dan apakah ia layak dipakai.
pub struct Checked {
    pub catalog: Catalog,
    pub report: Report,
}

impl Checked {
    /// Katalog dianggap `validated` hanya bila tidak ada temuan Error.
    /// Warning tidak menggagalkan: ia menunggu keputusan manusia.
    pub fn status(&self) -> &'static str {
        if self.report.errors() == 0 {
            "validated"
        } else {
            "failed"
        }
    }
}

/// Muat katalog, validasi statis, lalu (bila `fineract` diberikan) buktikan
/// setiap SQL terhadap schema Fineract yang sebenarnya.
pub async fn check(foundation: &Foundation, probe_fineract: bool) -> anyhow::Result<Checked> {
    let config = foundation.config();
    let catalog = loader::load(
        Path::new(&config.catalog_path),
        Path::new(&config.query_path),
    )?;

    let mut report = validate::validate(&catalog);

    if probe_fineract {
        probe::probe(&catalog, foundation.fineract_db(), &mut report).await;
    }

    Ok(Checked { catalog, report })
}

/// Muat katalog **sekali** untuk seluruh proses, lalu pastikan versinya
/// tercatat.
///
/// Satu pemuatan dipakai bersama worker dan route resolver: dua pemuatan
/// berarti plan dapat merujuk `content_hash` yang berbeda dari katalog yang
/// menerbitkan opsi, dan perbedaan itu tidak akan terlihat sebagai kesalahan
/// apa pun.
pub async fn prepare(foundation: &Foundation) -> anyhow::Result<(std::sync::Arc<Catalog>, Uuid)> {
    let checked = check(foundation, false).await?;
    let status = checked.status();
    let report = &checked.report;

    if report.errors() > 0 {
        warn!(
            content_hash = %checked.catalog.content_hash,
            errors = report.errors(),
            warnings = report.warnings(),
            "katalog TIDAK lolos validasi; capability darinya belum layak dieksekusi"
        );
    } else {
        info!(
            content_hash = %checked.catalog.content_hash,
            capabilities = checked.catalog.capabilities.len(),
            queries = checked.catalog.queries.len(),
            datasets = checked.catalog.datasets.len(),
            warnings = report.warnings(),
            "katalog tervalidasi"
        );
    }

    let pool = foundation.app_db().pool();
    let version_id = match repository::version_id(pool, &checked.catalog.content_hash).await? {
        Some(id) => id,
        // Versi dicatat apa adanya — termasuk bila statusnya `failed`. Plan tanpa
        // versi katalog untuk dirujuk tidak dapat diinvestigasi (migrasi 3).
        None => {
            repository::upsert_version(
                pool,
                &checked.catalog,
                status,
                serde_json::json!({
                    "errors": report.errors(),
                    "warnings": report.warnings(),
                    "probe": "skipped_on_startup",
                }),
            )
            .await?
        }
    };

    Ok((std::sync::Arc::new(checked.catalog), version_id))
}
