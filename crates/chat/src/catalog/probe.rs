//! Pemeriksaan SQL terhadap schema Fineract yang sebenarnya.
//!
//! Read-only dan **tidak pernah menjalankan** query-nya: `describe` menyiapkan
//! statement di server, sehingga sintaks, keberadaan kolom, jumlah parameter,
//! dan nama/urutan kolom hasil terbukti tanpa satu baris pun dibaca.
//!
//! Batasnya jelas dan disebut di `queries/CARRY-OVER.md`: ini membuktikan
//! bentuk, **bukan** bahwa angkanya benar, grainnya benar, atau scope-nya
//! benar-benar menyempitkan hasil.

use foundation::db::FineractDb;
use sqlx::{AssertSqlSafe, Column, Executor, SqlSafeStr};

use crate::catalog::{
    loader::Catalog,
    validate::{Finding, Report},
};

/// Jalankan pemeriksaan per query terhadap Fineract dan gabungkan temuannya.
pub async fn probe(catalog: &Catalog, fineract: &FineractDb, report: &mut Report) {
    for loaded in &catalog.queries {
        let query = &loaded.entry;
        let subject = format!("{} ({})", query.id, loaded.path);

        // Manifest non-Fineract (bila kelak ada) tidak diperiksa di sini.
        if query.database.as_deref().unwrap_or("fineract") != "fineract" {
            continue;
        }

        let Some(sql) = query
            .sql_file
            .as_deref()
            .and_then(|path| catalog.sql_files.get(path))
        else {
            continue; // sudah dilaporkan validasi statis
        };

        // SQL berasal dari file yang disetujui di `queries/`, bukan dari input
        // pengguna maupun model — itulah yang membuat AssertSqlSafe benar di
        // sini dan tidak di tempat lain.
        let statement = AssertSqlSafe(sql.trim().trim_end_matches(';').to_string()).into_sql_str();

        let prepared = match fineract.pool().describe(statement).await {
            Ok(prepared) => prepared,
            Err(error) => {
                report.findings.push(Finding {
                    severity: crate::catalog::validate::Severity::Error,
                    subject,
                    check: "sql_prepares_against_fineract".to_string(),
                    message: sanitize(&error.to_string()),
                });
                continue;
            }
        };

        let actual: Vec<String> = prepared
            .columns()
            .iter()
            .map(|column| column.name().to_string())
            .collect();

        let declared: Vec<String> = query
            .output_fields
            .iter()
            .map(|field| field.name.clone())
            .collect();

        if declared.is_empty() {
            continue; // sudah dilaporkan validasi statis
        }

        if actual != declared {
            report.findings.push(Finding {
                severity: crate::catalog::validate::Severity::Error,
                subject,
                check: "output_columns_match_contract".to_string(),
                message: format!(
                    "kolom hasil {actual:?} tidak sama dengan output_fields {declared:?} (nama dan urutan wajib sama)"
                ),
            });
        }
    }
}

/// Pesan error database dipakai oleh operator, bukan dikirim ke klien — tetapi
/// ia tetap dirapikan menjadi satu baris supaya laporan tetap terbaca.
fn sanitize(message: &str) -> String {
    message.replace('\n', " ").trim().to_string()
}
