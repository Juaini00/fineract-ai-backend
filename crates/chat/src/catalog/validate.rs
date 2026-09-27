//! Penegakan `checks` yang sudah dideklarasikan katalog itu sendiri.
//!
//! Aturannya bukan karangan validator ini: `knowledge/policies/query_safety.yaml`
//! dan blok `checks:` pada tiap capability/query yang menyatakannya. Yang
//! dikerjakan di sini hanya menjadikannya mekanis — `CARRY-OVER.md` menyebut
//! tepat kegagalan yang lahir saat aturan hanya hidup sebagai prosa.
//!
//! Yang **tidak** dibuktikan modul ini: angkanya benar. Memuat YAML dan
//! memvalidasi SQL membuktikan bentuk, bukan kebenaran hasil.

use std::collections::{BTreeMap, BTreeSet};

use crate::catalog::{
    loader::Catalog,
    model::{CHART_KINDS, Capability, DataScopeArea, Domain, IntentRef, QueryManifest},
};

/// Tingkat temuan. `Error` berarti entri tidak boleh dianggap approved;
/// `Warning` berarti perlu keputusan manusia, bukan perbaikan mekanis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Finding {
    pub severity: Severity,
    /// Berkas atau id yang menjadi subjek temuan.
    pub subject: String,
    /// Id check yang dilanggar, memakai nama dari blok `checks:` bila ada.
    pub check: String,
    pub message: String,
}

impl Finding {
    fn error(subject: impl Into<String>, check: &str, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            subject: subject.into(),
            check: check.to_string(),
            message: message.into(),
        }
    }

    fn warning(subject: impl Into<String>, check: &str, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Warning,
            subject: subject.into(),
            check: check.to_string(),
            message: message.into(),
        }
    }
}

#[derive(Debug, Default)]
pub struct Report {
    pub findings: Vec<Finding>,
}

impl Report {
    pub fn errors(&self) -> usize {
        self.count(Severity::Error)
    }

    pub fn warnings(&self) -> usize {
        self.count(Severity::Warning)
    }

    fn count(&self, severity: Severity) -> usize {
        self.findings
            .iter()
            .filter(|finding| finding.severity == severity)
            .count()
    }
}

/// Validasi statis: tidak menyentuh database sama sekali.
pub fn validate(catalog: &Catalog) -> Report {
    let mut findings = Vec::new();

    for (path, error) in &catalog.unreadable {
        findings.push(Finding::error(path, "yaml_parses", error.clone()));
    }

    let queries: BTreeMap<&str, &QueryManifest> = catalog
        .queries
        .iter()
        .map(|loaded| (loaded.entry.id.as_str(), &loaded.entry))
        .collect();

    let mut referenced_sql = BTreeSet::new();

    for loaded in &catalog.queries {
        check_query(catalog, &loaded.entry, &loaded.path, &mut referenced_sql, &mut findings);
    }

    for loaded in &catalog.capabilities {
        check_capability(catalog, &loaded.entry, &loaded.path, &queries, &mut findings);
    }

    check_resolvers(catalog, &mut findings);
    check_domain_alignment(catalog, &mut findings);

    // SQL yatim: ada di `queries/` tetapi tidak dirujuk manifest mana pun.
    // Bukan sekadar kerapian — SQL yang tidak punya kontrak tidak punya
    // output_fields, guard, maupun timeout, jadi ia tidak dapat dieksekusi
    // sebagai capability yang disetujui.
    for path in catalog.sql_files.keys() {
        if referenced_sql.contains(path.as_str()) {
            continue;
        }
        // Fragment dataset (`*.frag.sql`) memang dirakit kontrak Mode 2 yang
        // validatornya belum ada; dilaporkan terpisah oleh `coverage()`.
        if path.contains("/datasets/") {
            continue;
        }
        findings.push(Finding::warning(
            path,
            "sql_is_referenced",
            "file SQL tidak dirujuk manifest query mana pun",
        ));
    }

    Report { findings }
}

