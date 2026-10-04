//! Subcommand `catalog check`.
//!
//! Ini alat review carry-over: `knowledge/CARRY-OVER.md` menuntut tiap entri
//! diperiksa sebelum dipercaya, dan laporan ini yang membuat daftar entri mana
//! yang belum memenuhi syarat — bukan ingatan orang.

use chat::catalog::{self, Severity};
use foundation::Foundation;

/// Jalankan pemeriksaan katalog. `Ok(false)` berarti ada temuan Error.
pub async fn run(
    foundation: &Foundation,
    sync: bool,
    embed: bool,
    skip_probe: bool,
) -> anyhow::Result<bool> {
    let checked = catalog::check(foundation, !skip_probe).await?;
    let report = &checked.report;

    println!("content_hash : {}", checked.catalog.content_hash);
    println!(
        "dimuat       : {} capability, {} query manifest, {} dataset, {} file SQL",
        checked.catalog.capabilities.len(),
        checked.catalog.queries.len(),
        checked.catalog.datasets.len(),
        checked.catalog.sql_files.len()
    );
    println!(
        "probe        : {}",
        if skip_probe {
            "dilewati (--no-probe)"
        } else {
            "SQL disiapkan terhadap schema Fineract"
        }
    );
    println!();

    let mut findings: Vec<_> = report.findings.iter().collect();
    findings.sort_by(|a, b| {
        a.severity
            .cmp(&b.severity)
            .then_with(|| a.check.cmp(&b.check))
            .then_with(|| a.subject.cmp(&b.subject))
    });

    for finding in &findings {
        let label = match finding.severity {
            Severity::Error => "ERROR  ",
            Severity::Warning => "WARNING",
        };
        println!("{label} [{}] {}", finding.check, finding.subject);
        println!("         {}", finding.message);
    }

    if !findings.is_empty() {
        println!();
    }

    println!("Cakupan pemeriksaan:");
    for line in catalog::validate::coverage() {
        println!("  - {line}");
    }
    println!();
    println!(
        "Ringkasan    : {} error, {} warning → status katalog: {}",
        report.errors(),
        report.warnings(),
        checked.status()
    );
    let passed = report.errors() == 0;
    anyhow::ensure!(sync || !embed, "--embed wajib dipakai bersama --sync");

    if sync {
        let admission =
            chat::catalog::reindex::service::admit(foundation, checked, None, embed, None)
                .await
                .map_err(|error| anyhow::anyhow!("{error}"))?;
        let version_id = admission.run().catalog_version_id;
        let finished = admission.execute(foundation).await?;
        if let Some(version_id) = version_id.or(finished.catalog_version_id) {
            println!("Versi katalog dicatat: {version_id}");
        }
        if embed {
            println!(
                "Backfill embedding selesai: {} baris ({} total)",
                finished.embedded_row_count, finished.lexical_row_count
            );
        }
    }

    Ok(passed)
}
