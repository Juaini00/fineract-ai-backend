//! Penegakan D1–D3 (responses.md §3–§5) terhadap dokumen yang sudah disusun.
//!
//! > Response bukan hasil yang dipercaya, melainkan hasil yang **dihitung ulang
//! > dan ditolak bila tidak cocok.**
//!
//! Itu bukan slogan: tidak satu pun pemeriksaan di sini membaca niat composer.
//! Semuanya membandingkan dokumen dengan **ledger** yang sudah durable
//! (`job_node_runs`, dan kelak `datasets`) — sumber yang ditulis tahap lain,
//! bukan sumber yang sama yang dipakai composer menyusun klaimnya. Pemeriksaan
//! yang membaca sumber yang sama dengan yang diperiksa hanya membuktikan bahwa
//! composer konsisten dengan dirinya sendiri.
//!
//! Modul ini murni: tidak ada `sqlx`, tidak ada waktu, tidak ada I/O. Ledger
//! dibaca pemanggil dan diserahkan ke sini apa adanya.

use std::collections::BTreeSet;

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::engine::repository::SettledResponse;

/// Blok prosa: isinya diperiksa D3, dan **tidak pernah** menjadi dasar
/// pembuktian angka. Prosa yang membuktikan dirinya sendiri bukan pemeriksaan.
const PROSE_BLOCKS: [&str; 4] = ["narrative", "finding", "findings", "comparison"];

/// Field yang berisi prosa di dalam blok mana pun. Dipakai dua arah: yang
/// diperiksa D3, sekaligus yang dibuang saat mengumpulkan angka ber-evidence —
/// `comparison` menyumbang nilainya tetapi bukan kalimatnya.
const PROSE_KEYS: [&str; 3] = ["body", "text", "summary"];

/// Blok yang tidak diperiksa dan tidak membuktikan apa pun.
///
/// `limitation` adalah prosa server yang deterministik ("2 column(s) were
/// withheld"): angkanya dihasilkan kode yang sama yang menghitungnya, jadi
/// memeriksanya tidak menangkap apa pun, dan menjadikannya evidence akan
/// mengesahkan angka yang dikarang narasi hanya karena kebetulan muncul di
/// kalimat limitation.
const NEUTRAL_BLOCKS: [&str; 1] = ["limitation"];

const COMPLETENESS: [&str; 3] = ["Complete", "Partial", "Unknown"];

/// Fakta durable yang dipakai **menghitung ulang** klaim composer.
#[derive(Debug, Default, Clone)]
pub struct Ledger {
    /// `completeness` tiap kontributor pada `plan_version` aktif
    /// (`job_node_runs`, kelak juga `datasets`).
    pub contributors: Vec<String>,
    /// Slot yang diikat tanpa bertanya menurut `job_node_runs.input_binding_json`
    /// (K5) — binding yang BENAR-BENAR dikonsumsi node, bukan yang diingat
    /// proses penyusun.
    pub auto_bound: BTreeSet<String>,
    /// Entri `derivation` pada `evidence_json` (§4). Kosong hari ini: belum ada
    /// yang memproduksi angka turunan. Begitu narasi LLM ada, di sinilah
    /// "naik 12%" menjadi sah — dan hanya lewat sini.
    pub derivations: Vec<Value>,
}

/// Satu aturan yang dilanggar.
///
/// `block_id` yang terisi berarti fallback dapat memperbaikinya dengan membuang
/// blok itu; `None` berarti kegagalan tingkat dokumen dan yang diperbaiki
/// adalah klaimnya, bukan isinya.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Failure {
    pub rule: &'static str,
    pub block_id: Option<String>,
    pub detail: String,
}

/// Hasil validasi: yang disajikan, yang ditolak, dan laporannya.
#[derive(Debug)]
pub struct Validated {
    pub served: SettledResponse,
    /// Dokumen yang gagal validasi. **Tetap disimpan** sebagai bahan
    /// investigasi (#10): versi yang ditolak adalah satu-satunya bukti tentang
    /// apa yang nyaris disajikan.
    pub rejected: Option<SettledResponse>,
    pub report: Value,
}

impl Validated {
    /// Dokumen yang tidak melewati pemeriksaan karena memang tidak memuat data:
    /// `limitation`, `not_found`, dan kegagalan operasional. Tidak ada angka
    /// untuk digrounding dan tidak ada kontributor untuk dihitung ulang.
    pub fn unchecked(response: SettledResponse) -> Self {
        Self {
            served: response,
            rejected: None,
            report: json!({ "checked": false, "reason": "no_data_blocks" }),
        }
    }