fn check_query(
    catalog: &Catalog,
    query: &QueryManifest,
    path: &str,
    referenced_sql: &mut BTreeSet<String>,
    findings: &mut Vec<Finding>,
) {
    let subject = format!("{} ({path})", query.id);

    let Some(sql_file) = query.sql_file.as_deref() else {
        findings.push(Finding::error(
            subject,
            "sql_file_exists",
            "manifest tidak menyebut sql_file",
        ));
        return;
    };

    referenced_sql.insert(sql_file.to_string());

    let Some(sql) = catalog.sql_files.get(sql_file) else {
        findings.push(Finding::error(
            subject,
            "sql_file_exists",
            format!("sql_file tidak ada di disk: {sql_file}"),
        ));
        return;
    };

    let stripped = strip_literals_and_comments(sql);
    let trimmed = stripped.trim_start();

    // select_only
    let upper = trimmed.to_ascii_uppercase();
    // `WITH ... SELECT` tetap read-only dan diizinkan eksplisit oleh
    // query_safety.yaml; CTE yang menulis tertangkap pemeriksaan token
    // terlarang di bawah.
    if !upper.starts_with("SELECT") && !upper.starts_with("WITH") {
        findings.push(Finding::error(
            &subject,
            "sql_is_select_only",
            "SQL tidak diawali SELECT maupun WITH ... SELECT",
        ));
    } else if upper.starts_with("WITH") && !contains_word(&stripped, "SELECT") {
        findings.push(Finding::error(
            &subject,
            "sql_is_select_only",
            "SQL diawali WITH tetapi tidak memuat SELECT",
        ));
    }

    // single_statement
    let inner = stripped.trim_end().trim_end_matches(';');
    if inner.contains(';') {
        findings.push(Finding::error(
            &subject,
            "sql_is_single_statement",
            "SQL memuat lebih dari satu statement",
        ));
    }

    // unsafe commands — daftarnya dari policy, bukan konstanta di kode
    for command in &catalog.safety_policy.unsafe_commands {
        if contains_word(&stripped, command) {
            findings.push(Finding::error(
                &subject,
                "no_unsafe_sql_tokens",
                format!("SQL memuat perintah terlarang: {command}"),
            ));
        }
    }

    // placeholder_count_matches_parameters
    let placeholders = placeholder_indexes(&stripped);
    let declared = query.parameters.len();
    let highest = placeholders.iter().copied().max().unwrap_or(0);

    if highest != declared {
        findings.push(Finding::error(
            &subject,
            "placeholder_count_matches_parameters",
            format!("SQL memakai ${highest} tertinggi, manifest mendeklarasikan {declared} parameter"),
        ));
    } else {
        for index in 1..=declared {
            if !placeholders.contains(&index) {
                findings.push(Finding::error(
                    &subject,
                    "placeholder_count_matches_parameters",
                    format!(
                        "parameter ke-{index} ({}) tidak pernah dipakai SQL",
                        query.parameters[index - 1].name
                    ),
                ));
            }
        }
    }

    // office_filter_is_bound (D5: scope ditegakkan DI DALAM SQL)
    if query.guards.require_office_filter == Some(true) {
        // Dicari lewat POSISI parameter authorized_scope, bukan lewat nama
        // kolom: pada tabel m_office scope-nya adalah `o.id = ANY($1)`, dan
        // mencari literal "office_id" akan menuduh query yang justru benar.
        let scope_position = query
            .parameters
            .iter()
            .position(|parameter| parameter.source.as_deref() == Some("authorized_scope"));

        match scope_position {
            None => findings.push(Finding::error(
                &subject,
                "office_filter_is_bound",
                "tidak ada parameter bersumber authorized_scope; scope akan berasal dari input pengguna (I7)",
            )),
            Some(index) => {
                let placeholder = format!("${}", index + 1);

                if !stripped.contains(&format!("ANY({placeholder}")) {
                    findings.push(Finding::error(
                        &subject,
                        "office_filter_is_bound",
                        format!(
                            "parameter scope {placeholder} tidak dipakai sebagai predikat ANY({placeholder}...)"
                        ),
                    ));
                }

                // Pola opsional `($n IS NULL OR ...)` sah untuk filter pilihan,
                // tetapi pada scope ia berarti scope dapat dilewati dengan
                // mengirim NULL — dan itu bukan penyempitan, melainkan pintu.
                let bypass = format!("{placeholder}::bigint[] IS NULL OR");
                if stripped.contains(&bypass) || stripped.contains(&format!("{placeholder} IS NULL OR")) {
                    findings.push(Finding::error(
                        &subject,
                        "office_filter_is_bound",
                        format!("scope {placeholder} dapat dilewati saat NULL; scope tidak boleh opsional"),
                    ));
                }
            }
        }
    }

    if query.output_fields.is_empty() {
        findings.push(Finding::error(
            &subject,
            "output_columns_match_contract",
            "output_fields kosong: kontrak kolom hasil tidak dinyatakan",
        ));
    }

    for field in &query.output_fields {
        match field.sensitivity.as_deref() {
            None => findings.push(Finding::error(
                &subject,
                "every_query_output_has_sensitivity",
                format!("output_field '{}' tanpa sensitivity", field.name),
            )),
            Some(class) if !catalog.sensitivity_classes.contains(class) => {
                // Check milik columns/sensitivity.yaml sendiri: "every query
                // output field must declare one class listed here". Kelas yang
                // tidak terdaftar berarti kebijakan PII tidak punya aturan
                // untuknya — dan yang tidak punya aturan akan lolos diam-diam.
                findings.push(Finding::error(
                    &subject,
                    "every_query_output_has_sensitivity",
                    format!(
                        "output_field '{}' memakai kelas '{class}' yang tidak ada di knowledge/schema/fineract/columns/sensitivity.yaml",
                        field.name
                    ),
                ));
            }
            Some(_) => {}
        }
    }

    // grain (#11): setiap manifest harus menyatakan grain hasilnya, dan tiap
    // kolom grain wajib benar-benar salah satu output_fields. Grain yang kosong
    // membiarkan penggandaan baris tak terdeklarasi; grain yang menyebut kolom
    // yang tidak diterbitkan tidak dapat dibuktikan terhadap hasil.
    let output_names: BTreeSet<&str> = query
        .output_fields
        .iter()
        .map(|field| field.name.as_str())
        .collect();

    if query.grain.is_empty() {
        findings.push(Finding::error(
            &subject,
            "grain_declared",
            "grain kosong: berbutir apa satu baris hasil tidak dinyatakan (#11)",
        ));
    } else if !grain_is_subset(&query.grain, &output_names) {
        for column in &query.grain {
            if !output_names.contains(column.as_str()) {
                findings.push(Finding::error(
                    &subject,
                    "grain_subset_of_output",
                    format!("kolom grain '{column}' bukan salah satu output_fields"),
                ));
            }
        }
    }

    match query.timeout_ms {
        None => findings.push(Finding::warning(
            &subject,
            "timeout_declared",
            "timeout_ms tidak dinyatakan; kelas timeout probe vs analytical tidak dapat ditentukan",
        )),
        Some(ms) if !is_timeout_class(ms) => findings.push(Finding::error(
            &subject,
            "timeout_class",
            format!(
                "timeout_ms {ms} bukan kelas PROBE ({PROBE_QUERY_TIMEOUT_MS}) atau ANALYTICAL ({ANALYTICAL_QUERY_TIMEOUT_MS}); operations/runtime.md hanya mengenal dua kelas — satu angka untuk dua kelas query adalah kesalahan yang bisa diprediksi"
            ),
        )),
        Some(_) => {}
    }
}

/// Dua kelas timeout query yang diizinkan katalog (`operations/runtime.md`):
/// probe cepat & selektif vs query beragregasi/window/LATERAL. Tidak ada nilai
/// ketiga — 3 detik tidak cukup untuk agregasi produksi, dan satu angka untuk
/// dua kelas adalah kesalahan yang bisa diprediksi sekarang.
pub const PROBE_QUERY_TIMEOUT_MS: u64 = 3_000;
pub const ANALYTICAL_QUERY_TIMEOUT_MS: u64 = 15_000;

fn is_timeout_class(ms: u64) -> bool {
    ms == PROBE_QUERY_TIMEOUT_MS || ms == ANALYTICAL_QUERY_TIMEOUT_MS
}

/// Setiap kolom grain harus benar-benar salah satu output_fields manifest —
/// grain yang menyebut kolom yang tidak diterbitkan tidak dapat dibuktikan.
fn grain_is_subset(grain: &[String], output_names: &BTreeSet<&str>) -> bool {
    grain
        .iter()
        .all(|column| output_names.contains(column.as_str()))
}

/// FIN-153 (L1C): `knowledge/domains/*.yaml` dan `knowledge/data-scope/areas/*.yaml`
/// harus selaras dengan `docs/data/dataset-inventory.md` dan
/// `docs/product/2026-09-09-dataset-scope-decisions.md`. Enam kontradiksi
/// spesifik ditemukan pada audit FIN-153 dan tidak boleh masuk lagi; daftar
/// di bawah membuat kegagalannya mekanis, bukan hanya prosa `CARRY-OVER.md`.
fn check_domain_alignment(catalog: &Catalog, findings: &mut Vec<Finding>) {
    for loaded in &catalog.domains {
        check_domain(&loaded.entry, &loaded.path, &catalog.inventory, findings);
    }
    for loaded in &catalog.areas {
        check_area(&loaded.entry, &loaded.path, &catalog.inventory, findings);
    }
}

