//! Kebijakan klarifikasi: validasi jawaban sebelum apa pun ditulis.
//!
//! Urutan pemeriksaan mengikuti T6: idempotency → replay → ownership →
//! lifecycle → revision → validasi field. Jawaban yang ditolak **tidak**
//! mengubah form maupun melanjutkan eksekusi, dan tidak memicu panggilan model.
//!
//! Untuk field `single_choice` ada dua pemeriksaan yang **tidak boleh
//! digabung**: opsi pernah diterbitkan untuk form+field ini (C9), dan opsi itu
//! masih berada dalam scope terotorisasi saat submit (I7). Keanggotaan bukan
//! otorisasi — daftar opsi lama tidak menjadi izin baru.

use std::collections::BTreeMap;

use foundation::{error::ApiError, state::Foundation};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    catalog::Catalog,
    clarification::repository::{self, AcceptedAnswer, Form},
    engine::{
        compose,
        executor,
        planner,
        resolver::{self, Candidates, ResolverSlot},
    },
    job::{repository::Job, service as job_service},
};

/// Batas teks bebas (runtime.md §2 `MAX_RAW_TEXT_LENGTH`).
const MAX_RAW_TEXT_LENGTH: usize = 512;

/// Kesalahan per field; klien memperbaiki tanpa job baru.
#[derive(Debug, serde::Serialize)]
pub struct FieldError {
    pub field_id: String,
    /// Kode stabil supaya klien dapat membedakan "salah tipe" dari "opsi tidak
    /// pernah diterbitkan" tanpa mencocokkan kalimat.
    pub code: &'static str,
    pub message: String,
}

/// Satu halaman opsi, beserta pernyataan tentang apa yang **tidak** dikirim.
#[derive(Debug, serde::Serialize)]
pub struct OptionPage {
    pub clarification_id: Uuid,
    pub revision: i32,
    pub field_id: String,
    pub resolver_ref: String,
    pub options: Vec<IssuedOption>,
    pub cursor: usize,
    pub next_cursor: Option<usize>,
    /// Jumlah kandidat yang cocok dalam scope — bukan jumlah yang dikirim.
    pub matched_total: usize,
    /// Kandidat melebihi `RESOLVER_MAX_CANDIDATES`: daftar ini bukan populasi
    /// lengkap, dan klien wajib mempersempit dengan `q`.
    pub truncated: bool,
}

#[derive(Debug, serde::Serialize)]
pub struct IssuedOption {
    pub option_id: String,
    pub label: String,
    pub attributes: Map<String, Value>,
}

/// Form terbuka milik job ini.
pub async fn active_form(
    foundation: &Foundation,
    job_id: Uuid,
    owner_user_id: Uuid,
) -> Result<Form, ApiError> {
    job_service::owned(foundation, job_id, owner_user_id).await?;

    repository::open_form_of(foundation.app_db().pool(), job_id)
        .await
        .map_err(anyhow::Error::from)?
        .ok_or(ApiError::NotFound)
}

