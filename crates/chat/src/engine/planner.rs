//! Planner deterministik: memilih satu capability yang disetujui dan mengikat
//! parameternya.
//!
//! Pemilihan memakai arm leksikal lebih dulu, lalu exact-vector fallback atas
//! `knowledge_index` bila kecocokan leksikal tidak cukup kuat. Teks indeks
//! berasal dari prosa capability, bukan dari SQL, dan pengikatan parameter
//! hanya memakai default yang **sudah
//! dideklarasikan** katalog. Bila sebuah parameter wajib tidak dapat diisi
//! secara deterministik, hasilnya `Unsupported`; menebak nilainya berarti
//! menjawab pertanyaan yang tidak ditanyakan.
//!
//! Konsekuensinya jelas dan disengaja: pertanyaan yang tidak cocok dengan satu
//! pun capability tidak dijawab, bukan dijawab seadanya.

use std::collections::BTreeMap;

use chrono::{Datelike, NaiveDate, Utc};
use foundation::embedding::{EmbeddingClient, InputKind};
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
    /// Row cap capability untuk `limit: unbounded` tanpa `default_limit`
    /// (FIN-133). Nilainya adalah **cap**; executor mengikat `cap + 1` supaya
    /// "ada baris melebihi cap" teramati, lalu worker memotong hasil ke cap dan
    /// menyatakannya (`row_cap_reached`). Lineage menampilkan cap, bukan cap + 1.
    RowCap(i64),
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

