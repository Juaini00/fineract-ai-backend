//! Handler klarifikasi.
//!
//! `POST /chat/jobs/{id}/responses` menjawab form pada job yang **sama** — ia
//! tidak pernah membuat job pengganti (clarifications.md).

use std::{collections::BTreeMap, sync::Arc};

use axum::{
    Extension, Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
};
use foundation::{AuthUser, Envelope, error::ApiError, state::Foundation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    catalog::Catalog,
    clarification::{
        repository::Form,
        service::{self, OptionPage},
    },
    job::route::idempotency_key,
};

pub fn router() -> Router<Foundation> {
    Router::new()
        .route("/chat/jobs/{job_id}/clarification", get(active))
        .route("/chat/jobs/{job_id}/clarification/options", get(options))
        .route("/chat/jobs/{job_id}/responses", post(answer))
}

#[derive(Debug, Deserialize)]
pub struct AnswerRequest {
    clarification_id: Uuid,
    revision: i32,
    /// `answer` (default) atau `skip`. Skip memakai endpoint yang sama dan
    /// tidak pernah menjadi endpoint baru (clarifications.md).
    #[serde(default)]
    action: Action,
    /// Satu nilai per field. Untuk field `single_choice` nilainya adalah
    /// `option_id` yang server terbitkan — bukan teks bebas, dan bukan nilai
    /// binding yang dikirim klien (K1).
    #[serde(default)]
    answers: BTreeMap<String, String>,
}

#[derive(Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    #[default]
    Answer,
    Skip,
}

#[derive(Debug, Serialize)]
pub struct SkipAcknowledgement {
    job_id: Uuid,
    clarification_id: Uuid,
    revision: i32,
    lifecycle: &'static str,
    outcome: &'static str,
}

#[derive(Debug, Deserialize)]
pub struct OptionsQuery {
    field_id: String,
    #[serde(default)]
    cursor: usize,
    #[serde(default)]
    limit: Option<usize>,
    /// Penyaring atas kandidat yang sudah ter-scope. Ia mempersempit daftar
    /// yang ditampilkan; ia tidak pernah memperluas scope.
    #[serde(default, rename = "q")]
    search: Option<String>,
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

async fn options(
    State(foundation): State<Foundation>,
    Extension(catalog): Extension<Arc<Catalog>>,
    user: AuthUser,
    Path(job_id): Path<Uuid>,
    Query(query): Query<OptionsQuery>,
) -> Result<Json<Envelope<OptionPage>>, ApiError> {
    let page = service::options(
        &foundation,
        &catalog,
        job_id,
        user.user_id,
        &query.field_id,
        query.cursor,
        query.limit,
        query.search.as_deref(),
    )
    .await?;

    Ok(Json(Envelope::ok(page)))
}

async fn answer(
    State(foundation): State<Foundation>,
    Extension(catalog): Extension<Arc<Catalog>>,
    user: AuthUser,
    Path(job_id): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<AnswerRequest>,
) -> Result<impl IntoResponse, ApiError> {
    // Kunci divalidasi walau penyimpanan idempotency untuk operasi ini belum
    // dipakai: klien tidak boleh terbiasa mengirim tanpa kunci lalu menemukan
    // kontraknya berubah kemudian.
    let _key = idempotency_key(&headers, foundation.config())?;

    if request.action == Action::Skip {
        let form = service::skip(
            &foundation,
            job_id,
            user.user_id,
            request.clarification_id,
            request.revision,
        )
        .await?;

        // 200, bukan 202: skip sudah terminal saat handler membalas — response
        // document sudah durable. 202 akan menyiratkan masih ada pekerjaan.
        return Ok((
            StatusCode::OK,
            Json(Envelope::ok(SkipAcknowledgement {
                job_id,
                clarification_id: form.clarification_id,
                revision: form.revision,
                lifecycle: "Completed",
                outcome: "SkippedByUser",
            })),
        )
            .into_response());
    }

    let form = service::answer(
        &foundation,
        &catalog,
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
    )
        .into_response())
}
