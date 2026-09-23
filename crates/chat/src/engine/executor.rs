//! Eksekusi satu operasi yang diadmit Engine: SQL yang sudah disetujui,
//! read-only, dengan parameter terikat.
//!
//! Tiga hal yang tidak boleh dilonggarkan di sini:
//!
//! 1. SQL berasal dari berkas di `queries/` — tidak pernah dari model.
//! 2. Seluruh nilai masuk sebagai **parameter terikat**, tidak pernah
//!    diinterpolasi ke teks SQL.
//! 3. Eksekusi berjalan **di luar** transaksi commit aplikasi (I1) dan dibatasi
//!    dua sisi: `statement_timeout` di server dan timeout Tokio di klien. Yang
//!    pertama menghentikan query, yang kedua menghentikan penantian bila server
//!    tidak menjawab sama sekali.

use std::time::Duration;

use foundation::db::FineractDb;
use serde_json::{Map, Value};
use sqlx::{AssertSqlSafe, Column, Row, SqlSafeStr, TypeInfo, postgres::PgRow};

use crate::engine::planner::{Bound, Plan};

#[derive(Debug)]
pub struct Executed {
    pub rows: Vec<Map<String, Value>>,
    pub duration_ms: i64,
}

#[derive(Debug)]
pub enum ExecutionError {
    /// Query melewati batas waktunya. Hasilnya tidak diketahui — bukan nol.
    TimedOut { timeout_ms: u64 },
    Database(String),
}

impl ExecutionError {
    pub fn failure_code(&self) -> &'static str {
        match self {
            Self::TimedOut { .. } => "source_query_timeout",
            Self::Database(_) => "source_query_failed",
        }
    }
}

/// Jalankan query capability terhadap Fineract.
pub async fn execute(fineract: &FineractDb, plan: &Plan) -> Result<Executed, ExecutionError> {
    run(fineract, &plan.sql, &plan.parameters, plan.timeout_ms).await
}

/// Jalankan satu SQL yang sudah disetujui katalog dengan parameter terikat.
///
/// Dipakai capability (lewat [`execute`]) maupun resolver opsi: keduanya wajib
/// melewati batas waktu dan pengikatan parameter yang sama — dua jalur eksekusi
/// yang berbeda adalah dua tempat sebuah guard dapat menghilang.
pub async fn run(
    fineract: &FineractDb,
    sql: &str,
    parameters: &[Bound],
    timeout_ms: u64,
) -> Result<Executed, ExecutionError> {
    let started = std::time::Instant::now();
    let timeout = Duration::from_millis(timeout_ms);

    let run = async {
        let mut connection = fineract
            .pool()
            .acquire()
            .await
            .map_err(|error| ExecutionError::Database(error.to_string()))?;

        // Batas sisi server. `SET LOCAL` akan ikut hilang bersama transaksi;
        // di sini dipakai pada koneksi yang langsung dilepas setelahnya.
        // Angkanya berasal dari manifest katalog dan sudah dipastikan berupa
        // integer oleh tipe `u64` — bukan dari input pengguna maupun model.
        sqlx::query(AssertSqlSafe(format!(
            "SET statement_timeout = {}",
            timeout_ms.max(1)
        )))
        .execute(&mut *connection)
        .await
        .map_err(|error| ExecutionError::Database(error.to_string()))?;

        let statement = AssertSqlSafe(sql.trim().trim_end_matches(';').to_string()).into_sql_str();
        let mut query = sqlx::query(statement);

        for parameter in parameters {
            query = match parameter {
                Bound::Date(date) => query.bind(*date),
                Bound::OfficeIds(ids) => query.bind(ids.clone()),
                Bound::Bigint(value) => query.bind(*value),
                // Satu baris melebihi cap: satu-satunya cara mengetahui bahwa
                // hasilnya terpotong tanpa query hitung terpisah (FIN-133).
                Bound::RowCap(cap) => query.bind(cap.saturating_add(1)),
                Bound::Text(value) => query.bind(value.clone()),
                Bound::NullText => query.bind(Option::<String>::None),
                Bound::NullBigintArray => query.bind(Option::<Vec<i64>>::None),
                Bound::NullBigint => query.bind(Option::<i64>::None),
            };
        }

        query
            .fetch_all(&mut *connection)
            .await
            .map_err(|error| ExecutionError::Database(error.to_string()))
    };

    let rows = match tokio::time::timeout(timeout, run).await {
        Ok(result) => result?,
        Err(_) => return Err(ExecutionError::TimedOut { timeout_ms }),
    };

    Ok(Executed {
        rows: rows.iter().map(row_to_json).collect(),
        duration_ms: started.elapsed().as_millis() as i64,
    })
}

/// Petakan baris ke JSON.
///
/// `NUMERIC` menjadi **string**, bukan float: pembulatan biner pada angka uang
/// adalah cara klasik total berubah satu sen tanpa ada yang menyadarinya.
fn row_to_json(row: &PgRow) -> Map<String, Value> {
    let mut object = Map::new();

    for column in row.columns() {
        let name = column.name().to_string();
        let value = match column.type_info().name() {
            "INT2" | "INT4" => row
                .try_get::<Option<i32>, _>(column.ordinal())
                .ok()
                .flatten()
                .map(|value| Value::from(value as i64)),
            "INT8" => row
                .try_get::<Option<i64>, _>(column.ordinal())
                .ok()
                .flatten()
                .map(Value::from),
            "FLOAT4" | "FLOAT8" => row
                .try_get::<Option<f64>, _>(column.ordinal())
                .ok()
                .flatten()
                .and_then(serde_json::Number::from_f64)
                .map(Value::Number),
            "NUMERIC" => row
                .try_get::<Option<rust_decimal::Decimal>, _>(column.ordinal())
                .ok()
                .flatten()
                .map(|value| Value::String(value.normalize().to_string())),
            "BOOL" => row
                .try_get::<Option<bool>, _>(column.ordinal())
                .ok()
                .flatten()
                .map(Value::from),
            "DATE" => row
                .try_get::<Option<chrono::NaiveDate>, _>(column.ordinal())
                .ok()
                .flatten()
                .map(|value| Value::String(value.to_string())),
            "TIMESTAMP" | "TIMESTAMPTZ" => row
                .try_get::<Option<chrono::DateTime<chrono::Utc>>, _>(column.ordinal())
                .ok()
                .flatten()
                .map(|value| Value::String(value.to_rfc3339())),
            _ => row
                .try_get::<Option<String>, _>(column.ordinal())
                .ok()
                .flatten()
                .map(Value::String),
        };

        object.insert(name, value.unwrap_or(Value::Null));
    }

    object
}

/// Daftar office yang boleh diakses pemanggil.
///
/// Hari ini setiap pengguna adalah admin, jadi proyeksinya adalah seluruh
/// office pada tenant (`entitlements_source='admin_projection'`, migrasi 1).
/// Penyempitan dari request hanya boleh MEMPERSEMPIT, tidak pernah memperlebar
/// (I7) — karena itu ia dipotong terhadap daftar ini, bukan dipakai apa adanya.
pub async fn authorized_office_ids(
    fineract: &FineractDb,
    requested: &[i64],
) -> Result<Vec<i64>, ExecutionError> {
    let all: Vec<i64> = sqlx::query_scalar("SELECT id FROM m_office ORDER BY id")
        .fetch_all(fineract.pool())
        .await
        .map_err(|error| ExecutionError::Database(error.to_string()))?;

    if requested.is_empty() {
        return Ok(all);
    }

    Ok(all
        .into_iter()
        .filter(|office| requested.contains(office))
        .collect())
}
