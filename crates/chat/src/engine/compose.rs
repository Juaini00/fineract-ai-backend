//! Komposisi response deterministik dari hasil query.
//!
//! Tidak ada model yang menentukan angka. Blok dibentuk dari baris hasil dan
//! `output_fields` manifest; narasi baru boleh ditambahkan kelak sebagai
//! lapisan additive yang kegagalannya **tidak** menghapus structured output
//! (responses.md).
//!
//! Bentuk blok mengikuti responses.md §1 dan §2 **tanpa tafsir**: `block_id`
//! (bukan `id`), `schema_version` per blok, `derived_from` pada blok penyaji
//! data, dan kosakata yang tertutup pada sembilan tipe. Lineage tidak menjadi
//! blok — ia hidup di `evidence_json` (#10).

use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::engine::{
    dataset::Retained,
    planner::{Bound, Plan},
    repository::SettledResponse,
};

/// Versi skema **per blok** (§1). Ia terpisah dari versi dokumen supaya
/// menambah tipe blok baru tidak memaksa menaikkan versi seluruh dokumen.
pub const BLOCK_SCHEMA_VERSION: i64 = 1;

/// Kosakata blok responses.md §2 — **tertutup**. Tipe di luar daftar ini
/// ditolak validator, bukan diteruskan: klien melewati tipe yang tidak dikenal,
/// jadi tipe yang dikarang server berarti informasi yang tidak pernah sampai.
pub const BLOCK_TYPES: [&str; 9] = [
    "narrative",
    "metric",
    "table",
    "chart_spec",
    "comparison",
    "finding",
    "limitation",
    "suggestion",
    "note",
];

/// Blok penyaji data: `derived_from` **wajib** (§2 kolom ketiga). `narrative`
/// tidak di sini — ia hanya wajib berderivasi bila memuat angka, dan itu
/// diperiksa per isi, bukan per tipe.
pub const DATA_BLOCKS: [&str; 5] = ["metric", "table", "chart_spec", "comparison", "finding"];

/// Slot yang diikat tanpa bertanya: resolver dengan satu kandidat
/// (`resolver_unique`, K5) atau teks permintaan yang terurai deterministik
/// (`resolver_unique` | `deterministic_parse`, FIN-135, database-design.md
/// §4.14). Keduanya diungkap lewat mekanisme yang sama (D2) — hanya
/// alasannya yang berbeda.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoBound {
    pub field_id: String,
    pub label: Option<String>,
    pub provenance: &'static str,
}

impl AutoBound {
    fn describe(&self) -> String {
        match &self.label {
            Some(label) => format!("{} = {label}", self.field_id),
            None => self.field_id.clone(),
        }
    }

    pub fn from_deterministic(bind: &crate::engine::planner::DeterministicBind) -> Self {
        Self {
            field_id: bind.parameter.clone(),
            label: Some(bind.detail.clone()),
            provenance: "deterministic_parse",
        }
    }
}

/// `completeness_reason` dan `block_id` saat node mengembalikan lebih banyak
/// baris daripada row cap capability (FIN-133).
pub const ROW_CAP_REACHED: &str = "row_cap_reached";

/// Hasil node dipotong ke row cap: masih ada baris yang tidak ditampilkan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowCapReached {
    pub row_cap: usize,
}

/// Potong baris ke row cap plan (FIN-133).
///
/// Executor mengikat `cap + 1`, jadi lebih dari `cap` baris berarti populasi
/// yang cocok memang lebih besar dari yang boleh dibaca. `Some` HANYA saat
/// itu terjadi: hasil yang muat di bawah cap adalah jawaban utuh, dan
/// menyatakannya terpotong berarti membuat klaim yang tidak benar.
pub fn cap_rows(plan: &Plan, rows: &mut Vec<Map<String, Value>>) -> Option<RowCapReached> {
    let row_cap = usize::try_from(plan.row_cap()?).ok()?;
    if rows.len() <= row_cap {
        return None;
    }

    rows.truncate(row_cap);
    Some(RowCapReached { row_cap })
}

/// Bungkus §1 untuk satu blok.
///
/// Satu tempat, bukan sembilan: `block_id` yang kadang bernama `id` dan
/// `schema_version` yang kadang lupa ditulis adalah persis penyimpangan yang
/// ditemukan audit 2026-09-15.
pub fn block(block_id: &str, block_type: &str, derived_from: &[Value], payload: Value) -> Value {
    let mut object = match payload {
        Value::Object(map) => map,
        _ => Map::new(),
    };

    object.insert("block_id".into(), json!(block_id));
    object.insert("type".into(), json!(block_type));
    object.insert("schema_version".into(), json!(BLOCK_SCHEMA_VERSION));
    if !derived_from.is_empty() {
        object.insert("derived_from".into(), Value::Array(derived_from.to_vec()));
    }

    Value::Object(object)
}

/// Rujukan `derived_from` ke satu baris ledger (§1: daftar `node_run_id`
/// dan/atau `dataset_id`).
pub fn from_node(node_run_id: Uuid) -> Value {
    json!({ "node_run_id": node_run_id })
}

