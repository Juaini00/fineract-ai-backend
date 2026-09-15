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

use std::collections::BTreeMap;

use chrono::{Datelike, NaiveDate, Utc};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    catalog::{
        loader::Catalog,
        model::{Capability, QueryManifest},
    },
    engine::resolver::ResolverSlot,
};

/// Nilai parameter yang siap di-bind ke SQL, dalam urutan deklarasi manifest.
#[derive(Debug, Clone, PartialEq)]
pub enum Bound {
    Date(NaiveDate),
    OfficeIds(Vec<i64>),
    Bigint(i64),
    Text(String),
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
    ///
    /// Seluruh parameter yang kurang dikumpulkan sekaligus: kontrak klarifikasi
    /// mewajibkan satu form memuat semua ambiguitas yang sudah diketahui, bukan
    /// satu pertanyaan per putaran.
    NeedsClarification { capability: String, missing: Vec<Missing> },
    /// Tipe parameter belum didukung binder deterministik.
    ParameterUnsupported { capability: String, parameter: String, kind: String },
}

/// Satu parameter yang harus ditanyakan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Missing {
    pub name: String,
    pub kind: String,
    /// Slot identitas tidak boleh diikat dari teks bebas (K1); ia menuntut
    /// resolver yang menerbitkan opsi. Dua sumbernya: manifest query menandai
    /// parameter `transient_sensitive_input`, atau capability mendeklarasikan
    /// `probe:` untuknya.
    pub identity: bool,
    /// Resolver yang menerbitkan opsi untuk slot ini. `None` pada slot identitas
    /// berarti slot itu **tidak dapat ditanyakan sama sekali**.
    pub resolver: Option<ResolverSlot>,
}

impl Missing {
    /// Tipe semantik field pada form (`clarifications.md` — Form model).
    ///
    /// Slot ber-resolver **selalu** `single_choice`: jawabannya adalah id opsi
    /// yang server terbitkan, bukan teks yang diketik pengguna.
    pub fn field_type(&self) -> &'static str {
        if self.resolver.is_some() {
            return "single_choice";
        }

        match self.kind.as_str() {
            "date" => "date",
            "integer" | "bigint" | "decimal" => "number",
            "boolean" => "boolean",
            _ => "text",
        }
    }

    /// Slot identitas yang tidak punya resolver: tidak dapat ditanyakan (K1).
    pub fn unanswerable(&self) -> bool {
        self.identity && self.resolver.is_none()
    }
}

