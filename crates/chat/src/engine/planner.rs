//! Expands a verified model proposal using approved contracts only.

use std::collections::BTreeMap;

use chrono::{Datelike, NaiveDate};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    catalog::{
        loader::Catalog,
        model::{Capability, QueryManifest},
    },
    engine::{param_parse, resolver::ResolverSlot},
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
    /// Row cap capability untuk `limit: unbounded` tanpa `default_limit`
    /// (FIN-133). Nilainya adalah **cap**; executor mengikat `cap + 1` supaya
    /// "ada baris melebihi cap" teramati, lalu worker memotong hasil ke cap dan
    /// menyatakannya (`row_cap_reached`). Lineage menampilkan cap, bukan cap + 1.
    RowCap(i64),
}

/// Slot yang diikat dari teks permintaan sendiri, bukan dari default manifest
/// (FIN-135). Provenance `deterministic_parse` (K5, database-design.md §4.14)
/// — wajib diungkap lewat blok `note`, persis seperti `resolver_unique`.
#[derive(Debug, Clone, PartialEq)]
pub struct DeterministicBind {
    pub parameter: String,
    /// Kalimat pendek untuk pengungkapan, mis. "period 2026-01-01 to
    /// 2026-09-25 parsed from 'january to september 2026'".
    pub detail: String,
}

/// Nilai yang teks nyatakan tapi tidak dapat diikat (I5: tidak ada
/// penghilangan senyap). Slot tetap memakai default manifestnya — ini hanya
/// pengungkapan bahwa sesuatu yang disebut pengguna tidak diterapkan.
#[derive(Debug, Clone, PartialEq)]
pub struct UnappliedParam {
    pub parameter: String,
    pub detail: String,
}

/// Rencana untuk satu node `CuratedQuery`.
#[derive(Debug, Clone)]
pub struct Plan {
    pub capability_id: String,
    pub query_id: String,
    pub sql: String,
    pub sql_file: String,
    /// The query is tenant-wide reference/configuration, not an office fact.
    pub org_wide_reference: bool,
    /// Owner decision (D14): operational scheduler data read tenant-wide by an
    /// authenticated admin. Never combined with `org_wide_reference`.
    pub operational_tenant_wide: bool,
    /// D14 / XR-ASOF freshness hand-off declared by the capability.
    pub freshness: Option<super::freshness::FreshnessPlan>,
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
    /// Slot yang diikat dari teks permintaan (FIN-135) — wajib diungkap.
    pub deterministic_binds: Vec<DeterministicBind>,
    /// Nilai yang disebut teks tetapi tidak diterapkan (FIN-135) — wajib
    /// diungkap dengan default yang benar-benar dipakai.
    pub unapplied_params: Vec<UnappliedParam>,
    /// Chart yang dideklarasikan capability (responses.md §7), dengan dimensi
    /// waktu dari grain query — `None` bila capability tidak mendeklarasikannya.
    pub chart: Option<Chart>,
}

/// Bentuk data yang dibutuhkan chart `time_series`: sumbu waktu dari grain yang
/// dideklarasikan (L1.5), dan kolom grain lain sebagai seri.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chart {
    pub time_dimension: String,
    pub series: Vec<String>,
}

impl Plan {
    /// How the query is scoped: bound office facts, an org-wide master
    /// reference, or D14 operational tenant-wide scheduler data.
    pub fn scope_class(&self) -> &'static str {
        if self.operational_tenant_wide {
            "operational_tenant_wide"
        } else if self.org_wide_reference {
            "org_wide_reference"
        } else {
            "office_bound"
        }
    }

    /// Row cap yang berlaku pada eksekusi ini, bila planner mengikatnya.
    pub fn row_cap(&self) -> Option<i64> {
        self.parameters.iter().find_map(|bound| match bound {
            Bound::RowCap(cap) => Some(*cap),
            _ => None,
        })
    }
}