/// Rujukan `derived_from` ke satu dataset handle (§1). Ruang kunci yang sama
/// dengan `from_node`: validator menghitung `completeness` keduanya lewat
/// `contributor_key`.
pub fn from_dataset(dataset_id: Uuid) -> Value {
    json!({ "dataset_id": dataset_id })
}

/// §7 — sebuah chart mendeklarasikan bentuk data yang dibutuhkannya (time series
/// memerlukan dimensi waktu **terurut**). Bila data rujukan tidak memenuhi
/// bentuk itu, chart **tidak** dirender: ia turun menjadi `table` + `note`. Bukan
/// kegagalan — angkanya tetap utuh — dan bukan chart yang menyesatkan.
///
/// ponytail: hanya bentuk `time_series` yang dikenal hari ini. Satu bentuk cukup
/// menegakkan aturan §7; kategori chart lain ditambahkan saat benar-benar
/// diminta (YAGNI). Kompatibel → satu blok `chart_spec`; tidak → tabel + catatan.
pub fn chart_or_table(
    block_id: &str,
    time_dimension: &str,
    columns: &[String],
    rows: &[Map<String, Value>],
    derived_from: &[Value],
) -> Vec<Value> {
    if let Some(reason) = time_series_incompatibility(time_dimension, columns, rows) {
        return vec![
            table_block(columns, &[], rows, derived_from),
            block(
                &format!("{block_id}:downgraded"),
                "note",
                &[],
                json!({
                    "title": "Chart downgraded to a table",
                    "body": format!(
                        "The requested time-series chart was not rendered because {reason}. \
                         The same numbers are shown as a table instead."
                    ),
                    "downgraded_from": "chart_spec",
                    "reason": reason,
                }),
            ),
        ];
    }

    vec![block(
        block_id,
        "chart_spec",
        derived_from,
        json!({ "chart_type": "time_series", "time_dimension": time_dimension }),
    )]
}

/// `None` = kompatibel. `Some(reason)` menyebut kenapa chart tidak boleh
/// dirender: kolom waktu hilang, atau nilainya tidak terurut naik.
fn time_series_incompatibility(
    time_dimension: &str,
    columns: &[String],
    rows: &[Map<String, Value>],
) -> Option<&'static str> {
    if !columns.iter().any(|column| column == time_dimension) {
        return Some("the required time dimension column is absent");
    }

    // ponytail: perbandingan string cukup untuk periode/tanggal ISO yang
    // terurut leksikografis ("2026-01" < "2026-02"). Kalau kelak ada sumbu
    // waktu numerik non-ISO, bandingkan sebagai angka di sini.
    let ordered = rows
        .windows(2)
        .all(|pair| cell(&pair[0], time_dimension) <= cell(&pair[1], time_dimension));

    (!ordered).then_some("the time dimension is not ordered")
}

fn cell(row: &Map<String, Value>, key: &str) -> String {
    row.get(key).map(Value::to_string).unwrap_or_default()
}

/// §7 / dataset-lifecycle §7 — blok `table` yang dataset-nya `expired`/`purged`
/// menyatakan "detail data sudah kedaluwarsa". Ia **tidak** tampil sebagai nol
/// baris (I5) dan bukan kegagalan: handle bertahan setelah purge (hanya chunk
/// yang hilang), jadi angka ringkas yang sudah dihitung tetap valid pada titik
/// `as_of`. Pertanyaan lanjutan menjadi retrieval baru — tidak ada auto-reuse
/// handle mati (K13). Angka ringkas itu sendiri hidup sebagai blok `metric` yang
/// sudah inline pada response; fungsi ini hanya membentuk blok tabelnya.
pub fn expired_dataset_table(
    block_id: &str,
    dataset_id: Uuid,
    handle_state: &str,
    as_of: &Value,
) -> Value {
    block(
        block_id,
        "table",
        &[from_dataset(dataset_id)],
        json!({
            // `null`, bukan `[]`: `[]` tampak seperti "datanya nol". Detail
            // dinyatakan hilang, tidak didiamkan (I5).
            "rows": Value::Null,
            "columns": Value::Null,
            "detail_available": false,
            "handle_state": handle_state,
            "as_of": as_of,
            "body": "Row-level detail for this dataset is no longer available; \
                     the summary figures remain valid as of the stated date.",
        }),
    )
}

