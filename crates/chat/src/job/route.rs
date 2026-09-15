//! Handler job: accept/read/delegate saja.
//!
//! HTTP 202 berarti **durable acceptance** — bukan bahwa analisis berhasil
//! (`docs/contracts/api.md`).

use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use foundation::{AuthUser, Envelope, error::ApiError, state::Foundation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

use crate::job::{
    repository::Job,
    service::{self, Acceptance},
};

/// Header wajib untuk setiap operasi tulis job (`api.md`).
const IDEMPOTENCY_HEADER: &str = "Idempotency-Key";

/// Batas panjang pertanyaan. Ini **guard**, bukan angka hasil tuning: ia hanya
/// mencegah body raksasa menjadi beban gratis. Batas budget token yang mengikat
/// ada di `docs/operations/runtime.md` §6 dan diberlakukan Engine.
const MAX_REQUEST_CHARS: u64 = 4_000;

pub fn router() -> Router<Foundation> {
    Router::new()
        .route("/chat/jobs", post(create))
        .route("/chat/jobs/{job_id}", get(read))
        .route("/chat/jobs/{job_id}/response", get(response))
        .route("/chat/jobs/{job_id}/cancel", post(cancel))
}

#[derive(Debug, Deserialize, Validate)]
pub struct CreateJobRequest {
    session_id: Uuid,
    #[validate(length(min = 1, max = "MAX_REQUEST_CHARS"))]
    request_text: String,
    /// Penyempitan scope office. Kosong berarti seluruh scope yang diizinkan;
    /// ia tidak pernah memperlebar izin.
    #[serde(default)]
    office_ids: Vec<i64>,
}

#[derive(Debug, Serialize)]
pub struct JobAcknowledgement {
    job_id: Uuid,
    session_id: Uuid,
    lifecycle: String,
    /// Cursor event pada snapshot ini; subscriber melanjutkan sesudahnya.
    event_cursor: i64,
}

async fn create(
    State(foundation): State<Foundation>,
    user: AuthUser,
    headers: HeaderMap,
    Json(request): Json<CreateJobRequest>,
) -> Result<Response, ApiError> {
    request
        .validate()
        .map_err(|error| ApiError::Unprocessable(error.to_string()))?;

    let key = idempotency_key(&headers, foundation.config())?;

    let accepted = service::create(
        &foundation,
        user.user_id,
        request.session_id,
        &request.request_text,
        &request.office_ids,
        &key,
    )
    .await?;

    Ok(match accepted {
        Acceptance::Created(job) => {
            (StatusCode::ACCEPTED, Json(Envelope::ok(acknowledgement(&job)))).into_response()
        }
        // Replay mengembalikan acknowledgement tersimpan apa adanya: klien
        // wajib memperoleh job_id yang sama, bukan job kedua.
        Acceptance::Replayed { status, body } => (
            StatusCode::from_u16(status as u16).unwrap_or(StatusCode::ACCEPTED),
            Json(Envelope::ok(body)),
        )
            .into_response(),
    })
}

async fn read(
    State(foundation): State<Foundation>,
    user: AuthUser,
    Path(job_id): Path<Uuid>,
) -> Result<Json<Envelope<Job>>, ApiError> {
    let job = service::owned(&foundation, job_id, user.user_id).await?;
    Ok(Json(Envelope::ok(job)))
}

async fn response(
    State(foundation): State<Foundation>,
    user: AuthUser,
    Path(job_id): Path<Uuid>,
) -> Result<Json<Envelope<crate::engine::repository::ResponseDocument>>, ApiError> {
    let document = service::response(&foundation, job_id, user.user_id).await?;
    Ok(Json(Envelope::ok(document)))
}

async fn cancel(
    State(foundation): State<Foundation>,
    user: AuthUser,
    Path(job_id): Path<Uuid>,
) -> Result<Json<Envelope<Job>>, ApiError> {
    let job = service::cancel(&foundation, job_id, user.user_id).await?;
    Ok(Json(Envelope::ok(job)))
}

fn acknowledgement(job: &Job) -> JobAcknowledgement {
    JobAcknowledgement {
        job_id: job.id,
        session_id: job.session_id,
        lifecycle: job.lifecycle.clone(),
        event_cursor: job.last_event_sequence,
    }
}

/// Ambil dan validasi `Idempotency-Key`.
///
/// Batas panjang mengikuti CHECK pada tabel `idempotency_keys`: kunci yang
/// lolos di sini tetapi ditolak database akan muncul sebagai 500, padahal ia
/// kesalahan klien.
fn idempotency_key(headers: &HeaderMap, config: &foundation::Config) -> Result<String, ApiError> {
    let key = headers
        .get(IDEMPOTENCY_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .unwrap_or_default();

    let length = key.chars().count();
    if length < config.idempotency_key_min_length || length > config.idempotency_key_max_length {
        return Err(ApiError::Unprocessable(format!(
            "{IDEMPOTENCY_HEADER} must be between {} and {} characters",
            config.idempotency_key_min_length, config.idempotency_key_max_length
        )));
    }

    Ok(key.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> foundation::Config {
        // Hanya dua field yang dibaca fungsi ini; sisanya tidak relevan.
        serde_json::from_value(serde_json::json!({
            "app_database_url": "postgres://app/jarvis",
            "fineract_database_url": "postgres://ro/fineract",
            "fineract_tenant": "default",
            "jwt_access_secret": "a",
            "jwt_refresh_secret": "b",
        }))
        .unwrap()
    }

    fn headers_with(key: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(IDEMPOTENCY_HEADER, key.parse().unwrap());
        headers
    }

    #[test]
    fn accepts_key_within_database_limits() {
        let key = "a".repeat(16);
        assert_eq!(idempotency_key(&headers_with(&key), &config()).unwrap(), key);
    }

    #[test]
    fn rejects_missing_or_short_key() {
        assert!(idempotency_key(&HeaderMap::new(), &config()).is_err());
        assert!(idempotency_key(&headers_with("terlalu-pendek"), &config()).is_err());
    }

    #[test]
    fn rejects_key_longer_than_column_check() {
        let key = "a".repeat(256);
        assert!(idempotency_key(&headers_with(&key), &config()).is_err());
    }
}
