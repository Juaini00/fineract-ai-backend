//! Komposisi response deterministik dari hasil query.
//!
//! Tidak ada model yang menentukan angka. Blok dibentuk dari baris hasil dan
//! `output_fields` manifest; narasi baru boleh ditambahkan kelak sebagai
//! lapisan additive yang kegagalannya **tidak** menghapus structured output
//! (responses.md).

use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

use crate::engine::{
    planner::{Bound, Plan},
    repository::SettledResponse,
};

/// Slot yang diikat resolver tanpa bertanya (K5, provenance `resolver_unique`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoBound {
    pub field_id: String,
    pub label: Option<String>,
}

impl AutoBound {
    fn describe(&self) -> String {
        match &self.label {
            Some(label) => format!("{} = {label}", self.field_id),
            None => self.field_id.clone(),
        }
    }
}

/// Susun response dari baris hasil.
///
/// Satu baris → blok `metrics` (satu angka per kolom). Lebih dari satu baris →
/// blok `table`. Nol baris → outcome `Empty` dengan `completeness` `Complete`:
/// pencarian yang berhasil dan memang tidak menemukan apa pun berbeda dari
/// pencarian yang tidak selesai (engine.md melarang `Empty` + `Partial`).
pub fn analysis(
    plan: &Plan,
    rows: &[Map<String, Value>],
    duration_ms: i64,
    pii_enabled: bool,
    auto_bound: &[AutoBound],
) -> SettledResponse {
    let (visible, withheld) = visible_fields(plan, pii_enabled);
    let provenance = provenance_block(plan, rows.len(), duration_ms);

    let (outcome, mut blocks) = match rows.len() {
        0 => (
            "Empty",
            json!([
                {
                    "type": "narrative",
                    "id": "empty",
                    "body": "The approved query ran successfully and returned no rows for the requested scope and period.",
                },
                provenance,
            ]),
        ),
        1 => (
            "Answered",
            json!([metrics_block(&visible, &rows[0]), provenance]),
        ),
        _ => ("Answered", json!([table_block(&visible, rows), provenance])),
    };

    // I5 — tidak ada penghilangan senyap: kolom yang ditahan dinyatakan, bukan
    // sekadar hilang dari tabel.
    if !withheld.is_empty()
        && let Some(array) = blocks.as_array_mut()
    {
        array.push(json!({
            "type": "limitation",
            "id": "pii_withheld",
            "title": "Columns withheld",
            "body": format!(
                "PII is disabled for this deployment, so {} column(s) were withheld from the result: {}.",
                withheld.len(),
                withheld.join(", ")
            ),
            "withheld_columns": withheld,
        }));
    }

    // K5 / D2 — slot yang diikat resolver karena hanya ada satu kandidat TIDAK
    // sama dengan slot yang pengguna konfirmasi. Ia wajib dinyatakan; kalau
    // tidak, jawaban tampak seolah pengguna memilih nasabah itu sendiri.
    if !auto_bound.is_empty()
        && let Some(array) = blocks.as_array_mut()
    {
        array.push(json!({
            "type": "limitation",
            "id": "slots_auto_bound",
            "title": "Values chosen without asking",
            "body": format!(
                "{} value(s) were bound automatically because the approved resolver returned exactly \
                 one candidate inside your authorized scope. Nobody confirmed them: {}.",
                auto_bound.len(),
                auto_bound
                    .iter()
                    .map(AutoBound::describe)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            "auto_bound_slots": auto_bound
                .iter()
                .map(|slot| json!({
                    "field_id": slot.field_id,
                    "label": slot.label,
                    "provenance": "resolver_unique",
                }))
                .collect::<Vec<_>>(),
        }));
    }

    SettledResponse {
        kind: "analysis",
        outcome,
        // Query capability berjalan utuh dalam satu eksekusi: tidak ada bagian
        // yang dilewati, jadi klaimnya `Complete`. Begitu truncation atau
        // fan-in parsial ada, nilai ini WAJIB dihitung ulang — bukan disalin.
        completeness: "Complete",
        completeness_reason: format!("curated_query:{}", plan.query_id),
        response_hash: hex::encode(Sha256::digest(blocks.to_string().as_bytes())),
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

fn metrics_block(output_fields: &[String], row: &Map<String, Value>) -> Value {
    let metrics: Vec<Value> = output_fields
        .iter()
        .map(|field| {
            json!({
                "key": field,
                "value": row.get(field).cloned().unwrap_or(Value::Null),
            })
        })
        .collect();

    json!({ "type": "metrics", "id": "result", "metrics": metrics })
}

fn table_block(output_fields: &[String], rows: &[Map<String, Value>]) -> Value {
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

    json!({
        "type": "table",
        "id": "result",
        "columns": output_fields,
        "rows": values,
        "row_count": rows.len(),
    })
}

/// Parameter yang benar-benar terikat, dalam satu bentuk.
///
/// Dipakai dua tempat — blok `provenance` dan `job_node_runs.input_binding_json`
/// — dan sengaja satu fungsi: dua penyusunan yang sama-sama benar hari ini akan
/// menyimpang, dan yang menyimpang di sini adalah "dengan parameter apa angka
/// ini dihasilkan".
///
/// `office_ids` menjadi **jumlah**, bukan daftar: scope adalah metadata, dan
/// menuliskan seluruh daftarnya tidak menambah kemampuan menelusuri apa pun.
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
                    Bound::Bigint(value) => Value::from(*value),
                    Bound::Text(value) => Value::String(value.clone()),
                    Bound::OfficeIds(_) => Value::String(format!("{office_count} authorized offices")),
                    Bound::NullText | Bound::NullBigintArray | Bound::NullBigint => Value::Null,
                },
            })
        })
        .collect()
}

