//! Handler HTTP autentikasi: accept/read/delegate saja (overview §3).
//!
//! Refresh token dikirim sebagai cookie `HttpOnly`, bukan di body: token yang
//! terbaca JavaScript hanya berjarak satu XSS dari sesi yang diambil alih.

use axum::{
    Json, Router,
    extract::{ConnectInfo, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use cookie::{Cookie, SameSite, time::Duration as CookieDuration};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use validator::Validate;

use crate::{
    auth::{
        extractor::AuthUser,
        service::{self, Authenticated, Profile},
    },
    config::Config,
    envelope::Envelope,
    error::ApiError,
    state::Foundation,
};

pub fn router() -> Router<Foundation> {
    Router::new()
        .route("/auth/login", post(login))
        .route("/auth/refresh", post(refresh))
        .route("/auth/logout", post(logout))
        .route("/auth/me", get(me))
}

#[derive(Debug, Deserialize, Validate)]
pub struct LoginRequest {
    #[validate(length(min = 1, max = 255))]
    username: String,
    // Batas atas ada supaya input raksasa tidak menjadi beban Argon2 gratis.
    #[validate(length(min = 1, max = 1024))]
    password: String,
}

#[derive(Debug, Serialize)]
pub struct SessionResponse {
    access_token: String,
    token_type: &'static str,
    expires_in: i64,
    user: Profile,
}

async fn login(
    State(foundation): State<Foundation>,
    ConnectInfo(client): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(request): Json<LoginRequest>,
) -> Result<Response, ApiError> {
    request
        .validate()
        .map_err(|error| ApiError::Unprocessable(error.to_string()))?;

    let authenticated = service::login(
        &foundation,
        &request.username,
        &request.password,
        header_str(&headers, header::USER_AGENT),
        Some(client.ip()),
    )
    .await?;

    Ok(session_response(foundation.config(), authenticated))
}

async fn refresh(
    State(foundation): State<Foundation>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let presented = refresh_cookie(&headers, &foundation.config().auth_refresh_cookie_name)
        .ok_or(ApiError::Unauthorized)?;

    let authenticated = service::refresh(&foundation, &presented).await?;
    Ok(session_response(foundation.config(), authenticated))
}

async fn logout(
    State(foundation): State<Foundation>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let config = foundation.config();

    if let Some(presented) = refresh_cookie(&headers, &config.auth_refresh_cookie_name) {
        service::logout(&foundation, &presented).await?;
    }

    // Cookie tetap dihapus walau tokennya tidak dikenali: klien yang memanggil
    // logout harus selalu berakhir tanpa kredensial di browser.
    let mut response = Json(Envelope::ok(())).into_response();
    set_cookie(&mut response, clearing_cookie(config));
    Ok(response)
}

async fn me(
    State(foundation): State<Foundation>,
    user: AuthUser,
) -> Result<Json<Envelope<Profile>>, ApiError> {
    let profile = service::profile(&foundation, user.user_id).await?;
    Ok(Json(Envelope::ok(profile)))
}

fn session_response(config: &Config, authenticated: Authenticated) -> Response {
    let body = SessionResponse {
        access_token: authenticated.access_token,
        token_type: "Bearer",
        expires_in: authenticated.expires_in_secs,
        user: authenticated.user,
    };

    let mut response = (StatusCode::OK, Json(Envelope::ok(body))).into_response();
    set_cookie(
        &mut response,
        refresh_cookie_for(config, authenticated.refresh_token.expose()),
    );
    response
}

fn refresh_cookie_for(config: &Config, value: &str) -> Cookie<'static> {
    build_cookie(
        config,
        value.to_string(),
        CookieDuration::seconds(config.jwt_refresh_token_expiry_seconds as i64),
    )
}

fn clearing_cookie(config: &Config) -> Cookie<'static> {
    build_cookie(config, String::new(), CookieDuration::ZERO)
}

fn build_cookie(config: &Config, value: String, max_age: CookieDuration) -> Cookie<'static> {
    Cookie::build((config.auth_refresh_cookie_name.clone(), value))
        .path(config.auth_refresh_cookie_path.clone())
        .http_only(true)
        .secure(config.auth_refresh_cookie_secure)
        .same_site(same_site(&config.auth_refresh_cookie_same_site))
        .max_age(max_age)
        .build()
}

/// Nilai tidak dikenal jatuh ke `Strict` — sisi paling ketat, bukan paling
/// permisif.
fn same_site(configured: &str) -> SameSite {
    match configured.to_ascii_lowercase().as_str() {
        "lax" => SameSite::Lax,
        "none" => SameSite::None,
        _ => SameSite::Strict,
    }
}

fn set_cookie(response: &mut Response, cookie: Cookie<'_>) {
    if let Ok(value) = cookie.to_string().parse() {
        response.headers_mut().append(header::SET_COOKIE, value);
    }
}

fn refresh_cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(Cookie::split_parse)
        .filter_map(Result::ok)
        .find(|cookie| cookie.name() == name)
        .map(|cookie| cookie.value().to_string())
}

fn header_str(headers: &HeaderMap, name: header::HeaderName) -> Option<&str> {
    headers.get(name).and_then(|value| value.to_str().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_named_cookie_among_others() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            "theme=dark; refresh_token=abc123; other=1".parse().unwrap(),
        );

        assert_eq!(refresh_cookie(&headers, "refresh_token").as_deref(), Some("abc123"));
        assert_eq!(refresh_cookie(&headers, "missing"), None);
    }

    #[test]
    fn unknown_same_site_falls_back_to_strict() {
        assert_eq!(same_site("lax"), SameSite::Lax);
        assert_eq!(same_site("LAX"), SameSite::Lax);
        assert_eq!(same_site("bukan-nilai"), SameSite::Strict);
    }
}
