//! Bentuk katalog seperti yang benar-benar ada di `knowledge/`.
//!
//! Field yang tidak dipakai validator sengaja tidak dideklarasikan — serde
//! mengabaikan key yang tidak dikenal, dan menyalin seluruh skema YAML ke Rust
//! hanya menciptakan dua sumber kebenaran yang harus dijaga tetap sama.

use std::collections::BTreeMap;

use serde::Deserialize;

/// Capability: prosa yang dibaca planner + rujukan ke query yang dieksekusi.
#[derive(Debug, Clone, Deserialize)]
pub struct Capability {
    pub id: String,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub query_id: Option<String>,
    #[serde(default)]
    pub examples: Vec<String>,
    #[serde(default)]
    pub supported_intents: Vec<String>,
    #[serde(default)]
    pub parameters: BTreeMap<String, CapabilityParameter>,
    #[serde(default)]
    pub request_shape: Option<RequestShape>,
    #[serde(default)]
    pub guards: BTreeMap<String, serde_yaml::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CapabilityParameter {
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub default: Option<serde_yaml::Value>,
    /// Batas atas yang dideklarasikan capability, mis. `limit`. Nilai default
    /// yang melebihi cap dipotong ke cap — cap adalah janji, bukan saran.
    #[serde(default)]
    pub hard_cap: Option<i64>,
    /// Slot yang diisi resolver terotorisasi, bukan teks bebas (K1).
    #[serde(default)]
    pub probe: Option<ProbeRef>,
}

/// `probe:` pada parameter capability — shape dataset yang menerbitkan opsi
/// untuk slot ini.
#[derive(Debug, Clone, Deserialize)]
pub struct ProbeRef {
    pub dataset_id: String,
    pub shape_id: String,
    /// Kolom hasil resolver yang menjadi nilai binding slot.
    pub output_slot: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RequestShape {
    /// `none` berarti capability berjanji tidak mengeluarkan kolom PII.
    #[serde(default)]
    pub pii: Option<String>,
}

/// Manifest query: kontrak antara capability dan file SQL yang disetujui.
#[derive(Debug, Clone, Deserialize)]
pub struct QueryManifest {
    pub id: String,
    #[serde(default)]
    pub database: Option<String>,
    #[serde(default)]
    pub sql_file: Option<String>,
    #[serde(default)]
    pub parameters: Vec<QueryParameter>,
    #[serde(default)]
    pub output_fields: Vec<OutputField>,
    /// Grain hasil: himpunan minimal kolom `output_fields` yang mengidentifikasi
    /// satu baris hasil secara unik (#11). Query dengan join yang menggandakan
    /// baris harus menyatakan grainnya di sini, bukan membiarkannya tersirat.
    #[serde(default)]
    pub grain: Vec<String>,
    #[serde(default)]
    pub guards: QueryGuards,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    /// Shape dataset yang dibungkus manifest ini. Sebelum ini tautannya hanya
    /// hidup sebagai komentar prosa di tiap manifest resolver — persis kelas
    /// koneksi yang I6 menuntut ditegakkan mesin, bukan konvensi.
    #[serde(default)]
    pub resolves: Option<ResolvesShape>,
}

/// `resolves:` pada manifest query.
#[derive(Debug, Clone, Deserialize)]
pub struct ResolvesShape {
    pub dataset_id: String,
    pub shape_id: String,
}

/// `knowledge/datasets/**`. Hanya bagian yang dipakai resolver yang
/// dideklarasikan; sisanya (filters, fragment, recipe) belum punya konsumen.
#[derive(Debug, Clone, Deserialize)]
pub struct Dataset {
    pub id: String,
    #[serde(default)]
    pub entity: Option<DatasetEntity>,
    #[serde(default)]
    pub shapes: Vec<DatasetShape>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatasetEntity {
    /// Kolom yang menjadi `option_id` — identitas opsi yang diterbitkan.
    pub id_field: String,
    #[serde(default)]
    pub label_fields: Vec<String>,
    /// Label cadangan bila seluruh `label_fields` kosong/ditahan, mis.
    /// `"Client {client_id}"`. Tanpa ini sebuah opsi dapat terkirim tanpa teks
    /// apa pun untuk dipilih.
    #[serde(default)]
    pub label_fallback: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatasetShape {
    pub id: String,
    #[serde(default)]
    pub role: Option<String>,
    /// Batas baris yang dideklarasikan shape. Dipakai sebagai batas kandidat
    /// resolver; kelebihannya dinyatakan sebagai truncation (I5), bukan dibuang.
    #[serde(default)]
    pub row_cap: Option<usize>,
    #[serde(default)]
    pub produces: Vec<ProducedSlot>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProducedSlot {
    pub slot: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QueryParameter {
    pub name: String,
    /// Tipe pada YAML. `type` adalah kata kunci Rust, jadi field-nya bernama
    /// `kind` dan dipetakan lewat serde.
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub required: bool,
    /// `authorized_scope` berarti nilainya berasal dari otorisasi, bukan dari
    /// input pengguna (I7).
    #[serde(default)]
    pub source: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OutputField {
    pub name: String,
    #[serde(default)]
    pub sensitivity: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct QueryGuards {
    #[serde(default)]
    pub select_only: Option<bool>,
    #[serde(default)]
    pub single_statement: Option<bool>,
    #[serde(default)]
    pub require_office_filter: Option<bool>,
    #[serde(default)]
    pub parameterized_only: Option<bool>,
}

/// `knowledge/schema/fineract/columns/sensitivity.yaml`.
///
/// Hanya nama kelasnya yang dipakai validator: check file itu sendiri berbunyi
/// "every query output field must declare one class listed here".
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SensitivityClasses {
    #[serde(default)]
    pub classes: BTreeMap<String, serde_yaml::Value>,
}

/// `knowledge/policies/query_safety.yaml`.
///
/// Daftar perintah terlarang dibaca dari file, tidak ditanam di kode: kebijakan
/// yang disalin ke dua tempat akan menyimpang, dan yang menyimpang diam-diam
/// adalah yang di kode.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SafetyPolicy {
    #[serde(default)]
    pub unsafe_commands: Vec<String>,
}
