//! Handler klarifikasi.
//!
//! `POST /chat/jobs/{id}/responses` menjawab form pada job yang **sama** — ia
//! tidak pernah membuat job pengganti (clarifications.md).

use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
};
use foundation::{AuthUser, Envelope, error::ApiError, state::Foundation};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

use crate::{
    clarification::{repository::Form, service},
    job::route::idempotency_key,
};

pub fn router() -> Router<Foundation> {
    Router::new()
        .route("/chat/jobs/{job_id}/clarification", get(active))
        .route("/chat/jobs/{job_id}/responses", post(answer))
}

#[derive(Debug, Deserialize)]
pub struct AnswerRequest {
    clarification_id: Uuid,
    revision: i32,
    /// Nilai bertipe per field. Bentuk v1: teks yang diparse menurut tipe
    /// parameter. Opsi dari resolver (`option_id`) menyusul bersama resolver.
    answers: BTreeMap<String, String>,
}

#[derive(Debug, Serialize)]
pub struct AnswerAcknowledgement {
    job_id: Uuid,
    clarification_id: Uuid,
    revision: i32,
    lifecycle: &'static str,
}

async fn active(
    State(foundation): State<Foundation>,
    user: AuthUser,
    Path(job_id): Path<Uuid>,
) -> Result<Json<Envelope<Form>>, ApiError> {
    let form = service::active_form(&foundation, job_id, user.user_id).await?;
    Ok(Json(Envelope::ok(form)))
}

async fn answer(
    State(foundation): State<Foundation>,
    user: AuthUser,
    Path(job_id): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<AnswerRequest>,
) -> Result<impl IntoResponse, ApiError> {
    // Kunci divalidasi walau penyimpanan idempotency untuk operasi ini belum
    // dipakai: klien tidak boleh terbiasa mengirim tanpa kunci lalu menemukan
    // kontraknya berubah kemudian.
    let _key = idempotency_key(&headers, foundation.config())?;

    let form = service::answer(
        &foundation,
        job_id,
        user.user_id,
        request.clarification_id,
        request.revision,
        &request.answers,
    )
    .await?;

    // 202: jawaban sudah durable dan job kembali mengantre. `job.resumed`
    // dipancarkan saat worker benar-benar melanjutkan, bukan di sini.
    Ok((
        StatusCode::ACCEPTED,
        Json(Envelope::ok(AnswerAcknowledgement {
            job_id,
            clarification_id: form.clarification_id,
            revision: form.revision,
            lifecycle: "Queued",
        })),
    ))
}