impl Plan {
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
    /// Tidak ada kandidat capability sama sekali untuk permintaan ini.
    /// Provisional (build-order §6.5 / FIN-30): pada L0 retrieval masih leksikal
    /// saja, jadi "tidak ada kandidat" DAPAT berarti retrieval kita yang gagal,
    /// bukan permintaan yang benar-benar di luar cakupan. Split tetap dibuat;
    /// L2 (FIN-42, retrieval semantik) yang mereklasifikasi bila kapabilitasnya
    /// ternyata ada.
    OutOfScope,
    /// Ada kandidat/indeks tetapi tidak terpakai (mis. indeks menunjuk entri yang
    /// tidak ada di katalog termuat) — kegagalan sisi kita, bukan di luar cakupan.
    RetrievalMiss,
    /// Capability terpilih merujuk query yang tidak ada di katalog.
    QueryMissing(String),
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

#[derive(Debug, Clone, PartialEq)]
enum RetrievalOutcome {
    Candidate(String, f32),
    HealthyNoMatch,
    RetrievalUnavailable,
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
            Self::OutOfScope => "out_of_scope".to_string(),
            Self::RetrievalMiss => "retrieval_miss".to_string(),
            Self::QueryMissing(_) => "capability_query_missing".to_string(),
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
            Self::OutOfScope => {
                "No approved capability covers this request, so Jarvis will not answer it."
                    .to_string()
            }
            Self::RetrievalMiss => {
                "Jarvis could not retrieve a matching capability for this request.".to_string()
            }
            Self::QueryMissing(query_id) => format!(
                "The selected capability refers to query '{query_id}', which is not present in the approved catalog."
            ),
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

/// Susun rencana untuk sebuah permintaan.
pub async fn plan(
    pool: &PgPool,
    embedding: &EmbeddingClient,
    similarity_cutoff: f32,
    catalog: &Catalog,
    catalog_version_id: Uuid,
    request_text: &str,
    authorized_office_ids: &[i64],
    // `supplied`: jawaban klarifikasi yang sudah diterima, dikunci nama parameter.
    supplied: &BTreeMap<String, String>,
) -> sqlx::Result<Result<Plan, Unplannable>> {
    let lexical = best_capability(pool, catalog_version_id, request_text).await?;
    let semantic = || {
        semantic_capability(
            pool,
            embedding,
            similarity_cutoff,
            catalog_version_id,
            request_text,
        )
    };
    let retrieval = lexical_first(lexical, semantic)
        .await
        .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    let (capability_id, score) = match retrieval_candidate(retrieval) {
        Ok(candidate) => candidate,
        Err(problem) => return Ok(Err(problem)),
    };

    let Some(capability) = catalog
        .capabilities
        .iter()
        .map(|loaded| &loaded.entry)
        .find(|capability| capability.id == capability_id)
    else {
        // Indeks menunjuk entri yang tidak ada di katalog yang dimuat: versi
        // indeks dan versi disk berbeda. Ini kegagalan sisi kita (retrieval),
        // bukan permintaan di luar cakupan.
        return Ok(Err(Unplannable::RetrievalMiss));
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

    let parameters =
        match bind_parameters(catalog, capability, query, authorized_office_ids, supplied) {
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
        retrieval_score: score,
        graph_hash: hex::encode(Sha256::digest(graph_json.to_string().as_bytes())),
        graph_json,
        parameters,
        sql,
        sql_file,
    }))
}

/// Normalize a request into the lexical terms that retrieval can overlap.
///
/// Terms are unique and sorted, so the same words in a different order (or
/// repeated by a user) produce the same query and deterministic ranking. This
/// deliberately does not maintain a language-specific stopword list: catalog
/// vocabulary, not a guessed Indonesian lexicon, remains the source of truth.
fn lexical_terms(text: &str) -> Vec<String> {
    let mut terms: Vec<String> = text
        .split(|character: char| !character.is_alphanumeric())
        .filter(|term| !term.is_empty())
        .map(str::to_lowercase)
        .collect();
    terms.sort();
    terms.dedup();
    terms
}

fn lexical_match_is_confident(terms: &[String], matched_terms: i64) -> bool {
    matched_terms >= if terms.len() == 1 { 1 } else { 2 }
}

fn confident_lexical_candidate(
    terms: &[String],
    candidate: Option<(String, f32, i64)>,
) -> Option<(String, f32)> {
    candidate
        .filter(|(_, _, matched_terms)| lexical_match_is_confident(terms, *matched_terms))
        .map(|(source_id, score, _)| (source_id, score))
}

/// Retrieval leksikal. Kandidat multi-term harus cocok pada sedikitnya dua
/// term agar kata umum tidak mencegah exact-vector fallback.
async fn best_capability(
    pool: &PgPool,
    catalog_version_id: Uuid,
    request_text: &str,
) -> sqlx::Result<Option<(String, f32)>> {
    let terms = lexical_terms(request_text);
    if terms.is_empty() {
        return Ok(None);
    }

    // `matched_terms` tetap kriteria urutan utama (kandidat yang cocok pada
    // lebih banyak term request selalu menang lebih dulu). Perubahan FIN-136
    // ada di *tie-break* dua bagian, karena term count saja tidak cukup
    // membedakan capability bertetangga yang berbagi kosakata domain sama:
    //
    // 1. `title` diberi bobot 'A' (penuh) sementara sisa `retrieval_text`
    //    (deskripsi + contoh) diberi bobot 'C' (0.4x default). Tanpa ini,
    //    capability lain yang mengulang kata umum domain berkali-kali di
    //    banyak contohnya ("client" di hampir tiap contoh capability
    //    client-domain) bisa mengungguli kecocokan frasa persis pada
    //    capability targetnya sendiri — lihat kasus
    //    "Show client lifecycle summary." di FIN-136.
    // 2. Normalisasi rank `2` (bagi dengan panjang dokumen) menghukum
    //    capability yang skornya digelembungkan oleh banyak contoh panjang,
    //    dan lah yang menyelesaikan tie exact match/skor antara "Top
    //    withdrawals per month in the last 12 months." dan capability lain
    //    yang kebetulan cocok pada term count dan skor yang sama.
    let candidate = sqlx::query_as::<_, (String, f32, i64)>(
        "WITH query AS (
             SELECT to_tsquery('simple', array_to_string($2::text[], ' | ')) AS terms
         ), documents AS MATERIALIZED (
             SELECT source_id,
                    setweight(to_tsvector('simple', coalesce(title, '')), 'A')
                    || setweight(to_tsvector('simple', retrieval_text), 'C') AS document
             FROM knowledge_index
             WHERE catalog_version_id = $1
               AND source_type = 'capability'
         )
         SELECT documents.source_id,
                ts_rank_cd(documents.document, query.terms, 2) AS score,
                overlap.matched_terms
         FROM documents
         CROSS JOIN query
         CROSS JOIN LATERAL (
             SELECT count(*) AS matched_terms
             FROM unnest($2::text[]) AS term
             WHERE documents.document @@ to_tsquery('simple', term)
         ) AS overlap
         WHERE documents.document @@ query.terms
         ORDER BY overlap.matched_terms DESC, score DESC, documents.source_id ASC
         LIMIT 1",
    )
    .bind(catalog_version_id)
    .bind(&terms)
    .fetch_optional(pool)
    .await?;

