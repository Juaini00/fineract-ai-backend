//! Aturan autentikasi. Tidak ada `sqlx` di sini — hanya kebijakan.

use std::net::IpAddr;

use chrono::{Duration, Utc};
use tracing::{info, warn};
use uuid::Uuid;

use crate::{
    auth::{
        password, repository,
        token::{RefreshToken, TokenIssuer},
    },
    error::ApiError,
    state::Foundation,
};

/// Hasil login/refresh. Refresh token dikembalikan terpisah karena ia dikirim
/// lewat cookie HttpOnly, bukan body.
#[derive(Debug)]
pub struct Authenticated {
    pub access_token: String,
    pub expires_in_secs: i64,
    pub refresh_token: RefreshToken,
    pub user: Profile,
}

/// Identitas publik pengguna. `password_hash` tidak pernah ikut.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Profile {
    pub id: Uuid,
    pub username: String,
    pub email: Option<String>,
    pub full_name: Option<String>,
    pub role: String,
}

impl From<repository::User> for Profile {
    fn from(user: repository::User) -> Self {
        Self {
            id: user.id,
            username: user.username,
            email: user.email,
            full_name: user.full_name,
            role: user.role,
        }
    }
}

/// Pesan tunggal untuk setiap kegagalan autentikasi.
///
/// Membedakan "user tidak ada", "password salah", dan "akun nonaktif" pada
/// response berarti menyediakan oracle enumerasi akun secara gratis. Sebabnya
/// tetap terlihat — di log, bukan di response.
fn rejected() -> ApiError {
    ApiError::Unauthorized
}

pub async fn login(
    foundation: &Foundation,
    username: &str,
    password_input: &str,
    user_agent: Option<&str>,
    ip_address: Option<IpAddr>,
) -> Result<Authenticated, ApiError> {
    let pool = foundation.app_db().pool();

    let user = repository::find_user_by_username(pool, username)
        .await
        .map_err(anyhow::Error::from)?;

    let Some(user) = user else {
        // Tetap bayar biaya verifikasi: tanpa ini, waktu respons membedakan
        // "username ada" dari "tidak ada" tanpa perlu menebak password.
        verify_password(password_input.to_string(), DUMMY_HASH.to_string()).await?;
        warn!(username, "login ditolak: user tidak ditemukan");
        return Err(rejected());
    };

    let password_matches = verify_password(password_input.to_string(), user.password_hash.clone()).await?;
    if !password_matches {
        warn!(user_id = %user.id, "login ditolak: password salah");
        return Err(rejected());
    }

    if !user.is_active {
        warn!(user_id = %user.id, "login ditolak: akun nonaktif");
        return Err(rejected());
    }

    issue_session(foundation, user, user_agent, ip_address).await
}

/// Tukarkan refresh token dengan pasangan token baru, sekaligus merotasi.
pub async fn refresh(
    foundation: &Foundation,
    raw_refresh_token: &str,
) -> Result<Authenticated, ApiError> {
    let pool = foundation.app_db().pool();
    let presented = RefreshToken::from_raw(raw_refresh_token);

    let record = repository::find_refresh_token(pool, &presented.hash())
        .await
        .map_err(anyhow::Error::from)?
        .ok_or_else(rejected)?;

    let now = Utc::now();

    if record.revoked_at.is_some() {
        // Token yang sudah dirotasi dipakai lagi: entah dicuri, entah klien
        // menyimpan token lama. Keduanya ditangani sama — cabut seluruh
        // session, karena penyerang dan pengguna sah kini memegang rantai yang
        // sama dan tidak ada cara membedakannya.
        warn!(session_id = %record.session_id, "refresh token dipakai ulang; session dicabut");
        repository::revoke_session(pool, record.session_id)
            .await
            .map_err(anyhow::Error::from)?;
        return Err(rejected());
    }

    if record.expires_at <= now
        || record.session_revoked_at.is_some()
        || record.session_expires_at <= now
    {
        return Err(rejected());
    }

    let user = repository::find_user_by_id(pool, record.user_id)
        .await
        .map_err(anyhow::Error::from)?
        .ok_or_else(rejected)?;

    if !user.is_active {
        repository::revoke_session(pool, record.session_id)
            .await
            .map_err(anyhow::Error::from)?;
        return Err(rejected());
    }

    let next = RefreshToken::generate();
    let rotated = repository::rotate_refresh_token(
        pool,
        record.id,
        record.session_id,
        record.user_id,
        &next.hash(),
        now + refresh_ttl(foundation),
    )
    .await
    .map_err(anyhow::Error::from)?;

    if !rotated {
        // Permintaan refresh paralel dengan token yang sama: hanya satu boleh
        // menang (lihat repository::rotate_refresh_token).
        return Err(rejected());
    }

    let issued = TokenIssuer::new(foundation.config())
        .issue(user.id, record.session_id, &user.role)
        .map_err(ApiError::Internal)?;

    Ok(Authenticated {
        access_token: issued.token,
        expires_in_secs: issued.expires_in_secs,
        refresh_token: next,
        user: user.into(),
    })
}