/// Status baris `docs/data/dataset-inventory.md` (`catalog::inventory::parse`)
/// yang konsisten dengan sebuah domain berstatus `gap`: agreed-tapi-uncontracted.
const GAP_ROW_STATUS: &str = "gap";
/// ... dengan domain berstatus `deferred`: onboarding-dependent (D10/D15a/b).
const DEFERRED_ROW_STATUS: &str = "deferred-onboarding";
/// ... dengan sebuah `supported_intents`/capability disetujui.
const INHERITED_ROW_STATUS: &str = "inherited";
/// ... dengan sebuah `unsupported_intents` (hard reject, `BlockedByPolicy`).
const EXCLUDED_ROW_STATUS: &str = "excluded";

/// Lookup satu `inventory_ref` terhadap peta status sungguhan
/// (`catalog.inventory`, diparse dari dokumen — bukan konstanta yang
/// diduplikasi). `None` berarti id itu tidak menamai baris nyata mana pun,
/// mis. `FAKE-999`.
fn row_status<'a>(inventory: &'a BTreeMap<String, String>, id: &str) -> Option<&'a str> {
    inventory.get(id).map(String::as_str)
}

/// Domain yang cakupannya disepakati baseline §1/D01–D15 tetapi belum punya
/// capability disetujui — `deferred` (istilah MVP) mengaburkan ini dengan
/// domain onboarding-dependent (D10). Reintroduksi kontradiksi #1/#4.
const MUST_BE_GAP_NOT_DEFERRED: &[&str] = &["loan", "tax", "accounting", "audit"];

/// Intent yang secara eksplisit dan permanen dikecualikan dokumen (D12: tanpa
/// trial balance/laporan keuangan penuh; §1: bukan savings-only) tetapi
/// pernah muncul di `supported_intents`/`gap_intents` — kontradiksi #1, #6.
const FORBIDDEN_GAP_OR_SUPPORTED_PHRASES: &[(&str, &[&str])] = &[
    (
        "accounting",
        &[
            "trial balance",
            "balance sheet",
            "income statement",
            "profit and loss",
            "financial statements",
        ],
    ),
    (
        "group_center",
        &["group-owned savings reporting", "center-level savings summaries"],
    ),
];

/// Intent yang disepakati cakupannya (selektif/gap, bukan ditolak kebijakan)
/// tetapi pernah muncul di `unsupported_intents` — kontradiksi #3 (address).
const FORBIDDEN_HARD_REJECT_PHRASES: &[(&str, &[&str])] = &[(
    "client",
    &["address-level reporting", "address level reporting"],
)];

fn check_domain(
    domain: &Domain,
    path: &str,
    inventory: &BTreeMap<String, String>,
    findings: &mut Vec<Finding>,
) {
    let subject = format!("{} ({path})", domain.id);

    if domain.inventory_refs.is_empty() {
        findings.push(Finding::error(
            &subject,
            "domain_requires_inventory_ref",
            "domain tidak merujuk satu pun baris docs/data/dataset-inventory.md",
        ));
    }

    // Setiap ref harus benar-benar menamai baris di dokumen — bukan hanya
    // "tidak kosong". Ini yang menangkap id palsu (mis. FAKE-999).
    for id in &domain.inventory_refs {
        match row_status(inventory, id) {
            None => findings.push(Finding::error(
                &subject,
                "inventory_ref_must_exist_in_dataset_inventory",
                format!(
                    "'{id}' tidak ditemukan sebagai baris nyata di \
                     docs/data/dataset-inventory.md"
                ),
            )),
            Some(status) => {
                let expected = match domain.status.as_deref() {
                    Some("gap") => Some(GAP_ROW_STATUS),
                    Some("deferred") => Some(DEFERRED_ROW_STATUS),
                    _ => None,
                };
                if let Some(expected) = expected
                    && status != expected
                {
                    findings.push(Finding::error(
                        &subject,
                        "domain_status_must_match_referenced_row_status",
                        format!(
                            "domain berstatus '{}' tetapi baris '{id}' berstatus '{status}' \
                             di dataset-inventory.md (diharapkan '{expected}')",
                            domain.status.as_deref().unwrap_or("?")
                        ),
                    ));
                }
            }
        }
    }

    if MUST_BE_GAP_NOT_DEFERRED.contains(&domain.id.as_str())
        && domain.status.as_deref() == Some("deferred")
    {
        findings.push(Finding::error(
            &subject,
            "gap_domain_not_mislabeled_deferred",
            "domain ini cakupan disepakati tanpa capability (gap, dataset-inventory.md), \
             bukan deferred ke onboarding deployment (D10/D15a/b)",
        ));
    }

    // Setiap intent yang mengutip inventory_ref-nya sendiri diperiksa
    // terhadap status baris yang SEBENARNYA (bukan diklaim) — inilah yang
    // menangkap "supported intent untuk baris gap" dan "hard reject untuk
    // baris gap" yang diminta reviewer, secara mekanis dan umum (tidak
    // spesifik per domain seperti dua konstanta di bawah).
    check_intent_refs(
        &subject,
        "supported_intents",
        &domain.supported_intents,
        inventory,
        INHERITED_ROW_STATUS,
        "supported_intent_ref_must_be_inherited",
        findings,
    );
    check_intent_refs(
        &subject,
        "gap_intents",
        &domain.gap_intents,
        inventory,
        GAP_ROW_STATUS,
        "gap_intent_ref_must_be_gap",
        findings,
    );
    check_intent_refs(
        &subject,
        "unsupported_intents",
        &domain.unsupported_intents,
        inventory,
        EXCLUDED_ROW_STATUS,
        "hard_reject_ref_must_be_excluded",
        findings,
    );

    let gap_or_supported: Vec<String> = domain
        .supported_intents
        .iter()
        .chain(domain.gap_intents.iter())
        .map(|intent| intent.phrase().to_lowercase())
        .collect();

    if let Some((_, forbidden)) = FORBIDDEN_GAP_OR_SUPPORTED_PHRASES
        .iter()
        .find(|(id, _)| *id == domain.id)
    {
        for phrase in *forbidden {
            if gap_or_supported.iter().any(|text| text.contains(phrase)) {
                findings.push(Finding::error(
                    &subject,
                    "gap_intents_exclude_out_of_scope_phrases",
                    format!(
                        "intent '{phrase}' dikecualikan permanen oleh dokumen scope dan tidak \
                         boleh muncul di supported_intents/gap_intents"
                    ),
                ));
            }
        }
    }

    if let Some((_, forbidden)) = FORBIDDEN_HARD_REJECT_PHRASES
        .iter()
        .find(|(id, _)| *id == domain.id)
    {
        for phrase in *forbidden {
            if domain
                .unsupported_intents
                .iter()
                .any(|intent| intent.phrase().to_lowercase().contains(phrase))
            {
                findings.push(Finding::error(
                    &subject,
                    "gap_item_not_hard_rejected",
                    format!(
                        "'{phrase}' cakupannya disepakati (selektif/gap), bukan ditolak \
                         kebijakan — jangan cantumkan di unsupported_intents"
                    ),
                ));
            }
        }
    }

    if domain.id == "client"
        && !domain.gap_intents.iter().any(|intent| {
            let lower = intent.phrase().to_lowercase();
            lower.contains("loan") || lower.contains("share")
        })
    {
        findings.push(Finding::error(
            &subject,
            "client_all_account_types_declared",
            "client mencakup seluruh jenis account milik client (§1, CLI-5) — gap_intents \
             harus menyatakan loan/share account roster, bukan hanya savings",
        ));
    }
}