/// Provenance: dari mana angka itu berasal, dengan versi katalog yang dipakai.
///
/// `office_ids` dicatat sebagai **jumlah**, bukan daftar: scope adalah
/// metadata, dan menuliskan seluruh daftarnya pada response tidak menambah
/// kemampuan menelusuri apa pun.
fn provenance_block(plan: &Plan, row_count: usize, duration_ms: i64) -> Value {
    let bindings = bindings(plan);

    json!({
        "type": "provenance",
        "id": "evidence",
        "capability_id": plan.capability_id,
        "query_id": plan.query_id,
        "sql_file": plan.sql_file,
        "catalog_version_id": plan.catalog_version_id,
        "catalog_content_hash": plan.catalog_content_hash,
        "parameters": bindings,
        "row_count": row_count,
        "duration_ms": duration_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

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
        }
    }

    fn row(total: &str, count: i64) -> Map<String, Value> {
        let mut row = Map::new();
        row.insert("total_deposit_amount".into(), Value::String(total.into()));
        row.insert("deposit_count".into(), Value::from(count));
        row
    }

    #[test]
    fn single_row_becomes_metrics_with_provenance() {
        let response = analysis(&plan(), &[row("1500.00", 3)], 12, false, &[]);
        let blocks = response.blocks.as_array().unwrap();

        assert_eq!(response.outcome, "Answered");
        assert_eq!(response.completeness, "Complete");
        assert_eq!(blocks[0]["type"], "metrics");
        assert_eq!(blocks[0]["metrics"][0]["value"], "1500.00");
        assert_eq!(blocks[1]["type"], "provenance");
        assert_eq!(blocks[1]["query_id"], "savings.deposit_total");
    }

    #[test]
    fn many_rows_become_a_table() {
        let response = analysis(&plan(), &[row("1.00", 1), row("2.00", 2)], 30, false, &[]);
        let blocks = response.blocks.as_array().unwrap();

        assert_eq!(blocks[0]["type"], "table");
        assert_eq!(blocks[0]["row_count"], 2);
        assert_eq!(blocks[0]["columns"][0], "total_deposit_amount");
    }

    #[test]
    fn no_rows_is_empty_not_a_failure() {
        let response = analysis(&plan(), &[], 5, false, &[]);

        assert_eq!(response.outcome, "Empty");
        // engine.md melarang Empty + Partial: pencarian parsial tidak boleh
        // menyatakan populasi kosong.
        assert_eq!(response.completeness, "Complete");
    }

    #[test]
    fn pii_columns_are_withheld_and_declared_when_the_switch_is_off() {
        let mut plan = plan();
        plan.output_fields.push("client_display_name".into());
        plan.output_sensitivity.push("pii".into());

        let mut row = row("1.00", 1);
        row.insert("client_display_name".into(), Value::String("Budi".into()));

        let response = analysis(&plan, &[row.clone(), row], 5, false, &[]);
        let rendered = response.blocks.to_string();

        assert!(!rendered.contains("Budi"), "PII bocor ke response: {rendered}");
        let blocks = response.blocks.as_array().unwrap();
        assert_eq!(blocks[0]["columns"].as_array().unwrap().len(), 2);
        // Penahanan wajib dinyatakan, bukan sekadar kolomnya hilang (I5).
        let last = blocks.last().unwrap();
        assert_eq!(last["type"], "limitation");
        assert_eq!(last["withheld_columns"][0], "client_display_name");
    }

    #[test]
    fn pii_columns_appear_when_the_switch_is_on() {
        let mut plan = plan();
        plan.output_fields.push("client_display_name".into());
        plan.output_sensitivity.push("pii".into());

        let mut row = row("1.00", 1);
        row.insert("client_display_name".into(), Value::String("Budi".into()));

        let response = analysis(&plan, &[row], 5, true, &[]);
        assert!(response.blocks.to_string().contains("Budi"));
    }

    #[test]
    fn provenance_reports_scope_as_a_count_not_a_list() {
        let response = analysis(&plan(), &[row("1.00", 1)], 5, false, &[]);
        let provenance = &response.blocks.as_array().unwrap()[1];

        assert_eq!(provenance["parameters"][1]["value"], "3 authorized offices");
        assert_eq!(provenance["catalog_content_hash"], "hash");
    }
}
