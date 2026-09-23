//! Kebijakan baca dataset: otorisasi ulang tiap baca (I7) dan paginasi stabil (§4).

use chrono::{DateTime, Utc};
use foundation::{error::ApiError, state::Foundation};
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use crate::engine::{
    dataset::{
        Cursor, Denied, HandleState, authorize, decode,
        repository::{self, Handle},
    },
    executor,
};

/// Batas atas satu halaman. Klien boleh meminta lebih kecil, tidak lebih besar.
pub const MAX_PAGE_SIZE: i64 = 200;

/// Berapa chunk yang boleh dibuka untuk melayani satu halaman. Chunk dibaca
/// utuh, jadi angka ini adalah batas memori per request — bukan batas paginasi.
const MAX_CHUNKS_PER_PAGE: i64 = 4;

/// Handle sebagaimana dilihat klien. `handle_state` **selalu** ada (C13).
#[derive(Debug, Serialize)]
pub struct HandleView {
    pub dataset_id: Uuid,
    pub job_id: Uuid,
    pub session_id: Uuid,
    pub node_id: Option<String>,
    pub plan_version: Option<i32>,
    pub handle_state: HandleState,
    pub status: String,
    pub schema: Value,
    pub grain: Value,
    pub scope: Value,
    pub provenance: Value,
    pub sort_key: Value,
    pub completeness: String,
    pub completeness_reason: Option<String>,
    /// Set yang TERSIMPAN dibatasi cap — bukan klaim analitik, bukan preview (I4).
    pub truncated: bool,
    pub row_count_available: Option<i64>,
    /// `null` berarti **tidak diketahui**, bukan nol (I4).
    pub row_count_total: Option<i64>,
    pub byte_size: Option<i64>,
    pub chunk_count: i32,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub purged_at: Option<DateTime<Utc>>,
    /// Kenapa baris tidak dapat dibaca, bila memang tidak. Dinyatakan, tidak
    /// pernah didiamkan (I5).
    pub unavailable_reason: Option<&'static str>,
}

#[derive(Debug, Serialize)]
pub struct RowsPage {
    pub dataset_id: Uuid,
    pub handle_state: HandleState,
    /// Urutan yang membekukan paginasi. Halaman berbeda tidak mengubahnya (§4).
    pub sort_key: Value,
    pub rows: Vec<Value>,
    pub cursor: String,
    /// `null` berarti halaman terakhir.
    pub next_cursor: Option<String>,
    pub row_count_available: Option<i64>,
    pub row_count_total: Option<i64>,
    pub truncated: bool,
    pub completeness: String,
    pub completeness_reason: Option<String>,
    pub unavailable_reason: Option<&'static str>,
}

/// Ambil handle dan periksa ulang otorisasinya (I7).
///
/// Dipanggil pada **setiap** pembacaan — metadata maupun baris. Handle adalah
/// referensi, bukan grant. `requested` adalah `office_ids` dari request: ia
/// hanya **mempersempit** otorisasi (PRD §10), sama seperti pada
/// `POST /chat/jobs`; kosong berarti tanpa penyempitan.
async fn authorized(
    foundation: &Foundation,
    dataset_id: Uuid,
    user_id: Uuid,
    requested: &[i64],
) -> Result<Handle, ApiError> {
    let handle = repository::find(foundation.app_db().pool(), dataset_id)
        .await
        .map_err(anyhow::Error::from)?
        .ok_or(ApiError::NotFound)?;

    // Scope diverifikasi ulang terhadap otorisasi yang berlaku SEKARANG, bukan
    // terhadap `scope_json` yang tersimpan: yang tersimpan adalah konteks
    // keputusan untuk audit.
    let caller_offices = executor::authorized_office_ids(foundation.fineract_db(), requested)
        .await
        .map_err(|error| anyhow::anyhow!("scope tidak dapat diturunkan: {:?}", error))?;

    match authorize(
        handle.owner_user_id,
        &handle_offices(&handle.scope_json),
        user_id,
        &caller_offices,
    ) {
        Ok(()) => Ok(handle),
        // Milik orang lain: keberadaannya tidak diungkap.
        Err(Denied::NotOwner) => Err(ApiError::NotFound),
        Err(Denied::ScopeNarrowed) => Err(ApiError::Forbidden),
    }
}

pub async fn read(
    foundation: &Foundation,
    dataset_id: Uuid,
    user_id: Uuid,
    requested: &[i64],
) -> Result<HandleView, ApiError> {
    let handle = authorized(foundation, dataset_id, user_id, requested).await?;
    let state = HandleState::from_status(&handle.status);

    Ok(HandleView {
        dataset_id: handle.id,
        job_id: handle.job_id,
        session_id: handle.session_id,
        node_id: handle.node_id,
        plan_version: handle.plan_version,
        handle_state: state,
        status: handle.status,
        schema: handle.schema_json,
        grain: handle.grain_json,
        scope: handle.scope_json,
        provenance: handle.provenance_json,
        sort_key: handle.sort_key_json,
        completeness: handle.completeness,
        completeness_reason: handle.completeness_reason,
        truncated: handle.truncated,
        row_count_available: handle.row_count_available,
        row_count_total: handle.row_count_total,
        byte_size: handle.byte_size,
        chunk_count: handle.chunk_count,
        created_at: handle.created_at,
        expires_at: handle.expires_at,
        purged_at: handle.purged_at,
        unavailable_reason: unavailable_reason(state),
    })
}