    Ok(confident_lexical_candidate(&terms, candidate))
}

/// Wrapper publik atas `best_capability` — arm leksikal saja, tanpa fallback
/// semantik. Dipakai `retrieval-sweep` (FIN-142) untuk menguji kandidat
/// leksikal lewat kode produksi yang sama persis dengan `plan()`, tanpa
/// mengulang SQL-nya.
pub async fn lexical_candidate(
    pool: &PgPool,
    catalog_version_id: Uuid,
    request_text: &str,
) -> sqlx::Result<Option<(String, f32)>> {
    best_capability(pool, catalog_version_id, request_text).await
}

async fn lexical_first<F, Fut>(
    lexical: Option<(String, f32)>,
    semantic: F,
) -> anyhow::Result<RetrievalOutcome>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<RetrievalOutcome>>,
{
    match lexical {
        Some((capability_id, score)) => Ok(RetrievalOutcome::Candidate(capability_id, score)),
        None => semantic().await,
    }
}

fn retrieval_candidate(outcome: RetrievalOutcome) -> Result<(String, f32), Unplannable> {
    match outcome {
        RetrievalOutcome::Candidate(capability_id, score) => Ok((capability_id, score)),
        RetrievalOutcome::HealthyNoMatch => Err(Unplannable::OutOfScope),
        RetrievalOutcome::RetrievalUnavailable => Err(Unplannable::RetrievalMiss),
    }
}

async fn semantic_capability(
    pool: &PgPool,
    embedding: &EmbeddingClient,
    cutoff: f32,
    catalog_version_id: Uuid,
    request_text: &str,
) -> anyhow::Result<RetrievalOutcome> {
    if !embedding.available() {
        return Ok(RetrievalOutcome::RetrievalUnavailable);
    }
    let Some(metadata) =
        crate::catalog::repository::embedding_metadata(pool, catalog_version_id).await?
    else {
        return Ok(RetrievalOutcome::RetrievalUnavailable);
    };
    if !metadata_matches(
        metadata.embedding_model.as_deref(),
        metadata.embedding_dimensions,
        metadata.embedding_input_type.as_deref(),
        embedding.model(),
        embedding.dimensions(),
        embedding.document_input_type(),
    ) {
        return Ok(RetrievalOutcome::RetrievalUnavailable);
    }
    let vector = match provider_vector(
        embedding
            .embed(&[request_text.to_string()], InputKind::Query)
            .await,
    ) {
        Ok(vector) => vector,
        Err(outcome) => return Ok(outcome),
    };
    Ok(
        match crate::catalog::repository::best_vector_capability(
            pool,
            catalog_version_id,
            vector,
            cutoff,
        )
        .await?
        {
            Some((capability_id, score)) => RetrievalOutcome::Candidate(capability_id, score),
            None => RetrievalOutcome::HealthyNoMatch,
        },
    )
}

