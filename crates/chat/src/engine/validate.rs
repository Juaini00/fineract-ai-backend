//! Penegakan responses.md §1–§5 terhadap dokumen yang sudah disusun.
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
//! Empat pemeriksaan, dan semuanya mekanis:
//!
//! - **§1/§2** — bentuk blok dan kosakata tertutup. Tipe di luar sembilan tipe
//!   ditolak; klien melewati tipe yang tidak dikenal, jadi tipe yang dikarang
//!   server adalah informasi yang tidak pernah sampai.
//! - **D1 (§3)** — `completeness` dihitung **per blok** lewat `derived_from`,
//!   lalu diagregasi menjadi klaim dokumen. Satu arah: klaim yang lebih baik
//!   daripada hitungan ditolak.
//! - **D2 (§5)** — slot auto-bind diperiksa pada blok `note`.
//! - **D3 (§4)** — setiap angka pada prosa wajib berdasar.
//!
//! Modul ini murni: tidak ada `sqlx`, tidak ada waktu, tidak ada I/O. Ledger
//! dibaca pemanggil dan diserahkan ke sini apa adanya.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

use crate::engine::{
    compose::{self, BLOCK_SCHEMA_VERSION, BLOCK_TYPES, DATA_BLOCKS},
    repository::SettledResponse,
};

/// Blok prosa: isinya diperiksa D3, dan **tidak pernah** menjadi dasar
/// pembuktian angka. Prosa yang membuktikan dirinya sendiri bukan pemeriksaan.
const PROSE_BLOCKS: [&str; 3] = ["narrative", "finding", "comparison"];

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
/// kalimat limitation. `suggestion` sama: label + prefill, bukan klaim data.
const NEUTRAL_BLOCKS: [&str; 2] = ["limitation", "suggestion"];

/// Field struktural blok. Bukan angka klaim, jadi tidak boleh menggrounding
/// apa pun — `schema_version: 1` yang mengesahkan narasi "1 nasabah" adalah
/// persis kelas kebocoran yang membuat D3 tidak berguna.
const STRUCTURAL_KEYS: [&str; 4] = ["schema_version", "block_id", "type", "derived_from"];

const COMPLETENESS: [&str; 3] = ["Complete", "Partial", "Unknown"];

/// Fakta durable yang dipakai **menghitung ulang** klaim composer.
#[derive(Debug, Default, Clone)]
pub struct Ledger {
    /// `completeness` tiap kontributor, **berkunci identitasnya**
    /// (`job_node_runs.id`, kelak juga `datasets.id`). Kuncinya wajib: D1
    /// dihitung per blok lewat `derived_from`, dan daftar tanpa identitas hanya
    /// dapat dihitung di tingkat dokumen — persis pendekatan yang ditemukan
    /// audit sebagai penyimpangan.
    pub contributors: BTreeMap<String, String>,
    /// Slot yang diikat tanpa bertanya menurut `job_node_runs.input_binding_json`
    /// (K5) — binding yang BENAR-BENAR dikonsumsi node, bukan yang diingat
    /// proses penyusun.
    pub auto_bound: BTreeSet<String>,
    /// Entri `derivation` pada `evidence_json` (§4). Kosong hari ini: belum ada
    /// yang memproduksi angka turunan. Begitu narasi LLM ada, di sinilah
    /// "naik 12%" menjadi sah — dan hanya lewat sini.
    pub derivations: Vec<Value>,
    /// Parameter yang benar-benar dikonsumsi node. Sumber daftar pengecualian
    /// §4: tahun di dalam nama periode yang sudah muncul di parameter bukan
    /// klaim data.
    pub parameters: Vec<Value>,
    /// Baris hasil tiap node (`job_node_runs.output_json.rows`), berkunci
    /// identitasnya. Bentuk data yang dituntut `chart_spec` (§7) diperiksa atas
    /// baris INI, bukan atas baris yang dipegang composer.
    pub outputs: BTreeMap<String, Vec<Map<String, Value>>>,
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
    let blocks = response.blocks.as_array().map(Vec::as_slice).unwrap_or(&[]);
    let per_block = per_block_completeness(blocks, ledger);
    let computed = aggregate(&per_block);
    let failures = failures(&response, ledger, computed, &per_block);