    pub fn status(&self) -> &'static str {
        if self.rejected.is_some() {
            "fallback"
        } else {
            "passed"
        }
    }
}

/// Validasi dokumen, lalu — bila gagal — susun versi fallback deterministik.
pub fn apply(response: SettledResponse, ledger: &Ledger) -> Validated {
    let computed = worst(ledger.contributors.iter().map(String::as_str));
    let failures = failures(&response, ledger, computed);

    let report = json!({
        "checked": true,
        "computed_completeness": computed,
        "claimed_completeness": response.completeness,
        "failures": failures,
    });

    if failures.is_empty() {
        return Validated { served: response, rejected: None, report };
    }

    let served = fallback(&response, &failures, computed);
    Validated { served, rejected: Some(response), report }
}

/// Terburuk di antara kontributor.
///
/// Tanpa kontributor hasilnya `Unknown`, bukan `Complete`: "tidak ada yang bisa
/// dihitung" adalah ketidaktahuan, dan I4 menuntut ketidaktahuan terlihat di
/// data alih-alih diratakan menjadi kabar baik.
pub fn worst<'a>(values: impl IntoIterator<Item = &'a str>) -> &'static str {
    let mut seen = false;
    let mut severity = 0;

    for value in values {
        seen = true;
        severity = severity.max(rank(value));
    }

    if seen { COMPLETENESS[severity] } else { "Unknown" }
}

/// `Complete < Partial < Unknown`. Nilai tak dikenal diperlakukan sebagai
/// `Unknown` — gagal ke arah yang aman, bukan ke arah yang menyenangkan.
fn rank(value: &str) -> usize {
    COMPLETENESS.iter().position(|known| *known == value).unwrap_or(2)
}

fn failures(response: &SettledResponse, ledger: &Ledger, computed: &str) -> Vec<Failure> {
    let mut failures = Vec::new();
    let blocks = response.blocks.as_array().map(Vec::as_slice).unwrap_or(&[]);

    // D1 — satu arah. Composer boleh tahu celah yang tidak terlihat di ledger,
    // jadi klaim yang LEBIH BURUK diterima. Klaim yang lebih baik tidak pernah:
    // itulah yang membuat `Complete` di atas data `Partial` mustahil lolos
    // tanpa bergantung pada disiplin siapa pun.
    if rank(response.completeness) < rank(computed) {
        failures.push(Failure {
            rule: "D1",
            block_id: None,
            detail: format!(
                "composer mengklaim {} sementara ledger menghitung {} dari {} kontributor",
                response.completeness,
                computed,
                ledger.contributors.len()
            ),
        });
    }

    if response.completeness != "Complete" && response.completeness_reason.trim().is_empty() {
        failures.push(Failure {
            rule: "D1",
            block_id: None,
            detail: format!(
                "completeness {} tanpa completeness_reason",
                response.completeness
            ),
        });
    }

    // D2 — himpunan, bukan jumlah. Satu slot diungkap dan satu lagi diam
    // menghasilkan hitungan yang benar dan jawaban yang menyesatkan.
    let disclosed = disclosed_slots(blocks);
    if disclosed != ledger.auto_bound {
        let undisclosed: Vec<&str> = ledger
            .auto_bound
            .difference(&disclosed)
            .map(String::as_str)
            .collect();
        let invented: Vec<&str> = disclosed
            .difference(&ledger.auto_bound)
            .map(String::as_str)
            .collect();

        failures.push(Failure {
            rule: "D2",
            block_id: None,
            detail: format!(
                "slot auto-bind tidak cocok dengan input_binding_json — tidak diungkap: [{}], diungkap tanpa dasar: [{}]",
                undisclosed.join(", "),
                invented.join(", ")
            ),
        });
    }

    // D3 — setiap angka pada prosa wajib berdasar.
    let grounded = grounded_numerals(blocks, &ledger.derivations);
    for block in blocks {
        let block_type = block.get("type").and_then(Value::as_str).unwrap_or("");
        if !PROSE_BLOCKS.contains(&block_type) {
            continue;
        }

        for token in prose_numerals(block) {
            if readings(&token).iter().any(|value| grounded.contains(value)) {
                continue;
            }

            failures.push(Failure {
                rule: "D3",
                block_id: Some(block_id(block)),
                detail: format!(
                    "numeral {token} pada blok {block_type} tidak cocok dengan blok ber-evidence maupun entri derivation"
                ),
            });
        }
    }

    failures
}

