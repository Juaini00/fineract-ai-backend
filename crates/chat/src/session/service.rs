//! Kebijakan session: kepemilikan dan status.

use chrono::{DateTime, Utc};
use foundation::{error::ApiError, state::Foundation};
use uuid::Uuid;

use crate::session::repository::{self, Message, Session};

/// Batas atas satu halaman. Klien boleh meminta lebih kecil, tidak lebih besar:
/// halaman tak berbatas adalah cara paling mudah membuat satu request menarik
/// seluruh tabel.
pub const MAX_PAGE_SIZE: i64 = 50;

pub async fn create(
    foundation: &Foundation,
    owner_user_id: Uuid,
    title: Option<&str>,
) -> Result<Session, ApiError> {
    repository::insert(foundation.app_db().pool(), owner_user_id, title)
        .await
        .map_err(|error| anyhow::Error::from(error).into())
}

pub async fn list(
    foundation: &Foundation,
    owner_user_id: Uuid,
    before: Option<(DateTime<Utc>, Uuid)>,
    limit: i64,
) -> Result<Vec<Session>, ApiError> {
    repository::list_for_owner(
        foundation.app_db().pool(),
        owner_user_id,
        before,
        limit.clamp(1, MAX_PAGE_SIZE),
    )
    .await
    .map_err(|error| anyhow::Error::from(error).into())
}

/// Riwayat satu session. Kepemilikan diperiksa lewat `owned` lebih dulu (I7):
/// otorisasi dibaca dari `chat_sessions.owner_user_id`, bukan dari baris riwayat.
pub async fn messages(
    foundation: &Foundation,
    session_id: Uuid,
    owner_user_id: Uuid,
    before: Option<(DateTime<Utc>, Uuid)>,
    limit: i64,
) -> Result<Vec<Message>, ApiError> {
    owned(foundation, session_id, owner_user_id).await?;

    repository::list_messages(
        foundation.app_db().pool(),
        session_id,
        before,
        limit.clamp(1, MAX_PAGE_SIZE),
    )
    .await
    .map_err(|error| anyhow::Error::from(error).into())
}

/// Ambil session milik pengguna ini.
///
/// Session milik orang lain menghasilkan `NotFound`, bukan `Forbidden`:
/// membedakan keduanya memberi tahu penanya bahwa ID itu ada.
pub async fn owned(
    foundation: &Foundation,
    session_id: Uuid,
    owner_user_id: Uuid,
) -> Result<Session, ApiError> {
    let session = repository::find(foundation.app_db().pool(), session_id)
        .await
        .map_err(anyhow::Error::from)?
        .ok_or(ApiError::NotFound)?;

    if session.owner_user_id != owner_user_id {
        return Err(ApiError::NotFound);
    }

    Ok(session)
}
