//! Ekstraktor identitas dari `Authorization: Bearer <access token>`.
//!
//! Invarian I7: otorisasi tidak pernah dibaca dari state turunan. Identitas
//! berasal dari token yang terverifikasi, bukan dari memory, summary, atau
//! parameter request yang dikirim klien.

use axum::{
    extract::FromRequestParts,
    http::{header::AUTHORIZATION, request::Parts},
};
use uuid::Uuid;

use crate::{auth::token::TokenIssuer, error::ApiError, state::Foundation};

/// Pengguna yang sudah terautentikasi.
///
/// Access token bersifat stateless selama masa berlakunya (15 menit default):
/// pencabutan session berlaku pada refresh berikutnya, bukan seketika. Bila
/// kelak pencabutan seketika dibutuhkan, ia menjadi lookup session per request
/// — konsekuensinya satu query tambahan pada setiap endpoint.
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: Uuid,
    /// `auth_sessions.id`, bukan `chat_sessions.id`.
    pub auth_session_id: Uuid,
    pub role: String,
}

impl FromRequestParts<Foundation> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        foundation: &Foundation,
    ) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
            .map(str::trim)
            .filter(|token| !token.is_empty())
            .ok_or(ApiError::Unauthorized)?;

        let claims = TokenIssuer::new(foundation.config())
            .verify(token)
            .map_err(|_| ApiError::Unauthorized)?;

        Ok(Self {
            user_id: claims.sub,
            auth_session_id: claims.sid,
            role: claims.role,
        })
    }
}
