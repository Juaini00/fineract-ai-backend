//! Pembacaan `system_settings` (#15).
//!
//! Tabel ini append-only: nilai berlaku adalah `version` tertinggi per key.
//! **Fail closed** — bila baris tidak ada atau tidak dapat dibaca sebagai
//! bentuk yang diharapkan, PII dianggap mati.

use serde_json::Value;
use sqlx::{Postgres, Transaction};

/// Nilai PII yang berlaku saat job diterima, lengkap dengan versinya supaya
/// laporan lama tidak berubah makna ketika konfigurasi diubah.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PiiSetting {
    pub enabled: bool,
    pub mode: String,
    pub setting_version: i32,
}

impl Default for PiiSetting {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: "withhold".to_string(),
            setting_version: 0,
        }
    }
}

/// Baca nilai PII berlaku di dalam transaksi yang sedang berjalan, supaya
/// snapshot scope job benar-benar sezaman dengan penerimaannya.
pub async fn current_pii(tx: &mut Transaction<'_, Postgres>) -> sqlx::Result<PiiSetting> {
    let rows = sqlx::query_as::<_, (String, Value, i32)>(
        "SELECT DISTINCT ON (key) key, value_json, version
         FROM system_settings
         WHERE key IN ('pii.enabled', 'pii.mode') AND effective_from <= now()
         ORDER BY key, version DESC",
    )
    .fetch_all(&mut **tx)
    .await?;

    let mut setting = PiiSetting::default();

    for (key, value, version) in rows {
        match key.as_str() {
            // `as_bool()` yang gagal berarti nilai bukan boolean — tetap mati.
            "pii.enabled" => {
                setting.enabled = value.as_bool().unwrap_or(false);
                setting.setting_version = version;
            }
            "pii.mode" => {
                if let Some(mode) = value.as_str() {
                    setting.mode = mode.to_string();
                }
            }
            _ => {}
        }
    }

    Ok(setting)
}
