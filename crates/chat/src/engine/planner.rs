//! Planner deterministik: memilih satu capability yang disetujui dan mengikat
//! parameternya.
//!
//! Tidak ada model di sini. Pemilihan memakai retrieval leksikal atas
//! `knowledge_index` — teks yang diindeks berasal dari prosa capability, bukan
//! dari SQL — dan pengikatan parameter hanya memakai default yang **sudah
//! dideklarasikan** katalog. Bila sebuah parameter wajib tidak dapat diisi
//! secara deterministik, hasilnya `Unsupported`; menebak nilainya berarti
//! menjawab pertanyaan yang tidak ditanyakan.
//!
//! Konsekuensinya jelas dan disengaja: pertanyaan yang tidak cocok dengan satu
//! pun capability tidak dijawab, bukan dijawab seadanya.

use chrono::{Datelike, NaiveDate, Utc};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

use crate::catalog::{
    loader::Catalog,
    model::{Capability, QueryManifest},
};

/// Nilai parameter yang siap di-bind ke SQL, dalam urutan deklarasi manifest.
#[derive(Debug, Clone, PartialEq)]
pub enum Bound {
    Date(NaiveDate),
    OfficeIds(Vec<i64>),
    Bigint(i64),
    /// Parameter opsional yang tidak diisi. SQL menanganinya lewat pola
    /// `($n IS NULL OR ...)`.
    NullText,
    NullBigintArray,
    NullBigint,
}

/// Rencana untuk satu node `CuratedQuery`.
#[derive(Debug, Clone)]
pub struct Plan {
    pub capability_id: String,
    pub query_id: String,
    pub sql: String,
    pub sql_file: String,
    pub parameters: Vec<Bound>,
    pub parameter_names: Vec<String>,
    pub output_fields: Vec<String>,
    /// Kelas sensitivitas per kolom hasil, sejajar `output_fields`. Dibawa
    /// sampai komposisi karena penahanan PII diputuskan di sana.
    pub output_sensitivity: Vec<String>,
    pub timeout_ms: u64,
    pub catalog_version_id: Uuid,
    pub catalog_content_hash: String,
    pub retrieval_score: f32,
    pub graph_json: Value,
    pub graph_hash: String,
}

/// Kenapa sebuah permintaan tidak dapat direncanakan. Setiap varian menjadi
/// alasan yang dinyatakan pada response — tidak ada kegagalan senyap (I5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unplannable {
    /// Tidak ada capability yang cukup dekat dengan permintaan.
    NoCapabilityMatched,
    /// Capability terpilih merujuk query yang tidak ada di katalog.
    QueryMissing(String),
    /// Parameter wajib tidak dapat diisi tanpa bertanya lebih dulu.
    ParameterNeedsClarification { capability: String, parameter: String },
    /// Tipe parameter belum didukung binder deterministik.
    ParameterUnsupported { capability: String, parameter: String, kind: String },
}

impl Unplannable {
    /// Alasan pendek yang aman ditulis ke `completeness_reason`.
    pub fn reason(&self) -> String {
        match self {
            Self::NoCapabilityMatched => "no_capability_matched".to_string(),
            Self::QueryMissing(_) => "capability_query_missing".to_string(),
            Self::ParameterNeedsClarification { .. } => "parameter_needs_clarification".to_string(),
            Self::ParameterUnsupported { .. } => "parameter_binding_unsupported".to_string(),
        }
    }

    /// Kalimat untuk blok `limitation`. Bahasa Inggris: teks produk publik.
    pub fn explain(&self) -> String {
        match self {
            Self::NoCapabilityMatched => {
                "No approved capability covers this request, so Jarvis will not answer it."
                    .to_string()
            }
            Self::QueryMissing(query_id) => format!(
                "The selected capability refers to query '{query_id}', which is not present in the approved catalog."
            ),
            Self::ParameterNeedsClarification { capability, parameter } => format!(
                "Capability '{capability}' requires '{parameter}', and that value cannot be \
                 derived from the request. Asking follow-up questions is not implemented yet."
            ),
            Self::ParameterUnsupported { capability, parameter, kind } => format!(
                "Capability '{capability}' declares parameter '{parameter}' of type '{kind}', \
                 which the deterministic planner cannot bind."
            ),
        }
    }
}