    let report = json!({
        "checked": true,
        "computed_completeness": computed,
        "claimed_completeness": response.completeness,
        "block_completeness": per_block,
        "failures": failures,
    });

    if failures.is_empty() {
        return Validated { served: response, rejected: None, report };
    }

    let served = fallback(&response, &failures, ledger);
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

/// §3 aturan 2 — `completeness` tiap blok adalah yang **terburuk di antara
/// seluruh kontributornya**, ditelusuri lewat `derived_from`.
///
/// Rujukan yang tidak ada di ledger menghasilkan `Unknown`: blok yang mengaku
/// berasal dari sesuatu yang tidak tercatat bukan blok yang lengkap.
fn per_block_completeness(blocks: &[Value], ledger: &Ledger) -> BTreeMap<String, &'static str> {
    let mut per_block = BTreeMap::new();

    for block in blocks {
        let Some(refs) = block.get("derived_from").and_then(Value::as_array) else {
            continue;
        };

        let contributors: Vec<&str> = refs
            .iter()
            .map(|reference| {
                contributor_key(reference)
                    .and_then(|key| ledger.contributors.get(&key))
                    .map(String::as_str)
                    .unwrap_or("Unknown")
            })
            .collect();

        per_block.insert(block_id(block), worst(contributors));
    }

    per_block
}

