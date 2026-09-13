//! Error domain untuk handler HTTP.
//!
//! Aturan yang ditegakkan di sini (AGENTS.md): **error publik tersanitasi**.
//! Detail internal (SQL, prompt, stack, pesan `anyhow` mentah) dicatat lewat
//! tracing dan tidak pernah diserialisasi ke klien. Pesan publik berbahasa
//! Inggris (clarifications.md: "Public product text is English").
//!
//! ## Kenapa `Display`/`Error`/`From` ditulis manual, bukan `thiserror`
//!
//! Crate ini bernama `core`, sama dengan crate standar Rust. Derive `thiserror`
//! mengeluarkan path `::core::fmt` / `::core::convert`; pada kompilasi doctest
//! (`rustdoc --test`), rustdoc melewatkan crate ini sendiri sebagai
//! `--extern core=<rlib core>`, sehingga path itu menunjuk crate ini — bukan
//! std `core` — dan `cargo test` gagal. `std::fmt`/`std::convert` tidak
//! tertimpa dan aman. JANGAN kembalikan ke `#[derive(thiserror::Error)]` di
//! crate ini tanpa menyelesaikan bentrokan nama tersebut.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use tracing::error;

use crate::envelope::Envelope;

/// Kesalahan yang dapat dikembalikan handler sebagai response HTTP.
#[derive(Debug)]
pub enum ApiError {
    /// Sumber daya tidak ditemukan (atau sengaja tidak diungkap keberadaannya).
    NotFound,

    /// Autentikasi tidak ada/tidak sah. Mekanisme bearer final menunggu
    /// `docs/security/access-data-policy.md`; varian ini hanya menyediakan
    /// bentuk response yang stabil.
    Unauthorized,

    /// Autentikasi sah tetapi tanpa izin untuk sumber daya ini.
    Forbidden,

    /// Konflik state — mis. job nonterminal kedua pada session yang sama (409).
    Conflict(String),

    /// Input valid secara bentuk tetapi gagal validasi domain/field (422).
    Unprocessable(String),

    /// Kegagalan internal tak terduga. Detail asli dicatat ke log; klien hanya
    /// menerima pesan generik.
    Internal(anyhow::Error),
}

impl ApiError {
    /// Kode machine-readable + pesan publik yang aman, per varian.
    fn public(&self) -> (StatusCode, &'static str, String) {
        match self {
            ApiError::NotFound => (StatusCode::NOT_FOUND, "NOT_FOUND", "Not found".to_string()),
            ApiError::Unauthorized => {
                (StatusCode::UNAUTHORIZED, "UNAUTHORIZED", "Unauthorized".to_string())
            }
            ApiError::Forbidden => (StatusCode::FORBIDDEN, "FORBIDDEN", "Forbidden".to_string()),
            ApiError::Conflict(message) => {
                (StatusCode::CONFLICT, "CONFLICT", message.clone())
            }
            ApiError::Unprocessable(message) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "INVALID",
                message.clone(),
            ),
            ApiError::Internal(source) => {
                // Satu-satunya tempat detail internal boleh terlihat: log.
                error!(error = %source, "internal error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "INTERNAL",
                    "Internal server error".to_string(),
                )
            }
        }
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiError::NotFound => write!(f, "not found"),
            ApiError::Unauthorized => write!(f, "unauthorized"),
            ApiError::Forbidden => write!(f, "forbidden"),
            ApiError::Conflict(message) => write!(f, "conflict: {message}"),
            ApiError::Unprocessable(message) => write!(f, "unprocessable: {message}"),
            ApiError::Internal(source) => write!(f, "{source}"),
        }
    }
}

impl std::error::Error for ApiError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ApiError::Internal(source) => Some(source.as_ref()),
            _ => None,
        }
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(source: anyhow::Error) -> Self {
        ApiError::Internal(source)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = self.public();
        (status, Json(Envelope::<()>::err(code, message))).into_response()
    }
}

#[cfg(test)]
mod tests {
    use axum::body::to_bytes;
    use serde_json::Value;

    use super::*;

    async fn body_of(response: Response) -> Value {
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn not_found_maps_to_404_envelope() {
        let response = ApiError::NotFound.into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let body = body_of(response).await;
        assert_eq!(body["success"], false);
        assert_eq!(body["data"], Value::Null);
        assert_eq!(body["error"]["code"], "NOT_FOUND");
    }

    #[tokio::test]
    async fn conflict_preserves_public_message() {
        let response = ApiError::Conflict("another job is running".into()).into_response();
        assert_eq!(response.status(), StatusCode::CONFLICT);

        let body = body_of(response).await;
        assert_eq!(body["error"]["message"], "another job is running");
    }

    #[tokio::test]
    async fn internal_never_leaks_source() {
        let secret = "SELECT password FROM users"; // contoh detail yang dilarang bocor
        let response = ApiError::Internal(anyhow::anyhow!("db failure: {secret}")).into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let body = body_of(response).await;
        let rendered = body.to_string();
        assert_eq!(body["error"]["message"], "Internal server error");
        assert!(!rendered.contains("password"), "detail internal bocor: {rendered}");
    }

    #[test]
    fn from_anyhow_wraps_as_internal() {
        let error: ApiError = anyhow::anyhow!("boom").into();
        assert!(matches!(error, ApiError::Internal(_)));
    }

    #[test]
    fn internal_error_reports_its_source() {
        let error = ApiError::Internal(anyhow::anyhow!("boom"));
        assert!(std::error::Error::source(&error).is_some());
    }
}