/// Susun rencana untuk sebuah permintaan.
pub async fn plan(
    pool: &PgPool,
    catalog: &Catalog,
    catalog_version_id: Uuid,
    request_text: &str,
    authorized_office_ids: &[i64],
) -> sqlx::Result<Result<Plan, Unplannable>> {
    let Some((capability_id, score)) = best_capability(pool, catalog_version_id, request_text).await?
    else {
        return Ok(Err(Unplannable::NoCapabilityMatched));
    };

    let Some(capability) = catalog
        .capabilities
        .iter()
        .map(|loaded| &loaded.entry)
        .find(|capability| capability.id == capability_id)
    else {
        // Indeks menunjuk entri yang tidak ada di katalog yang dimuat: versi
        // indeks dan versi disk berbeda.
        return Ok(Err(Unplannable::NoCapabilityMatched));
    };

    let Some(query_id) = capability.query_id.clone() else {
        return Ok(Err(Unplannable::QueryMissing(capability.id.clone())));
    };

    let Some(query) = catalog
        .queries
        .iter()
        .map(|loaded| &loaded.entry)
        .find(|query| query.id == query_id)
    else {
        return Ok(Err(Unplannable::QueryMissing(query_id)));
    };

    let Some(sql_file) = query.sql_file.clone() else {
        return Ok(Err(Unplannable::QueryMissing(query_id)));
    };

    let Some(sql) = catalog.sql_files.get(&sql_file).cloned() else {
        return Ok(Err(Unplannable::QueryMissing(query_id)));
    };

    let parameters = match bind_parameters(capability, query, authorized_office_ids) {
        Ok(parameters) => parameters,
        Err(problem) => return Ok(Err(problem)),
    };

    let graph_json = serde_json::json!({
        "nodes": [{
            "node_id": "main",
            "kind": "CuratedQuery",
            "capability_id": capability.id,
            "query_id": query.id,
            "depends_on": [],
        }],
    });

    Ok(Ok(Plan {
        capability_id: capability.id.clone(),
        query_id: query.id.clone(),
        parameter_names: query
            .parameters
            .iter()
            .map(|parameter| parameter.name.clone())
            .collect(),
        output_fields: query
            .output_fields
            .iter()
            .map(|field| field.name.clone())
            .collect(),
        output_sensitivity: query
            .output_fields
            .iter()
            .map(|field| {
                // Kolom tanpa kelas diperlakukan sebagai `pii`: fail closed.
                // Validator katalog sudah menolak keadaan ini, tetapi default
                // yang aman tidak boleh bergantung pada validator yang lain.
                field.sensitivity.clone().unwrap_or_else(|| "pii".to_string())
            })
            .collect(),
        // Manifest tanpa timeout sudah dilaporkan validator; di sini dipakai
        // kelas probe yang lebih ketat, bukan yang lebih longgar.
        timeout_ms: query.timeout_ms.unwrap_or(3_000),
        catalog_version_id,
        catalog_content_hash: catalog.content_hash.clone(),
        retrieval_score: score,
        graph_hash: hex::encode(Sha256::digest(graph_json.to_string().as_bytes())),
        graph_json,
        parameters,
        sql,
        sql_file,
    }))
}

/// Retrieval leksikal. Arm embedding belum ada; bila kelak ditambahkan, ia
/// fail-closed ke arm ini saat model/dimensi tidak cocok (#7).
async fn best_capability(
    pool: &PgPool,
    catalog_version_id: Uuid,
    request_text: &str,
) -> sqlx::Result<Option<(String, f32)>> {
    sqlx::query_as::<_, (String, f32)>(
        "SELECT source_id,
                ts_rank(to_tsvector('simple', retrieval_text),
                        plainto_tsquery('simple', $2)) AS score
         FROM knowledge_index
         WHERE catalog_version_id = $1
           AND source_type = 'capability'
           AND to_tsvector('simple', retrieval_text) @@ plainto_tsquery('simple', $2)
         ORDER BY score DESC, source_id
         LIMIT 1",
    )
    .bind(catalog_version_id)
    .bind(request_text)
    .fetch_optional(pool)
    .await
}