/// Terbitkan satu halaman opsi untuk sebuah field `single_choice`.
///
/// Baris `clarification_options` ditulis untuk halaman yang **benar-benar
/// dikirim**, bukan untuk seluruh hasil resolver: tabel itu adalah bukti "opsi
/// ini pernah kami tampilkan", dan mencatat yang tidak pernah terlihat
/// menghapus artinya.
pub async fn options(
    foundation: &Foundation,
    catalog: &Catalog,
    job_id: Uuid,
    owner_user_id: Uuid,
    field_id: &str,
    cursor: usize,
    limit: Option<usize>,
    search: Option<&str>,
) -> Result<OptionPage, ApiError> {
    let job = job_service::owned(foundation, job_id, owner_user_id).await?;
    let form = repository::open_form_of(foundation.app_db().pool(), job_id)
        .await
        .map_err(anyhow::Error::from)?
        .ok_or(ApiError::NotFound)?;

    let field = find_field(&form.fields_json, field_id).ok_or(ApiError::NotFound)?;
    let slot = slot_of(catalog, &field).ok_or_else(|| {
        ApiError::Conflict(format!("Field '{field_id}' is not answered by choosing an option"))
    })?;

    let config = foundation.config();
    // Default halaman berasal dari `row_cap` shape bila ada — katalog yang
    // menyatakan berapa banyak opsi masuk akal untuk entitas itu; config hanya
    // memberi cadangan dan batas atas yang dapat diminta klien.
    let page_size = limit
        .unwrap_or_else(|| slot.page_size.min(config.resolver_page_size))
        .clamp(1, config.resolver_page_size_max);

    let candidates = fetch(foundation, &job, &slot, search).await?;

    let page: Vec<_> = candidates
        .items
        .iter()
        .skip(cursor)
        .take(page_size)
        .cloned()
        .collect();

    repository::issue_options(
        foundation.app_db().pool(),
        &form,
        field_id,
        &slot.query_id,
        &cursor.to_string(),
        &page
            .iter()
            .map(|candidate| {
                (
                    candidate.option_id.clone(),
                    // `label` ikut ke binding: pengungkapan K5 dan riwayat
                    // memerlukan teks yang benar-benar dilihat pengguna, dan
                    // opsi dipurge saat form terminal.
                    serde_json::json!({ "value": candidate.binding, "label": candidate.label }),
                    candidate.label.clone(),
                    Value::Object(candidate.attributes.clone()),
                )
            })
            .collect::<Vec<_>>(),
    )
    .await
    .map_err(anyhow::Error::from)?;

    let next = cursor + page.len();

    Ok(OptionPage {
        clarification_id: form.clarification_id,
        revision: form.revision,
        field_id: field_id.to_string(),
        resolver_ref: slot.query_id.clone(),
        cursor,
        next_cursor: (next < candidates.items.len()).then_some(next),
        matched_total: candidates.matched_total,
        truncated: candidates.truncated,
        options: page
            .into_iter()
            .map(|candidate| IssuedOption {
                option_id: candidate.option_id,
                label: candidate.label,
                attributes: candidate.attributes,
            })
            .collect(),
    })
}

/// T8 — pengguna memilih tidak melanjutkan.
///
/// Skip **bukan cancel**: ia menghasilkan response document dan job berakhir
/// `Completed` + `SkippedByUser`, sedangkan cancel berakhir `Cancelled` tanpa
/// kewajiban dokumen. Keduanya sengaja tidak digabung — menggabungkannya akan
/// membuat "pengguna berhenti bertanya" tidak dapat dibedakan dari "pekerjaan
/// dibatalkan" pada laporan mana pun.
pub async fn skip(
    foundation: &Foundation,
    job_id: Uuid,
    owner_user_id: Uuid,
    clarification_id: Uuid,
    revision: i32,
) -> Result<Form, ApiError> {
    let job = job_service::owned(foundation, job_id, owner_user_id).await?;

    if job.lifecycle != "WaitingForUser" {
        return Err(ApiError::Conflict(format!(
            "Job is {}, not waiting for an answer",
            job.lifecycle
        )));
    }

    let pool = foundation.app_db().pool();
    let form = repository::open_form_of(pool, job_id)
        .await
        .map_err(anyhow::Error::from)?
        .ok_or(ApiError::NotFound)?;

    if form.clarification_id != clarification_id || form.revision != revision {
        return Err(ApiError::Conflict(
            "Clarification revision is stale; reload the active form".to_string(),
        ));
    }

    // `Complete` dilarang: pengguna berhenti di tengah. Yang membedakan
    // `Partial` dari `Unknown` adalah apakah ada output node yang benar-benar
    // durable — bukan tebakan tentang seberapa jauh job sempat berjalan.
    let completed_nodes = repository::completed_node_count(pool, job_id)
        .await
        .map_err(anyhow::Error::from)?;

    let stored = repository::skip(
        pool,
        &form,
        job.session_id,
        owner_user_id,
        skipped_response(&form, completed_nodes),
    )
    .await
    .map_err(anyhow::Error::from)?;

    if !stored {
        return Err(ApiError::Conflict(
            "This clarification is no longer open".to_string(),
        ));
    }

    Ok(form)
}