/// Bandingkan setiap entri yang mengutip `inventory_ref` terhadap status
/// baris sungguhan; entri tanpa ref (`IntentRef::Plain`) dilewati — itu bukan
/// pernyataan atas satu baris inventaris tertentu (mis. "create loan
/// account").
fn check_intent_refs(
    subject: &str,
    field: &str,
    intents: &[IntentRef],
    inventory: &BTreeMap<String, String>,
    expected_status: &str,
    check_id: &str,
    findings: &mut Vec<Finding>,
) {
    for intent in intents {
        let Some(id) = intent.inventory_ref() else {
            continue;
        };
        match row_status(inventory, id) {
            None => findings.push(Finding::error(
                subject,
                "inventory_ref_must_exist_in_dataset_inventory",
                format!(
                    "{field} '{}' mengutip '{id}', tidak ditemukan sebagai baris nyata di \
                     docs/data/dataset-inventory.md",
                    intent.phrase()
                ),
            )),
            Some(status) if status != expected_status => findings.push(Finding::error(
                subject,
                check_id,
                format!(
                    "{field} '{}' mengutip '{id}' (status sebenarnya '{status}'), diharapkan \
                     '{expected_status}'",
                    intent.phrase()
                ),
            )),
            Some(_) => {}
        }
    }
}

fn check_area(
    area: &DataScopeArea,
    path: &str,
    inventory: &BTreeMap<String, String>,
    findings: &mut Vec<Finding>,
) {
    let subject = format!("{} ({path})", area.id);

    if area.inventory_refs.is_empty() {
        findings.push(Finding::error(
            &subject,
            "area_requires_inventory_ref",
            "area tidak merujuk satu pun baris docs/data/dataset-inventory.md",
        ));
    }

    for id in &area.inventory_refs {
        match row_status(inventory, id) {
            None => findings.push(Finding::error(
                &subject,
                "inventory_ref_must_exist_in_dataset_inventory",
                format!(
                    "'{id}' tidak ditemukan sebagai baris nyata di \
                     docs/data/dataset-inventory.md"
                ),
            )),
            Some(status) => {
                let expected = match area.status.as_deref() {
                    Some("rejected_group") => Some(EXCLUDED_ROW_STATUS),
                    _ => None,
                };
                if let Some(expected) = expected
                    && status != expected
                {
                    findings.push(Finding::error(
                        &subject,
                        "area_status_must_match_referenced_row_status",
                        format!(
                            "area berstatus '{}' tetapi baris '{id}' berstatus '{status}' di \
                             dataset-inventory.md (diharapkan '{expected}')",
                            area.status.as_deref().unwrap_or("?")
                        ),
                    ));
                }
            }
        }
    }

    // D09: audit tindakan Fineract sendiri (`m_portfolio_command_source`)
    // cakupannya disepakati — kontradiksi #2 bila ia masuk excluded_tables.
    if area.id == "audit_users_operations"
        && area
            .excluded_tables
            .iter()
            .any(|table| table == "m_portfolio_command_source")
    {
        findings.push(Finding::error(
            &subject,
            "audit_source_command_in_scope",
            "'m_portfolio_command_source' adalah D09 (source audit Fineract, cakupan \
             disepakati) — tidak boleh masuk excluded_tables",
        ));
    }

    // CLI-6: alamat client selektif/gap — kontradiksi #3 bila ia masuk
    // excluded_tables (BlockedByPolicy), bukan conditional_tables (gap).
    if area.id == "client_foundation"
        && area
            .excluded_tables
            .iter()
            .any(|table| table == "m_client_address")
    {
        findings.push(Finding::error(
            &subject,
            "client_address_not_hard_excluded",
            "'m_client_address' adalah CLI-6 (selektif/gap, §1) — tidak boleh masuk \
             excluded_tables, pindahkan ke conditional_tables",
        ));
    }
}

/// Integritas tautan resolver — I6: koneksi yang hanya hidup sebagai prosa
/// dicatat sebagai utang, bukan dianggap selesai.
///
/// Empat hal yang diperiksa:
/// 1. `resolves:` menunjuk dataset + shape yang benar-benar ada;
/// 2. satu shape tidak dibungkus dua manifest (mana yang dipakai akan menjadi
///    tebakan urutan pembacaan direktori);
/// 3. resolver hanya boleh punya parameter bersumber `authorized_scope` — satu
///    parameter lain berarti ada nilai yang datang dari luar otorisasi;
/// 4. shape resolver punya `entity.id_field`, tanpanya opsi tidak punya id.
fn check_resolvers(catalog: &Catalog, findings: &mut Vec<Finding>) {
    let mut wrapped: BTreeMap<(&str, &str), Vec<&str>> = BTreeMap::new();

    for loaded in &catalog.queries {
        let Some(resolves) = &loaded.entry.resolves else {
            continue;
        };
        let subject = format!("{} ({})", loaded.entry.id, loaded.path);

        wrapped
            .entry((&resolves.dataset_id, &resolves.shape_id))
            .or_default()
            .push(&loaded.entry.id);

        let Some(dataset) = catalog.dataset(&resolves.dataset_id) else {
            findings.push(Finding::error(
                &subject,
                "resolver_shape_exists",
                format!("dataset '{}' tidak ada di knowledge/datasets", resolves.dataset_id),
            ));
            continue;
        };

        if !dataset
            .shapes
            .iter()
            .any(|shape| shape.id == resolves.shape_id)
        {
            findings.push(Finding::error(
                &subject,
                "resolver_shape_exists",
                format!(
                    "dataset '{}' tidak punya shape '{}'",
                    resolves.dataset_id, resolves.shape_id
                ),
            ));
        }

        if dataset.entity.is_none() {
            findings.push(Finding::error(
                &subject,
                "resolver_entity_declared",
                format!(
                    "dataset '{}' tanpa blok entity: opsi tidak punya id maupun label",
                    resolves.dataset_id
                ),
            ));
        }

        for parameter in &loaded.entry.parameters {
            if parameter.source.as_deref() != Some("authorized_scope") {
                findings.push(Finding::error(
                    &subject,
                    "resolver_parameters_are_scope_only",
                    format!(
                        "resolver mendeklarasikan parameter '{}' yang bukan authorized_scope; \
                         nilainya akan datang dari luar otorisasi (I7)",
                        parameter.name
                    ),
                ));
            }
        }
    }

    for ((dataset_id, shape_id), manifests) in &wrapped {
        if manifests.len() > 1 {
            findings.push(Finding::error(
                format!("{dataset_id}/{shape_id}"),
                "resolver_shape_has_one_query",
                format!("shape dibungkus lebih dari satu manifest: {}", manifests.join(", ")),
            ));
        }
    }

    // Shape ber-role resolver yang tidak dibungkus manifest mana pun: ia tidak
    // dapat menerbitkan opsi, jadi slot yang menunjuknya akan gagal diam-diam.
    for loaded in &catalog.datasets {
        for shape in &loaded.entry.shapes {
            if shape.role.as_deref() != Some("resolver") {
                continue;
            }
            if !wrapped.contains_key(&(loaded.entry.id.as_str(), shape.id.as_str())) {
                findings.push(Finding::warning(
                    format!("{} ({})", loaded.entry.id, loaded.path),
                    "resolver_shape_has_one_query",
                    format!(
                        "shape resolver '{}' tidak dibungkus manifest query mana pun; \
                         slot yang menunjuknya tidak akan punya opsi",
                        shape.id
                    ),
                ));
            }
        }
    }
}

