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

/// Dipanggil saat startup bila `CATALOG_VALIDATE_ON_STARTUP=true`.
///
/// Startup **tidak** digagalkan oleh temuan: katalog carry-over memang belum
/// direview (`knowledge/CARRY-OVER.md`), dan menolak boot hanya memindahkan
/// review menjadi penghalang tanpa mempercepatnya. Yang ditegakkan adalah
/// sebaliknya — versi katalog dicatat apa adanya, `failed` tetap `failed`, dan
/// Engine kelak menolak mengeksekusi capability dari versi yang bukan
/// `validated`.
pub async fn validate_on_startup(foundation: &Foundation) -> anyhow::Result<()> {
    let checked = check(foundation, false).await?;
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
            warnings = report.warnings(),
            "katalog tervalidasi"
        );
    }

    if foundation.config().catalog_sync_on_startup {
        let version_id = repository::upsert_version(
            foundation.app_db().pool(),
            &checked.catalog,
            checked.status(),
            serde_json::json!({
                "errors": report.errors(),
                "warnings": report.warnings(),
                "probe": "skipped_on_startup",
            }),
        )
        .await?;
        info!(%version_id, "versi katalog dicatat");
    }

    Ok(())
}
