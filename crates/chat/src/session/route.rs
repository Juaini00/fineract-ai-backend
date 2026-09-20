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
    repository::{Message, Session},
    service::{self, MAX_PAGE_SIZE},
};

pub fn router() -> Router<Foundation> {
    Router::new()
        .route("/chat/sessions", post(create).get(list))
        .route("/chat/sessions/{session_id}", get(read))
        .route("/chat/sessions/{session_id}/messages", get(messages))
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

#[derive(Debug, Deserialize)]
pub struct MessageQuery {
    limit: Option<i64>,
    before_created_at: Option<DateTime<Utc>>,
    before_id: Option<Uuid>,
}

#[derive(Debug, Serialize)]
pub struct MessagePage {
    messages: Vec<Message>,
    next_before_created_at: Option<DateTime<Utc>>,
    next_before_id: Option<Uuid>,
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
    let before = cursor(query.before_updated_at, query.before_id, "before_updated_at")?;

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

async fn messages(
    State(foundation): State<Foundation>,
    user: AuthUser,
    Path(session_id): Path<Uuid>,
    Query(query): Query<MessageQuery>,
) -> Result<Json<Envelope<MessagePage>>, ApiError> {
    let before = cursor(query.before_created_at, query.before_id, "before_created_at")?;
    let limit = query.limit.unwrap_or(MAX_PAGE_SIZE);

    let messages = service::messages(&foundation, session_id, user.user_id, before, limit).await?;
    let last = messages.last();

    Ok(Json(Envelope::ok(MessagePage {
        next_before_created_at: last.map(|message| message.created_at),
        next_before_id: last.map(|message| message.id),
        messages,
    })))
}

/// Cursor keyset selalu berpasangan: satu komponen saja tidak menentukan
/// posisi, dan menebak komponen yang hilang melewatkan atau menggandakan baris.
fn cursor(
    at: Option<DateTime<Utc>>,
    id: Option<Uuid>,
    at_field: &str,
) -> Result<Option<(DateTime<Utc>, Uuid)>, ApiError> {
    match (at, id) {
        (Some(at), Some(id)) => Ok(Some((at, id))),
        (None, None) => Ok(None),
        _ => Err(ApiError::Unprocessable(format!(
            "{at_field} and before_id must be provided together"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_requires_both_components() {
        let at = Utc::now();
        let id = Uuid::new_v4();

        assert_eq!(
            cursor(Some(at), Some(id), "before_created_at").unwrap(),
            Some((at, id))
        );
        assert_eq!(cursor(None, None, "before_created_at").unwrap(), None);
        assert!(cursor(Some(at), None, "before_created_at").is_err());
        assert!(cursor(None, Some(id), "before_updated_at").is_err());
    }
}