/// §3 aturan 3 — `completeness` dokumen adalah yang terburuk di antara seluruh
/// blok.
fn aggregate(per_block: &BTreeMap<String, &'static str>) -> &'static str {
    worst(per_block.values().copied())
}

/// Identitas kontributor pada satu entri `derived_from`: `node_run_id` atau
/// `dataset_id` (§1). Keduanya memakai ruang kunci yang sama karena keduanya
/// UUID dan keduanya menyumbang `completeness`.
fn contributor_key(reference: &Value) -> Option<String> {
    ["node_run_id", "dataset_id"]
        .iter()
        .find_map(|field| reference.get(*field).and_then(Value::as_str))
        .map(str::to_string)
        .or_else(|| reference.as_str().map(str::to_string))
}

fn failures(
    response: &SettledResponse,
    ledger: &Ledger,
    computed: &str,
    per_block: &BTreeMap<String, &'static str>,
) -> Vec<Failure> {
    let mut failures = Vec::new();
    let blocks = response.blocks.as_array().map(Vec::as_slice).unwrap_or(&[]);

    failures.extend(shape_failures(blocks));
    failures.extend(chart_failures(blocks, ledger));

    // D1 — satu arah. Composer boleh tahu celah yang tidak terlihat di ledger,
    // jadi klaim yang LEBIH BURUK diterima. Klaim yang lebih baik tidak pernah:
    // itulah yang membuat `Complete` di atas data `Partial` mustahil lolos
    // tanpa bergantung pada disiplin siapa pun.
    if rank(response.completeness) < rank(computed) {
        failures.push(Failure {
            rule: "D1",
            block_id: None,
            detail: format!(
                "composer mengklaim {} sementara ledger menghitung {} dari {} blok penyaji data ({})",
                response.completeness,
                computed,
                per_block.len(),
                per_block
                    .iter()
                    .map(|(id, value)| format!("{id}={value}"))
                    .collect::<Vec<_>>()
                    .join(", ")
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
                "slot auto-bind pada blok `note` tidak cocok dengan input_binding_json — tidak diungkap: [{}], diungkap tanpa dasar: [{}]",
                undisclosed.join(", "),
                invented.join(", ")
            ),
        });
    }

    // D3 — setiap angka pada prosa wajib berdasar.
    let grounded = grounded_numerals(blocks, &ledger.derivations);
    let excluded = excluded_numerals(&ledger.parameters);

    for block in blocks {
        let block_type = block.get("type").and_then(Value::as_str).unwrap_or("");
        if !PROSE_BLOCKS.contains(&block_type) {
            continue;
        }

        for token in prose_numerals(block) {
            let readings = readings(&token);
            if readings
                .iter()
                .any(|value| grounded.contains(value) || excluded.contains(value))
            {
                continue;
            }

            failures.push(Failure {
                rule: "D3",
                block_id: Some(block_id(block)),
                detail: format!(
                    "numeral {token} pada blok {block_type} tidak cocok dengan blok ber-evidence, entri derivation, maupun daftar pengecualian §4"
                ),
            });
        }
    }

    failures
}

/// §1 dan §2 — bentuk blok dan kosakata tertutup.
///
/// Tanpa pemeriksaan ini, D1 diam-diam melemah: blok tanpa `derived_from` tidak
/// punya kontributor untuk dihitung, dan "tidak ada yang bisa dihitung" akan
/// lolos sebagai dokumen yang memang tidak menyajikan data.
fn shape_failures(blocks: &[Value]) -> Vec<Failure> {
    let mut failures = Vec::new();

    for block in blocks {
        let id = block_id(block);
        let block_type = block.get("type").and_then(Value::as_str).unwrap_or("");

        if !BLOCK_TYPES.contains(&block_type) {
            failures.push(Failure {
                rule: "§2",
                block_id: Some(id.clone()),
                detail: format!(
                    "tipe blok `{block_type}` di luar kosakata: {}",
                    BLOCK_TYPES.join(", ")
                ),
            });
            // Blok di luar kosakata sudah dibuang; memeriksa bentuknya hanya
            // menambah kebisingan pada laporan yang sama.
            continue;
        }

        if block.get("block_id").and_then(Value::as_str).is_none_or(str::is_empty) {
            failures.push(Failure {
                rule: "§1",
                block_id: Some(id.clone()),
                detail: "blok tanpa `block_id`".to_string(),
            });
        }

        if block.get("schema_version").and_then(Value::as_i64) != Some(BLOCK_SCHEMA_VERSION) {
            failures.push(Failure {
                rule: "§1",
                block_id: Some(id.clone()),
                detail: format!("blok tanpa `schema_version` = {BLOCK_SCHEMA_VERSION}"),
            });
        }

        let has_refs = block
            .get("derived_from")
            .and_then(Value::as_array)
            .is_some_and(|refs| !refs.is_empty());

        // Blok penyaji data selalu; `narrative` hanya bila ia memuat angka
        // (§2), karena kalimat tanpa angka tidak menyajikan data.
        let needs_refs = DATA_BLOCKS.contains(&block_type)
            || (block_type == "narrative" && !prose_numerals(block).is_empty());

        if needs_refs && !has_refs {
            failures.push(Failure {
                rule: "§1",
                block_id: Some(id),
                detail: format!("blok `{block_type}` menyajikan data tanpa `derived_from`"),
            });
        }
    }

    failures
}

/// §7 — `chart_spec` mendeklarasikan bentuk data yang dibutuhkannya, dan data
/// rujukannya di ledger wajib memenuhinya. Chart yang tidak dapat dibuktikan —
/// bentuk tak dideklarasikan, data tidak ada di ledger, atau data tidak
/// kompatibel — dibuang; tabel yang memuat angkanya tetap tersaji.
fn chart_failures(blocks: &[Value], ledger: &Ledger) -> Vec<Failure> {
    blocks
        .iter()
        .filter(|block| block.get("type").and_then(Value::as_str) == Some("chart_spec"))
        .filter_map(|block| {
            let problem = chart_problem(block, ledger)?;
            Some(Failure {
                rule: "§7",
                block_id: Some(block_id(block)),
                detail: format!(
                    "chart_spec tidak memenuhi bentuk yang dideklarasikannya: {problem}"
                ),
            })
        })
        .collect()
}

fn chart_problem(block: &Value, ledger: &Ledger) -> Option<String> {
    if block.get("chart_type").and_then(Value::as_str) != Some("time_series") {
        return Some("chart_type bukan time_series".to_string());
    }
    let Some(time_dimension) = block.get("time_dimension").and_then(Value::as_str) else {
        return Some("time_dimension tidak dideklarasikan".to_string());
    };

    let refs = block.get("derived_from").and_then(Value::as_array)?;
    refs.iter().find_map(|reference| {
        let Some(rows) = contributor_key(reference).and_then(|key| ledger.outputs.get(&key)) else {
            return Some("data rujukan tidak ada di ledger".to_string());
        };
        compose::time_series_incompatibility(time_dimension, rows).map(str::to_string)
    })
}

/// Versi konservatif yang menggantikan dokumen yang ditolak (§6).
///
/// **Deterministik, dan itu wajib**: jalur kegagalan tidak boleh bergantung
/// pada komponen yang barusan gagal. Tidak ada model di sini — blok yang gagal
/// dibuang, sisanya dipertahankan apa adanya, satu blok `limitation` menyebut
/// apa yang dibuang beserta alasannya, dan `completeness` dihitung ulang **atas
/// blok yang tersisa** (§6), bukan atas dokumen yang sudah tidak ada.
fn fallback(rejected: &SettledResponse, failures: &[Failure], ledger: &Ledger) -> SettledResponse {
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
    kept.push(compose::block(
        "validation_rejected",
        "limitation",
        &[],
        json!({
            "title": "Part of this answer was withheld",
            "body": format!(
                "The composed answer failed {} check(s) against the durable ledger, so {} block(s) were dropped and the remaining claim was recomputed.",
                failures.len(),
                dropped.len()
            ),
            "failed_rules": rules,
            "failures": failures,
        }),
    ));

    let has_data = kept.iter().any(|block| {
        block
            .get("type")
            .and_then(Value::as_str)
            .is_some_and(|block_type| DATA_BLOCKS.contains(&block_type))
    });

    // §6 — dihitung ulang atas blok yang TERSISA. Blok yang dibuang tidak lagi
    // menyumbang klaim apa pun, dan blok yang bertahan tetap menyumbangnya.
    let recomputed = aggregate(&per_block_completeness(&kept, ledger));
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
            worst([recomputed, rejected.completeness])
        } else {
            "Unknown"
        },
        completeness_reason: format!("validation_failed:{}", rules.join("+")),
        response_hash: hex::encode(Sha256::digest(blocks.to_string().as_bytes())),
        // Lineage bertahan apa adanya: versi fallback menjawab pertanyaan yang
        // sama dari operasi yang sama, dan investigasi butuh jejak itu justru
        // ketika dokumennya ditolak.
        evidence: rejected.evidence.clone(),
        blocks,
    }
}