/// Versi konservatif yang menggantikan dokumen yang ditolak (§6).
///
/// **Deterministik, dan itu wajib**: jalur kegagalan tidak boleh bergantung
/// pada komponen yang barusan gagal. Tidak ada model di sini — blok yang gagal
/// dibuang, sisanya dipertahankan apa adanya, satu blok `limitation` menyebut
/// apa yang dibuang beserta alasannya, dan `completeness` dihitung ulang.
fn fallback(rejected: &SettledResponse, failures: &[Failure], computed: &'static str) -> SettledResponse {
    let dropped: BTreeSet<&str> = failures
        .iter()
        .filter_map(|failure| failure.block_id.as_deref())
        .collect();

    let mut kept: Vec<Value> = rejected
        .blocks
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[])
        .iter()
        .filter(|block| !dropped.contains(block_id(block).as_str()))
        .cloned()
        .collect();

    let mut rules: Vec<&str> = failures.iter().map(|failure| failure.rule).collect();
    rules.sort_unstable();
    rules.dedup();

    // I5 — tidak ada penghilangan senyap. Blok yang dibuang wajib dinyatakan,
    // bukan sekadar tidak ada.
    kept.push(json!({
        "type": "limitation",
        "id": "validation_rejected",
        "title": "Part of this answer was withheld",
        "body": format!(
            "The composed answer failed {} check(s) against the durable ledger, so {} block(s) were dropped and the remaining claim was recomputed.",
            failures.len(),
            dropped.len()
        ),
        "failed_rules": rules,
        "failures": failures,
    }));

    let has_data = kept.iter().any(|block| {
        matches!(
            block.get("type").and_then(Value::as_str),
            Some("metrics" | "table")
        )
    });

    let blocks = Value::Array(kept);

    SettledResponse {
        // Tanpa satu pun blok data yang tersisa, hasilnya adalah response
        // `limitation` — bukan dokumen kosong, dan bukan job yang gagal
        // diam-diam.
        kind: if has_data { rejected.kind } else { "limitation" },
        outcome: if has_data { rejected.outcome } else { "Unsupported" },
        // Fallback tidak pernah MEMPERBAIKI klaim: composer yang tadinya
        // mengaku `Partial` tidak boleh keluar sebagai `Complete` hanya karena
        // dokumennya ditolak.
        completeness: if has_data {
            worst([computed, rejected.completeness])
        } else {
            "Unknown"
        },
        completeness_reason: format!("validation_failed:{}", rules.join("+")),
        response_hash: hex::encode(Sha256::digest(blocks.to_string().as_bytes())),
        blocks,
    }
}