impl Unplannable {
    /// Alasan pendek yang aman ditulis ke `completeness_reason`.
    pub fn reason(&self) -> String {
        match self {
            Self::NoCapabilityMatched => "no_capability_matched".to_string(),
            Self::QueryMissing(_) => "capability_query_missing".to_string(),
            Self::NeedsClarification { missing, .. } if missing.iter().any(Missing::unanswerable) => {
                "identity_slot_without_resolver".to_string()
            }
            Self::NeedsClarification { .. } => "parameter_needs_clarification".to_string(),
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
            Self::NeedsClarification { capability, missing } => {
                let blocked: Vec<&str> = missing
                    .iter()
                    .filter(|item| item.unanswerable())
                    .map(|item| item.name.as_str())
                    .collect();
                let names: Vec<&str> = missing.iter().map(|item| item.name.as_str()).collect();

                if blocked.is_empty() {
                    format!(
                        "Capability '{capability}' requires {} and those values cannot be derived \
                         from the request.",
                        names.join(", ")
                    )
                } else {
                    format!(
                        "Capability '{capability}' requires {}, and no approved resolver publishes \
                         options for it. An identity value is never bound from free text, so this \
                         request is not answered rather than answered from a guess.",
                        blocked.join(", ")
                    )
                }
            }
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
    // `supplied`: jawaban klarifikasi yang sudah diterima, dikunci nama parameter.
    supplied: &BTreeMap<String, String>,
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

    let parameters = match bind_parameters(catalog, capability, query, authorized_office_ids, supplied)
    {
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
    catalog: &Catalog,
    capability: &Capability,
    query: &QueryManifest,
    authorized_office_ids: &[i64],
    supplied: &BTreeMap<String, String>,
) -> Result<Vec<Bound>, Unplannable> {
    let today = Utc::now().date_naive();
    let mut bound = Vec::with_capacity(query.parameters.len());
    let mut missing: Vec<Missing> = Vec::new();

    for parameter in &query.parameters {
        // Jawaban klarifikasi menang atas default — itulah gunanya bertanya.
        // Scope TIDAK PERNAH diambil dari sini (I7): ia hanya berasal dari
        // otorisasi, dan pengguna tidak dapat memperluasnya lewat jawaban.
        if parameter.source.as_deref() != Some("authorized_scope")
            && let Some(answer) = supplied.get(&parameter.name)
        {
            match typed_answer(&parameter.kind, answer) {
                Some(value) => {
                    bound.push(value);
                    continue;
                }
                None => {
                    // Jawaban tersimpan tidak dapat diparse lagi: bentuknya
                    // berubah atau manifest berubah. Ditanyakan ulang, bukan
                    // ditebak.
                    missing.push(missing_parameter(catalog, capability, parameter));
                    bound.push(Bound::NullText);
                    continue;
                }
            }
        }

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
            // Offset relatif yang dideklarasikan katalog, mis.
            // `business_today - 12m`. Ditangani di sini karena katalog SUDAH
            // menjawabnya: menanyakannya kepada pengguna berarti bertanya
            // tentang sesuatu yang sudah diputuskan.
            (_, Some(expression)) if relative_date(expression, today).is_some() => {
                Bound::Date(relative_date(expression, today).expect("sudah diperiksa di guard"))
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
            // `unbounded` menyatakan "memang tanpa batas" — NULL yang disengaja,
            // dan SQL menanganinya lewat pola `($n IS NULL OR ...)`.
            (_, Some("unbounded")) => match null_for(&parameter.kind) {
                Some(null) => null,
                None => {
                    return Err(Unplannable::ParameterUnsupported {
                        capability: capability.id.clone(),
                        parameter: parameter.name.clone(),
                        kind: parameter.kind.clone(),
                    });
                }
            },
            // Tanpa default sama sekali. Parameter WAJIB di sini harus
            // ditanyakan, bukan diikat NULL: mendeklarasikan parameter tanpa
            // memberinya nilai bukan izin untuk mengosongkannya. Query yang
            // menerima NULL pada slot pencarian akan menjawab "tidak ada hasil"
            // untuk pertanyaan yang tidak pernah diajukan (I5).
            (_, None) if parameter.required => {
                missing.push(missing_parameter(catalog, capability, parameter));
                // Placeholder supaya posisi parameter tetap sejajar; pengikatan
                // gagal di akhir sehingga nilai ini tidak pernah dieksekusi.
                Bound::NullText
            }
            (_, None) => match null_for(&parameter.kind) {
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

    if !missing.is_empty() {
        return Err(Unplannable::NeedsClarification {
            capability: capability.id.clone(),
            missing,
        });
    }

    Ok(bound)
}

fn missing_parameter(
    catalog: &Catalog,
    capability: &Capability,
    parameter: &crate::catalog::model::QueryParameter,
) -> Missing {
    // Resolver dideklarasikan capability, bukan disimpulkan dari nama parameter:
    // menebak "yang bernama *_id pasti identitas" akan memperlakukan kolom
    // pencarian sebagai binding identitas dan sebaliknya.
    let resolver = capability
        .parameters
        .get(&parameter.name)
        .and_then(|declared| declared.probe.as_ref())
        .and_then(|probe| ResolverSlot::from_catalog(catalog, probe, &parameter.kind));

    let declared_probe = capability
        .parameters
        .get(&parameter.name)
        .is_some_and(|declared| declared.probe.is_some());

    Missing {
        name: parameter.name.clone(),
        kind: parameter.kind.clone(),
        // Dua sumber: manifest query menandainya input sensitif, atau capability
        // mendeklarasikan `probe:`. Selebihnya — termasuk kolom pencarian nama —
        // adalah masukan pencarian, bukan binding identitas.
        identity: parameter.source.as_deref() == Some("transient_sensitive_input") || declared_probe,
        resolver,
    }
}

/// Hitung `business_today - <N><unit>`.
///
/// Unit: `d` hari, `w` pekan, `m` bulan, `y` tahun. Bentuk lain menghasilkan
/// `None` — ekspresi yang tidak dikenal tidak pernah ditebak menjadi tanggal.
fn relative_date(expression: &str, today: NaiveDate) -> Option<NaiveDate> {
    let rest = expression.trim().strip_prefix("business_today")?.trim();
    let amount = rest.strip_prefix('-')?.trim();

    let split = amount.find(|character: char| !character.is_ascii_digit())?;
    let (count, unit) = amount.split_at(split);
    let count: i64 = count.parse().ok()?;

    match unit.trim() {
        "d" => today.checked_sub_signed(chrono::Duration::days(count)),
        "w" => today.checked_sub_signed(chrono::Duration::weeks(count)),
        "m" | "mo" => subtract_months(today, count),
        "y" => subtract_months(today, count.checked_mul(12)?),
        _ => None,
    }
}

/// Kurangi bulan tanpa pernah menghasilkan tanggal yang tidak ada: 31 Maret
/// dikurangi satu bulan menjadi 28/29 Februari, bukan gagal diam-diam.
fn subtract_months(date: NaiveDate, months: i64) -> Option<NaiveDate> {
    let total = date.year() as i64 * 12 + (date.month() as i64 - 1) - months;
    let year = i32::try_from(total.div_euclid(12)).ok()?;
    let month = total.rem_euclid(12) as u32 + 1;

    (0..4).find_map(|back| {
        NaiveDate::from_ymd_opt(year, month, date.day().checked_sub(back)?)
    })
}

/// Ubah jawaban bertipe menjadi nilai terikat. `None` berarti jawaban tidak
/// sesuai tipe yang dideklarasikan manifest.
pub fn typed_answer(kind: &str, answer: &str) -> Option<Bound> {
    match kind {
        "date" => NaiveDate::parse_from_str(answer.trim(), "%Y-%m-%d").ok().map(Bound::Date),
        "integer" | "bigint" => answer.trim().parse::<i64>().ok().map(Bound::Bigint),
        "string" => Some(Bound::Text(answer.trim().to_string())),
        _ => None,
    }
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

    /// Katalog kosong: test ini menguji pengikatan parameter, dan satu-satunya
    /// hal yang dibaca dari katalog adalah resolver — yang di sini memang tidak
    /// ada, sehingga slot identitas tampak sebagai tidak dapat ditanyakan.
    fn catalog() -> Catalog {
        Catalog {
            capabilities: Vec::new(),
            queries: Vec::new(),
            datasets: Vec::new(),
            safety_policy: Default::default(),
            sensitivity_classes: Default::default(),
            sql_files: Default::default(),
            content_hash: String::new(),
            unreadable: Vec::new(),
        }
    }

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
                    probe: None,
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
            resolves: None,
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

        let bound = bind_parameters(&catalog(), &capability, &query, &[1, 2], &BTreeMap::new()).unwrap();
        let today = Utc::now().date_naive();

        assert_eq!(bound.len(), 3);
        assert_eq!(bound[0], Bound::Date(today.with_day(1).unwrap()));
        assert_eq!(bound[1], Bound::Date(today));
        assert_eq!(bound[2], Bound::OfficeIds(vec![1, 2]));
    }

    #[test]
    fn declared_but_unfilled_required_parameter_is_asked_not_nulled() {
        // Capability mendeklarasikan `search` tetapi tidak memberinya nilai.
        // Mengikatnya NULL akan membuat query pencarian menjawab "tidak ada
        // hasil" untuk nama yang tidak pernah ditanyakan.
        let capability = Capability {
            parameters: BTreeMap::from([(
                "search".to_string(),
                CapabilityParameter { required: true, default: None, hard_cap: None, probe: None },
            )]),
            ..capability(&[])
        };
        let query = query(vec![parameter("search", "string", true, None)]);

        let problem = bind_parameters(&catalog(), &capability, &query, &[1], &BTreeMap::new()).unwrap_err();
        let Unplannable::NeedsClarification { missing, .. } = problem else {
            panic!("seharusnya ditanyakan");
        };
        assert_eq!(missing[0].name, "search");
    }

    #[test]
    fn optional_parameters_without_default_become_null() {
        let capability = capability(&[("office_ids", "authorized_scope")]);
        let query = query(vec![
            parameter("office_ids", "array_bigint", true, Some("authorized_scope")),
            parameter("currency_code", "string", false, None),
            parameter("product_ids", "array_bigint", false, None),
        ]);

        let bound = bind_parameters(&catalog(), &capability, &query, &[7], &BTreeMap::new()).unwrap();
        assert_eq!(bound[1], Bound::NullText);
        assert_eq!(bound[2], Bound::NullBigintArray);
    }

    #[test]
    fn required_parameter_without_derivable_value_is_not_guessed() {
        let capability = capability(&[]);
        let query = query(vec![parameter("account_number", "string", true, None)]);

        let problem = bind_parameters(&catalog(), &capability, &query, &[1], &BTreeMap::new()).unwrap_err();
        let Unplannable::NeedsClarification { missing, .. } = &problem else {
            panic!("seharusnya menuntut klarifikasi: {problem:?}");
        };
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].name, "account_number");
        assert_eq!(problem.reason(), "parameter_needs_clarification");
    }

    #[test]
    fn relative_date_default_is_computed_not_asked() {
        let today = NaiveDate::from_ymd_opt(2026, 9, 15).unwrap();

        assert_eq!(
            relative_date("business_today - 12m", today),
            Some(NaiveDate::from_ymd_opt(2025, 9, 15).unwrap())
        );
        assert_eq!(
            relative_date("business_today - 30d", today),
            Some(NaiveDate::from_ymd_opt(2026, 8, 16).unwrap())
        );
        assert_eq!(
            relative_date("business_today - 1y", today),
            Some(NaiveDate::from_ymd_opt(2025, 9, 15).unwrap())
        );
    }

    #[test]
    fn month_arithmetic_never_produces_a_date_that_does_not_exist() {
        // 31 Maret dikurangi satu bulan: 31 Februari tidak ada.
        let end_of_march = NaiveDate::from_ymd_opt(2026, 3, 31).unwrap();
        assert_eq!(
            relative_date("business_today - 1m", end_of_march),
            Some(NaiveDate::from_ymd_opt(2026, 2, 28).unwrap())
        );
    }

    #[test]
    fn unknown_relative_expression_is_never_guessed() {
        let today = NaiveDate::from_ymd_opt(2026, 9, 15).unwrap();

        assert_eq!(relative_date("business_today + 1m", today), None);
        assert_eq!(relative_date("kemarin", today), None);
        assert_eq!(relative_date("business_today - 12 lunar", today), None);
    }

    #[test]
    fn supplied_answer_wins_over_default() {
        let capability = capability(&[("from_date", "start_of_month(business_today)")]);
        let query = query(vec![parameter("from_date", "date", true, None)]);

        let mut supplied = BTreeMap::new();
        supplied.insert("from_date".to_string(), "2026-01-01".to_string());

        let bound = bind_parameters(&catalog(), &capability, &query, &[1], &supplied).unwrap();
        assert_eq!(bound[0], Bound::Date(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()));
    }

    #[test]
    fn supplied_answer_can_never_widen_scope() {
        let capability = capability(&[]);
        let query = query(vec![parameter(
            "office_ids",
            "array_bigint",
            true,
            Some("authorized_scope"),
        )]);

        let mut supplied = BTreeMap::new();
        supplied.insert("office_ids".to_string(), "1,2,3,4,5,6,7,8,9".to_string());

        // Scope tetap dari otorisasi (I7), jawaban diabaikan.
        let bound = bind_parameters(&catalog(), &capability, &query, &[3], &supplied).unwrap();
        assert_eq!(bound[0], Bound::OfficeIds(vec![3]));
    }

    #[test]
    fn all_missing_parameters_are_collected_into_one_question() {
        let capability = capability(&[]);
        let query = query(vec![
            parameter("from_date", "date", true, None),
            parameter("to_date", "date", true, None),
        ]);

        let problem = bind_parameters(&catalog(), &capability, &query, &[1], &BTreeMap::new()).unwrap_err();
        let Unplannable::NeedsClarification { missing, .. } = problem else {
            panic!("seharusnya menuntut klarifikasi");
        };

        // Satu form memuat seluruh ambiguitas yang sudah diketahui.
        assert_eq!(missing.len(), 2);
        assert_eq!(missing[0].field_type(), "date");
    }

    #[test]
    fn numeric_literal_default_is_bound_and_capped() {
        let capability = capability_with_cap(&[("limit", "250")], Some(100));
        let query = query(vec![parameter("limit", "integer", true, None)]);

        let bound = bind_parameters(&catalog(), &capability, &query, &[1], &BTreeMap::new()).unwrap();
        // 250 melebihi hard_cap 100 → dipotong, bukan diteruskan apa adanya.
        assert_eq!(bound[0], Bound::Bigint(100));
    }

    #[test]
    fn numeric_literal_default_within_cap_is_kept() {
        let capability = capability_with_cap(&[("limit", "10")], Some(100));
        let query = query(vec![parameter("limit", "integer", true, None)]);

        assert_eq!(bind_parameters(&catalog(), &capability, &query, &[1], &BTreeMap::new()).unwrap()[0], Bound::Bigint(10));
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

        let bound = bind_parameters(&catalog(), &capability, &query, &[3, 4], &BTreeMap::new()).unwrap();
        assert_eq!(bound[0], Bound::OfficeIds(vec![3, 4]));
    }
}