/// Logout: cabut session milik refresh token yang dikirim.
///
/// Tidak mengungkap apakah token valid — logout selalu tampak berhasil.
pub async fn logout(foundation: &Foundation, raw_refresh_token: &str) -> Result<(), ApiError> {
    let pool = foundation.app_db().pool();
    let presented = RefreshToken::from_raw(raw_refresh_token);

    if let Some(record) = repository::find_refresh_token(pool, &presented.hash())
        .await
        .map_err(anyhow::Error::from)?
    {
        repository::revoke_session(pool, record.session_id)
            .await
            .map_err(anyhow::Error::from)?;
    }

    Ok(())
}

pub async fn profile(foundation: &Foundation, user_id: Uuid) -> Result<Profile, ApiError> {
    let user = repository::find_user_by_id(foundation.app_db().pool(), user_id)
        .await
        .map_err(anyhow::Error::from)?
        .ok_or(ApiError::Unauthorized)?;

    if !user.is_active {
        return Err(ApiError::Unauthorized);
    }

    Ok(user.into())
}

/// Seed admin pertama saat tabel `users` kosong. Hanya dipanggil bila
/// [`crate::Config::may_bootstrap_admin`] mengizinkan (local).
pub async fn bootstrap_admin(foundation: &Foundation) -> anyhow::Result<()> {
    let config = foundation.config();
    let pool = foundation.app_db().pool();

    if repository::count_users(pool).await? > 0 {
        return Ok(());
    }

    let Some(raw_password) = config.auth_bootstrap_admin_password.as_deref() else {
        anyhow::bail!("AUTH_BOOTSTRAP_ADMIN_ENABLED=true tetapi AUTH_BOOTSTRAP_ADMIN_PASSWORD kosong");
    };

    let hash = password::hash(raw_password)?;
    let created = repository::insert_admin(
        pool,
        &config.auth_bootstrap_admin_username,
        config.auth_bootstrap_admin_email.as_deref(),
        &hash,
    )
    .await?;

    match created {
        Some(user_id) => info!(%user_id, username = %config.auth_bootstrap_admin_username, "admin bootstrap dibuat"),
        None => info!("admin bootstrap dilewati: username sudah dipakai"),
    }

    Ok(())
}

async fn issue_session(
    foundation: &Foundation,
    user: repository::User,
    user_agent: Option<&str>,
    ip_address: Option<IpAddr>,
) -> Result<Authenticated, ApiError> {
    let now = Utc::now();
    let refresh_token = RefreshToken::generate();
    let expires_at = now + refresh_ttl(foundation);

    let session_id = repository::create_session(
        foundation.app_db().pool(),
        user.id,
        user_agent,
        ip_address.map(|address| address.to_string()),
        &refresh_token.hash(),
        // Umur session mengikuti umur refresh token: session yang hidup lebih
        // lama daripada tokennya tidak dapat dipakai, dan yang lebih pendek
        // membuat token sah ditolak tanpa sebab yang terlihat.
        expires_at,
        expires_at,
    )
    .await
    .map_err(anyhow::Error::from)?;

    let issued = TokenIssuer::new(foundation.config())
        .issue(user.id, session_id, &user.role)
        .map_err(ApiError::Internal)?;

    info!(user_id = %user.id, %session_id, "login berhasil");

    Ok(Authenticated {
        access_token: issued.token,
        expires_in_secs: issued.expires_in_secs,
        refresh_token,
        user: user.into(),
    })
}

fn refresh_ttl(foundation: &Foundation) -> Duration {
    Duration::seconds(foundation.config().jwt_refresh_token_expiry_seconds as i64)
}

/// Argon2 sengaja mahal (puluhan milidetik). Menjalankannya langsung di worker
/// async menahan seluruh task lain pada thread itu.
async fn verify_password(input: String, stored_hash: String) -> Result<bool, ApiError> {
    tokio::task::spawn_blocking(move || password::verify(&input, &stored_hash))
        .await
        .map_err(|error| ApiError::Internal(anyhow::anyhow!("verifikasi password panik: {error}")))
}

/// Hash Argon2id dari string acak. Dipakai hanya untuk menyamakan waktu
/// respons saat username tidak ada; tidak ada password yang cocok dengannya.
const DUMMY_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$YnVrYW5wYXNzd29yZA$Zp0k6hFbYBoVn4TQx4HCnPgkm6cAaDfgtLLPMVFCxTU";
