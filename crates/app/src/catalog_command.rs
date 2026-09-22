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

    if sync {
        let version_id = catalog::repository::upsert_version(
            foundation.app_db().pool(),
            &checked.catalog,
            checked.status(),
            serde_json::json!({
                "errors": report.errors(),
                "warnings": report.warnings(),
                "probe": !skip_probe,
            }),
        )
        .await?;
        println!("Versi katalog dicatat: {version_id}");
    }

    if embed {
        anyhow::ensure!(sync, "--embed wajib dipakai bersama --sync");
        let client = foundation::embedding::EmbeddingClient::new(foundation.config())?;
        anyhow::ensure!(
            client.available(),
            "EMBEDDING_API_KEY belum diisi; backfill embedding tidak dapat dijalankan"
        );
        let pending = catalog::repository::pending_embeddings(foundation.app_db().pool()).await?;
        println!("Embedding NULL: {} baris", pending.len());
        for chunk in pending.chunks(32) {
            let texts: Vec<String> = chunk.iter().map(|row| row.retrieval_text.clone()).collect();
            let vectors = client
                .embed(&texts, foundation::embedding::InputKind::Document)
                .await?;
            let rows: Vec<_> = chunk
                .iter()
                .zip(vectors)
                .map(|(row, vector)| (row.id, vector))
                .collect();
            catalog::repository::persist_embeddings(
                foundation.app_db().pool(),
                &rows,
                client.model(),
                client.dimensions(),
                client.document_input_type(),
            )
            .await?;
        }
        println!("Backfill embedding selesai: {} baris", pending.len());
    }

    Ok(report.errors() == 0)
}