fn block_id(block: &Value) -> String {
    block
        .get("block_id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// Slot auto-bind yang benar-benar diungkap dokumen.
///
/// **Hanya blok `note`** (§5). Blok lain yang kebetulan membawa
/// `auto_bound_slots` tidak dihitung: memeriksa "blok apa pun yang membawa
/// field ini" berarti pemeriksaannya lolos terhadap bentuk yang salah, dan
/// itulah penyimpangan yang ditemukan audit.
fn disclosed_slots(blocks: &[Value]) -> BTreeSet<String> {
    blocks
        .iter()
        .filter(|block| block.get("type").and_then(Value::as_str) == Some("note"))
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

/// Daftar pengecualian §4 — **tertutup**.
///
/// Yang diambil dari ledger: angka di dalam nilai parameter yang benar-benar
/// dikonsumsi node (tahun dan tanggal periode). Angka pada label yang disalin
/// apa adanya dari evidence sudah tercakup jalur lain: label itu hadir sebagai
/// field struktural pada blok `note`, yang memang menggrounding.
///
/// ponytail: "nomor urut blok" sengaja TIDAK dikecualikan. Mengecualikan
/// bilangan kecil 1..n akan mengesahkan "3 nasabah cocok" pada dokumen
/// berblok tiga, dan arah kesalahannya adalah arah yang salah. Konsekuensinya
/// validator di sini lebih ketat daripada §4 — bukan lebih longgar. Kalau
/// kelak ada prosa yang benar-benar menyebut nomor urut blok, tambahkan
/// pengecualian yang mensyaratkan konteks ("blok 2"), bukan angkanya saja.
fn excluded_numerals(parameters: &[Value]) -> BTreeSet<String> {
    let mut excluded = BTreeSet::new();

    for parameter in parameters {
        if let Some(value) = parameter.get("value") {
            collect(value, &mut excluded);
        }
    }

    excluded
}

/// Kumpulkan bentuk kanonik tiap angka, melewati field prosa dan field
/// struktural.
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
            .filter(|(key, _)| {
                !PROSE_KEYS.contains(&key.as_str()) && !STRUCTURAL_KEYS.contains(&key.as_str())
            })
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
    use uuid::Uuid;

    use super::*;

    fn node(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    /// Ledger dengan satu kontributor bernama, seperti `job_node_runs` satu
    /// node hari ini.
    fn ledger(completeness: &str) -> Ledger {
        Ledger {
            contributors: BTreeMap::from([(node(1).to_string(), completeness.to_string())]),
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
            evidence: json!({ "lineage": [], "derivations": [] }),
            blocks,
        }
    }

    fn metric(value: Value) -> Value {
        metric_from(value, node(1))
    }

    fn metric_from(value: Value, from: Uuid) -> Value {
        compose::block(
            "metric:total",
            "metric",
            &[compose::from_node(from)],
            json!({ "key": "total", "value": value, "unit": Value::Null, "period": Value::Null }),
        )
    }

    fn narrative(body: &str) -> Value {
        compose::block(
            "summary",
            "narrative",
            &[compose::from_node(node(1))],
            json!({ "body": body }),
        )
    }

    fn note(field_id: &str) -> Value {
        compose::block(
            "slots_auto_bound",
            "note",
            &[],
            json!({ "auto_bound_slots": [{ "field_id": field_id, "label": Value::Null }] }),
        )
    }

    fn rules(validated: &Validated) -> Vec<String> {
        validated.report["failures"]
            .as_array()
            .unwrap()
            .iter()
            .map(|failure| failure["rule"].as_str().unwrap().to_string())
            .collect()
    }

    // --- §1 / §2: bentuk dan kosakata ---

    /// RESP-8.10 — tipe blok yang tidak dikenal klien dilewati tanpa merusak
    /// render. Konsekuensinya (§1) server tidak boleh memancarkannya sama
    /// sekali: informasi yang wajib sampai tidak boleh hidup di tipe yang
    /// dilewati. Kosakata §2 karena itu ditegakkan di sisi server.
    #[test]
    fn resp_8_10_a_type_outside_the_vocabulary_is_rejected() {
        let invented = compose::block("evidence", "provenance", &[], json!({ "query_id": "x" }));
        let validated = apply(
            response("Complete", json!([metric(json!(10)), invented])),
            &ledger("Complete"),
        );

        assert_eq!(validated.status(), "fallback");
        assert!(rules(&validated).contains(&"§2".to_string()));
        // Blok di luar kosakata dibuang, bukan diteruskan.
        let kept: Vec<&str> = validated
            .served
            .blocks
            .as_array()
            .unwrap()
            .iter()
            .map(|block| block["block_id"].as_str().unwrap())
            .collect();
        assert_eq!(kept, vec!["metric:total", "validation_rejected"]);
    }

    #[test]
    fn a_block_without_block_id_or_schema_version_is_rejected() {
        let malformed = json!({ "type": "metric", "id": "result", "value": 10 });
        // Klaim `Unknown` mengisolasi §1: blok malformed tak menyumbang
        // kontributor, jadi `Complete` di atasnya juga akan memicu D1 — di sini
        // yang diuji adalah penolakan bentuk, bukan interaksi D1 itu.
        let validated = apply(
            response("Unknown", json!([malformed])),
            &ledger("Complete"),
        );

        let detail = validated.report["failures"].to_string();
        assert!(rules(&validated).iter().all(|rule| rule == "§1"), "{detail}");
        assert!(detail.contains("block_id"), "{detail}");
        assert!(detail.contains("schema_version"), "{detail}");
        assert!(detail.contains("derived_from"), "{detail}");
    }

    #[test]
    fn a_narrative_without_numerals_needs_no_derived_from() {
        let plain = compose::block("empty", "narrative", &[], json!({ "body": "No rows." }));
        let validated = apply(response("Unknown", json!([plain])), &ledger("Complete"));

        assert_eq!(validated.status(), "passed");
    }

    // --- D1 ---

    #[test]
    fn worst_of_nothing_is_unknown_not_complete() {
        assert_eq!(worst(std::iter::empty()), "Unknown");
        assert_eq!(worst(["Complete", "Partial"]), "Partial");
        assert_eq!(worst(["Partial", "Unknown"]), "Unknown");
        assert_eq!(worst(["Complete", "Complete"]), "Complete");
    }

    /// RESP-8.1 — response `Complete` ditolak bila salah satu node
    /// kontributornya `Partial`.
    #[test]
    fn resp_8_1_claiming_complete_over_a_partial_contributor_is_rejected() {
        let validated = apply(
            response("Complete", json!([metric(json!(10))])),
            &ledger("Partial"),
        );

        assert_eq!(validated.status(), "fallback");
        assert_eq!(validated.report["failures"][0]["rule"], "D1");
        // Dihitung PER BLOK lewat derived_from, lalu diagregasi (§3 aturan 2–3).
        assert_eq!(validated.report["block_completeness"]["metric:total"], "Partial");
        assert_eq!(validated.report["computed_completeness"], "Partial");
        // Dokumen yang ditolak TETAP ada sebagai bahan investigasi (#10).
        assert_eq!(validated.rejected.unwrap().completeness, "Complete");
        // Yang disajikan dihitung ulang, bukan diperbaiki sebagian.
        assert_eq!(validated.served.completeness, "Partial");
    }

    /// RESP-8.1 — satu blok `Partial` menjatuhkan klaim dokumen meski blok lain
    /// `Complete`. Inilah yang tidak dapat ditangkap perhitungan tingkat
    /// dokumen: blok yang benar tetap disajikan, klaimnya yang turun.
    #[test]
    fn resp_8_1_the_worst_block_decides_the_document() {
        let ledger = Ledger {
            contributors: BTreeMap::from([
                (node(1).to_string(), "Complete".to_string()),
                (node(2).to_string(), "Partial".to_string()),
            ]),
            ..Ledger::default()
        };

        // Dua blok BERBEDA: satu Complete (node 1), satu Partial (node 2).
        let complete = metric(json!(10));
        let partial = compose::block(
            "metric:secondary",
            "metric",
            &[compose::from_node(node(2))],
            json!({ "key": "secondary", "value": 20, "unit": Value::Null, "period": Value::Null }),
        );
        let blocks = json!([complete, partial]);
        let validated = apply(response("Complete", blocks), &ledger);

        assert_eq!(validated.report["block_completeness"]["metric:total"], "Complete");
        assert_eq!(validated.report["block_completeness"]["metric:secondary"], "Partial");
        assert_eq!(validated.report["computed_completeness"], "Partial");
        assert_eq!(validated.report["failures"][0]["rule"], "D1");
    }

    #[test]
    fn a_block_deriving_from_something_the_ledger_never_recorded_is_unknown() {
        let blocks = json!([metric_from(json!(10), node(99))]);
        let validated = apply(response("Complete", blocks), &ledger("Complete"));

        assert_eq!(validated.report["computed_completeness"], "Unknown");
        assert_eq!(validated.report["failures"][0]["rule"], "D1");
    }

    #[test]
    fn claiming_worse_than_the_ledger_is_accepted() {
        let validated = apply(
            response("Unknown", json!([metric(json!(10))])),
            &ledger("Complete"),
        );

        assert_eq!(validated.status(), "passed");
        assert!(validated.rejected.is_none());
    }

    #[test]
    fn fallback_never_improves_a_claim_the_composer_made_worse() {
        // D3 gagal, tetapi klaim composer (`Partial`) lebih buruk daripada
        // hitungan ledger (`Complete`) — fallback tidak boleh menaikkannya.
        let validated = apply(
            response("Partial", json!([metric(json!(10)), narrative("naik 42%")])),
            &ledger("Complete"),
        );

        assert_eq!(validated.status(), "fallback");
        assert_eq!(validated.served.completeness, "Partial");
    }

    #[test]
    fn a_non_complete_claim_without_a_reason_is_rejected() {
        let mut document = response("Partial", json!([metric(json!(10))]));
        document.completeness_reason = "   ".into();

        let validated = apply(document, &ledger("Partial"));
        assert_eq!(validated.report["failures"][0]["rule"], "D1");
    }

    // --- D2 ---

    /// RESP-8.4 — slot auto-bind yang tidak diungkap menyebabkan validasi
    /// gagal.
    #[test]
    fn resp_8_4_an_auto_bound_slot_the_document_never_discloses_is_rejected() {
        let ledger = Ledger {
            auto_bound: BTreeSet::from(["client_id".to_string()]),
            ..ledger("Complete")
        };

        let validated = apply(response("Complete", json!([metric(json!(10))])), &ledger);

        assert_eq!(validated.status(), "fallback");
        assert_eq!(validated.report["failures"][0]["rule"], "D2");
        assert!(
            validated.report["failures"][0]["detail"]
                .as_str()
                .unwrap()
                .contains("client_id")
        );
    }

    /// RESP-8.4 — pengungkapan hanya sah pada blok `note` (§5). Blok lain yang
    /// membawa field yang sama tidak dihitung: pemeriksaan yang lolos terhadap
    /// bentuk yang salah bukan pemeriksaan.
    #[test]
    fn resp_8_4_disclosure_outside_the_note_block_does_not_count() {
        let ledger = Ledger {
            auto_bound: BTreeSet::from(["client_id".to_string()]),
            ..ledger("Complete")
        };

        let elsewhere = compose::block(
            "slots_auto_bound",
            "limitation",
            &[],
            json!({ "auto_bound_slots": [{ "field_id": "client_id" }] }),
        );

        let validated = apply(
            response("Complete", json!([metric(json!(10)), elsewhere])),
            &ledger,
        );
        assert_eq!(validated.report["failures"][0]["rule"], "D2");
    }

    #[test]
    fn resp_8_4_a_note_block_matching_the_ledger_exactly_passes() {
        let ledger = Ledger {
            auto_bound: BTreeSet::from(["client_id".to_string()]),
            ..ledger("Complete")
        };

        let blocks = json!([metric(json!(10)), note("client_id")]);
        assert_eq!(apply(response("Complete", blocks), &ledger).status(), "passed");
    }

    #[test]
    fn disclosing_a_slot_the_ledger_never_bound_is_also_rejected() {
        let blocks = json!([metric(json!(10)), note("office_id")]);

        let validated = apply(response("Complete", blocks), &ledger("Complete"));
        assert_eq!(validated.report["failures"][0]["rule"], "D2");
    }

    // --- D3 ---

    /// RESP-8.2 — narasi dengan angka yang tidak ada di blok mana pun dan tanpa
    /// `derivation` ditolak.
    #[test]
    fn resp_8_2_a_narrative_number_that_no_block_carries_is_rejected() {
        let blocks = json!([
            metric(json!("1500.00")),
            narrative("Total deposits reached 9999 this period."),
        ]);

        let validated = apply(response("Complete", blocks), &ledger("Complete"));

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
            .map(|block| block["block_id"].as_str().unwrap())
            .collect();
        assert_eq!(kept, vec!["metric:total", "validation_rejected"]);
    }

    #[test]
    fn display_formatting_of_a_grounded_number_is_accepted() {
        // Satu angka, empat penulisan: nilai mentah, pemisah ribuan Inggris,
        // pemisah ribuan Indonesia, dan nol desimal yang tidak bermakna.
        let blocks = json!([
            metric(json!("1500.00")),
            narrative("Total 1,500 — atau 1.500, tepatnya 1500.0."),
        ]);

        assert_eq!(
            apply(response("Complete", blocks), &ledger("Complete")).status(),
            "passed"
        );
    }

    #[test]
    fn a_number_only_a_limitation_sentence_mentions_does_not_ground_anything() {
        // Prosa deterministik server tidak boleh mengesahkan angka narasi.
        let limitation = compose::block(
            "pii_withheld",
            "limitation",
            &[],
            json!({ "body": "3 column(s) were withheld." }),
        );
        let blocks = json!([
            metric(json!(10)),
            limitation,
            narrative("We found 3 matching clients."),
        ]);

        let validated = apply(response("Complete", blocks), &ledger("Complete"));
        assert_eq!(validated.report["failures"][0]["rule"], "D3");
    }

    #[test]
    fn schema_version_never_grounds_a_narrative_number() {
        // Regresi: `schema_version: 1` hadir di SETIAP blok. Bila ia ikut
        // menggrounding, narasi "1 nasabah" lolos selamanya.
        let blocks = json!([metric(json!(10)), narrative("Tepat 1 nasabah cocok.")]);

        let validated = apply(response("Complete", blocks), &ledger("Complete"));
        assert_eq!(validated.report["failures"][0]["rule"], "D3");
    }

    /// RESP-8.3 — angka turunan ber-`derivation` diterima.
    #[test]
    fn resp_8_3_a_declared_derivation_grounds_a_number_no_block_carries() {
        let ledger = Ledger {
            derivations: vec![json!({
                "id": "growth",
                "formula": "(b - a) / a",
                "inputs": ["metric:total"],
                "result": 12,
                "rounding": "0dp",
            })],
            ..ledger("Complete")
        };

        let blocks = json!([metric(json!(10)), narrative("Naik 12% dari periode lalu.")]);
        assert_eq!(apply(response("Complete", blocks), &ledger).status(), "passed");
    }

    /// §4 daftar pengecualian — tanggal periode yang sudah terikat parameter
    /// bukan klaim data.
    #[test]
    fn a_period_bound_in_the_ledger_parameters_is_excluded() {
        let ledger = Ledger {
            parameters: vec![json!({ "name": "from_date", "value": "2026-09-01" })],
            ..ledger("Complete")
        };

        let blocks = json!([metric(json!(10)), narrative("Periode mulai 2026.")]);
        assert_eq!(apply(response("Complete", blocks), &ledger).status(), "passed");
    }

    #[test]
    fn a_table_value_grounds_the_narrative_that_cites_it() {
        let table = compose::block(
            "result",
            "table",
            &[compose::from_node(node(1))],
            json!({ "columns": ["office", "total"], "rows": [["Head", 4217]], "row_count": 1 }),
        );
        let blocks = json!([table, narrative("The largest office holds 4217.")]);

        assert_eq!(
            apply(response("Complete", blocks), &ledger("Complete")).status(),
            "passed"
        );
    }

    #[test]
    fn the_deterministic_empty_narrative_carries_no_numerals() {
        // Regresi terhadap composer hari ini: narasi "nol baris" wajib lolos
        // D3 tanpa satu pun blok data untuk menggroundingnya.
        let blocks = json!([compose::block(
            "empty",
            "narrative",
            &[compose::from_node(node(1))],
            json!({ "body": "The approved query ran successfully and returned no rows." }),
        )]);

        assert_eq!(
            apply(response("Complete", blocks), &ledger("Complete")).status(),
            "passed"
        );
    }

    /// RESP-8.5 — versi `failed` tetap tersimpan setelah fallback disajikan,
    /// dan RESP-8.6 — fallback disusun tanpa memanggil model: fungsi ini murni,
    /// tanpa I/O, dan hasilnya sama untuk input yang sama.
    #[test]
    fn resp_8_5_and_8_6_the_rejected_version_survives_and_the_fallback_is_deterministic() {
        let blocks = json!([metric(json!(10)), narrative("Total tercatat 9999.")]);

        let first = apply(response("Complete", blocks.clone()), &ledger("Complete"));
        let second = apply(response("Complete", blocks), &ledger("Complete"));

        // RESP-8.5 — dokumen yang ditolak masih utuh dan dapat diinvestigasi.
        let rejected = first.rejected.as_ref().expect("versi failed tersimpan");
        assert_eq!(rejected.completeness, "Complete");
        assert!(rejected.blocks.to_string().contains("9999"));
        assert!(first.report["failures"].as_array().unwrap().len() == 1);

        // RESP-8.6 — dua penyusunan menghasilkan dokumen yang identik byte per
        // byte. Tidak ada model yang dapat menjanjikan itu.
        assert_eq!(first.served.response_hash, second.served.response_hash);
        assert_eq!(first.served.blocks, second.served.blocks);
    }

    #[test]
    fn dropping_every_data_block_turns_the_document_into_a_limitation() {
        let blocks = json!([narrative("Total tercatat 9999.")]);
        let validated = apply(response("Complete", blocks), &ledger("Complete"));

        assert_eq!(validated.served.kind, "limitation");
        assert_eq!(validated.served.outcome, "Unsupported");
        assert_eq!(validated.served.completeness, "Unknown");
        // Bukan dokumen kosong: alasannya ikut.
        assert_eq!(validated.served.blocks.as_array().unwrap().len(), 1);
        assert_eq!(validated.served.completeness_reason, "validation_failed:D3");
    }

    #[test]
    fn the_fallback_limitation_block_itself_obeys_the_block_contract() {
        let validated = apply(
            response("Complete", json!([metric(json!(10)), narrative("9999")])),
            &ledger("Complete"),
        );

        let block = validated.served.blocks.as_array().unwrap().last().unwrap().clone();
        assert_eq!(block["block_id"], "validation_rejected");
        assert_eq!(block["type"], "limitation");
        assert_eq!(block["schema_version"], BLOCK_SCHEMA_VERSION);
        // Revalidasi dokumen fallback tidak menemukan pelanggaran bentuk baru.
        assert!(shape_failures(validated.served.blocks.as_array().unwrap()).is_empty());
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
            response("Complete", json!([metric(json!(10)), narrative("9999")])),
            &ledger("Complete"),
        );

        assert_ne!(validated.served.response_hash, "hash");
    }
}
