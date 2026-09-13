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
    #[serde(default)]
    pub guards: QueryGuards,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QueryParameter {
    pub name: String,
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