/// Kenapa sebuah permintaan tidak dapat direncanakan. Setiap varian menjadi
/// alasan yang dinyatakan pada response — tidak ada kegagalan senyap (I5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unplannable {
    Interaction(super::understanding::Intent),
    PlannerFailure(&'static str),
    InvalidSource(&'static str),
    /// Tenant-wide master access is limited to an active admin of this tenant.
    GlobalReferenceUnauthorized,
    /// No office-to-product relation proves office-specific applicability.
    GlobalReferenceOfficeUnavailable,
    /// Scheduler data is tenant-level; no approved relationship ties a job or
    /// run to an office, so an office filter is never invented for it (D14).
    OperationalOfficeUnavailable,
    /// A scheduler window is answered only on the stored timezone-naive clock,
    /// and only when the request says so; calendar words and timezone names
    /// are never mapped onto it (owner decision, D14, 2026-10-15).
    StoredClockWindowRequired(&'static str),
    /// This deployment has no product-level recurring contribution frequency.
    RecurringProductFrequencyUnavailable,
    /// Parameter wajib tidak dapat diisi tanpa bertanya lebih dulu.
    ///
    /// Seluruh parameter yang kurang dikumpulkan sekaligus: kontrak klarifikasi
    /// mewajibkan satu form memuat semua ambiguitas yang sudah diketahui, bukan
    /// satu pertanyaan per putaran.
    NeedsClarification {
        capability: String,
        missing: Vec<Missing>,
    },
    /// Tipe parameter belum didukung binder deterministik.
    ParameterUnsupported {
        capability: String,
        parameter: String,
        kind: String,
    },
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
            Self::Interaction(_) => "conversation".to_string(),
            Self::PlannerFailure(code) => (*code).to_string(),
            Self::InvalidSource(_) => "planner_output_invalid".to_string(),
            Self::GlobalReferenceUnauthorized => "global_reference_not_authorized".to_string(),
            Self::GlobalReferenceOfficeUnavailable => {
                "global_reference_office_unavailable".to_string()
            }
            Self::OperationalOfficeUnavailable => "operational_office_unavailable".to_string(),
            Self::StoredClockWindowRequired(_) => "stored_clock_window_required".to_string(),
            Self::RecurringProductFrequencyUnavailable => {
                "recurring_product_frequency_unavailable".to_string()
            }
            Self::NeedsClarification { missing, .. }
                if missing.iter().any(Missing::unanswerable) =>
            {
                "identity_slot_without_resolver".to_string()
            }
            Self::NeedsClarification { .. } => "parameter_needs_clarification".to_string(),
            Self::ParameterUnsupported { .. } => "parameter_binding_unsupported".to_string(),
        }
    }

    /// Kalimat untuk blok `limitation`. Bahasa Inggris: teks produk publik.
    pub fn explain(&self) -> String {
        match self {
            Self::Interaction(_) => "This turn does not request an analytical query.".to_string(),
            Self::PlannerFailure("planner_no_fit") => "No single approved capability fits this request as asked, so it was not answered and no source query was run. A request that combines facts an approved capability can read (for example job run observations) with a claim no approved capability can establish (for example that a business period or every account is covered or current) is not answered from the supported part alone.".to_string(),
            Self::PlannerFailure(_) | Self::InvalidSource(_) => "The planner could not produce a verified plan; no source query was run.".to_string(),
            Self::GlobalReferenceUnauthorized => {
                "This tenant-wide master requires an active administrator; no source query was run.".to_string()
            }
            Self::GlobalReferenceOfficeUnavailable => {
                "The product master is organization-wide; no approved relationship proves which products are available or used in the requested office.".to_string()
            }
            Self::OperationalOfficeUnavailable => {
                "Scheduler job and run data is tenant-level operational data; no approved relationship ties a job or run to an office, so it cannot be answered for one office.".to_string()
            }
            Self::StoredClockWindowRequired(why) => {
                let detail = match *why {
                    "calendar_or_timezone_expression" => {
                        "The request mixes an explicit window with a relative calendar period; the scheduler storage timezone is not established, so none is interpreted."
                    }
                    "explicit_window_not_stated" => {
                        "The request does not state an explicit month range or date range; calendar words and timezone names are not interpreted."
                    }
                    _ => "The request does not say its window is on the stored scheduler clock.",
                };
                format!(
                    "Scheduler timestamps are timezone-naive. {detail} No source query was run. Restate the window as explicit dates on the stored clock, for example 'from 1 April 2026 to 30 April 2026 on the stored clock'."
                )
            }
            Self::RecurringProductFrequencyUnavailable => {
                "Recurring contribution frequency is not recorded on the product master in this deployment; an account schedule cannot stand in for product configuration.".to_string()
            }
            Self::NeedsClarification {
                capability,
                missing,
            } => {
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
            Self::ParameterUnsupported {
                capability,
                parameter,
                kind,
            } => format!(
                "Capability '{capability}' declares parameter '{parameter}' of type '{kind}', \
                 which the deterministic planner cannot bind."
            ),
        }
    }
}

/// Chart hanya bila capability mendeklarasikannya DAN grain query memuat
/// dimensi waktu. Validator katalog menolak deklarasi tanpa grain waktu; di sini
/// kombinasi itu tetap tidak menghasilkan chart (fail closed).
fn chart(capability: &Capability, query: &QueryManifest) -> Option<Chart> {
    capability.chart.as_ref()?;
    let time_dimension = query.time_dimension()?.to_string();
    let series = query
        .grain
        .iter()
        .filter(|column| **column != time_dimension)
        .cloned()
        .collect();

    Some(Chart {
        time_dimension,
        series,
    })
}

pub fn plan(
    catalog: &Catalog,
    catalog_version_id: Uuid,
    request_text: &str,
    authorized_office_ids: &[i64],
    global_reference_allowed: bool,
    explicit_office_scope: bool,
    supplied: &BTreeMap<String, String>,
    proposal: &super::proposal::Proposal,
    shortlist: &[String],
    today: NaiveDate,
) -> Result<Plan, Unplannable> {
    use super::proposal::Decision;
    let invalid = || Unplannable::PlannerFailure("planner_output_invalid");
    match proposal.decision {
        Decision::Abstain => return Err(Unplannable::PlannerFailure("planner_abstained")),
        Decision::NoFit => return Err(Unplannable::PlannerFailure("planner_no_fit")),
        Decision::Planned => {}
    }
    // Graph validation happens before this boundary; multi-node dispatch is L8.
    if proposal.nodes.len() != 1 || !proposal.nodes[0].depends_on.is_empty() {
        return Err(invalid());
    }
    let node = &proposal.nodes[0];
    if !shortlist.contains(&node.capability_id) {
        return Err(invalid());
    }
    let capability = catalog
        .capabilities
        .iter()
        .map(|loaded| &loaded.entry)
        .find(|entry| {
            entry.id == node.capability_id && entry.status.as_deref() == Some("approved_mvp")
        })
        .ok_or_else(invalid)?;

    let query_id = capability.query_id.clone().ok_or_else(invalid)?;
    let query = catalog
        .queries
        .iter()
        .map(|loaded| &loaded.entry)
        .find(|query| query.id == query_id)
        .ok_or_else(invalid)?;
    if query.guards.org_wide_reference == Some(true) {
        if !global_reference_allowed {
            return Err(Unplannable::GlobalReferenceUnauthorized);
        }
        if explicit_office_scope || asks_for_office_specific_products(request_text) {
            return Err(Unplannable::GlobalReferenceOfficeUnavailable);
        }
    }
    if query.guards.operational_tenant_wide == Some(true) {
        if !global_reference_allowed {
            return Err(Unplannable::GlobalReferenceUnauthorized);
        }
        if explicit_office_scope || asks_for_office_specific_products(request_text) {
            return Err(Unplannable::OperationalOfficeUnavailable);
        }
        if query.parameters.iter().any(|p| p.name == "from_date")
            && let Some(why) = param_parse::stored_clock_window_issue(request_text, today)
        {
            return Err(Unplannable::StoredClockWindowRequired(why));
        }
    }
    if query.id == "products.recurring_deposit_product_master"
        && has_any_term(request_text, &["frequency", "frekuensi"])
    {
        return Err(Unplannable::RecurringProductFrequencyUnavailable);
    }

    let sql_file = query.sql_file.clone().ok_or_else(invalid)?;
    let sql = catalog
        .sql_files
        .get(&sql_file)
        .cloned()
        .ok_or_else(invalid)?;
    let (verified_bindings, mut grounded, slot_provenance) = verify_slots(
        catalog,
        capability,
        query,
        node,
        supplied,
        request_text,
        today,
    )?;
    let probes: Vec<_> = node
        .slots
        .iter()
        .filter_map(|(name, slot)| {
            if matches!(slot, super::proposal::Slot::Probe { .. }) {
                query
                    .parameters
                    .iter()
                    .find(|p| p.name == *name)
                    .map(|p| missing_parameter(catalog, capability, p))
            } else {
                None
            }
        })
        .collect();
    let bound = bind_parameters(
        catalog,
        capability,
        query,
        authorized_office_ids,
        &verified_bindings,
        today,
    );
    let (parameters, defaults, unapplied_params) = match bound {
        Ok(values) if probes.is_empty() => values,
        Ok(_) => {
            return Err(Unplannable::NeedsClarification {
                capability: capability.id.clone(),
                missing: probes,
            });
        }
        Err(Unplannable::NeedsClarification { mut missing, .. }) => {
            for probe in probes {
                if !missing.iter().any(|item| item.name == probe.name) {
                    missing.push(probe);
                }
            }
            if missing.iter().any(Missing::unanswerable) {
                return Err(invalid());
            }
            return Err(Unplannable::NeedsClarification {
                capability: capability.id.clone(),
                missing,
            });
        }
        Err(_) => return Err(invalid()),
    };
    grounded.extend(defaults);
    let date = |name: &str| {
        query
            .parameters
            .iter()
            .zip(&parameters)
            .find_map(|(parameter, value)| match value {
                Bound::Date(date) if parameter.name == name => Some(*date),
                _ => None,
            })
    };
    if let (Some(from), Some(to)) = (date("from_date"), date("to_date"))
        && (from > to
            || capability
                .guards
                .get("max_date_range_days")
                .and_then(yaml_i64)
                .is_some_and(|cap| (to - from).num_days() > cap))
    {
        return Err(invalid());
    }

    let graph_json = serde_json::json!({
        "nodes": [{
            "node_id": "main",
            "kind": "CuratedQuery",
            "capability_id": capability.id,
            "query_id": query.id,
            "depends_on": [],
            "proposal_node_id": node.id,
            "slot_provenance": slot_provenance,
            "schema_version": 1,
        }],
    });

    Ok(Plan {
        capability_id: capability.id.clone(),
        query_id: query.id.clone(),
        org_wide_reference: query.guards.org_wide_reference == Some(true),
        operational_tenant_wide: query.guards.operational_tenant_wide == Some(true),
        freshness: freshness_plan(catalog, capability)?,
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
                field
                    .sensitivity
                    .clone()
                    .unwrap_or_else(|| "pii".to_string())
            })
            .collect(),
        // Manifest tanpa timeout sudah dilaporkan validator; di sini dipakai
        // kelas probe yang lebih ketat, bukan yang lebih longgar.
        timeout_ms: query.timeout_ms.unwrap_or(3_000),
        catalog_version_id,
        catalog_content_hash: catalog.content_hash.clone(),
        retrieval_score: 0.0,
        graph_hash: hex::encode(Sha256::digest(graph_json.to_string().as_bytes())),
        graph_json,
        parameters,
        deterministic_binds: grounded,
        unapplied_params,
        chart: chart(capability, query),
        sql,
        sql_file,
    })
}