fn check_capability(
    catalog: &Catalog,
    capability: &Capability,
    path: &str,
    queries: &BTreeMap<&str, &QueryManifest>,
    findings: &mut Vec<Finding>,
) {
    let subject = format!("{} ({path})", capability.id);

    let Some(query_id) = capability.query_id.as_deref() else {
        findings.push(Finding::error(
            subject,
            "query_exists",
            "capability tidak menyebut query_id",
        ));
        return;
    };

    let Some(query) = queries.get(query_id) else {
        findings.push(Finding::error(
            subject,
            "query_exists",
            format!("query_id tidak ada di knowledge/queries: {query_id}"),
        ));
        return;
    };

    let query_parameters: BTreeSet<&str> = query
        .parameters
        .iter()
        .map(|parameter| parameter.name.as_str())
        .collect();

    // required_parameters_match_query
    for name in capability.parameters.keys() {
        if !query_parameters.contains(name.as_str()) {
            findings.push(Finding::error(
                &subject,
                "required_parameters_match_query",
                format!("parameter '{name}' tidak dikenal query {query_id}"),
            ));
        }
    }

    for parameter in &query.parameters {
        if !parameter.required || parameter.source.as_deref() == Some("authorized_scope") {
            continue;
        }
        let supplied = capability
            .parameters
            .get(&parameter.name)
            .is_some_and(|declared| declared.required || declared.default.is_some());

        if !supplied {
            findings.push(Finding::error(
                &subject,
                "required_parameters_match_query",
                format!(
                    "query mewajibkan '{}' tetapi capability tidak menyediakannya (tanpa required maupun default)",
                    parameter.name
                ),
            ));
        }
    }

    // Slot identitas: yang menuntut resolver wajib benar-benar punya satu, dan
    // yang punya wajib menghasilkan kolom binding yang dijanjikan. K1 hanya
    // berlaku bila tautannya diperiksa — kalau tidak, slot itu menjadi teks
    // bebas yang menyamar sebagai pilihan.
    for (name, declared) in &capability.parameters {
        let Some(probe) = &declared.probe else {
            continue;
        };

        match catalog.resolver_for(&probe.dataset_id, &probe.shape_id) {
            None => findings.push(Finding::error(
                &subject,
                "probe_has_resolver",
                format!(
                    "parameter '{name}' menunjuk probe {}/{} yang tidak punya query resolver disetujui",
                    probe.dataset_id, probe.shape_id
                ),
            )),
            Some(resolver) => {
                if !resolver
                    .query
                    .output_fields
                    .iter()
                    .any(|field| field.name == probe.output_slot)
                {
                    findings.push(Finding::error(
                        &subject,
                        "probe_has_resolver",
                        format!(
                            "parameter '{name}' mengambil output_slot '{}' yang tidak ada di kolom hasil {}",
                            probe.output_slot, resolver.query.id
                        ),
                    ));
                }
            }
        }
    }

    // Slot `transient_sensitive_input` tanpa probe: tidak dapat ditanyakan sama
    // sekali (K1). Warning, bukan error — capability-nya sah, ia hanya tidak
    // dapat dijalankan sampai resolvernya ada, dan Engine menyatakan itu pada
    // response alih-alih menebak.
    for parameter in &query.parameters {
        if parameter.source.as_deref() != Some("transient_sensitive_input") {
            continue;
        }
        let has_probe = capability
            .parameters
            .get(&parameter.name)
            .is_some_and(|declared| declared.probe.is_some());

        if !has_probe {
            findings.push(Finding::warning(
                &subject,
                "identity_slot_has_resolver",
                format!(
                    "slot identitas '{}' tidak punya probe: permintaan yang memerlukannya akan \
                     dijawab Unsupported, bukan ditanyakan sebagai teks bebas",
                    parameter.name
                ),
            ));
        }
    }

    // no_pii_output
    let claims_no_pii = capability
        .request_shape
        .as_ref()
        .and_then(|shape| shape.pii.as_deref())
        == Some("none");

    if claims_no_pii {
        for field in &query.output_fields {
            let sensitivity = field.sensitivity.as_deref().unwrap_or("tidak dinyatakan");
            // `masked_output` adalah turunan yang identifier aslinya sudah
            // dipotong (columns/sensitivity.yaml), jadi ia tidak membuat sebuah
            // capability aggregate berubah menjadi mengungkap identitas.
            if sensitivity != "public_business" && sensitivity != "masked_output" {
                findings.push(Finding::error(
                    &subject,
                    "no_pii_output",
                    format!(
                        "request_shape.pii=none tetapi kolom '{}' bersensitivitas {sensitivity}",
                        field.name
                    ),
                ));
            }
        }
    }

    // office_scope_required
    let requires_scope = capability
        .guards
        .get("require_office_scope")
        .and_then(serde_yaml::Value::as_bool)
        == Some(true);

    if requires_scope && query.guards.require_office_filter != Some(true) {
        findings.push(Finding::error(
            &subject,
            "office_scope_required",
            format!("capability menuntut office scope tetapi query {query_id} tidak require_office_filter"),
        ));
    }

    // chart_matches_grain (responses.md §7): chart hanya sah di atas bentuk data
    // yang dideklarasikan query. Di sini, bukan saat runtime — chart yang tidak
    // mungkin kompatibel adalah deklarasi yang salah, bukan data yang turun.
    if let Some(problem) = chart_problem(capability, query) {
        findings.push(Finding::error(&subject, "chart_matches_grain", problem));
    }

    if capability.examples.is_empty() {
        findings.push(Finding::warning(
            &subject,
            "examples_present",
            "tanpa examples: klaim 'contoh dijalankan ujung ke ujung' tidak dapat diuji",
        ));
    }

    if capability.description.is_none() && capability.display_name.is_none() {
        let has_retrieval_text =
            !capability.examples.is_empty() || !capability.supported_intents.is_empty();

        let finding = if has_retrieval_text {
            Finding::warning(
                &subject,
                "prose_present",
                "tanpa display_name/description; retrieval hanya bertumpu pada examples",
            )
        } else {
            Finding::error(
                &subject,
                "prose_present",
                "tanpa prosa maupun examples; capability tidak dapat diindeks sehingga tidak akan pernah terpilih",
            )
        };
        findings.push(finding);
    }
}

