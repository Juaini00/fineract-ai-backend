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
    model::{CHART_KINDS, Capability, QueryManifest},
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
        "BELUM DIVALIDASI: datasets/** selain entity+shape resolver, fragment *.frag.sql, metrics/**, schema/**, parameters/**, domains/**, responses/**",
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
}