fn verify_slots(
    catalog: &Catalog,
    capability: &Capability,
    query: &QueryManifest,
    node: &super::proposal::Node,
    supplied: &BTreeMap<String, String>,
    request: &str,
    today: NaiveDate,
) -> Result<(BTreeMap<String, String>, Vec<DeterministicBind>, Value), Unplannable> {
    use super::proposal::Slot;
    let invalid = || Unplannable::PlannerFailure("planner_output_invalid");
    let mut bindings = supplied.clone();
    let mut disclosures = Vec::new();
    let mut provenance = serde_json::Map::new();
    for (name, slot) in &node.slots {
        let parameter = query
            .parameters
            .iter()
            .find(|p| p.name == *name)
            .ok_or_else(invalid)?;
        let identity = missing_parameter(catalog, capability, parameter).identity;
        if supplied.contains_key(name) || parameter.source.as_deref() == Some("authorized_scope") {
            return Err(invalid());
        }
        let (value, source, span) = match slot {
            Slot::Value { value, source } if !identity => {
                let span = source.span(request).map_err(Unplannable::InvalidSource)?;
                let value = match parameter.kind.as_str() {
                    "integer" | "bigint" => value.as_i64().map(|v| v.to_string()),
                    "string" | "date" => value.as_str().map(str::to_owned),
                    _ => None,
                }
                .ok_or_else(invalid)?;
                let grounded = if name == "currency_code" {
                    param_parse::parse_currency(&source.quote).is_some_and(|p| p.code == value)
                } else if name == "limit" {
                    param_parse::parse_limit(&source.quote)
                        .is_some_and(|p| p.value.to_string() == value)
                        || source.quote == value
                } else {
                    source.quote == value
                };
                if !grounded || typed_answer(&parameter.kind, &value).is_none() {
                    return Err(Unplannable::InvalidSource("value_not_supported_by_quote"));
                }
                (value, source, span)
            }
            Slot::DatePhrase { source } if parameter.kind == "date" && !identity => {
                let span = source.span(request).map_err(Unplannable::InvalidSource)?;
                let period = if capability
                    .guards
                    .get("schedule_period")
                    .and_then(serde_yaml::Value::as_bool)
                    == Some(true)
                {
                    param_parse::parse_schedule_period(&source.quote, today)
                } else {
                    param_parse::parse_period(&source.quote, today)
                }
                .or_else(|| param_parse::parse_month_phrase(&source.quote, today))
                .ok_or_else(invalid)?;
                let date = match name.as_str() {
                    "from_date" => period.from,
                    "to_date" => period.to,
                    _ => return Err(invalid()),
                };
                (date.to_string(), source, span)
            }
            Slot::Probe { arguments } if identity => {
                let resolver = missing_parameter(catalog, capability, parameter)
                    .resolver
                    .ok_or_else(invalid)?;
                // The current approved resolver manifests accept only authorized
                // scope. Undeclared search arguments cannot invent a SQL filter.
                if !arguments.is_empty() {
                    return Err(invalid());
                }
                provenance.insert(
                    name.clone(),
                    serde_json::json!({ "kind": "probe", "resolver_ref": resolver.query_id }),
                );
                continue;
            }
            _ => return Err(invalid()),
        };
        let (start, end) = span;
        if matches!(parameter.kind.as_str(), "integer" | "bigint")
            && let Some(cap) = declared_cap(capability, capability.parameters.get(name))
            && value
                .parse::<i64>()
                .map_or(true, |number| number <= 0 || number > cap)
        {
            return Err(invalid());
        }
        provenance.insert(
            name.clone(),
            serde_json::json!({
                "kind": "request", "quote": source.quote, "occurrence": source.occurrence,
                "source_span": [start, end], "value": value,
            }),
        );
        disclosures.push(DeterministicBind {
            parameter: name.clone(),
            detail: format!(
                "{name} {value} grounded in request bytes [{start}, {end}) ('{}')",
                source.quote
            ),
        });
        bindings.insert(name.clone(), value);
    }
    // A recognized explicit period cannot disappear into catalog defaults.
    if param_parse::parse_period(request, today).is_some() {
        for p in &query.parameters {
            if matches!(p.name.as_str(), "from_date" | "to_date") && !bindings.contains_key(&p.name)
            {
                return Err(Unplannable::InvalidSource("period_binding_missing"));
            }
        }
    }
    for (name, stated) in [
        (
            "currency_code",
            param_parse::parse_currency(request).is_some(),
        ),
        ("limit", param_parse::parse_limit(request).is_some()),
    ] {
        if stated && query.parameters.iter().any(|p| p.name == name) && !bindings.contains_key(name)
        {
            return Err(invalid());
        }
    }
    if let (Some(from), Some(to)) = (bindings.get("from_date"), bindings.get("to_date")) {
        let from = NaiveDate::parse_from_str(from, "%Y-%m-%d").map_err(|_| invalid())?;
        let to = NaiveDate::parse_from_str(to, "%Y-%m-%d").map_err(|_| invalid())?;
        if from > to
            || capability
                .guards
                .get("max_date_range_days")
                .and_then(serde_yaml::Value::as_i64)
                .is_some_and(|cap| (to - from).num_days() > cap)
        {
            return Err(invalid());
        }
    }
    Ok((bindings, disclosures, Value::Object(provenance)))
}