/// Dokumen skip: menyatakan pengguna memilih berhenti, dan menandai gap-nya
/// eksplisit (I5 — tidak ada penghilangan senyap).
fn skipped_response(form: &Form, completed_nodes: i64) -> repository::SkippedResponse {
    let unanswered = unanswered_fields(&form.fields_json);
    let partial = completed_nodes > 0;

    let blocks = serde_json::json!([
        // Bentuk blok mengikuti responses.md §1 dan §2, sama seperti dokumen
        // `analysis`: satu kosakata, satu bungkus, satu tempat ia dibuat.
        compose::block(
            "skipped",
            "narrative",
            &[],
            serde_json::json!({
                "title": "Stopped at your request",
                "body": "You chose not to provide the remaining input, so Jarvis stopped here \
                         instead of guessing a value.",
            }),
        ),
        compose::block(
            "skipped_inputs",
            "limitation",
            &[],
            serde_json::json!({
                "title": "What is missing",
                "body": if unanswered.is_empty() {
                    "No further input was supplied before the request was stopped.".to_string()
                } else {
                    format!(
                        "{} input(s) were never supplied, so no figure is reported for them: {}.",
                        unanswered.len(),
                        unanswered.join(", ")
                    )
                },
                "unanswered_fields": unanswered,
                // Hasil parsial yang sudah durable TIDAK dibuang
                // (clarifications.md); jumlahnya dinyatakan supaya pembaca tahu
                // ada sesuatu untuk dilihat.
                "completed_nodes": completed_nodes,
            }),
        ),
    ]);

    repository::SkippedResponse {
        // engine.md: `SkippedByUser` hanya sah dengan `Partial` atau `Unknown`.
        completeness: if partial { "Partial" } else { "Unknown" },
        completeness_reason: "skipped_by_user".to_string(),
        response_hash: hex::encode(Sha256::digest(blocks.to_string().as_bytes())),
        blocks,
        unanswered_fields: unanswered,
    }
}