fn block_id(block: &Value) -> String {
    block
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// Slot auto-bind yang benar-benar diungkap dokumen (blok `slots_auto_bound`).
fn disclosed_slots(blocks: &[Value]) -> BTreeSet<String> {
    blocks
        .iter()
        .filter_map(|block| block.get("auto_bound_slots").and_then(Value::as_array))
        .flatten()
        .filter_map(|slot| slot.get("field_id").and_then(Value::as_str))
        .map(str::to_string)
        .collect()
}

/// Angka yang boleh dirujuk prosa: seluruh skalar pada blok ber-evidence,
/// ditambah `result` tiap entri `derivation`.
fn grounded_numerals(blocks: &[Value], derivations: &[Value]) -> BTreeSet<String> {
    let mut grounded = BTreeSet::new();

    for block in blocks {
        let block_type = block.get("type").and_then(Value::as_str).unwrap_or("");
        if NEUTRAL_BLOCKS.contains(&block_type) {
            continue;
        }
        collect(block, &mut grounded);
    }

    for derivation in derivations {
        if let Some(result) = derivation.get("result") {
            collect(result, &mut grounded);
        }
    }

    grounded
}

/// Kumpulkan bentuk kanonik tiap angka, melewati field prosa.
fn collect(value: &Value, out: &mut BTreeSet<String>) {
    match value {
        Value::Number(number) => out.extend(canonical(&number.to_string())),
        Value::String(text) => {
            for token in numerals(text) {
                out.extend(readings(&token));
            }
        }
        Value::Array(items) => items.iter().for_each(|item| collect(item, out)),
        Value::Object(fields) => fields
            .iter()
            .filter(|(key, _)| !PROSE_KEYS.contains(&key.as_str()))
            .for_each(|(_, field)| collect(field, out)),
        _ => {}
    }
}

fn prose_numerals(block: &Value) -> Vec<String> {
    PROSE_KEYS
        .iter()
        .filter_map(|key| block.get(*key).and_then(Value::as_str))
        .flat_map(numerals)
        .collect()
}

/// Potong tiap deret angka beserta pemisahnya. Pemisah di ekor dibuang, supaya
/// titik akhir kalimat tidak menjadi bagian dari angka.
fn numerals(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut tokens = Vec::new();
    let mut index = 0;

    while index < chars.len() {
        if !chars[index].is_ascii_digit() {
            index += 1;
            continue;
        }

        let start = index;
        while index < chars.len()
            && (chars[index].is_ascii_digit() || matches!(chars[index], ',' | '.' | '_'))
        {
            index += 1;
        }

        let mut end = index;
        while end > start && !chars[end - 1].is_ascii_digit() {
            end -= 1;
        }

        tokens.push(chars[start..end].iter().collect());
    }

    tokens
}

/// Dua pembacaan yang sama-sama sah untuk satu token: `.` sebagai desimal
/// (Inggris) dan `,` sebagai desimal (Indonesia).
///
/// Angka yang sama ditulis dua konvensi bukan angka yang tidak berdasar, dan
/// menolaknya hanya akan mengajari orang mematikan validator.
fn readings(token: &str) -> Vec<String> {
    let english = token.replace(['_', ','], "");
    let indonesian = token.replace(['_', '.'], "").replace(',', ".");

    let mut readings: Vec<String> = canonical(&english).into_iter().collect();
    if indonesian != english {
        readings.extend(canonical(&indonesian));
    }
    readings
}

/// Bentuk kanonik tanpa float: nol di depan dan nol di belakang dibuang,
/// sehingga `1500`, `1500.00` dan `01500.0` menjadi satu nilai yang sama.
///
/// Sengaja bekerja atas string. Membandingkan uang lewat `f64` berarti dua
/// angka yang identik di layar dapat berbeda di memori, dan kegagalannya muncul
/// sebagai validasi yang menolak narasi yang sebenarnya benar.
fn canonical(value: &str) -> Option<String> {
    let (integer, fraction) = match value.split_once('.') {
        Some((_, fraction)) if fraction.contains('.') => return None,
        Some((integer, fraction)) => (integer, fraction),
        None => (value, ""),
    };

    if integer.is_empty() && fraction.is_empty() {
        return None;
    }
    if !integer.chars().all(|c| c.is_ascii_digit()) || !fraction.chars().all(|c| c.is_ascii_digit())
    {
        return None;
    }

    let integer = match integer.trim_start_matches('0') {
        "" => "0",
        trimmed => trimmed,
    };
    let fraction = fraction.trim_end_matches('0');

    Some(if fraction.is_empty() {
        integer.to_string()
    } else {
        format!("{integer}.{fraction}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ledger(contributors: &[&str]) -> Ledger {
        Ledger {
            contributors: contributors.iter().map(|value| value.to_string()).collect(),
            ..Ledger::default()
        }
    }

    fn response(completeness: &'static str, blocks: Value) -> SettledResponse {
        SettledResponse {
            kind: "analysis",
            outcome: "Answered",
            completeness,
            completeness_reason: "curated_query:savings.deposit_total".into(),
            response_hash: "hash".into(),
            blocks,
        }
    }

    fn metrics(value: Value) -> Value {
        json!({ "type": "metrics", "id": "result", "metrics": [{ "key": "total", "value": value }] })
    }

    fn narrative(body: &str) -> Value {
        json!({ "type": "narrative", "id": "summary", "body": body })
    }

    // --- D1 ---

    #[test]
    fn worst_of_nothing_is_unknown_not_complete() {
        assert_eq!(worst(std::iter::empty()), "Unknown");
        assert_eq!(worst(["Complete", "Partial"]), "Partial");
        assert_eq!(worst(["Partial", "Unknown"]), "Unknown");
        assert_eq!(worst(["Complete", "Complete"]), "Complete");
    }

    #[test]
    fn claiming_complete_over_partial_ledger_is_rejected() {
        let validated = apply(
            response("Complete", json!([metrics(json!(10))])),
            &ledger(&["Partial"]),
        );

        assert_eq!(validated.status(), "fallback");
        assert_eq!(validated.report["failures"][0]["rule"], "D1");
        // Dokumen yang ditolak TETAP ada sebagai bahan investigasi (#10).
        assert_eq!(validated.rejected.unwrap().completeness, "Complete");
        // Yang disajikan dihitung ulang, bukan diperbaiki sebagian.
        assert_eq!(validated.served.completeness, "Partial");
    }

    #[test]
    fn claiming_worse_than_the_ledger_is_accepted() {
        let validated = apply(
            response("Unknown", json!([metrics(json!(10))])),
            &ledger(&["Complete"]),
        );

        assert_eq!(validated.status(), "passed");
        assert!(validated.rejected.is_none());
    }

    #[test]
    fn fallback_never_improves_a_claim_the_composer_made_worse() {
        // D3 gagal, tetapi klaim composer (`Partial`) lebih buruk daripada
        // hitungan ledger (`Complete`) — fallback tidak boleh menaikkannya.
        let validated = apply(
            response("Partial", json!([metrics(json!(10)), narrative("naik 42%")])),
            &ledger(&["Complete"]),
        );

        assert_eq!(validated.status(), "fallback");
        assert_eq!(validated.served.completeness, "Partial");
    }

    #[test]
    fn a_non_complete_claim_without_a_reason_is_rejected() {
        let mut document = response("Partial", json!([metrics(json!(10))]));
        document.completeness_reason = "   ".into();

        let validated = apply(document, &ledger(&["Partial"]));
        assert_eq!(validated.report["failures"][0]["rule"], "D1");
    }

    // --- D2 ---

    #[test]
    fn auto_bound_slot_that_the_document_never_discloses_is_rejected() {
        let ledger = Ledger {
            contributors: vec!["Complete".into()],
            auto_bound: BTreeSet::from(["client_id".to_string()]),
            derivations: Vec::new(),
        };

        let validated = apply(response("Complete", json!([metrics(json!(10))])), &ledger);

        assert_eq!(validated.status(), "fallback");
        assert_eq!(validated.report["failures"][0]["rule"], "D2");
        assert!(
            validated.report["failures"][0]["detail"]
                .as_str()
                .unwrap()
                .contains("client_id")
        );
    }

    #[test]
    fn disclosure_matching_the_ledger_exactly_passes() {
        let ledger = Ledger {
            contributors: vec!["Complete".into()],
            auto_bound: BTreeSet::from(["client_id".to_string()]),
            derivations: Vec::new(),
        };

        let blocks = json!([
            metrics(json!(10)),
            {
                "type": "limitation",
                "id": "slots_auto_bound",
                "auto_bound_slots": [{ "field_id": "client_id", "label": "Siti" }],
            },
        ]);

        assert_eq!(apply(response("Complete", blocks), &ledger).status(), "passed");
    }

    #[test]
    fn disclosing_a_slot_the_ledger_never_bound_is_also_rejected() {
        let blocks = json!([
            metrics(json!(10)),
            {
                "type": "limitation",
                "id": "slots_auto_bound",
                "auto_bound_slots": [{ "field_id": "office_id" }],
            },
        ]);

        let validated = apply(response("Complete", blocks), &ledger(&["Complete"]));
        assert_eq!(validated.report["failures"][0]["rule"], "D2");
    }

    // --- D3 ---

    #[test]
    fn a_narrative_number_that_no_block_carries_is_rejected() {
        let blocks = json!([
            metrics(json!("1500.00")),
            narrative("Total deposits reached 9999 this period."),
        ]);

        let validated = apply(response("Complete", blocks), &ledger(&["Complete"]));

        assert_eq!(validated.status(), "fallback");
        assert_eq!(validated.report["failures"][0]["rule"], "D3");
        assert_eq!(validated.report["failures"][0]["block_id"], "summary");
        // Blok yang gagal dibuang; blok yang lolos dipertahankan apa adanya.
        let kept: Vec<&str> = validated
            .served
            .blocks
            .as_array()
            .unwrap()
            .iter()
            .map(|block| block["id"].as_str().unwrap())
            .collect();
        assert_eq!(kept, vec!["result", "validation_rejected"]);
    }

    #[test]
    fn display_formatting_of_a_grounded_number_is_accepted() {
        // Satu angka, empat penulisan: nilai mentah, pemisah ribuan Inggris,
        // pemisah ribuan Indonesia, dan nol desimal yang tidak bermakna.
        let blocks = json!([
            metrics(json!("1500.00")),
            narrative("Total 1,500 — atau 1.500, tepatnya 1500.0."),
        ]);

        assert_eq!(
            apply(response("Complete", blocks), &ledger(&["Complete"])).status(),
            "passed"
        );
    }

    #[test]
    fn a_number_only_a_limitation_sentence_mentions_does_not_ground_anything() {
        // Prosa deterministik server tidak boleh mengesahkan angka narasi.
        let blocks = json!([
            metrics(json!(10)),
            { "type": "limitation", "id": "pii_withheld", "body": "3 column(s) were withheld." },
            narrative("We found 3 matching clients."),
        ]);

        let validated = apply(response("Complete", blocks), &ledger(&["Complete"]));
        assert_eq!(validated.report["failures"][0]["rule"], "D3");
    }

    #[test]
    fn a_declared_derivation_grounds_a_number_no_block_carries() {
        let ledger = Ledger {
            contributors: vec!["Complete".into()],
            auto_bound: BTreeSet::new(),
            derivations: vec![json!({
                "id": "growth",
                "formula": "(b - a) / a",
                "inputs": ["result.total"],
                "result": 12,
                "rounding": "0dp",
            })],
        };

        let blocks = json!([metrics(json!(10)), narrative("Naik 12% dari periode lalu.")]);
        assert_eq!(apply(response("Complete", blocks), &ledger).status(), "passed");
    }

    #[test]
    fn a_table_value_grounds_the_narrative_that_cites_it() {
        let blocks = json!([
            { "type": "table", "id": "result", "columns": ["office", "total"], "rows": [["Head", 4217]], "row_count": 1 },
            narrative("The largest office holds 4217."),
        ]);

        assert_eq!(
            apply(response("Complete", blocks), &ledger(&["Complete"])).status(),
            "passed"
        );
    }

    #[test]
    fn the_deterministic_empty_narrative_carries_no_numerals() {
        // Regresi terhadap composer hari ini: narasi "nol baris" wajib lolos
        // D3 tanpa satu pun blok data untuk menggroundingnya.
        let blocks = json!([
            narrative("The approved query ran successfully and returned no rows."),
            { "type": "provenance", "id": "evidence", "row_count": 0 },
        ]);

        assert_eq!(
            apply(response("Complete", blocks), &ledger(&["Complete"])).status(),
            "passed"
        );
    }

    #[test]
    fn dropping_every_data_block_turns_the_document_into_a_limitation() {
        let blocks = json!([narrative("Total tercatat 9999.")]);
        let validated = apply(response("Complete", blocks), &ledger(&["Complete"]));

        assert_eq!(validated.served.kind, "limitation");
        assert_eq!(validated.served.outcome, "Unsupported");
        assert_eq!(validated.served.completeness, "Unknown");
        // Bukan dokumen kosong: alasannya ikut.
        assert_eq!(validated.served.blocks.as_array().unwrap().len(), 1);
        assert_eq!(
            validated.served.completeness_reason,
            "validation_failed:D3"
        );
    }

    // --- normalisasi ---

    #[test]
    fn canonical_collapses_padding_and_rejects_non_numbers() {
        assert_eq!(canonical("1500.00").as_deref(), Some("1500"));
        assert_eq!(canonical("01500.50").as_deref(), Some("1500.5"));
        assert_eq!(canonical("0.0").as_deref(), Some("0"));
        assert_eq!(canonical("1.234.567"), None);
        assert_eq!(canonical(""), None);
    }

    #[test]
    fn numerals_stop_at_sentence_punctuation() {
        assert_eq!(numerals("naik 12."), vec!["12"]);
        assert_eq!(numerals("1,500 dan 2.5"), vec!["1,500", "2.5"]);
        assert!(numerals("tanpa angka").is_empty());
    }

    #[test]
    fn a_fallback_document_is_rehashed_not_inherited() {
        let validated = apply(
            response("Complete", json!([metrics(json!(10)), narrative("9999")])),
            &ledger(&["Complete"]),
        );

        assert_ne!(validated.served.response_hash, "hash");
    }
}