/// Ikat parameter query memakai default yang dideklarasikan capability.
///
/// Urutannya mengikuti deklarasi manifest, karena itulah urutan `$1..$n` pada
/// SQL — validator katalog sudah memastikan keduanya cocok.
fn bind_parameters(
    capability: &Capability,
    query: &QueryManifest,
    authorized_office_ids: &[i64],
) -> Result<Vec<Bound>, Unplannable> {
    let today = Utc::now().date_naive();
    let mut bound = Vec::with_capacity(query.parameters.len());

    for parameter in &query.parameters {
        let declared = capability.parameters.get(&parameter.name);
        let default = declared
            .and_then(|declared| declared.default.as_ref())
            .and_then(|value| value.as_str());

        let value = match (parameter.source.as_deref(), default) {
            // Scope selalu dari otorisasi, tidak pernah dari input (I7).
            (Some("authorized_scope"), _) | (_, Some("authorized_scope")) => {
                Bound::OfficeIds(authorized_office_ids.to_vec())
            }
            (_, Some("business_today")) => Bound::Date(today),
            (_, Some("start_of_month(business_today)")) => {
                Bound::Date(today.with_day(1).unwrap_or(today))
            }
            // Default literal berupa angka, mis. `limit: "10"`. Dipotong ke
            // `hard_cap` bila ada: cap yang dideklarasikan capability adalah
            // janji kepada sumber data, bukan saran.
            (_, Some(literal))
                if matches!(parameter.kind.as_str(), "integer" | "bigint")
                    && literal.parse::<i64>().is_ok() =>
            {
                let value = literal.parse::<i64>().expect("sudah diperiksa di guard");
                let capped = declared
                    .and_then(|declared| declared.hard_cap)
                    .map_or(value, |cap| value.min(cap));
                Bound::Bigint(capped)
            }
            // `unbounded` berarti "tanpa batas", yaitu NULL pada pola
            // `($n IS NULL OR ...)` — bukan nilai yang dikarang.
            (_, Some("unbounded")) | (_, None) => match null_for(&parameter.kind) {
                Some(null) if !parameter.required || declared.is_some() => null,
                Some(_) | None if parameter.required => {
                    return Err(Unplannable::ParameterNeedsClarification {
                        capability: capability.id.clone(),
                        parameter: parameter.name.clone(),
                    });
                }
                Some(null) => null,
                None => {
                    return Err(Unplannable::ParameterUnsupported {
                        capability: capability.id.clone(),
                        parameter: parameter.name.clone(),
                        kind: parameter.kind.clone(),
                    });
                }
            },
            (_, Some(_)) => {
                return Err(Unplannable::ParameterUnsupported {
                    capability: capability.id.clone(),
                    parameter: parameter.name.clone(),
                    kind: parameter.kind.clone(),
                });
            }
        };

        bound.push(value);
    }

    Ok(bound)
}