fn has_any_term(text: &str, wanted: &[&str]) -> bool {
    text.split(|character: char| !character.is_alphanumeric())
        .any(|term| wanted.iter().any(|word| term.eq_ignore_ascii_case(word)))
}

/// A global master cannot answer an office-specific availability question
/// without a verified product-to-office relation (FIN-109 / XR-SCOPE). Scheduler
/// data has no office relationship either (D14), so it reuses this test.
fn asks_for_office_specific_products(text: &str) -> bool {
    has_any_term(text, &["office", "branch", "cabang", "kantor"])
}

/// Resolve the capability's `freshness:` declaration to approved probe SQL.
/// The catalog validator guarantees both manifests exist and have the shape the
/// engine reads; a missing one here is still a planner failure, never a skip.
fn freshness_plan(
    catalog: &Catalog,
    capability: &Capability,
) -> Result<Option<super::freshness::FreshnessPlan>, Unplannable> {
    use super::freshness::{FreshnessPlan, ProbePlan};
    let Some(declared) = &capability.freshness else {
        return Ok(None);
    };
    let invalid = || Unplannable::PlannerFailure("planner_output_invalid");
    let probe = |query_id: &str| -> Result<ProbePlan, Unplannable> {
        let query = catalog
            .queries
            .iter()
            .map(|loaded| &loaded.entry)
            .find(|query| query.id == query_id)
            .ok_or_else(invalid)?;
        let sql = query
            .sql_file
            .as_ref()
            .and_then(|file| catalog.sql_files.get(file))
            .cloned()
            .ok_or_else(invalid)?;
        Ok(ProbePlan {
            query_id: query.id.clone(),
            sql,
            timeout_ms: query.timeout_ms.unwrap_or(3_000),
        })
    };
    Ok(Some(FreshnessPlan {
        batch_job: declared.batch_job.clone(),
        batch: probe(&declared.batch_probe)?,
        coverage: probe(&declared.coverage_probe)?,
    }))
}