fn metadata_matches(
    model: Option<&str>,
    dimensions: Option<i32>,
    document_input_type: Option<&str>,
    runtime_model: &str,
    runtime_dimensions: usize,
    runtime_document_input_type: &str,
) -> bool {
    model == Some(runtime_model)
        && dimensions == Some(runtime_dimensions as i32)
        && document_input_type == Some(runtime_document_input_type)
}

fn provider_vector(result: anyhow::Result<Vec<Vec<f32>>>) -> Result<Vec<f32>, RetrievalOutcome> {
    result
        .ok()
        .and_then(|vectors| vectors.into_iter().next())
        .ok_or(RetrievalOutcome::RetrievalUnavailable)
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

    Ok(bound)
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

    (0..4).find_map(|back| NaiveDate::from_ymd_opt(year, month, date.day().checked_sub(back)?))
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
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

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

    #[tokio::test]
    async fn lexical_candidate_wins_without_invoking_vector_arm() {
        let invoked = Arc::new(AtomicBool::new(false));
        let marker = invoked.clone();
        let result = lexical_first(Some(("lexical".into(), 0.8)), move || async move {
            marker.store(true, Ordering::SeqCst);
            Ok(RetrievalOutcome::Candidate("vector".into(), 0.99))
        })
        .await
        .unwrap();
        assert_eq!(result, RetrievalOutcome::Candidate("lexical".into(), 0.8));
        assert!(!invoked.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn lexical_miss_preserves_semantic_candidate() {
        let invoked = Arc::new(AtomicBool::new(false));
        let marker = invoked.clone();
        let result = lexical_first(None, move || async move {
            marker.store(true, Ordering::SeqCst);
            Ok(RetrievalOutcome::Candidate("vector".into(), 0.7))
        })
        .await
        .unwrap();
        assert_eq!(result, RetrievalOutcome::Candidate("vector".into(), 0.7));
        assert!(invoked.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn lexical_miss_preserves_healthy_semantic_no_match() {
        let result = lexical_first(None, || async { Ok(RetrievalOutcome::HealthyNoMatch) })
            .await
            .unwrap();
        assert_eq!(result, RetrievalOutcome::HealthyNoMatch);
        assert_eq!(retrieval_candidate(result), Err(Unplannable::OutOfScope));
    }

    #[tokio::test]
    async fn lexical_miss_preserves_unavailable_semantic_arm() {
        let result = lexical_first(None, || async {
            Ok(RetrievalOutcome::RetrievalUnavailable)
        })
        .await
        .unwrap();
        assert_eq!(result, RetrievalOutcome::RetrievalUnavailable);
        assert_eq!(retrieval_candidate(result), Err(Unplannable::RetrievalMiss));
    }

    #[test]
    fn mismatched_semantic_metadata_is_unavailable() {
        assert!(!metadata_matches(
            Some("different-model"),
            Some(1024),
            Some("search_document"),
            "configured-model",
            1024,
            "search_document",
        ));
    }

    #[test]
    fn provider_failure_is_retrieval_unavailable() {
        let outcome = provider_vector(Err(anyhow::anyhow!("provider failed"))).unwrap_err();
        assert_eq!(outcome, RetrievalOutcome::RetrievalUnavailable);

        let empty_outcome = provider_vector(Ok(Vec::new())).unwrap_err();
        assert_eq!(empty_outcome, RetrievalOutcome::RetrievalUnavailable);
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
            safety_policy: Default::default(),
            sensitivity_classes: Default::default(),
            unapproved_surfaces: Default::default(),
            deferred_domains: Default::default(),
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
            defaults: BTreeMap::new(),
        }
    }

    fn query(parameters: Vec<QueryParameter>) -> QueryManifest {
        QueryManifest {
            id: "savings.deposit_total".into(),
            database: None,
            sql_file: None,
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
    fn lexical_terms_are_normalized_unique_and_deterministically_sorted() {
        // Ranking and OR-overlap are asserted on the served retrieval path.
        // This pure-logic test only owns normalization invariants.
        assert_eq!(
            lexical_terms("Berapa total portfolio aktif bulan ini, portfolio?"),
            vec!["aktif", "berapa", "bulan", "ini", "portfolio", "total"]
        );
    }

    #[tokio::test]
    async fn lexical_confidence_keeps_one_term_but_falls_back_for_one_of_many() {
        let one_term = confident_lexical_candidate(
            &lexical_terms("portfolio"),
            Some(("lexical".into(), 0.8, 1)),
        );
        assert_eq!(one_term, Some(("lexical".into(), 0.8)));

        let many_terms = confident_lexical_candidate(
            &lexical_terms("total portfolio aktif bulan ini"),
            Some(("weak-lexical".into(), 0.4, 1)),
        );
        let semantic_invoked = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let invoked = semantic_invoked.clone();
        let result = lexical_first(many_terms, move || async move {
            invoked.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(RetrievalOutcome::Candidate("semantic".into(), 0.7))
        })
        .await
        .unwrap();

        assert_eq!(result, RetrievalOutcome::Candidate("semantic".into(), 0.7));
        assert!(semantic_invoked.load(std::sync::atomic::Ordering::SeqCst));
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

        let bound =
            bind_parameters(&catalog(), &capability, &query, &[1, 2], &BTreeMap::new()).unwrap();
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

        let problem =
            bind_parameters(&catalog(), &capability, &query, &[1], &BTreeMap::new()).unwrap_err();
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

        let bound =
            bind_parameters(&catalog(), &capability, &query, &[7], &BTreeMap::new()).unwrap();
        assert_eq!(bound[1], Bound::NullText);
        assert_eq!(bound[2], Bound::NullBigintArray);
    }

    #[test]
    fn required_parameter_without_derivable_value_is_not_guessed() {
        let capability = capability(&[]);
        let query = query(vec![parameter("account_number", "string", true, None)]);

        let problem =
            bind_parameters(&catalog(), &capability, &query, &[1], &BTreeMap::new()).unwrap_err();
        let Unplannable::NeedsClarification { missing, .. } = &problem else {
            panic!("seharusnya menuntut klarifikasi: {problem:?}");
        };
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].name, "account_number");
        assert_eq!(problem.reason(), "parameter_needs_clarification");
    }

    #[test]
    fn unplannable_reason_splits_out_of_scope_from_retrieval_miss() {
        // §6.5 (FIN-30): `no_capability_matched` was split. out_of_scope = no
        // lexical candidate at all; retrieval_miss = candidate/index skew (our
        // failure). The two must be distinguishable in both the machine reason
        // and the public explanation (I5: no silent conflation).
        assert_eq!(Unplannable::OutOfScope.reason(), "out_of_scope");
        assert_eq!(Unplannable::RetrievalMiss.reason(), "retrieval_miss");
        assert_ne!(
            Unplannable::OutOfScope.explain(),
            Unplannable::RetrievalMiss.explain()
        );
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

        let problem =
            bind_parameters(&catalog(), &capability, &query, &[1], &BTreeMap::new()).unwrap_err();
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

        let bound =
            bind_parameters(&catalog(), &capability, &query, &[1], &BTreeMap::new()).unwrap();
        // 250 melebihi hard_cap 100 → dipotong, bukan diteruskan apa adanya.
        assert_eq!(bound[0], Bound::Bigint(100));
    }

    #[test]
    fn numeric_literal_default_within_cap_is_kept() {
        let capability = capability_with_cap(&[("limit", "10")], Some(100));
        let query = query(vec![parameter("limit", "integer", true, None)]);

        assert_eq!(
            bind_parameters(&catalog(), &capability, &query, &[1], &BTreeMap::new()).unwrap()[0],
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
        bind_parameters(&catalog(), capability, query, &[1], &BTreeMap::new()).unwrap()[0].clone()
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

        let bound =
            bind_parameters(&catalog(), &capability, &query, &[3, 4], &BTreeMap::new()).unwrap();
        assert_eq!(bound[0], Bound::OfficeIds(vec![3, 4]));
    }
}