fn null_for(kind: &str) -> Option<Bound> {
    match kind {
        "string" => Some(Bound::NullText),
        "array_bigint" => Some(Bound::NullBigintArray),
        "bigint" | "integer" => Some(Bound::NullBigint),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::model::{CapabilityParameter, QueryParameter};
    use std::collections::BTreeMap;

    fn capability(parameters: &[(&str, &str)]) -> Capability {
        capability_with_cap(parameters, None)
    }

    fn capability_with_cap(parameters: &[(&str, &str)], hard_cap: Option<i64>) -> Capability {
        let mut declared = BTreeMap::new();
        for (name, default) in parameters {
            declared.insert(
                name.to_string(),
                CapabilityParameter {
                    required: false,
                    default: Some(serde_yaml::Value::String(default.to_string())),
                    hard_cap,
                },
            );
        }

        Capability {
            id: "savings_deposit_total".into(),
            status: None,
            domain: None,
            display_name: None,
            description: None,
            query_id: Some("savings.deposit_total".into()),
            examples: Vec::new(),
            supported_intents: Vec::new(),
            parameters: declared,
            request_shape: None,
            guards: BTreeMap::new(),
        }
    }

    fn query(parameters: Vec<QueryParameter>) -> QueryManifest {
        QueryManifest {
            id: "savings.deposit_total".into(),
            database: None,
            sql_file: None,
            parameters,
            output_fields: Vec::new(),
            guards: Default::default(),
            timeout_ms: None,
        }
    }

    fn parameter(name: &str, kind: &str, required: bool, source: Option<&str>) -> QueryParameter {
        QueryParameter {
            name: name.into(),
            kind: kind.into(),
            required,
            source: source.map(str::to_string),
        }
    }

    #[test]
    fn binds_dates_and_scope_from_declared_defaults() {
        let capability = capability(&[
            ("from_date", "start_of_month(business_today)"),
            ("to_date", "business_today"),
            ("office_ids", "authorized_scope"),
        ]);
        let query = query(vec![
            parameter("from_date", "date", true, None),
            parameter("to_date", "date", true, None),
            parameter("office_ids", "array_bigint", true, Some("authorized_scope")),
        ]);

        let bound = bind_parameters(&capability, &query, &[1, 2]).unwrap();
        let today = Utc::now().date_naive();

        assert_eq!(bound.len(), 3);
        assert_eq!(bound[0], Bound::Date(today.with_day(1).unwrap()));
        assert_eq!(bound[1], Bound::Date(today));
        assert_eq!(bound[2], Bound::OfficeIds(vec![1, 2]));
    }

    #[test]
    fn optional_parameters_without_default_become_null() {
        let capability = capability(&[("office_ids", "authorized_scope")]);
        let query = query(vec![
            parameter("office_ids", "array_bigint", true, Some("authorized_scope")),
            parameter("currency_code", "string", false, None),
            parameter("product_ids", "array_bigint", false, None),
        ]);

        let bound = bind_parameters(&capability, &query, &[7]).unwrap();
        assert_eq!(bound[1], Bound::NullText);
        assert_eq!(bound[2], Bound::NullBigintArray);
    }

    #[test]
    fn required_parameter_without_derivable_value_is_not_guessed() {
        let capability = capability(&[]);
        let query = query(vec![parameter("account_number", "string", true, None)]);

        let problem = bind_parameters(&capability, &query, &[1]).unwrap_err();
        assert_eq!(
            problem,
            Unplannable::ParameterNeedsClarification {
                capability: "savings_deposit_total".into(),
                parameter: "account_number".into(),
            }
        );
        assert_eq!(problem.reason(), "parameter_needs_clarification");
    }

    #[test]
    fn numeric_literal_default_is_bound_and_capped() {
        let capability = capability_with_cap(&[("limit", "250")], Some(100));
        let query = query(vec![parameter("limit", "integer", true, None)]);

        let bound = bind_parameters(&capability, &query, &[1]).unwrap();
        // 250 melebihi hard_cap 100 → dipotong, bukan diteruskan apa adanya.
        assert_eq!(bound[0], Bound::Bigint(100));
    }

    #[test]
    fn numeric_literal_default_within_cap_is_kept() {
        let capability = capability_with_cap(&[("limit", "10")], Some(100));
        let query = query(vec![parameter("limit", "integer", true, None)]);

        assert_eq!(bind_parameters(&capability, &query, &[1]).unwrap()[0], Bound::Bigint(10));
    }

    #[test]
    fn scope_comes_from_authorization_even_when_capability_is_silent() {
        let capability = capability(&[]);
        let query = query(vec![parameter(
            "office_ids",
            "array_bigint",
            true,
            Some("authorized_scope"),
        )]);

        let bound = bind_parameters(&capability, &query, &[3, 4]).unwrap();
        assert_eq!(bound[0], Bound::OfficeIds(vec![3, 4]));
    }
}
