//! Handler baca dataset.
//!
//! Tidak ada endpoint tulis: dataset dibuat oleh worker dan **immutable setelah
//! `ready`** (§1). Koreksi atau penyegaran adalah dataset baru, bukan UPDATE.

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::get,
};
use foundation::{AuthUser, Envelope, error::ApiError, state::Foundation};
use serde::Deserialize;
use uuid::Uuid;

use crate::engine::dataset::{
    Cursor,
    service::{self, HandleView, RowsPage},
};

pub fn router() -> Router<Foundation> {
    Router::new()
        .route("/chat/datasets/{dataset_id}", get(read))
        .route("/chat/datasets/{dataset_id}/rows", get(rows))
}

#[derive(Debug, Deserialize)]
pub struct RowsQuery {
    cursor: Option<String>,
    limit: Option<i64>,
}

async fn read(
    State(foundation): State<Foundation>,
    user: AuthUser,
    Path(dataset_id): Path<Uuid>,
) -> Result<Json<Envelope<HandleView>>, ApiError> {
    let handle = service::read(&foundation, dataset_id, user.user_id).await?;
    Ok(Json(Envelope::ok(handle)))
}

async fn rows(
    State(foundation): State<Foundation>,
    user: AuthUser,
    Path(dataset_id): Path<Uuid>,
    Query(query): Query<RowsQuery>,
) -> Result<Json<Envelope<RowsPage>>, ApiError> {
    let cursor = cursor(query.cursor.as_deref())?;
    let limit = query.limit.unwrap_or(service::MAX_PAGE_SIZE);

    let page = service::rows(&foundation, dataset_id, user.user_id, cursor, limit).await?;
    Ok(Json(Envelope::ok(page)))
}

/// Cursor rusak ditolak, tidak pernah diperlakukan sebagai awal: menebak berarti
/// diam-diam mengulang halaman pertama, dan pengulangan itu tidak muncul sebagai
/// kesalahan apa pun.
fn cursor(raw: Option<&str>) -> Result<Cursor, ApiError> {
    match raw {
        None => Ok(Cursor::START),
        Some(raw) => Cursor::parse(raw)
            .ok_or_else(|| ApiError::Unprocessable("cursor is not a valid dataset cursor".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ds_8_3_missing_cursor_starts_at_the_first_row() {
        // DS-8.3 — awal yang eksplisit, dan cursor rusak yang ditolak.
        assert_eq!(cursor(None).unwrap(), Cursor::START);
        assert_eq!(cursor(Some("1:1000")).unwrap().row, 1000);
        assert!(cursor(Some("halaman-2")).is_err());
    }
}
