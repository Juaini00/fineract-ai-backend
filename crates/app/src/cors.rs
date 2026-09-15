//! CORS untuk dashboard web.
//!
//! Frontend dan backend saat pengembangan berbagi host yang sama (maka cookie
//! refresh `SameSite=Strict` tetap mengalir) tetapi memakai port berbeda, jadi
//! request tetap cross-origin. Tanpa layer ini axum tidak memancarkan header
//! CORS sama sekali dan preflight `OPTIONS` dijawab `405 Method Not Allowed`.
//!
//! Kredensial (`withCredentials: true` pada axios, cookie refresh) menuntut
//! `Access-Control-Allow-Credentials: true` dan origin yang eksplisit — `*`
//! tidak pernah sah bersama kredensial.

use axum::http::{
    header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE},
    request::Parts,
    HeaderName, HeaderValue, Method,
};
use tower_http::cors::{AllowHeaders, AllowMethods, AllowOrigin, CorsLayer};

/// Origin loopback pada port berapa pun — Vite dev server memakai port dinamis.
fn is_loopback_origin(origin: &HeaderValue, _parts: &Parts) -> bool {
    let Ok(text) = origin.to_str() else {
        return false;
    };
    let rest = text
        .strip_prefix("http://")
        .or_else(|| text.strip_prefix("https://"))
        .unwrap_or(text);
    let host = rest.split(['/', ':', '?']).next().unwrap_or("");
    matches!(host, "localhost" | "127.0.0.1" | "[::1]" | "::1")
}

/// Susun `CorsLayer` dari daftar origin yang dipisah koma.
///
/// Daftar kosong berarti mode pengembangan: seluruh origin loopback diizinkan.
/// Saat diisi, hanya origin itu yang diizinkan (mode production).
pub fn layer(allowed_origins: &str) -> CorsLayer {
    let explicit: Vec<HeaderValue> = allowed_origins
        .split(',')
        .map(str::trim)
        .filter(|origin| !origin.is_empty())
        .filter_map(|origin| HeaderValue::from_str(origin).ok())
        .collect();

    let allow_origin = if explicit.is_empty() {
        AllowOrigin::predicate(is_loopback_origin)
    } else {
        AllowOrigin::list(explicit)
    };

    CorsLayer::new()
        .allow_origin(allow_origin)
        .allow_credentials(true)
        .allow_headers(AllowHeaders::list([
            AUTHORIZATION,
            CONTENT_TYPE,
            ACCEPT,
            // Header wajib pada POST /chat/jobs dan POST /chat/jobs/{id}/responses.
            HeaderName::from_static("idempotency-key"),
            // Kursor reconnect SSE dikirim sebagai header, bukan query string.
            HeaderName::from_static("last-event-id"),
        ]))
        .allow_methods(AllowMethods::list([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ]))
}