/// `None` = tidak ada chart, atau chart-nya sah atas grain query.
fn chart_problem(capability: &Capability, query: &QueryManifest) -> Option<String> {
    let chart = capability.chart.as_ref()?;

    if !CHART_KINDS.contains(&chart.kind.as_str()) {
        return Some(format!(
            "chart.kind '{}' tidak dikenal; yang dikenal: {}",
            chart.kind,
            CHART_KINDS.join(", ")
        ));
    }

    query.time_dimension().is_none().then(|| {
        format!(
            "chart time_series di atas query {} yang grain-nya tidak memuat kolom bertipe date",
            query.id
        )
    })
}

/// Cakupan validator ini, dinyatakan terbuka supaya "lulus" tidak dibaca
/// sebagai "seluruh katalog terbukti benar" (I5).
pub fn coverage() -> &'static [&'static str] {
    &[
        "capabilities/**: query_id, parameter, PII output, office scope, prosa",
        "capabilities/**: chart hanya time_series di atas grain yang memuat kolom date",
        "queries/**: sql_file, SELECT-only, single statement, token terlarang, placeholder, office binding, output_fields, grain dinyatakan + subset output_fields",
        "queries/**/*.sql (non-dataset): keterhubungan ke manifest",
        "resolver: resolves: -> dataset/shape ada, satu manifest per shape, parameter hanya authorized_scope, entity dideklarasikan",
        "capabilities/**: probe: -> resolver ada dan output_slot benar-benar kolom hasilnya",
        "domains/**, data-scope/areas/**: inventory_refs wajib ada; enam kontradiksi FIN-153 (D12 trial balance, D09 audit source, address selektif, gap vs deferred, client seluruh jenis account, group/center di luar savings) tidak boleh masuk lagi",
        "BELUM DIVALIDASI: datasets/** selain entity+shape resolver, fragment *.frag.sql, metrics/**, schema/**, parameters/**, responses/**",
        "BELUM DIBUKTIKAN oleh validator mana pun: kebenaran angka, grain, dan semantik as-of (jalankan contohnya)",
    ]
}

/// Buang string literal dan komentar sebelum mencari token.
///
/// Tanpa ini, kata `DROP` di dalam komentar atau literal akan dilaporkan
/// sebagai perintah terlarang, dan validator yang memberi alarm palsu akan
/// diabaikan orang.
fn strip_literals_and_comments(sql: &str) -> String {
    let mut output = String::with_capacity(sql.len());
    let mut chars = sql.chars().peekable();
    let mut in_string = false;

    while let Some(character) = chars.next() {
        if in_string {
            if character == '\'' {
                in_string = false;
            }
            continue;
        }

        match character {
            '\'' => {
                in_string = true;
                output.push(' ');
            }
            '-' if chars.peek() == Some(&'-') => {
                for next in chars.by_ref() {
                    if next == '\n' {
                        output.push('\n');
                        break;
                    }
                }
            }
            _ => output.push(character),
        }
    }

    output
}

fn contains_word(haystack: &str, word: &str) -> bool {
    let upper = haystack.to_ascii_uppercase();
    let needle = word.to_ascii_uppercase();

    upper
        .match_indices(&needle)
        .any(|(index, matched)| {
            let before = upper[..index].chars().next_back();
            let after = upper[index + matched.len()..].chars().next();
            let boundary = |character: Option<char>| {
                character.is_none_or(|value| !value.is_ascii_alphanumeric() && value != '_')
            };
            boundary(before) && boundary(after)
        })
}