fn unanswered_fields(fields_json: &Value) -> Vec<String> {
    fields_json
        .as_array()
        .map(|fields| {
            fields
                .iter()
                .filter_map(|field| field.get("field_id").and_then(Value::as_str))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Terima jawaban untuk form terbuka.
pub async fn answer(
    foundation: &Foundation,
    catalog: &Catalog,
    job_id: Uuid,
    owner_user_id: Uuid,
    clarification_id: Uuid,
    revision: i32,
    answers: &BTreeMap<String, String>,
) -> Result<Form, ApiError> {
    let job = job_service::owned(foundation, job_id, owner_user_id).await?;

    if job.lifecycle != "WaitingForUser" {
        return Err(ApiError::Conflict(format!(
            "Job is {}, not waiting for an answer",
            job.lifecycle
        )));
    }

    let form = repository::open_form_of(foundation.app_db().pool(), job_id)
        .await
        .map_err(anyhow::Error::from)?
        .ok_or(ApiError::NotFound)?;

    // Revision basi tidak boleh diam-diam menyunting riwayat yang sudah
    // diterima: klien wajib memuat ulang form yang berlaku.
    if form.clarification_id != clarification_id || form.revision != revision {
        return Err(ApiError::Conflict(
            "Clarification revision is stale; reload the active form".to_string(),
        ));
    }

    let (accepted, errors) = resolve_answers(foundation, catalog, &job, &form, answers).await?;
    if !errors.is_empty() {
        return Err(ApiError::Unprocessable(
            serde_json::to_string(&errors).unwrap_or_else(|_| "invalid answers".to_string()),
        ));
    }

    let stored = repository::accept_answers(
        foundation.app_db().pool(),
        &form,
        job.session_id,
        owner_user_id,
        &accepted,
        foundation.config().job_ttl_running_secs,
    )
    .await
    .map_err(anyhow::Error::from)?;

    if !stored {
        // Pengiriman lain menang lebih dulu.
        return Err(ApiError::Conflict(
            "This clarification was already answered".to_string(),
        ));
    }

    Ok(form)
}

/// Validasi jawaban terhadap field yang benar-benar diterbitkan form ini.
///
/// Field yang tidak ada di form ditolak, bukan diabaikan: menerimanya berarti
/// klien dapat menyuntikkan parameter yang tidak pernah ditanyakan.
async fn resolve_answers(
    foundation: &Foundation,
    catalog: &Catalog,
    job: &Job,
    form: &Form,
    answers: &BTreeMap<String, String>,
) -> Result<(Vec<AcceptedAnswer>, Vec<FieldError>), ApiError> {
    let fields = form.fields_json.as_array().cloned().unwrap_or_default();
    let mut errors = structural_errors(&fields, answers);
    let mut accepted = Vec::new();

    for (field_id, value) in answers {
        let Some(field) = find_field(&form.fields_json, field_id) else {
            continue; // sudah dilaporkan `structural_errors`
        };
        if value.chars().count() > MAX_RAW_TEXT_LENGTH {
            continue;
        }

        match slot_of(catalog, &field) {
            Some(slot) => {
                match option_answer(foundation, job, form, field_id, value, &slot).await? {
                    Ok(answer) => accepted.push(answer),
                    Err(error) => errors.push(error),
                }
            }
            None => match typed_answer(&field, field_id, value) {
                Ok(answer) => accepted.push(answer),
                Err(error) => errors.push(error),
            },
        }
    }

    Ok((accepted, errors))
}

/// Pemeriksaan yang tidak menyentuh katalog maupun database: field dikenal,
/// panjang teks, dan kelengkapan field wajib.
fn structural_errors(fields: &[Value], answers: &BTreeMap<String, String>) -> Vec<FieldError> {
    let mut errors = Vec::new();

    for (field_id, value) in answers {
        let known = fields
            .iter()
            .any(|field| field.get("field_id").and_then(Value::as_str) == Some(field_id.as_str()));

        if !known {
            errors.push(FieldError {
                field_id: field_id.clone(),
                code: "unknown_field",
                message: "Unknown field for this clarification".to_string(),
            });
        } else if value.chars().count() > MAX_RAW_TEXT_LENGTH {
            errors.push(FieldError {
                field_id: field_id.clone(),
                code: "too_long",
                message: format!("Value exceeds {MAX_RAW_TEXT_LENGTH} characters"),
            });
        }
    }

    for field in fields {
        let required = field.get("required").and_then(Value::as_bool).unwrap_or(true);
        let field_id = field.get("field_id").and_then(Value::as_str).unwrap_or("");

        if required && !answers.contains_key(field_id) {
            errors.push(FieldError {
                field_id: field_id.to_string(),
                code: "required",
                message: "This field is required".to_string(),
            });
        }
    }

    errors
}

fn typed_answer(field: &Value, field_id: &str, value: &str) -> Result<AcceptedAnswer, FieldError> {
    let parameter_kind = field
        .get("parameter_kind")
        .and_then(Value::as_str)
        .unwrap_or("string");

    if planner::typed_answer(parameter_kind, value).is_none() {
        let field_type = field.get("type").and_then(Value::as_str).unwrap_or("text");
        return Err(FieldError {
            field_id: field_id.to_string(),
            code: "type_mismatch",
            message: match field_type {
                "date" => "Expected a calendar date formatted as YYYY-MM-DD".to_string(),
                "number" => "Expected a whole number".to_string(),
                other => format!("Value does not satisfy field type '{other}'"),
            },
        });
    }

    Ok(AcceptedAnswer {
        field_id: field_id.to_string(),
        answer_kind: "typed_value",
        raw_text: Some(value.to_string()),
        binding_json: serde_json::json!({ "value": value }),
        provenance: "user_confirmed",
        resolver_ref: None,
        option_set_ref: None,
    })
}

/// Jawaban berupa id opsi. Dua pemeriksaan terpisah, keduanya wajib.
async fn option_answer(
    foundation: &Foundation,
    job: &Job,
    form: &Form,
    field_id: &str,
    option_id: &str,
    slot: &ResolverSlot,
) -> Result<Result<AcceptedAnswer, FieldError>, ApiError> {
    // 1. Pernah diterbitkan untuk form+field ini (C9).
    let issued = repository::issued_option(foundation.app_db().pool(), form.id, field_id, option_id)
        .await
        .map_err(anyhow::Error::from)?;

    let Some((binding_json, label, resolver_ref)) = issued else {
        return Ok(Err(FieldError {
            field_id: field_id.to_string(),
            code: "option_not_issued",
            message: "This option was never issued for this clarification".to_string(),
        }));
    };

    // 2. Masih dalam scope terotorisasi SAAT INI (I7). Keanggotaan pada langkah
    //    1 hanya membuktikan penerbitan; izin dapat menyempit sesudahnya, dan
    //    daftar lama tidak boleh menjadi izin baru.
    let candidates = fetch(foundation, job, slot, None).await?;
    if !candidates
        .items
        .iter()
        .any(|candidate| candidate.option_id == option_id)
    {
        return Ok(Err(FieldError {
            field_id: field_id.to_string(),
            code: "option_out_of_scope",
            message: "This option is no longer inside your authorized scope".to_string(),
        }));
    }

    // 3. Binding wajib memenuhi tipe parameter yang akan menerimanya.
    let value = binding_json.get("value").cloned().unwrap_or(Value::Null);
    if !resolver::binding_matches_kind(&slot.binding_kind, &value) {
        return Ok(Err(FieldError {
            field_id: field_id.to_string(),
            code: "binding_type_mismatch",
            message: "The stored binding no longer satisfies the parameter type".to_string(),
        }));
    }

    Ok(Ok(AcceptedAnswer {
        field_id: field_id.to_string(),
        answer_kind: "option_id",
        // TIDAK ADA raw_text: tidak ada teks bebas yang menjadi binding (K1).
        raw_text: None,
        binding_json: serde_json::json!({
            "value": resolver::binding_text(&value),
            "label": label,
        }),
        provenance: "user_confirmed",
        resolver_ref,
        option_set_ref: Some(option_id.to_string()),
    }))
}

/// Jalankan resolver dalam scope yang dihitung ulang dari otorisasi.
async fn fetch(
    foundation: &Foundation,
    job: &Job,
    slot: &ResolverSlot,
    search: Option<&str>,
) -> Result<Candidates, ApiError> {
    let requested: Vec<i64> = job
        .scope_json
        .get("office_ids")
        .and_then(Value::as_array)
        .map(|ids| ids.iter().filter_map(Value::as_i64).collect())
        .unwrap_or_default();

    let authorized = executor::authorized_office_ids(foundation.fineract_db(), &requested)
        .await
        .map_err(|error| anyhow::anyhow!("resolver scope: {}", error.failure_code()))?;

    // Sakelar PII disnapshot saat job diterima (#15): label opsi tunduk pada
    // sakelar yang berlaku untuk job ini, bukan yang berlaku sekarang.
    let pii_enabled = job
        .scope_json
        .get("pii")
        .and_then(|pii| pii.get("enabled"))
        .and_then(Value::as_bool)
        .unwrap_or(false);

    resolver::candidates(
        foundation.fineract_db(),
        slot,
        &authorized,
        pii_enabled,
        search,
        foundation.config().resolver_max_candidates,
    )
    .await
        .map_err(|error| anyhow::anyhow!("resolver: {}", error.failure_code()).into())
}

fn find_field(fields_json: &Value, field_id: &str) -> Option<Value> {
    fields_json
        .as_array()?
        .iter()
        .find(|field| field.get("field_id").and_then(Value::as_str) == Some(field_id))
        .cloned()
}

/// Rebuild resolver sebuah field dari form. Form adalah catatan durable tentang
/// apa yang ditanyakan; membangunnya ulang dari katalog saja akan mengikuti
/// katalog yang mungkin sudah berubah sejak form terbit.
fn slot_of(catalog: &Catalog, field: &Value) -> Option<ResolverSlot> {
    let declared = field.get("resolver")?;
    let probe = serde_json::from_value(declared.clone()).ok()?;
    let kind = field
        .get("parameter_kind")
        .and_then(Value::as_str)
        .unwrap_or("string");

    ResolverSlot::from_catalog(catalog, &probe, kind)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fields() -> Value {
        json!([
            { "field_id": "from_date", "type": "date", "parameter_kind": "date", "required": true },
            { "field_id": "limit", "type": "number", "parameter_kind": "integer", "required": false }
        ])
    }

    fn field(id: &str) -> Value {
        find_field(&fields(), id).unwrap()
    }

    #[test]
    fn accepts_well_typed_answers() {
        let answer = typed_answer(&field("from_date"), "from_date", "2026-01-01").unwrap();

        assert_eq!(answer.answer_kind, "typed_value");
        assert_eq!(answer.provenance, "user_confirmed");
        assert_eq!(answer.binding_json["value"], "2026-01-01");
    }

    #[test]
    fn rejects_wrong_type_with_a_field_level_message() {
        let error = typed_answer(&field("from_date"), "from_date", "kemarin").unwrap_err();

        assert_eq!(error.code, "type_mismatch");
        assert!(error.message.contains("YYYY-MM-DD"));
    }

    fn answers(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    fn structural(pairs: &[(&str, &str)]) -> Vec<FieldError> {
        structural_errors(fields().as_array().unwrap(), &answers(pairs))
    }

    #[test]
    fn rejects_fields_the_form_never_issued() {
        // Field asing = parameter yang tidak pernah ditanyakan; menerimanya
        // memberi klien jalan menyuntikkan binding.
        let errors = structural(&[("from_date", "2026-01-01"), ("office_ids", "1,2")]);

        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].field_id, "office_ids");
        assert_eq!(errors[0].code, "unknown_field");
    }

    #[test]
    fn missing_required_field_is_reported() {
        let errors = structural(&[("limit", "5")]);

        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].field_id, "from_date");
        assert_eq!(errors[0].code, "required");
    }

    #[test]
    fn oversized_free_text_is_rejected() {
        let long = "x".repeat(MAX_RAW_TEXT_LENGTH + 1);
        let errors = structural_errors(
            json!([{ "field_id": "search", "type": "text", "parameter_kind": "string", "required": true }])
                .as_array()
                .unwrap(),
            &answers(&[("search", &long)]),
        );

        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].code, "too_long");
    }

    #[test]
    fn well_typed_answers_pass_the_structural_checks() {
        assert!(structural(&[("from_date", "2026-01-01")]).is_empty());
    }

    fn form(fields: Value) -> Form {
        Form {
            id: Uuid::nil(),
            job_id: Uuid::nil(),
            clarification_id: Uuid::nil(),
            revision: 1,
            schema_version: 1,
            purpose: None,
            stage_label: None,
            fields_json: fields,
            state: "open".into(),
            expires_at: None,
            created_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn skip_is_never_allowed_to_claim_completeness() {
        // engine.md: `SkippedByUser` + `Complete` dilarang. Pengguna berhenti di
        // tengah; tidak ada klaim kelengkapan yang dapat dibuat atas data sumber.
        for nodes in [0, 1, 7] {
            let response = skipped_response(&form(fields()), nodes);
            assert_ne!(response.completeness, "Complete", "nodes={nodes}");
        }
    }

    #[test]
    fn completeness_follows_durable_output_not_optimism() {
        // Tanpa satu pun output node durable, hasilnya TIDAK DIKETAHUI — bukan
        // "sebagian" yang menyiratkan ada sesuatu untuk dilihat.
        assert_eq!(skipped_response(&form(fields()), 0).completeness, "Unknown");
        assert_eq!(skipped_response(&form(fields()), 2).completeness, "Partial");
    }

    #[test]
    fn skipped_document_names_the_gap() {
        let response = skipped_response(&form(fields()), 0);
        let blocks = response.blocks.as_array().unwrap();

        assert_eq!(response.completeness_reason, "skipped_by_user");
        assert_eq!(blocks[0]["type"], "narrative");
        // I5 — gap dinyatakan, bukan sekadar tidak ada angka.
        assert_eq!(blocks[1]["block_id"], "skipped_inputs");
        assert_eq!(blocks[1]["unanswered_fields"][0], "from_date");
        assert_eq!(response.unanswered_fields.len(), 2);
    }

    #[test]
    fn a_field_without_a_declared_resolver_is_never_treated_as_a_choice() {
        // `slot_of` menolak field tanpa blok `resolver`, jadi tidak ada jalur
        // yang diam-diam menerima teks bebas sebagai id opsi.
        let catalog = crate::catalog::loader::Catalog {
            capabilities: Vec::new(),
            queries: Vec::new(),
            datasets: Vec::new(),
            safety_policy: Default::default(),
            sensitivity_classes: Default::default(),
            sql_files: Default::default(),
            content_hash: String::new(),
            unreadable: Vec::new(),
        };

        assert!(slot_of(&catalog, &field("from_date")).is_none());
        // Dan field yang MENGAKU choice tetapi resolvernya tidak ada di katalog
        // juga menjadi None — bukan diturunkan menjadi teks bebas.
        let orphan = json!({
            "field_id": "client_id",
            "type": "single_choice",
            "parameter_kind": "integer",
            "resolver": { "dataset_id": "hilang", "shape_id": "hilang", "output_slot": "client_id" },
        });
        assert!(slot_of(&catalog, &orphan).is_none());
    }
}