/// Seam lokal (FIN-44, keputusan owner): purge handle milik pemanggil sekarang,
/// agar DS-8.2 dapat dibuktikan tanpa menunggu TTL ≥24 jam.
///
/// Route-nya hanya dipasang saat `APP_ENV=local` (lihat `route::local_router`);
/// di sini otorisasi yang sama dengan pembacaan tetap berlaku — handle milik
/// orang lain tetap 404. Purge memakai aturan reaper: dataset job nonterminal
/// ditolak `409`, bukan dipurge.
pub async fn purge_now(
    foundation: &Foundation,
    dataset_id: Uuid,
    user_id: Uuid,
) -> Result<HandleView, ApiError> {
    authorized(foundation, dataset_id, user_id, &[]).await?;

    let purged = repository::purge_now(foundation.app_db().pool(), dataset_id)
        .await
        .map_err(anyhow::Error::from)?;
    if !purged {
        return Err(ApiError::Conflict(
            "dataset is not ready or its job is not terminal".into(),
        ));
    }

    read(foundation, dataset_id, user_id, &[]).await
}

/// Satu halaman baris, keyset atas `(chunk_index, row)`.
///
/// Handle mati tetap dijawab `200` dengan `handle_state`-nya: dataset yang
/// `expired`/`purged` **masih terbaca statusnya** (§7). Yang tidak boleh terjadi
/// adalah halaman kosong yang tampak seperti "datanya memang nol".
pub async fn rows(
    foundation: &Foundation,
    dataset_id: Uuid,
    user_id: Uuid,
    requested: &[i64],
    cursor: Cursor,
    limit: i64,
) -> Result<RowsPage, ApiError> {
    let handle = authorized(foundation, dataset_id, user_id, requested).await?;
    let state = HandleState::from_status(&handle.status);
    let limit = limit.clamp(1, MAX_PAGE_SIZE);

    let mut page = RowsPage {
        dataset_id: handle.id,
        handle_state: state,
        sort_key: handle.sort_key_json,
        rows: Vec::new(),
        cursor: cursor.encode(),
        next_cursor: None,
        row_count_available: handle.row_count_available,
        row_count_total: handle.row_count_total,
        truncated: handle.truncated,
        completeness: handle.completeness,
        completeness_reason: handle.completeness_reason,
        unavailable_reason: unavailable_reason(state),
    };

    if !state.is_readable() {
        return Ok(page);
    }

    let stored = repository::chunks_from(
        foundation.app_db().pool(),
        dataset_id,
        cursor.chunk_index,
        MAX_CHUNKS_PER_PAGE,
    )
    .await
    .map_err(anyhow::Error::from)?;

    let available = handle.row_count_available.unwrap_or_default();
    let mut position = cursor.row;
    // Chunk tempat baris berikutnya berada; dipakai menyusun cursor lanjutan.
    let mut next_chunk = cursor.chunk_index;

    for chunk in &stored {
        if page.rows.len() as i64 >= limit {
            break;
        }

        let decoded = match decode(
            &chunk.format,
            chunk.encoding_version,
            chunk.payload.as_ref(),
        ) {
            Ok(rows) => rows,
            // Chunk yang ditulis encoding lebih baru: berhenti dan NYATAKAN,
            // jangan lanjutkan seolah dataset habis di sini (I5).
            Err(_) => {
                page.unavailable_reason = Some("chunk_encoding_unsupported");
                return Ok(page);
            }
        };

        // Cursor menunjuk baris, chunk dibaca utuh: offset di dalam chunk hanya
        // menentukan dari mana halaman ini mulai, bukan baris mana yang ada.
        let offset = (position - chunk.row_from).max(0) as usize;
        for row in decoded.into_iter().skip(offset) {
            if page.rows.len() as i64 >= limit {
                break;
            }

            page.rows.push(row);
            position += 1;
        }

        // Chunk habis terbaca → baris berikutnya ada di chunk sesudahnya.
        next_chunk = if position >= chunk.row_to {
            chunk.chunk_index + 1
        } else {
            chunk.chunk_index
        };
    }

    if position < available {
        page.next_cursor = Some(
            Cursor {
                chunk_index: next_chunk,
                row: position,
            }
            .encode(),
        );
    }

    Ok(page)
}

fn unavailable_reason(state: HandleState) -> Option<&'static str> {
    match state {
        HandleState::Live => None,
        HandleState::Expired => Some("dataset_expired"),
        HandleState::Purged => Some("dataset_purged"),
        HandleState::None => Some("dataset_not_materialized"),
    }
}

/// Office yang disnapshot pada `scope_json` saat dataset dibuat.
fn handle_offices(scope_json: &Value) -> Vec<i64> {
    scope_json
        .get("office_ids")
        .and_then(Value::as_array)
        .map(|ids| ids.iter().filter_map(Value::as_i64).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn ds_8_4_scope_is_read_from_the_snapshot_not_guessed() {
        // DS-8.4 — scope handle yang tidak dapat dibaca berarti kosong, bukan
        // "cocok dengan apa pun".
        assert_eq!(handle_offices(&json!({ "office_ids": [1, 2] })), vec![1, 2]);
        assert!(handle_offices(&json!({})).is_empty());
        assert!(handle_offices(&json!({ "office_ids": "all" })).is_empty());
    }

    #[test]
    fn ds_8_2_a_dead_handle_always_says_why() {
        // DS-8.2 — kedaluwarsa dinyatakan, tidak pernah tampil sebagai nol baris.
        assert_eq!(unavailable_reason(HandleState::Live), None);
        assert_eq!(
            unavailable_reason(HandleState::Expired),
            Some("dataset_expired")
        );
        assert_eq!(
            unavailable_reason(HandleState::Purged),
            Some("dataset_purged")
        );
        assert_eq!(
            unavailable_reason(HandleState::None),
            Some("dataset_not_materialized")
        );
    }
}
