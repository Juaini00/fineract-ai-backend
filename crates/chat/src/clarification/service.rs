//! Kebijakan klarifikasi: validasi jawaban sebelum apa pun ditulis.
//!
//! Urutan pemeriksaan mengikuti T6: idempotency → replay → ownership →
//! lifecycle → revision → validasi field. Jawaban yang ditolak **tidak**
//! mengubah form maupun melanjutkan eksekusi, dan tidak memicu panggilan model.

use std::collections::BTreeMap;

use foundation::{error::ApiError, state::Foundation};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    clarification::repository::{self, Form},
    engine::planner,
    job::service as job_service,
};

/// Batas teks bebas (runtime.md §2 `MAX_RAW_TEXT_LENGTH`).
const MAX_RAW_TEXT_LENGTH: usize = 512;

/// Kesalahan per field; klien memperbaiki tanpa job baru.
#[derive(Debug, serde::Serialize)]
pub struct FieldError {
    pub field_id: String,
    pub message: String,
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

/// Terima jawaban untuk form terbuka.
pub async fn answer(
    foundation: &Foundation,
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

    let errors = validate(&form.fields_json, answers);
    if !errors.is_empty() {
        return Err(ApiError::Unprocessable(
            serde_json::to_string(&errors).unwrap_or_else(|_| "invalid answers".to_string()),
        ));
    }

    let accepted = repository::accept_answers(
        foundation.app_db().pool(),
        &form,
        job.session_id,
        owner_user_id,
        answers,
        foundation.config().job_ttl_running_secs,
    )
    .await
    .map_err(anyhow::Error::from)?;

    if !accepted {
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
fn validate(fields_json: &Value, answers: &BTreeMap<String, String>) -> Vec<FieldError> {
    let mut errors = Vec::new();
    let fields = fields_json.as_array().cloned().unwrap_or_default();

    for (field_id, value) in answers {
        let Some(field) = fields
            .iter()
            .find(|field| field.get("field_id").and_then(Value::as_str) == Some(field_id.as_str()))
        else {
            errors.push(FieldError {
                field_id: field_id.clone(),
                message: "Unknown field for this clarification".to_string(),
            });
            continue;
        };

        if value.chars().count() > MAX_RAW_TEXT_LENGTH {
            errors.push(FieldError {
                field_id: field_id.clone(),
                message: format!("Value exceeds {MAX_RAW_TEXT_LENGTH} characters"),
            });
            continue;
        }

        let parameter_kind = field
            .get("parameter_kind")
            .and_then(Value::as_str)
            .unwrap_or("string");

        if planner::typed_answer(parameter_kind, value).is_none() {
            let field_type = field.get("type").and_then(Value::as_str).unwrap_or("text");
            errors.push(FieldError {
                field_id: field_id.clone(),
                message: match field_type {
                    "date" => "Expected a calendar date formatted as YYYY-MM-DD".to_string(),
                    "number" => "Expected a whole number".to_string(),
                    other => format!("Value does not satisfy field type '{other}'"),
                },
            });
        }
    }

    for field in &fields {
        let required = field.get("required").and_then(Value::as_bool).unwrap_or(true);
        let field_id = field.get("field_id").and_then(Value::as_str).unwrap_or("");

        if required && !answers.contains_key(field_id) {
            errors.push(FieldError {
                field_id: field_id.to_string(),
                message: "This field is required".to_string(),
            });
        }
    }

    errors
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

    fn answers(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    #[test]
    fn accepts_well_typed_answers() {
        assert!(validate(&fields(), &answers(&[("from_date", "2026-01-01")])).is_empty());
    }

    #[test]
    fn rejects_wrong_type_with_a_field_level_message() {
        let errors = validate(&fields(), &answers(&[("from_date", "kemarin")]));

        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].field_id, "from_date");
        assert!(errors[0].message.contains("YYYY-MM-DD"));
    }

    #[test]
    fn rejects_fields_the_form_never_issued() {
        // Field asing = parameter yang tidak pernah ditanyakan; menerimanya
        // memberi klien jalan menyuntikkan binding.
        let errors = validate(&fields(), &answers(&[("from_date", "2026-01-01"), ("office_ids", "1,2")]));

        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].field_id, "office_ids");
    }

    #[test]
    fn missing_required_field_is_reported() {
        let errors = validate(&fields(), &answers(&[("limit", "5")]));

        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].field_id, "from_date");
    }

    #[test]
    fn oversized_free_text_is_rejected() {
        let long = "x".repeat(MAX_RAW_TEXT_LENGTH + 1);
        let fields = json!([
            { "field_id": "search", "type": "text", "parameter_kind": "string", "required": true }
        ]);

        let errors = validate(&fields, &answers(&[("search", &long)]));
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("512"));
    }
}
