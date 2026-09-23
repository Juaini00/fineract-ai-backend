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

/// `office_ids` berbentuk daftar dipisah koma (`?office_ids=1,2`). Ia hanya
/// mempersempit otorisasi (PRD §10); tidak hadir berarti tanpa penyempitan.
#[derive(Debug, Deserialize)]
pub struct ReadQuery {
    office_ids: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RowsQuery {
    cursor: Option<String>,
    limit: Option<i64>,
    office_ids: Option<String>,
}

async fn read(
    State(foundation): State<Foundation>,
    user: AuthUser,
    Path(dataset_id): Path<Uuid>,
    Query(query): Query<ReadQuery>,
) -> Result<Json<Envelope<HandleView>>, ApiError> {
    let requested = office_scope(query.office_ids.as_deref())?;
    let handle = service::read(&foundation, dataset_id, user.user_id, &requested).await?;
    Ok(Json(Envelope::ok(handle)))
}

async fn rows(
    State(foundation): State<Foundation>,
    user: AuthUser,
    Path(dataset_id): Path<Uuid>,
    Query(query): Query<RowsQuery>,
) -> Result<Json<Envelope<RowsPage>>, ApiError> {
    let requested = office_scope(query.office_ids.as_deref())?;
    let cursor = cursor(query.cursor.as_deref())?;
    let limit = query.limit.unwrap_or(service::MAX_PAGE_SIZE);

    let page = service::rows(
        &foundation,
        dataset_id,
        user.user_id,
        &requested,
        cursor,
        limit,
    )
    .await?;
    Ok(Json(Envelope::ok(page)))
}

/// Penyempitan scope dari query. Nilai yang tidak dapat dibaca ditolak, tidak
/// pernah diabaikan: `office_ids=` atau `office_ids=1,x` yang diperlakukan
/// sebagai "tanpa penyempitan" diam-diam MEMPERLEBAR apa yang diminta klien.
fn office_scope(raw: Option<&str>) -> Result<Vec<i64>, ApiError> {
    let Some(raw) = raw else {
        return Ok(Vec::new());
    };

    raw.split(',')
        .map(|part| part.trim().parse::<i64>().ok().filter(|id| *id > 0))
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| {
            ApiError::Unprocessable(
                "office_ids must be a comma-separated list of office ids".into(),
            )
        })
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

    #[test]
    fn office_scope_narrows_or_is_rejected_never_ignored() {
        assert_eq!(office_scope(None).unwrap(), Vec::<i64>::new());
        assert_eq!(office_scope(Some("1, 2")).unwrap(), vec![1, 2]);
        // Kosong atau rusak ditolak: menganggapnya "tanpa penyempitan" berarti
        // memperlebar scope yang diminta klien.
        assert!(office_scope(Some("")).is_err());
        assert!(office_scope(Some("1,x")).is_err());
        assert!(office_scope(Some("1,,2")).is_err());
        assert!(office_scope(Some("0")).is_err());
    }
}