fn placeholder_indexes(sql: &str) -> BTreeSet<usize> {
    let mut indexes = BTreeSet::new();
    let bytes: Vec<char> = sql.chars().collect();

    let mut position = 0;
    while position < bytes.len() {
        if bytes[position] == '$' {
            let mut end = position + 1;
            while end < bytes.len() && bytes[end].is_ascii_digit() {
                end += 1;
            }
            if end > position + 1 {
                let digits: String = bytes[position + 1..end].iter().collect();
                if let Ok(index) = digits.parse::<usize>() {
                    indexes.insert(index);
                }
            }
            position = end;
        } else {
            position += 1;
        }
    }

    indexes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsafe_token_inside_literal_or_comment_is_not_reported() {
        let sql = "SELECT 'DROP TABLE x' AS note -- DELETE nanti\nFROM m_client";
        let stripped = strip_literals_and_comments(sql);

        assert!(!contains_word(&stripped, "DROP"));
        assert!(!contains_word(&stripped, "DELETE"));
        assert!(contains_word(&stripped, "SELECT"));
    }

    #[test]
    fn word_match_is_bounded() {
        assert!(contains_word("SELECT a FROM t", "SELECT"));
        // `updated_at` bukan perintah UPDATE.
        assert!(!contains_word("SELECT updated_at FROM t", "UPDATE"));
    }

    #[test]
    fn placeholders_are_collected_by_index() {
        let found = placeholder_indexes("WHERE a = $1::date AND b = ANY($3::bigint[]) AND c = $1");
        assert_eq!(found.into_iter().collect::<Vec<_>>(), vec![1, 3]);
    }

    #[test]
    fn semicolon_inside_literal_does_not_count_as_second_statement() {
        let stripped = strip_literals_and_comments("SELECT 'a;b' FROM t;");
        assert!(!stripped.trim_end().trim_end_matches(';').contains(';'));
    }

    #[test]
    fn grain_must_be_subset_of_output_fields() {
        let output_names: BTreeSet<&str> =
            ["client_id", "savings_account_id", "currency_code"].into_iter().collect();

        // Grain yang seluruh kolomnya diterbitkan diterima.
        let valid = vec!["client_id".to_string(), "savings_account_id".to_string()];
        assert!(grain_is_subset(&valid, &output_names));

        // Satu kolom yang tidak ada di output_fields menolak seluruh grain.
        let unknown = vec!["client_id".to_string(), "office_id".to_string()];
        assert!(!grain_is_subset(&unknown, &output_names));
    }

    #[test]
    fn timeout_must_be_one_of_two_classes() {
        assert!(is_timeout_class(PROBE_QUERY_TIMEOUT_MS));
        assert!(is_timeout_class(ANALYTICAL_QUERY_TIMEOUT_MS));
        // Nilai warisan yang tidak memetakan ke kelas mana pun ditolak.
        assert!(!is_timeout_class(5_000));
        assert!(!is_timeout_class(8_000));
    }

    fn manifest(grain: &[&str]) -> QueryManifest {
        serde_yaml::from_str(&format!(
            "id: q\noutput_fields:\n  - {{ name: month_start, type: date }}\n  - {{ name: office_id, type: integer }}\ngrain: [{}]\n",
            grain.join(", ")
        ))
        .unwrap()
    }

    fn charted(kind: &str) -> Capability {
        serde_yaml::from_str(&format!("id: c\nquery_id: q\nchart: {{ kind: {kind} }}\n")).unwrap()
    }

    #[test]
    fn chart_requires_a_declared_time_grain() {
        // Grain tanpa kolom date: chart adalah kesalahan katalog, bukan
        // downgrade saat runtime.
        assert!(chart_problem(&charted("time_series"), &manifest(&["office_id"])).is_some());
        assert!(chart_problem(&charted("time_series"), &manifest(&["month_start"])).is_none());
        // Jenis chart di luar kosakata ditolak.
        assert!(chart_problem(&charted("pie"), &manifest(&["month_start"])).is_some());
        // Tanpa deklarasi chart tidak ada yang diperiksa.
        let plain: Capability = serde_yaml::from_str("id: c\nquery_id: q\n").unwrap();
        assert!(chart_problem(&plain, &manifest(&["office_id"])).is_none());
    }

    fn parsed_domain(yaml: &str) -> Domain {
        serde_yaml::from_str(yaml).unwrap()
    }

    fn parsed_area(yaml: &str) -> DataScopeArea {
        serde_yaml::from_str(yaml).unwrap()
    }

    fn has_check(findings: &[Finding], check: &str) -> bool {
        findings.iter().any(|finding| finding.check == check)
    }

    /// Peta status tetap untuk unit test — dibuat sekali di sini, bukan
    /// diklaim benar oleh domain/area yang diuji. Nilainya sengaja mencampur
    /// kelima kategori kosakata `dataset-inventory.md` supaya setiap test
    /// bisa memilih id yang statusnya SESUAI (kasus positif) atau SENGAJA
    /// TIDAK sesuai (kasus negatif/reintroduksi kontradiksi), tanpa
    /// tergantung pada dokumen sungguhan berubah.
    fn test_inventory() -> BTreeMap<String, String> {
        [
            ("CLI-1", "inherited"),
            ("CLI-4", "gap"),
            ("CLI-5", "gap"),
            ("CLI-6", "gap"),
            ("D09", "gap"),
            ("D10", "deferred-onboarding"),
            ("D12", "gap"),
            ("D13", "excluded"),
            ("LOAN-1", "gap"),
        ]
        .into_iter()
        .map(|(id, status)| (id.to_string(), status.to_string()))
        .collect()
    }

    // FIN-153 (L1C): domain/area tanpa rujukan inventaris tidak punya
    // otoritas cakupan — build-order.md §3 L1C "Done when" poin 1.
    #[test]
    fn domain_without_inventory_ref_is_rejected() {
        let domain = parsed_domain("id: loan\nstatus: gap\n");
        let mut findings = Vec::new();
        check_domain(&domain, "path", &test_inventory(), &mut findings);
        assert!(has_check(&findings, "domain_requires_inventory_ref"));
    }

    #[test]
    fn area_without_inventory_ref_is_rejected() {
        let area = parsed_area("id: tax\n");
        let mut findings = Vec::new();
        check_area(&area, "path", &test_inventory(), &mut findings);
        assert!(has_check(&findings, "area_requires_inventory_ref"));
    }

    // Reviewer (2026-09-27, "FIN-153 validator acceptance still unmet"): un
    // id yang tidak menamai baris nyata (mis. FAKE-999) harus gagal — bukan
    // hanya "tidak kosong".
    #[test]
    fn a_fake_inventory_ref_that_names_no_real_row_is_rejected() {
        let domain = parsed_domain("id: loan\nstatus: gap\ninventory_refs: [FAKE-999]\n");
        let mut findings = Vec::new();
        check_domain(&domain, "path", &test_inventory(), &mut findings);
        assert!(has_check(&findings, "inventory_ref_must_exist_in_dataset_inventory"));

        let area = parsed_area("id: tax\ninventory_refs: [FAKE-999]\n");
        let mut findings = Vec::new();
        check_area(&area, "path", &test_inventory(), &mut findings);
        assert!(has_check(&findings, "inventory_ref_must_exist_in_dataset_inventory"));
    }

    // A `gap` domain whose own inventory_refs actually resolve to `inherited`
    // (or any other mismatched status) means the domain-level status claim
    // itself contradicts the document, not just a missing citation.
    #[test]
    fn gap_domain_referencing_an_inherited_row_is_rejected() {
        let domain = parsed_domain("id: loan\nstatus: gap\ninventory_refs: [CLI-1]\n");
        let mut findings = Vec::new();
        check_domain(&domain, "path", &test_inventory(), &mut findings);
        assert!(has_check(&findings, "domain_status_must_match_referenced_row_status"));
    }

    // Reviewer: "a supported intent for a gap-only row fails".
    #[test]
    fn supported_intent_citing_a_gap_row_is_rejected() {
        let domain = parsed_domain(
            "id: client\nstatus: approved_mvp\ninventory_refs: [CLI-1]\nsupported_intents:\n  - { phrase: \"client address\", inventory_ref: CLI-6 }\n",
        );
        let mut findings = Vec::new();
        check_domain(&domain, "path", &test_inventory(), &mut findings);
        assert!(has_check(&findings, "supported_intent_ref_must_be_inherited"));
    }

    // Reviewer: "a hard reject for a gap row fails".
    #[test]
    fn hard_reject_citing_a_gap_row_is_rejected() {
        let domain = parsed_domain(
            "id: client\nstatus: approved_mvp\ninventory_refs: [CLI-1]\ngap_intents:\n  - { phrase: \"client loan account roster\", inventory_ref: CLI-5 }\n  - { phrase: \"client share account roster\", inventory_ref: CLI-5 }\nunsupported_intents:\n  - { phrase: \"client address\", inventory_ref: CLI-6 }\n",
        );
        let mut findings = Vec::new();
        check_domain(&domain, "path", &test_inventory(), &mut findings);
        assert!(has_check(&findings, "hard_reject_ref_must_be_excluded"));
    }

    // A `gap_intent` citing a row this document does not mark `gap` (mis. it
    // is actually `excluded`, D13-style) is the general form of the same bug.
    #[test]
    fn gap_intent_citing_an_excluded_row_is_rejected() {
        let domain = parsed_domain(
            "id: accounting\nstatus: gap\ninventory_refs: [D12]\ngap_intents:\n  - { phrase: \"stretchy report execution\", inventory_ref: D13 }\n",
        );
        let mut findings = Vec::new();
        check_domain(&domain, "path", &test_inventory(), &mut findings);
        assert!(has_check(&findings, "gap_intent_ref_must_be_gap"));
    }

    // D13 (Fineract's own stretchy reports) stays excluded regardless: an
    // out-of-scope-style area citing D13 must pass cleanly when its status
    // and citation actually agree.
    #[test]
    fn d13_excluded_area_remains_correctly_modeled() {
        let area = parsed_area("id: out_of_scope_areas\nstatus: rejected_group\ninventory_refs: [D13]\n");
        let mut findings = Vec::new();
        check_area(&area, "path", &test_inventory(), &mut findings);
        assert!(findings.is_empty());

        // The inverse — an out-of-scope area citing a merely-`gap` row — is
        // the same class of bug as the client-address contradiction, now
        // caught mechanically instead of only via the two hardcoded phrase
        // lists below.
        let mislabeled = parsed_area("id: out_of_scope_areas\nstatus: rejected_group\ninventory_refs: [D12]\n");
        let mut findings = Vec::new();
        check_area(&mislabeled, "path", &test_inventory(), &mut findings);
        assert!(has_check(&findings, "area_status_must_match_referenced_row_status"));
    }

    // Kontradiksi #4: tax/loan/accounting/audit "deferred" (istilah MVP)
    // mengaburkan gap (disepakati, belum dikontrak) dengan deferred-onboarding
    // (D10/D15a/b).
    #[test]
    fn loan_and_tax_mislabeled_deferred_are_rejected() {
        for id in ["loan", "tax", "accounting", "audit"] {
            let domain = parsed_domain(&format!(
                "id: {id}\nstatus: deferred\ninventory_refs: [D10]\n"
            ));
            let mut findings = Vec::new();
            check_domain(&domain, "path", &test_inventory(), &mut findings);
            assert!(
                has_check(&findings, "gap_domain_not_mislabeled_deferred"),
                "{id} seharusnya ditolak sebagai deferred"
            );
        }

        // Domain onboarding-dependent yang sungguh deferred (bukan di daftar
        // MUST_BE_GAP_NOT_DEFERRED) tidak boleh ikut ditolak.
        let custom_datatables = parsed_domain(
            "id: custom_datatables\nstatus: deferred\ninventory_refs: [D10]\n",
        );
        let mut findings = Vec::new();
        check_domain(&custom_datatables, "path", &test_inventory(), &mut findings);
        assert!(!has_check(&findings, "gap_domain_not_mislabeled_deferred"));
    }

    // Kontradiksi #1: D12 mengecualikan trial balance secara permanen; ia
    // tidak boleh muncul di supported_intents/gap_intents accounting.
    #[test]
    fn accounting_trial_balance_reintroduced_is_rejected() {
        let domain = parsed_domain(
            "id: accounting\nstatus: gap\ninventory_refs: [D12]\nsupported_intents:\n  - trial balance\n",
        );
        let mut findings = Vec::new();
        check_domain(&domain, "path", &test_inventory(), &mut findings);
        assert!(has_check(&findings, "gap_intents_exclude_out_of_scope_phrases"));
    }

    // Kontradiksi #6: group/center mencakup seluruh jenis account (§1), tidak
    // savings-only — supported_intents tidak boleh mengklaim sebaliknya.
    #[test]
    fn group_center_savings_only_reintroduced_is_rejected() {
        let domain = parsed_domain(
            "id: group_center\nstatus: candidate\ninventory_refs: [CLI-4]\nsupported_intents:\n  - group-owned savings reporting\n",
        );
        let mut findings = Vec::new();
        check_domain(&domain, "path", &test_inventory(), &mut findings);
        assert!(has_check(&findings, "gap_intents_exclude_out_of_scope_phrases"));
    }

    // Kontradiksi #3: alamat client selektif/gap (§1 CLI-6), bukan ditolak
    // kebijakan — tidak boleh masuk unsupported_intents (BlockedByPolicy).
    #[test]
    fn client_address_hard_reject_reintroduced_is_rejected() {
        let domain = parsed_domain(
            "id: client\nstatus: approved_mvp\ninventory_refs: [CLI-1]\ngap_intents:\n  - client loan account roster\nunsupported_intents:\n  - address-level reporting\n",
        );
        let mut findings = Vec::new();
        check_domain(&domain, "path", &test_inventory(), &mut findings);
        assert!(has_check(&findings, "gap_item_not_hard_rejected"));
    }

    // Kontradiksi #5: client mencakup seluruh jenis account (§1, CLI-5) —
    // gap_intents harus menyatakan loan/share, bukan hanya savings.
    #[test]
    fn client_missing_all_account_types_is_rejected() {
        let domain = parsed_domain("id: client\nstatus: approved_mvp\ninventory_refs: [CLI-1]\n");
        let mut findings = Vec::new();
        check_domain(&domain, "path", &test_inventory(), &mut findings);
        assert!(has_check(&findings, "client_all_account_types_declared"));
    }

    // Kontradiksi #2: D09 (source audit Fineract) cakupannya disepakati; tidak
    // boleh masuk excluded_tables area audit_users_operations.
    #[test]
    fn audit_area_excluding_source_command_reintroduced_is_rejected() {
        let area = parsed_area(
            "id: audit_users_operations\ninventory_refs: [D09]\nexcluded_tables:\n  - m_portfolio_command_source\n",
        );
        let mut findings = Vec::new();
        check_area(&area, "path", &test_inventory(), &mut findings);
        assert!(has_check(&findings, "audit_source_command_in_scope"));
    }

    // Katalog yang sudah selaras (bentuk hasil FIN-153) tidak boleh dilaporkan.
    #[test]
    fn aligned_domain_and_area_pass_clean() {
        let domain = parsed_domain(
            "id: client\nstatus: approved_mvp\ninventory_refs: [CLI-1, CLI-5, CLI-6]\ngap_intents:\n  - { phrase: \"client loan account roster\", inventory_ref: CLI-5 }\n  - { phrase: \"client share account roster\", inventory_ref: CLI-5 }\n  - { phrase: \"client address\", inventory_ref: CLI-6 }\nsupported_intents:\n  - { phrase: \"client identity resolve\", inventory_ref: CLI-1 }\n",
        );
        let mut findings = Vec::new();
        check_domain(&domain, "path", &test_inventory(), &mut findings);
        assert!(findings.is_empty(), "{findings:?}");

        let area = parsed_area(
            "id: audit_users_operations\ninventory_refs: [D09]\nexcluded_tables:\n  - m_appuser\n",
        );
        let mut findings = Vec::new();
        check_area(&area, "path", &test_inventory(), &mut findings);
        assert!(findings.is_empty());
    }
}
