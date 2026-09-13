//! Handler session percakapan.

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use foundation::{AuthUser, Envelope, error::ApiError, state::Foundation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

use crate::session::{
    repository::Session,
    service::{self, MAX_PAGE_SIZE},
};

pub fn router() -> Router<Foundation> {
    Router::new()
        .route("/chat/sessions", post(create).get(list))
        .route("/chat/sessions/{session_id}", get(read))
}

#[derive(Debug, Deserialize, Validate)]
pub struct CreateSessionRequest {
    #[validate(length(min = 1, max = 255))]
    title: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    limit: Option<i64>,
    /// Cursor keyset: pasangan `updated_at` + `id` dari baris terakhir halaman
    /// sebelumnya. Keduanya wajib bersama — satu saja tidak menentukan posisi.
    before_updated_at: Option<DateTime<Utc>>,
    before_id: Option<Uuid>,
}

#[derive(Debug, Serialize)]
pub struct SessionPage {
    sessions: Vec<Session>,
    next_before_updated_at: Option<DateTime<Utc>>,
    next_before_id: Option<Uuid>,
}

async fn create(
    State(foundation): State<Foundation>,
    user: AuthUser,
    Json(request): Json<CreateSessionRequest>,
) -> Result<impl IntoResponse, ApiError> {
    request
        .validate()
        .map_err(|error| ApiError::Unprocessable(error.to_string()))?;

    let session = service::create(&foundation, user.user_id, request.title.as_deref()).await?;
    Ok((StatusCode::CREATED, Json(Envelope::ok(session))))
}

async fn list(
    State(foundation): State<Foundation>,
    user: AuthUser,
    Query(query): Query<ListQuery>,
) -> Result<Json<Envelope<SessionPage>>, ApiError> {
    let before = match (query.before_updated_at, query.before_id) {
        (Some(updated_at), Some(id)) => Some((updated_at, id)),
        (None, None) => None,
        _ => {
            return Err(ApiError::Unprocessable(
                "before_updated_at and before_id must be provided together".to_string(),
            ));
        }
    };

    let limit = query.limit.unwrap_or(MAX_PAGE_SIZE);
    let sessions = service::list(&foundation, user.user_id, before, limit).await?;
    let last = sessions.last();

    Ok(Json(Envelope::ok(SessionPage {
        next_before_updated_at: last.map(|session| session.updated_at),
        next_before_id: last.map(|session| session.id),
        sessions,
    })))
}

async fn read(
    State(foundation): State<Foundation>,
    user: AuthUser,
    Path(session_id): Path<Uuid>,
) -> Result<Json<Envelope<Session>>, ApiError> {
    let session = service::owned(&foundation, session_id, user.user_id).await?;
    Ok(Json(Envelope::ok(session)))
}
