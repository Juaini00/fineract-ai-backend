//! Kebijakan job: fingerprint idempotency, pemetaan error, kepemilikan.

use foundation::{error::ApiError, state::Foundation};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::job::repository::{self, Accepted, CreateError, Job};

/// Hasil penerimaan job: baru, atau replay acknowledgement yang tersimpan.
#[derive(Debug)]
pub enum Acceptance {
    Created(Box<Job>),
    Replayed { status: i32, body: Value },
}

pub async fn create(
    foundation: &Foundation,
    owner_user_id: Uuid,
    session_id: Uuid,
    request_text: &str,
    office_ids: &[i64],
    idempotency_key: &str,
) -> Result<Acceptance, ApiError> {
    let config = foundation.config();

    let accepted = repository::create_job(
        foundation.app_db().pool(),
        owner_user_id,
        session_id,
        request_text,
        office_ids,
        &config.fineract_tenant,
        idempotency_key,
        &fingerprint(session_id, request_text, office_ids),
        config.idempotency_ttl_secs,
    )
    .await
    .map_err(map_create_error)?;

    Ok(match accepted {
        Accepted::Created(job) => Acceptance::Created(job),
        Accepted::Replayed { status, body } => Acceptance::Replayed { status, body },
    })
}

/// Ambil job milik pengguna ini; milik orang lain tampak `NotFound`.
pub async fn owned(
    foundation: &Foundation,
    job_id: Uuid,
    owner_user_id: Uuid,
) -> Result<Job, ApiError> {
    let job = repository::find(foundation.app_db().pool(), job_id)
        .await
        .map_err(anyhow::Error::from)?
        .ok_or(ApiError::NotFound)?;

    if job.owner_user_id != owner_user_id {
        return Err(ApiError::NotFound);
    }

    Ok(job)
}

/// Minta pembatalan. `false` berarti job sudah terminal — bukan kesalahan.
pub async fn cancel(
    foundation: &Foundation,
    job_id: Uuid,
    owner_user_id: Uuid,
) -> Result<Job, ApiError> {
    // Ownership diperiksa lebih dulu supaya cancel tidak menjadi cara menebak
    // keberadaan job milik orang lain.
    owned(foundation, job_id, owner_user_id).await?;

    repository::request_cancel(foundation.app_db().pool(), job_id, owner_user_id)
        .await
        .map_err(anyhow::Error::from)?;

    // Snapshot dibaca ulang: klien menerima state sesudah permintaan, termasuk
    // saat job ternyata sudah terminal dan tidak berubah.
    owned(foundation, job_id, owner_user_id).await
}

/// Ambil response final job. `NotFound` selama belum ada response durable —
/// klien tidak boleh menyimpulkan hasil dari lifecycle saja.
pub async fn response(
    foundation: &Foundation,
    job_id: Uuid,
    owner_user_id: Uuid,
) -> Result<crate::engine::repository::ResponseDocument, ApiError> {
    let job = owned(foundation, job_id, owner_user_id).await?;
    let version = job.final_response_version.ok_or(ApiError::NotFound)?;

    crate::engine::repository::find_response(foundation.app_db().pool(), job_id, version)
        .await
        .map_err(anyhow::Error::from)?
        .ok_or(ApiError::NotFound)
}

/// Sidik jari payload kanonik — bukan body mentah, karena yang perlu diketahui
/// hanya "sama atau tidak" dan payload dapat memuat PII.
///
/// `session_id` ikut di-hash sehingga kunci yang dipakai ulang untuk session
/// lain terdeteksi sebagai mismatch, bukan diterima diam-diam.
fn fingerprint(session_id: Uuid, request_text: &str, office_ids: &[i64]) -> String {
    let mut offices: Vec<String> = office_ids.iter().map(i64::to_string).collect();
    offices.sort();

    let canonical = format!(
        "session={session_id}\nrequest={request_text}\noffices={}",
        offices.join(",")
    );

    hex::encode(Sha256::digest(canonical.as_bytes()))
}

fn map_create_error(error: CreateError) -> ApiError {
    match error {
        CreateError::SessionNotFound => ApiError::NotFound,
        CreateError::SessionNotActive(status) => {
            ApiError::Conflict(format!("Session is {status}, not active"))
        }
        CreateError::SessionBusy => {
            ApiError::Conflict("Another job is still active in this session".to_string())
        }
        CreateError::KeyReused => {
            ApiError::Conflict("Idempotency-Key was already used with a different payload".to_string())
        }
        CreateError::InProgress => {
            ApiError::Conflict("An identical request is still being processed".to_string())
        }
        CreateError::Database(error) => anyhow::Error::from(error).into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_ignores_office_order() {
        let session = Uuid::new_v4();
        assert_eq!(
            fingerprint(session, "berapa portfolio", &[2, 1]),
            fingerprint(session, "berapa portfolio", &[1, 2])
        );
    }

    #[test]
    fn fingerprint_changes_with_session_request_and_scope() {
        let session = Uuid::new_v4();
        let base = fingerprint(session, "berapa portfolio", &[1]);

        assert_ne!(base, fingerprint(Uuid::new_v4(), "berapa portfolio", &[1]));
        assert_ne!(base, fingerprint(session, "berapa portfolio lain", &[1]));
        assert_ne!(base, fingerprint(session, "berapa portfolio", &[1, 2]));
    }

    #[test]
    fn fingerprint_is_not_the_raw_payload() {
        // Payload dapat memuat PII; yang tersimpan hanya hash.
        let printed = fingerprint(Uuid::new_v4(), "nasabah Budi Santoso", &[]);
        assert!(!printed.contains("Budi"));
        assert_eq!(printed.len(), 64);
    }
}