/// Susun response dari baris hasil.
///
/// Satu baris → satu blok `metric` **per kolom** (§2: satu nilai bernama).
/// Lebih dari satu baris → blok `table`. Nol baris → outcome `Empty` dengan
/// `completeness` `Complete`: pencarian yang berhasil dan memang tidak
/// menemukan apa pun berbeda dari pencarian yang tidak selesai (engine.md
/// melarang `Empty` + `Partial`).
///
/// `row_cap` (FIN-133): baris sudah dipotong ke cap oleh [`cap_rows`]; jawaban
/// menjadi `Partial` dengan alasan `row_cap_reached` dan blok `limitation`
/// menyatakan bahwa masih ada baris yang tidak ditampilkan.
#[allow(clippy::too_many_arguments)]
pub fn analysis(
    plan: &Plan,
    rows: &[Map<String, Value>],
    duration_ms: i64,
    pii_enabled: bool,
    auto_bound: &[AutoBound],
    node_run_id: Uuid,
    retained: &Retained,
    row_cap: Option<RowCapReached>,
) -> SettledResponse {
    let (visible, withheld) = visible_fields(plan, pii_enabled);
    let derived_from = [from_node(node_run_id)];
    let period = period(plan);

    let (outcome, mut blocks) = match rows.len() {
        0 => (
            "Empty",
            vec![block(
                "empty",
                "narrative",
                &derived_from,
                json!({
                    "body": "The approved query ran successfully and returned no rows for the requested scope and period.",
                }),
            )],
        ),
        1 => (
            "Answered",
            metric_blocks(&visible, &rows[0], &period, &derived_from),
        ),
        _ => (
            "Answered",
            vec![table_block(&visible, &withheld, rows, &derived_from)],
        ),
    };

    // I5 — tidak ada penghilangan senyap: kolom yang ditahan dinyatakan, bukan
    // sekadar hilang dari tabel (§7, PII #15).
    if !withheld.is_empty() {
        blocks.push(block(
            "pii_withheld",
            "limitation",
            &[],
            json!({
                "title": "Columns withheld",
                "body": format!(
                    "PII is disabled for this deployment, so {} column(s) were withheld from the result: {}.",
                    withheld.len(),
                    withheld.join(", ")
                ),
                "withheld_columns": withheld,
            }),
        ));
    }

    // DS-8.1 — `truncated=true` hidup bersama `Complete` HANYA bila batasnya
    // dinyatakan dan tidak mengubah jawaban. Jawaban di atas dihitung atas
    // seluruh baris node; yang dibatasi hanya handle yang dirujuk lineage,
    // dan itu dinyatakan di sini, bukan dibiarkan ditemukan saat paginasi.
    if let Some(truncation) = &retained.truncation {
        blocks.push(block(
            "dataset_truncated",
            "limitation",
            &[],
            json!({
                "title": "Retained dataset is capped",
                "body": format!(
                    "The answer covers all {} row(s) returned by the approved query. The dataset \
                     handle retained for paging keeps only the first {} row(s) ({}).",
                    rows.len(),
                    truncation.row_count_available,
                    truncation.reason
                ),
                "dataset_id": retained.dataset_id,
                "reason": truncation.reason,
                "row_count_available": truncation.row_count_available,
                "row_count_total": rows.len(),
            }),
        ));
    }

    // FIN-133 / I5 — row cap tercapai: jawaban hanya memuat sebagian populasi
    // yang cocok, dan itu dinyatakan, bukan dibiarkan terbaca sebagai utuh.
    if let Some(reached) = row_cap {
        blocks.push(block(
            ROW_CAP_REACHED,
            "limitation",
            &[],
            json!({
                "title": "Result capped",
                "body": format!(
                    "The approved capability caps this result at {} row(s). More rows matched \
                     the request; only the first {} are shown.",
                    reached.row_cap, reached.row_cap
                ),
                "row_cap": reached.row_cap,
                "rows_shown": reached.row_cap,
                "more_rows_exist": true,
            }),
        ));
    }

    // FIN-135 / I5 — teks menyebut sesuatu yang tidak dapat diikat (mis. nama
    // entitas tanpa resolver, K1): default tetap dipakai, tetapi itu wajib
    // dinyatakan, bukan hilang diam-diam. Bukan D2 (tidak ada yang diikat,
    // jadi bukan `note`/`auto_bound_slots`) — ini `limitation`, kelas yang
    // sama dengan kolom yang ditahan dan baris yang dipotong di atas.
    if !plan.unapplied_params.is_empty() {
        blocks.push(block(
            "params_not_applied",
            "limitation",
            &[],
            json!({
                "title": "Values not applied",
                "body": format!(
                    "{} value(s) mentioned in your request were not applied; the declared \
                     default is used instead: {}.",
                    plan.unapplied_params.len(),
                    plan.unapplied_params
                        .iter()
                        .map(|item| format!("{} — {}", item.parameter, item.detail))
                        .collect::<Vec<_>>()
                        .join("; ")
                ),
                "not_applied_params": plan.unapplied_params
                    .iter()
                    .map(|item| json!({ "parameter": item.parameter, "detail": item.detail }))
                    .collect::<Vec<_>>(),
            }),
        ));
    }

    // D2 (§5) — pengungkapan auto-bind hidup di blok `note`, bukan
    // `limitation`: slot yang diikat tanpa bertanya (resolver satu kandidat,
    // atau teks permintaan terurai deterministik, FIN-135) adalah **asumsi
    // yang diambil**, bukan batas jawaban. Validator memeriksa blok inilah
    // yang memuatnya.
    if !auto_bound.is_empty() {
        let resolver_bound: Vec<&AutoBound> = auto_bound
            .iter()
            .filter(|slot| slot.provenance == "resolver_unique")
            .collect();
        let parsed_bound: Vec<&AutoBound> = auto_bound
            .iter()
            .filter(|slot| slot.provenance == "deterministic_parse")
            .collect();

        let mut sentences = Vec::new();
        if !resolver_bound.is_empty() {
            sentences.push(format!(
                "{} value(s) were bound automatically because the approved resolver returned \
                 exactly one candidate inside your authorized scope: {}.",
                resolver_bound.len(),
                resolver_bound
                    .iter()
                    .map(|slot| slot.describe())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        if !parsed_bound.is_empty() {
            sentences.push(format!(
                "{} value(s) were parsed directly from your request text: {}.",
                parsed_bound.len(),
                parsed_bound
                    .iter()
                    .map(|slot| slot.describe())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }

        blocks.push(block(
            "slots_auto_bound",
            "note",
            &[],
            json!({
                "title": "Values chosen without asking",
                "body": sentences.join(" "),
                "auto_bound_slots": auto_bound
                    .iter()
                    .map(|slot| json!({
                        "field_id": slot.field_id,
                        "label": slot.label,
                        "provenance": slot.provenance,
                    }))
                    .collect::<Vec<_>>(),
            }),
        ));
    }

    let blocks = Value::Array(blocks);

    // Query capability berjalan utuh dalam satu eksekusi: tidak ada bagian
    // yang dilewati, jadi klaimnya `Complete` — kecuali row cap memotongnya.
    // Validator tetap menghitungnya ulang dari ledger lewat `derived_from`:
    // klaim ini tidak pernah menjadi kebenaran hanya karena ditulis di sini.
    let (completeness, completeness_reason) = match row_cap {
        Some(_) => ("Partial", ROW_CAP_REACHED.to_string()),
        None => ("Complete", format!("curated_query:{}", plan.query_id)),
    };

    SettledResponse {
        kind: "analysis",
        outcome,
        completeness,
        completeness_reason,
        response_hash: hex::encode(Sha256::digest(blocks.to_string().as_bytes())),
        evidence: evidence(
            plan,
            node_run_id,
            retained.dataset_id,
            rows.len(),
            duration_ms,
        ),
        blocks,
    }
}

/// Pisahkan kolom yang boleh tampil dari yang ditahan.
///
/// Sakelar PII bersifat global (#15) dan fail closed: saat mati, kolom berkelas
/// `pii` tidak pernah keluar — termasuk ke penyimpanan output node, bukan hanya
/// ke response.
pub fn visible_fields(plan: &Plan, pii_enabled: bool) -> (Vec<String>, Vec<String>) {
    let mut visible = Vec::new();
    let mut withheld = Vec::new();

    for (field, sensitivity) in plan.output_fields.iter().zip(&plan.output_sensitivity) {
        if !pii_enabled && sensitivity == "pii" {
            withheld.push(field.clone());
        } else {
            visible.push(field.clone());
        }
    }

    (visible, withheld)
}

/// Buang kolom yang ditahan dari baris hasil.
pub fn redact(rows: &[Map<String, Value>], withheld: &[String]) -> Vec<Map<String, Value>> {
    if withheld.is_empty() {
        return rows.to_vec();
    }

    rows.iter()
        .map(|row| {
            row.iter()
                .filter(|(key, _)| !withheld.contains(key))
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect()
        })
        .collect()
}

/// Satu nilai bernama = satu blok `metric` (§2).
///
/// `unit` belum diketahui katalog dan karena itu ditulis `null`, bukan
/// dihilangkan: field yang hilang tidak dapat dibedakan dari field yang tidak
/// diketahui, dan I4 menuntut ketidaktahuan terlihat.
fn metric_blocks(
    output_fields: &[String],
    row: &Map<String, Value>,
    period: &Value,
    derived_from: &[Value],
) -> Vec<Value> {
    output_fields
        .iter()
        .map(|field| {
            block(
                &format!("metric:{field}"),
                "metric",
                derived_from,
                json!({
                    "key": field,
                    "value": row.get(field).cloned().unwrap_or(Value::Null),
                    "unit": Value::Null,
                    "period": period,
                }),
            )
        })
        .collect()
}

fn table_block(
    output_fields: &[String],
    withheld: &[String],
    rows: &[Map<String, Value>],
    derived_from: &[Value],
) -> Value {
    let values: Vec<Value> = rows
        .iter()
        .map(|row| {
            Value::Array(
                output_fields
                    .iter()
                    .map(|field| row.get(field).cloned().unwrap_or(Value::Null))
                    .collect(),
            )
        })
        .collect();

    block(
        "result",
        "table",
        derived_from,
        json!({
            "columns": output_fields,
            "rows": values,
            "row_count": rows.len(),
            // §7 — blok tabel MENDEKLARASIKAN kolom yang ditahan; blok
            // limitation menyatakannya. Keduanya, bukan salah satu.
            "withheld_columns": withheld,
        }),
    )
}

/// Parameter yang benar-benar terikat, dalam satu bentuk.
///
/// Dipakai dua tempat — `evidence_json` dan `job_node_runs.input_binding_json`
/// — dan sengaja satu fungsi: dua penyusunan yang sama-sama benar hari ini akan
/// menyimpang, dan yang menyimpang di sini adalah "dengan parameter apa angka
/// ini dihasilkan".
///
/// `office_ids` menjadi **jumlah**, bukan daftar: scope adalah metadata, dan
/// menuliskan seluruh daftarnya tidak menambah kemampuan menelusuri apa pun.
///
/// Row cap (FIN-133) ditulis sebagai **cap**, bukan `cap + 1` yang diikat
/// executor: baris sentinel itu mekanisme deteksi, bukan parameter yang
/// dipilih siapa pun.
pub fn bindings(plan: &Plan) -> Value {
    let office_count = plan
        .parameters
        .iter()
        .find_map(|parameter| match parameter {
            Bound::OfficeIds(ids) => Some(ids.len()),
            _ => None,
        })
        .unwrap_or(0);

    plan.parameter_names
        .iter()
        .zip(&plan.parameters)
        .map(|(name, value)| {
            json!({
                "name": name,
                "value": match value {
                    Bound::Date(date) => Value::String(date.to_string()),
                    Bound::Bigint(value) | Bound::RowCap(value) => Value::from(*value),
                    Bound::Text(value) => Value::String(value.clone()),
                    Bound::OfficeIds(_) => Value::String(format!("{office_count} authorized offices")),
                    Bound::NullText | Bound::NullBigintArray | Bound::NullBigint => Value::Null,
                },
            })
        })
        .collect()
}

/// Periode yang terikat plan, untuk blok `metric` (§2: nilai + unit + periode).
fn period(plan: &Plan) -> Value {
    let dates: Map<String, Value> = plan
        .parameter_names
        .iter()
        .zip(&plan.parameters)
        .filter_map(|(name, bound)| match bound {
            Bound::Date(date) => Some((name.clone(), Value::String(date.to_string()))),
            _ => None,
        })
        .collect();

    if dates.is_empty() {
        Value::Null
    } else {
        Value::Object(dates)
    }
}

/// `as_of` eksplisit bila katalog mengikat parameter bernama demikian.
fn as_of(plan: &Plan) -> Value {
    plan.parameter_names
        .iter()
        .zip(&plan.parameters)
        .find_map(|(name, bound)| match bound {
            Bound::Date(date) if name.contains("as_of") => Some(Value::String(date.to_string())),
            _ => None,
        })
        .unwrap_or(Value::Null)
}

/// Lineage (#10, responses.md §4): rantai finding → metric → operasi → dataset
/// → sumber.
///
/// Ia **bukan blok**. Blok yang tidak dikenal klien dilewati tanpa merusak
/// render (§1), jadi menaruh jejak asal angka di dalam blok berarti jejak itu
/// hilang pada klien pertama yang tidak mengenalnya. `evidence_json` adalah
/// kolomnya sendiri dan selalu ikut dokumen.
///
/// `dataset_id` adalah handle yang meretensi hasil node ini — satu-satunya
/// jalan klien mengetahui id untuk `GET /chat/datasets/{id}` (FIN-43, §6.6).
pub fn evidence(
    plan: &Plan,
    node_run_id: Uuid,
    dataset_id: Uuid,
    row_count: usize,
    duration_ms: i64,
) -> Value {
    json!({
        "lineage": [
            {
                "node_run_id": node_run_id,
                "dataset_id": dataset_id,
                "capability_id": plan.capability_id,
                "query_id": plan.query_id,
                "sql_file": plan.sql_file,
                "catalog_version_id": plan.catalog_version_id,
                "catalog_content_hash": plan.catalog_content_hash,
                "as_of": as_of(plan),
                // #14 — belum ada konsolidasi mata uang.
                "exchange_rate_id": Value::Null,
                "parameters": bindings(plan),
                "row_count": row_count,
                "duration_ms": duration_ms,
            }
        ],
        // §4 — angka turunan yang sah untuk prosa. Kosong selama belum ada
        // narasi LLM; begitu ada, hanya lewat sini ia menjadi sah.
        "derivations": [],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan() -> Plan {
        Plan {
            capability_id: "savings_deposit_total".into(),
            query_id: "savings.deposit_total".into(),
            sql: "SELECT 1".into(),
            sql_file: "queries/savings/deposit_total.sql".into(),
            parameters: vec![
                Bound::Date(chrono::NaiveDate::from_ymd_opt(2026, 9, 1).unwrap()),
                Bound::OfficeIds(vec![1, 2, 3]),
            ],
            parameter_names: vec!["from_date".into(), "office_ids".into()],
            output_fields: vec!["total_deposit_amount".into(), "deposit_count".into()],
            output_sensitivity: vec!["public_business".into(), "public_business".into()],
            timeout_ms: 3000,
            catalog_version_id: Uuid::nil(),
            catalog_content_hash: "hash".into(),
            retrieval_score: 0.5,
            graph_json: json!({}),
            graph_hash: "graph".into(),
            deterministic_binds: Vec::new(),
            unapplied_params: Vec::new(),
        }
    }

    fn row(total: &str, count: i64) -> Map<String, Value> {
        let mut row = Map::new();
        row.insert("total_deposit_amount".into(), Value::String(total.into()));
        row.insert("deposit_count".into(), Value::from(count));
        row
    }

    fn node() -> Uuid {
        Uuid::from_u128(7)
    }

    fn dataset() -> Retained {
        Retained {
            dataset_id: Uuid::from_u128(11),
            truncation: None,
        }
    }

    /// Setiap blok memenuhi §1 dan §2 sekaligus.
    fn assert_shape(blocks: &Value) {
        for block in blocks.as_array().unwrap() {
            let block_type = block["type"].as_str().unwrap();
            assert!(
                BLOCK_TYPES.contains(&block_type),
                "tipe di luar kosakata §2: {block_type}"
            );
            assert!(block["block_id"].is_string(), "block_id hilang: {block}");
            assert!(
                block.get("id").is_none(),
                "field `id` lama masih ada: {block}"
            );
            assert_eq!(block["schema_version"], BLOCK_SCHEMA_VERSION);
            if DATA_BLOCKS.contains(&block_type) {
                assert!(
                    block["derived_from"]
                        .as_array()
                        .is_some_and(|refs| !refs.is_empty()),
                    "blok data tanpa derived_from: {block}"
                );
            }
        }
    }

    #[test]
    fn single_row_becomes_one_metric_block_per_named_value() {
        let response = analysis(
            &plan(),
            &[row("1500.00", 3)],
            12,
            false,
            &[],
            node(),
            &dataset(),
            None,
        );
        let blocks = response.blocks.as_array().unwrap();

        assert_eq!(response.outcome, "Answered");
        assert_eq!(response.completeness, "Complete");
        assert_shape(&response.blocks);
        // §2: `metric` tunggal, satu nilai bernama per blok — bukan `metrics`.
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0]["type"], "metric");
        assert_eq!(blocks[0]["block_id"], "metric:total_deposit_amount");
        assert_eq!(blocks[0]["value"], "1500.00");
        assert_eq!(blocks[0]["period"]["from_date"], "2026-09-01");
        assert_eq!(
            blocks[0]["derived_from"][0]["node_run_id"],
            node().to_string()
        );
    }

    #[test]
    fn many_rows_become_a_table() {
        let rows = [row("1.00", 1), row("2.00", 2)];
        let response = analysis(&plan(), &rows, 30, false, &[], node(), &dataset(), None);
        let blocks = response.blocks.as_array().unwrap();

        assert_shape(&response.blocks);
        assert_eq!(blocks[0]["type"], "table");
        assert_eq!(blocks[0]["row_count"], 2);
        assert_eq!(blocks[0]["columns"][0], "total_deposit_amount");
    }

    #[test]
    fn no_rows_is_empty_not_a_failure() {
        let response = analysis(&plan(), &[], 5, false, &[], node(), &dataset(), None);

        assert_eq!(response.outcome, "Empty");
        // engine.md melarang Empty + Partial: pencarian parsial tidak boleh
        // menyatakan populasi kosong.
        assert_eq!(response.completeness, "Complete");
        assert_shape(&response.blocks);
    }

    /// RESP-8.8 — PII dimatikan: kolom identitas tidak muncul dan penahanannya
    /// dinyatakan.
    #[test]
    fn resp_8_8_pii_columns_are_withheld_and_declared_when_the_switch_is_off() {
        let mut plan = plan();
        plan.output_fields.push("client_display_name".into());
        plan.output_sensitivity.push("pii".into());

        let mut row = row("1.00", 1);
        row.insert("client_display_name".into(), Value::String("Budi".into()));

        let response = analysis(
            &plan,
            &[row.clone(), row],
            5,
            false,
            &[],
            node(),
            &dataset(),
            None,
        );
        let rendered = response.blocks.to_string();

        assert!(
            !rendered.contains("Budi"),
            "PII bocor ke response: {rendered}"
        );
        let blocks = response.blocks.as_array().unwrap();
        assert_eq!(blocks[0]["columns"].as_array().unwrap().len(), 2);
        // §7 — tabel mendeklarasikan kolom yang ditahan…
        assert_eq!(blocks[0]["withheld_columns"][0], "client_display_name");
        // …dan blok limitation menyatakannya (I5).
        let last = blocks.last().unwrap();
        assert_eq!(last["type"], "limitation");
        assert_eq!(last["block_id"], "pii_withheld");
        assert_eq!(last["withheld_columns"][0], "client_display_name");
    }

    #[test]
    fn pii_columns_appear_when_the_switch_is_on() {
        let mut plan = plan();
        plan.output_fields.push("client_display_name".into());
        plan.output_sensitivity.push("pii".into());

        let mut row = row("1.00", 1);
        row.insert("client_display_name".into(), Value::String("Budi".into()));

        let response = analysis(&plan, &[row], 5, true, &[], node(), &dataset(), None);
        assert!(response.blocks.to_string().contains("Budi"));
    }

    /// RESP-8.4 — pengungkapan auto-bind hidup di blok `note` (§5), bukan
    /// `limitation`. Validator memeriksa blok itu; mengungkapnya di tempat lain
    /// sama dengan tidak mengungkapnya.
    #[test]
    fn resp_8_4_auto_bound_slots_are_disclosed_in_a_note_block() {
        let auto_bound = [AutoBound {
            field_id: "client_id".into(),
            label: Some("Siti".into()),
            provenance: "resolver_unique",
        }];
        let response = analysis(
            &plan(),
            &[row("1.00", 1)],
            5,
            false,
            &auto_bound,
            node(),
            &dataset(),
            None,
        );
        let blocks = response.blocks.as_array().unwrap();

        assert_shape(&response.blocks);
        let note = blocks
            .iter()
            .find(|b| b["type"] == "note")
            .expect("blok note");
        assert_eq!(note["block_id"], "slots_auto_bound");
        assert_eq!(note["auto_bound_slots"][0]["field_id"], "client_id");
        assert_eq!(note["auto_bound_slots"][0]["provenance"], "resolver_unique");
        assert!(
            !blocks.iter().any(|b| b["type"] == "limitation"),
            "auto-bind tidak boleh diungkap sebagai limitation"
        );
    }

    /// RESP-8.3 — lineage dapat ditelusuri: ia ada di `evidence_json`, bukan di
    /// dalam blok yang boleh dilewati klien.
    #[test]
    fn resp_8_3_lineage_lives_in_evidence_not_in_a_block() {
        let response = analysis(
            &plan(),
            &[row("1.00", 1)],
            5,
            false,
            &[],
            node(),
            &dataset(),
            None,
        );

        let lineage = &response.evidence["lineage"][0];
        assert_eq!(lineage["node_run_id"], node().to_string());
        assert_eq!(lineage["query_id"], "savings.deposit_total");
        assert_eq!(lineage["catalog_content_hash"], "hash");
        assert_eq!(lineage["parameters"][1]["value"], "3 authorized offices");
        // FIN-43 — lineage membawa handle yang nyata, bukan `null`: tanpa ini
        // klien tidak punya jalan menuju `GET /chat/datasets/{id}`.
        assert_eq!(lineage["dataset_id"], dataset().dataset_id.to_string());
        assert!(response.evidence["derivations"].is_array());

        // Tidak ada blok `provenance`: ia bukan bagian dari kosakata §2.
        assert!(
            !response.blocks.to_string().contains("provenance"),
            "lineage masih bocor ke dalam blok"
        );
    }

    fn capped_plan(cap: i64) -> Plan {
        let mut plan = plan();
        plan.parameters.push(Bound::RowCap(cap));
        plan.parameter_names.push("limit".into());
        plan
    }

    /// FIN-133 — hanya kelebihan baris yang memotong; hasil yang muat di bawah
    /// cap, atau plan tanpa row cap, dibiarkan utuh.
    #[test]
    fn cap_rows_truncates_only_when_the_node_returned_more_than_the_cap() {
        let mut over = vec![row("1.00", 1), row("2.00", 2), row("3.00", 3)];
        assert_eq!(
            cap_rows(&capped_plan(2), &mut over),
            Some(RowCapReached { row_cap: 2 })
        );
        assert_eq!(over.len(), 2);
        assert_eq!(over[1]["deposit_count"], 2);

        let mut exact = vec![row("1.00", 1), row("2.00", 2)];
        assert_eq!(cap_rows(&capped_plan(2), &mut exact), None);
        assert_eq!(exact.len(), 2);

        let mut uncapped = vec![row("1.00", 1), row("2.00", 2), row("3.00", 3)];
        assert_eq!(cap_rows(&plan(), &mut uncapped), None);
        assert_eq!(uncapped.len(), 3);
    }

    /// FIN-133 — row cap tercapai: `Partial` + `row_cap_reached`, dinyatakan
    /// lewat blok `limitation`, lineage menampilkan cap (bukan cap + 1), dan
    /// dokumennya lolos validator di atas node `Partial`.
    #[test]
    fn row_cap_reached_is_partial_disclosed_and_passes_the_validator() {
        use crate::engine::validate::{self, Ledger};
        use std::collections::BTreeMap;

        let plan = capped_plan(2);
        let mut rows = vec![row("1.00", 1), row("2.00", 2), row("3.00", 3)];
        let reached = cap_rows(&plan, &mut rows);
        let response = analysis(&plan, &rows, 5, false, &[], node(), &dataset(), reached);

        assert_eq!(response.outcome, "Answered");
        assert_eq!(response.completeness, "Partial");
        assert_eq!(response.completeness_reason, ROW_CAP_REACHED);
        assert_shape(&response.blocks);

        let blocks = response.blocks.as_array().unwrap();
        assert_eq!(blocks[0]["row_count"], 2);
        let limitation = blocks
            .iter()
            .find(|b| b["block_id"] == ROW_CAP_REACHED)
            .expect("blok row_cap_reached");
        assert_eq!(limitation["type"], "limitation");
        assert!(limitation.get("derived_from").is_none());
        assert_eq!(limitation["row_cap"], 2);
        assert_eq!(limitation["rows_shown"], 2);
        assert_eq!(limitation["more_rows_exist"], true);

        let lineage = &response.evidence["lineage"][0];
        assert_eq!(lineage["parameters"][2]["name"], "limit");
        assert_eq!(lineage["parameters"][2]["value"], 2);
        assert_eq!(lineage["row_count"], 2);

        let ledger = Ledger {
            contributors: BTreeMap::from([(node().to_string(), "Partial".to_string())]),
            ..Ledger::default()
        };
        assert_eq!(validate::apply(response, &ledger).status(), "passed");
    }

    /// FIN-133 — cap ada tetapi tidak terlampaui: jawaban utuh, tanpa blok.
    #[test]
    fn row_cap_not_exceeded_stays_complete_without_a_limitation() {
        let plan = capped_plan(5);
        let mut rows = vec![row("1.00", 1), row("2.00", 2)];
        let reached = cap_rows(&plan, &mut rows);
        let response = analysis(&plan, &rows, 5, false, &[], node(), &dataset(), reached);

        assert_eq!(response.completeness, "Complete");
        assert_eq!(
            response.completeness_reason,
            "curated_query:savings.deposit_total"
        );
        assert!(
            !response.blocks.to_string().contains(ROW_CAP_REACHED),
            "row cap yang tidak tercapai tidak boleh diungkap"
        );
    }

    fn time_row(period: &str, value: i64) -> Map<String, Value> {
        let mut row = Map::new();
        row.insert("period".into(), Value::String(period.into()));
        row.insert("total".into(), Value::from(value));
        row
    }

    /// RESP-8.7 — chart yang tidak kompatibel turun menjadi tabel, bukan gagal
    /// dan bukan menyesatkan. "Bukan gagal" dibuktikan dengan melewatkan hasil
    /// downgrade lewat validator: statusnya `passed`.
    #[test]
    fn resp_8_7_an_incompatible_chart_downgrades_to_a_table_not_a_failure() {
        use crate::engine::validate::{self, Ledger};
        use std::collections::BTreeMap;

        let columns = vec!["period".to_string(), "total".to_string()];
        let derived_from = [from_node(node())];

        // Terurut naik → chart_spec dipertahankan.
        let ordered = [time_row("2026-01", 1), time_row("2026-02", 2)];
        let compatible = chart_or_table("trend", "period", &columns, &ordered, &derived_from);
        assert_eq!(compatible.len(), 1);
        assert_eq!(compatible[0]["type"], "chart_spec");

        // Tidak terurut → turun menjadi tabel + catatan, tanpa chart.
        let unordered = [time_row("2026-02", 2), time_row("2026-01", 1)];
        let downgraded = chart_or_table("trend", "period", &columns, &unordered, &derived_from);
        let blocks: Vec<&str> = downgraded
            .iter()
            .map(|b| b["type"].as_str().unwrap())
            .collect();
        assert_eq!(blocks, vec!["table", "note"]);
        assert!(
            !downgraded.iter().any(|b| b["type"] == "chart_spec"),
            "chart yang menyesatkan tidak boleh dirender"
        );
        let note = &downgraded[1];
        assert_eq!(note["downgraded_from"], "chart_spec");
        assert_eq!(note["reason"], "the time dimension is not ordered");

        // Kolom waktu hilang juga menurunkan.
        let no_time = chart_or_table(
            "trend",
            "period",
            &["total".to_string()],
            &ordered,
            &derived_from,
        );
        assert_eq!(
            no_time[1]["reason"],
            "the required time dimension column is absent"
        );

        // Bukan kegagalan: dokumen hasil downgrade lolos validator apa adanya.
        let response = SettledResponse {
            kind: "analysis",
            outcome: "Answered",
            completeness: "Complete",
            completeness_reason: "curated_query:x".into(),
            response_hash: "h".into(),
            evidence: json!({ "lineage": [], "derivations": [] }),
            blocks: Value::Array(downgraded),
        };
        let ledger = Ledger {
            contributors: BTreeMap::from([(node().to_string(), "Complete".to_string())]),
            ..Ledger::default()
        };
        assert_eq!(validate::apply(response, &ledger).status(), "passed");
    }

    /// RESP-8.9 — dataset kedaluwarsa: tabel menyatakan detail tidak lagi
    /// tersedia (bukan nol baris), angka ringkas tetap terbaca dengan `as_of`.
    #[test]
    fn resp_8_9_an_expired_dataset_states_expiry_while_summary_numbers_survive() {
        let dataset_id = Uuid::from_u128(42);
        let as_of = json!("2026-09-15");
        let table = expired_dataset_table("result", dataset_id, "purged", &as_of);

        assert_eq!(table["type"], "table");
        assert_eq!(table["detail_available"], false);
        assert_eq!(table["handle_state"], "purged");
        assert_eq!(table["as_of"], as_of);
        // Detail dinyatakan hilang, TIDAK didiamkan sebagai nol baris (I5).
        assert!(
            table["rows"].is_null(),
            "detail kedaluwarsa tidak boleh tampil sebagai []"
        );
        assert!(
            table["body"]
                .as_str()
                .unwrap()
                .contains("no longer available")
        );
        // Handle bertahan setelah purge, jadi ia sah menjadi kontributor lineage.
        assert_eq!(
            table["derived_from"][0]["dataset_id"],
            dataset_id.to_string()
        );
        assert_shape(&json!([table.clone()]));

        // Angka ringkas — sudah dihitung sebelum purge — tetap terbaca dengan
        // `as_of`, hidup sebagai blok `metric` yang inline pada response.
        let summary = block(
            "metric:total_balance",
            "metric",
            &[from_dataset(dataset_id)],
            json!({ "key": "total_balance", "value": "1500.00", "unit": Value::Null, "period": as_of }),
        );
        assert_eq!(summary["value"], "1500.00");
        assert_eq!(summary["period"], as_of);
    }
}