/// Batas atas yang dideklarasikan capability untuk sebuah parameter numerik:
/// `hard_cap` per-parameter diutamakan, lalu `guards.max_limit` (FIN-133).
fn declared_cap(
    capability: &Capability,
    declared: Option<&crate::catalog::model::CapabilityParameter>,
) -> Option<i64> {
    declared
        .and_then(|declared| declared.hard_cap)
        .or_else(|| capability.guards.get("max_limit").and_then(yaml_i64))
}

/// Bind verified values, accepted answers, authorization and declared defaults.
fn bind_parameters(
    catalog: &Catalog,
    capability: &Capability,
    query: &QueryManifest,
    authorized_office_ids: &[i64],
    supplied: &BTreeMap<String, String>,
    today: NaiveDate,
) -> Result<(Vec<Bound>, Vec<DeterministicBind>, Vec<UnappliedParam>), Unplannable> {
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
            // FIN-133 — `limit: unbounded` bukan izin membaca tanpa batas bila
            // capability mendeklarasikan batasnya: `default_limit` adalah
            // ukuran jawaban yang diminta, cap adalah janji kepada sumber data.
            (_, Some("unbounded"))
                if parameter.name == "limit"
                    && matches!(parameter.kind.as_str(), "integer" | "bigint") =>
            {
                unbounded_limit(capability, declared)
            }
            // `unbounded` tanpa cap menyatakan "memang tanpa batas" — NULL yang
            // disengaja, dan SQL menanganinya lewat pola `($n IS NULL OR ...)`.
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

    Ok((bound, Vec::new(), Vec::new()))
}

/// Keputusan owner FIN-133 untuk `limit` ber-`default: unbounded` tanpa nilai
/// dari pengguna:
///
/// 1. `defaults.default_limit` ada → itulah ukuran jawaban yang diminta,
///    dipotong ke cap. Tidak ada pengungkapan: jawabannya memang sebesar itu.
/// 2. Tanpa `default_limit` tetapi ada cap (`hard_cap`, selain itu
///    `guards.max_limit`) → [`Bound::RowCap`]; kelebihan baris diungkap.
/// 3. Tanpa cap sama sekali → NULL, "memang tanpa batas".
fn unbounded_limit(
    capability: &Capability,
    declared: Option<&crate::catalog::model::CapabilityParameter>,
) -> Bound {
    let cap = declared
        .and_then(|declared| declared.hard_cap)
        .or_else(|| capability.guards.get("max_limit").and_then(yaml_i64));

    match capability.defaults.get("default_limit").and_then(yaml_i64) {
        Some(requested) => Bound::Bigint(cap.map_or(requested, |cap| requested.min(cap))),
        None => cap.map_or(Bound::NullBigint, Bound::RowCap),
    }
}

/// Angka YAML yang ditulis sebagai bilangan (`50`) maupun string (`"50"`).
fn yaml_i64(value: &serde_yaml::Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_str()?.trim().parse().ok())
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
        identity: parameter.source.as_deref() == Some("transient_sensitive_input")
            || declared_probe,
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
        "m" | "mo" => param_parse::subtract_months(today, count),
        "y" => param_parse::subtract_months(today, count.checked_mul(12)?),
        _ => None,
    }
}

/// Ubah jawaban bertipe menjadi nilai terikat. `None` berarti jawaban tidak
/// sesuai tipe yang dideklarasikan manifest.
pub fn typed_answer(kind: &str, answer: &str) -> Option<Bound> {
    match kind {
        "date" => NaiveDate::parse_from_str(answer.trim(), "%Y-%m-%d")
            .ok()
            .map(Bound::Date),
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
    use chrono::Utc;

    #[test]
    fn verifier_grounds_dates_and_preserves_utf8_request_spans() {
        let capability = capability(&[]);
        let query = query(vec![
            parameter("from_date", "date", true, None),
            parameter("to_date", "date", true, None),
        ]);
        let node: super::super::proposal::Node = serde_json::from_value(serde_json::json!({
            "id": "n1", "capability_id": capability.id, "depends_on": [],
            "slots": {
                "from_date": { "kind": "date_phrase", "source": { "quote": "bulan ini", "occurrence": 0 } },
                "to_date": { "kind": "date_phrase", "source": { "quote": "bulan ini", "occurrence": 0 } }
            }
        })).unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 10, 12).unwrap();
        let (values, _, provenance) = verify_slots(
            &catalog(),
            &capability,
            &query,
            &node,
            &BTreeMap::new(),
            "é bulan ini",
            today,
        )
        .unwrap();
        assert_eq!(values["from_date"], "2026-10-01");
        assert_eq!(values["to_date"], "2026-10-12");
        assert_eq!(
            provenance["from_date"]["source_span"],
            serde_json::json!([3, 12])
        );
    }

    #[test]
    fn verifier_grounds_each_range_bound_in_its_own_month_quote() {
        let capability = capability(&[]);
        let query = query(vec![
            parameter("from_date", "date", true, None),
            parameter("to_date", "date", true, None),
        ]);
        let node: super::super::proposal::Node = serde_json::from_value(serde_json::json!({
            "id": "n1", "capability_id": capability.id, "depends_on": [],
            "slots": {
                "from_date": { "kind": "date_phrase", "source": { "quote": "April 2026", "occurrence": 0 } },
                "to_date": { "kind": "date_phrase", "source": { "quote": "May 2026", "occurrence": 0 } }
            }
        })).unwrap();
        let (values, _, _) = verify_slots(
            &catalog(),
            &capability,
            &query,
            &node,
            &BTreeMap::new(),
            "Which jobs failed from April 2026 to May 2026?",
            NaiveDate::from_ymd_opt(2026, 10, 15).unwrap(),
        )
        .unwrap();
        assert_eq!(values["from_date"], "2026-04-01");
        assert_eq!(values["to_date"], "2026-05-31");
    }

    #[test]
    fn verifier_rejects_scope_fabricated_values_unknown_slots_and_caps() {
        let capability = capability_with_cap(&[("limit", "unbounded")], Some(10));
        let query = query(vec![
            parameter("limit", "integer", false, None),
            parameter("office_ids", "array_bigint", true, Some("authorized_scope")),
        ]);
        for slots in [
            serde_json::json!({"limit": {"kind": "value", "value": 20, "source": {"quote": "20", "occurrence": 0}}}),
            serde_json::json!({"limit": {"kind": "value", "value": 5, "source": {"quote": "20", "occurrence": 0}}}),
            serde_json::json!({"limit": {"kind": "value", "value": 5, "source": {"quote": "5", "occurrence": 0}}}),
            serde_json::json!({"office_ids": {"kind": "value", "value": [999], "source": {"quote": "20", "occurrence": 0}}}),
            serde_json::json!({"undeclared": {"kind": "probe", "arguments": {}}}),
        ] {
            let node = serde_json::from_value(serde_json::json!({ "id": "n1", "capability_id": capability.id, "depends_on": [], "slots": slots })).unwrap();
            let problem = verify_slots(
                &catalog(),
                &capability,
                &query,
                &node,
                &BTreeMap::new(),
                "top 20",
                NaiveDate::from_ymd_opt(2026, 10, 12).unwrap(),
            )
            .unwrap_err();
            assert_eq!(problem.reason(), "planner_output_invalid");
        }
    }

    #[test]
    fn verifier_classifies_catalog_defaults_as_invalid_source_not_calendar_input() {
        let capability = capability(&[]);
        let query = query(vec![parameter("from_date", "date", true, None)]);
        let node = serde_json::from_value(serde_json::json!({
            "id": "n1", "capability_id": capability.id, "depends_on": [],
            "slots": {"from_date": {"kind": "date_phrase", "source": {"quote": "start_of_month(business_today)", "occurrence": 0}}}
        })).unwrap();
        let error = verify_slots(
            &catalog(),
            &capability,
            &query,
            &node,
            &BTreeMap::new(),
            "Total setoran tabungan bulan ini",
            NaiveDate::from_ymd_opt(2026, 10, 12).unwrap(),
        )
        .unwrap_err();
        assert_eq!(error, Unplannable::InvalidSource("source_quote_not_found"));
    }

    #[test]
    fn verifier_rejects_replacement_of_accepted_identity_and_omitted_period() {
        let capability = capability(&[]);
        let query = query(vec![
            parameter(
                "client_id",
                "integer",
                true,
                Some("transient_sensitive_input"),
            ),
            parameter("from_date", "date", true, None),
        ]);
        for slots in [
            serde_json::json!({}),
            serde_json::json!({"client_id": {"kind": "value", "value": 7, "source": {"quote": "7", "occurrence": 0}}}),
        ] {
            let node = serde_json::from_value(serde_json::json!({ "id": "n1", "capability_id": capability.id, "depends_on": [], "slots": slots })).unwrap();
            let supplied = BTreeMap::from([("client_id".into(), "7".into())]);
            assert!(
                verify_slots(
                    &catalog(),
                    &capability,
                    &query,
                    &node,
                    &supplied,
                    "client 7 bulan ini",
                    NaiveDate::from_ymd_opt(2026, 10, 12).unwrap()
                )
                .is_err()
            );
        }
    }
    // FIN-137: this is now the only proof of K1's "identity slot without a
    // resolver is never asked" — `resolver/noresolver-*.yml` used to exercise
    // it at the HTTP surface through `savings_account_identity_lookup`, whose
    // `account_number` had no `probe:`. That was the last capability in the
    // approved catalog in that state (`cargo run -p app -- catalog` shows zero
    // `transient_sensitive_input` parameters left without a probe), so the
    // Bruno scenario had no fixture left to reproduce it with. The mechanism
    // itself (`Missing::unanswerable`, `Unplannable::reason`/`explain`) is
    // unchanged — only pure logic, so a unit test proves it just as well.
    #[test]
    fn identity_slot_without_resolver_is_unanswerable_not_asked_as_text() {
        let missing = Missing {
            name: "account_number".to_string(),
            kind: "string".to_string(),
            identity: true,
            resolver: None,
        };
        assert!(missing.unanswerable());
        assert_eq!(missing.field_type(), "text");

        let unplannable = Unplannable::NeedsClarification {
            capability: "savings_account_identity_lookup".to_string(),
            missing: vec![missing],
        };
        assert_eq!(unplannable.reason(), "identity_slot_without_resolver");
        assert!(unplannable.explain().contains("account_number"));
        assert!(unplannable.explain().contains("never bound from free text"));
    }

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
            domains: Vec::new(),
            areas: Vec::new(),
            inventory: Default::default(),
            safety_policy: Default::default(),
            office_scope_policy: Default::default(),
            sensitivity_classes: Default::default(),
            unapproved_surfaces: Default::default(),
            deferred_domains: Default::default(),
            sql_files: Default::default(),
            content_hash: String::new(),
            unreadable: Vec::new(),
        }
    }

    /// D14 owner decision: scheduler data has no office relationship, so an
    /// office-specific question is refused (never answered with an invented
    /// filter), and a non-admin cannot read it tenant-wide.
    #[test]
    fn operational_scheduler_data_refuses_an_office_scope_instead_of_inventing_a_filter() {
        use crate::catalog::loader::Loaded;

        let mut capability = capability(&[]);
        capability.id = "scheduler_job_failed_runs".into();
        capability.status = Some("approved_mvp".into());
        capability.query_id = Some("scheduler.job_failed_runs".into());
        let mut manifest = query(vec![
            parameter("from_date", "date", true, None),
            parameter("to_date", "date", true, None),
        ]);
        manifest.id = "scheduler.job_failed_runs".into();
        manifest.guards.operational_tenant_wide = Some(true);
        let mut catalog = catalog();
        catalog.capabilities.push(Loaded {
            path: "c.yaml".into(),
            entry: capability,
        });
        catalog.queries.push(Loaded {
            path: "q.yaml".into(),
            entry: manifest,
        });

        let proposal = super::super::proposal::Proposal::parse(
            r#"{"schema_version":1,"decision":"planned","confidence":"high","nodes":[{"id":"n1","capability_id":"scheduler_job_failed_runs","depends_on":[],"slots":{}}]}"#,
        )
        .unwrap();
        let shortlist = ["scheduler_job_failed_runs".to_string()];
        let today = NaiveDate::from_ymd_opt(2026, 10, 15).unwrap();
        let attempt = |text: &str, admin: bool, explicit_office: bool| {
            plan(
                &catalog,
                Uuid::nil(),
                text,
                &[1, 2],
                admin,
                explicit_office,
                &BTreeMap::new(),
                &proposal,
                &shortlist,
                today,
            )
            .map(|_| ())
        };

        assert_eq!(
            attempt(
                "Which batch jobs failed in the Branch 001 office?",
                true,
                false
            ),
            Err(Unplannable::OperationalOfficeUnavailable)
        );
        assert_eq!(
            attempt("Which batch jobs failed?", true, true),
            Err(Unplannable::OperationalOfficeUnavailable)
        );
        assert_eq!(
            attempt("Which batch jobs failed?", false, false),
            Err(Unplannable::GlobalReferenceUnauthorized)
        );
        // Owner decision (stored-clock windows only): calendar words and an
        // unstated clock never reach the naive scheduler timestamps.
        assert_eq!(
            attempt(
                "Which batch jobs failed last night on the stored clock?",
                true,
                false
            ),
            Err(Unplannable::StoredClockWindowRequired(
                "explicit_window_not_stated"
            ))
        );
        assert_eq!(
            attempt("Which batch jobs failed in April 2026?", true, false),
            Err(Unplannable::StoredClockWindowRequired(
                "stored_clock_not_stated"
            ))
        );
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
            unsupported_intents: Vec::new(),
            parameters: declared,
            request_shape: None,
            guards: BTreeMap::new(),
            defaults: BTreeMap::new(),
            chart: None,
            freshness: None,
        }
    }

    fn query(parameters: Vec<QueryParameter>) -> QueryManifest {
        QueryManifest {
            id: "savings.deposit_total".into(),
            database: None,
            sql_file: None,
            tables: Vec::new(),
            parameters,
            output_fields: Vec::new(),
            grain: Vec::new(),
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

        let (bound, ..) = bind_parameters(
            &catalog(),
            &capability,
            &query,
            &[1, 2],
            &BTreeMap::new(),
            Utc::now().date_naive(),
        )
        .unwrap();
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
                CapabilityParameter {
                    required: true,
                    default: None,
                    hard_cap: None,
                    probe: None,
                },
            )]),
            ..capability(&[])
        };
        let query = query(vec![parameter("search", "string", true, None)]);

        let problem = bind_parameters(
            &catalog(),
            &capability,
            &query,
            &[1],
            &BTreeMap::new(),
            Utc::now().date_naive(),
        )
        .unwrap_err();
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

        let (bound, ..) = bind_parameters(
            &catalog(),
            &capability,
            &query,
            &[7],
            &BTreeMap::new(),
            Utc::now().date_naive(),
        )
        .unwrap();
        assert_eq!(bound[1], Bound::NullText);
        assert_eq!(bound[2], Bound::NullBigintArray);
    }

    #[test]
    fn required_parameter_without_derivable_value_is_not_guessed() {
        let capability = capability(&[]);
        let query = query(vec![parameter("account_number", "string", true, None)]);

        let problem = bind_parameters(
            &catalog(),
            &capability,
            &query,
            &[1],
            &BTreeMap::new(),
            Utc::now().date_naive(),
        )
        .unwrap_err();
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

        let (bound, ..) = bind_parameters(
            &catalog(),
            &capability,
            &query,
            &[1],
            &supplied,
            Utc::now().date_naive(),
        )
        .unwrap();
        assert_eq!(
            bound[0],
            Bound::Date(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap())
        );
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
        let (bound, ..) = bind_parameters(
            &catalog(),
            &capability,
            &query,
            &[3],
            &supplied,
            Utc::now().date_naive(),
        )
        .unwrap();
        assert_eq!(bound[0], Bound::OfficeIds(vec![3]));
    }

    #[test]
    fn all_missing_parameters_are_collected_into_one_question() {
        let capability = capability(&[]);
        let query = query(vec![
            parameter("from_date", "date", true, None),
            parameter("to_date", "date", true, None),
        ]);

        let problem = bind_parameters(
            &catalog(),
            &capability,
            &query,
            &[1],
            &BTreeMap::new(),
            Utc::now().date_naive(),
        )
        .unwrap_err();
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

        let (bound, ..) = bind_parameters(
            &catalog(),
            &capability,
            &query,
            &[1],
            &BTreeMap::new(),
            Utc::now().date_naive(),
        )
        .unwrap();
        // 250 melebihi hard_cap 100 → dipotong, bukan diteruskan apa adanya.
        assert_eq!(bound[0], Bound::Bigint(100));
    }

    #[test]
    fn numeric_literal_default_within_cap_is_kept() {
        let capability = capability_with_cap(&[("limit", "10")], Some(100));
        let query = query(vec![parameter("limit", "integer", true, None)]);

        assert_eq!(
            bind_parameters(
                &catalog(),
                &capability,
                &query,
                &[1],
                &BTreeMap::new(),
                Utc::now().date_naive()
            )
            .unwrap()
            .0[0],
            Bound::Bigint(10)
        );
    }

    /// Capability dengan parameter `limit` saja, plus `guards.max_limit` dan
    /// `defaults.default_limit` opsional — tiga sumber batas FIN-133.
    fn limited(
        default: &str,
        hard_cap: Option<i64>,
        max_limit: Option<i64>,
        default_limit: Option<i64>,
    ) -> (Capability, QueryManifest) {
        let mut capability = capability_with_cap(&[("limit", default)], hard_cap);
        if let Some(max_limit) = max_limit {
            capability
                .guards
                .insert("max_limit".into(), serde_yaml::Value::from(max_limit));
        }
        if let Some(default_limit) = default_limit {
            capability.defaults.insert(
                "default_limit".into(),
                serde_yaml::Value::from(default_limit),
            );
        }
        (
            capability,
            query(vec![parameter("limit", "integer", false, None)]),
        )
    }

    fn bind_limit(capability: &Capability, query: &QueryManifest) -> Bound {
        bind_parameters(
            &catalog(),
            capability,
            query,
            &[1],
            &BTreeMap::new(),
            Utc::now().date_naive(),
        )
        .unwrap()
        .0[0]
            .clone()
    }

    /// FIN-133 kasus 1 — `default_limit` adalah ukuran jawaban yang diminta,
    /// dipotong ke `hard_cap`, selain itu ke `guards.max_limit`.
    #[test]
    fn unbounded_limit_binds_declared_default_limit_clamped_to_cap() {
        // client_random_sample: default_limit 5, hard_cap 50.
        let (capability, query) = limited("unbounded", Some(50), Some(50), Some(5));
        assert_eq!(bind_limit(&capability, &query), Bound::Bigint(5));

        let (capability, query) = limited("unbounded", Some(50), None, Some(80));
        assert_eq!(bind_limit(&capability, &query), Bound::Bigint(50));

        let (capability, query) = limited("unbounded", None, Some(30), Some(80));
        assert_eq!(bind_limit(&capability, &query), Bound::Bigint(30));
    }

    /// FIN-133 kasus 2 — tanpa `default_limit`, cap menjadi row cap; `hard_cap`
    /// lebih diutamakan daripada `guards.max_limit`.
    #[test]
    fn unbounded_limit_without_default_limit_binds_the_row_cap() {
        // savings_client_activity: hard_cap 100.
        let (capability, query) = limited("unbounded", Some(100), Some(500), None);
        assert_eq!(bind_limit(&capability, &query), Bound::RowCap(100));

        let (capability, query) = limited("unbounded", None, Some(100), None);
        assert_eq!(bind_limit(&capability, &query), Bound::RowCap(100));
    }

    /// FIN-133 kasus 3 — tanpa cap yang dideklarasikan, `unbounded` tetap NULL.
    #[test]
    fn unbounded_limit_without_any_cap_stays_null() {
        let (capability, query) = limited("unbounded", None, None, None);
        assert_eq!(bind_limit(&capability, &query), Bound::NullBigint);
    }

    /// Default literal (top-N `limit: "10"`) tidak tersentuh keputusan
    /// FIN-133, bahkan bila capability juga mendeklarasikan `default_limit`.
    #[test]
    fn literal_limit_default_wins_over_default_limit() {
        let (capability, query) = limited("10", Some(100), Some(100), Some(5));
        assert_eq!(bind_limit(&capability, &query), Bound::Bigint(10));
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

        let (bound, ..) = bind_parameters(
            &catalog(),
            &capability,
            &query,
            &[3, 4],
            &BTreeMap::new(),
            Utc::now().date_naive(),
        )
        .unwrap();
        assert_eq!(bound[0], Bound::OfficeIds(vec![3, 4]));
    }
}
