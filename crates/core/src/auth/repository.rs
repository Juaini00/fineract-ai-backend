//! Satu-satunya tempat SQL autentikasi (`route → service → repository → database`).
//!
//! Query memakai API runtime `sqlx::query*`, bukan macro `query!`: macro
//! menuntut database hidup atau cache offline saat **kompilasi**, dan itu
//! menjadikan `cargo check` tergantung infrastruktur.

use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use uuid::Uuid;

/// Baris `users` yang dibutuhkan jalur auth.
#[derive(Debug, Clone, FromRow)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub email: Option<String>,
    pub full_name: Option<String>,
    pub role: String,
    pub is_active: bool,
    pub password_hash: String,
}

/// Refresh token beserta state session pemiliknya — cukup untuk memutuskan
/// rotasi tanpa query kedua.
#[derive(Debug, Clone, FromRow)]
pub struct RefreshRecord {
    pub id: Uuid,
    pub session_id: Uuid,
    pub user_id: Uuid,
    pub expires_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub session_revoked_at: Option<DateTime<Utc>>,
    pub session_expires_at: DateTime<Utc>,
}

pub async fn find_user_by_username(pool: &PgPool, username: &str) -> sqlx::Result<Option<User>> {
    sqlx::query_as::<_, User>(
        "SELECT id, username, email, full_name, role, is_active, password_hash
         FROM users WHERE username = $1",
    )
    .bind(username)
    .fetch_optional(pool)
    .await
}

pub async fn find_user_by_id(pool: &PgPool, user_id: Uuid) -> sqlx::Result<Option<User>> {
    sqlx::query_as::<_, User>(
        "SELECT id, username, email, full_name, role, is_active, password_hash
         FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

pub async fn count_users(pool: &PgPool) -> sqlx::Result<i64> {
    sqlx::query_scalar::<_, i64>("SELECT count(*) FROM users")
        .fetch_one(pool)
        .await
}

/// Sisipkan admin seed. `ON CONFLICT DO NOTHING` supaya dua proses yang start
/// bersamaan tidak saling menggagalkan.
pub async fn insert_admin(
    pool: &PgPool,
    username: &str,
    email: Option<&str>,
    password_hash: &str,
) -> sqlx::Result<Option<Uuid>> {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO users (username, email, password_hash, role)
         VALUES ($1, $2, $3, 'admin')
         ON CONFLICT (username) DO NOTHING
         RETURNING id",
    )
    .bind(username)
    .bind(email)
    .bind(password_hash)
    .fetch_optional(pool)
    .await
}

/// Buat auth session + refresh token pertamanya dalam satu transaksi: session
/// tanpa token yang dapat dipakai adalah baris yatim yang tidak pernah dibaca.
pub async fn create_session(
    pool: &PgPool,
    user_id: Uuid,
    user_agent: Option<&str>,
    // Dikirim sebagai teks lalu di-cast ke `inet` di SQL: sqlx tidak memetakan
    // `std::net::IpAddr` ke Postgres tanpa dependency tambahan, dan satu cast
    // lebih murah daripada satu crate.
    ip_address: Option<String>,
    token_hash: &str,
    session_expires_at: DateTime<Utc>,
    token_expires_at: DateTime<Utc>,
) -> sqlx::Result<Uuid> {
    let (_window, mut tx) = crate::commit_isolation::begin(pool).await?;

    let session_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO auth_sessions (user_id, user_agent, ip_address, expires_at, last_seen_at)
         VALUES ($1, $2, $3::inet, $4, now())
         RETURNING id",
    )
    .bind(user_id)
    .bind(user_agent)
    .bind(ip_address)
    .bind(session_expires_at)
    .fetch_one(&mut *tx)
    .await?;

    insert_refresh_token(&mut tx, session_id, user_id, token_hash, token_expires_at).await?;

    sqlx::query("UPDATE users SET last_login_at = now(), updated_at = now() WHERE id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(session_id)
}

pub async fn find_refresh_token(
    pool: &PgPool,
    token_hash: &str,
) -> sqlx::Result<Option<RefreshRecord>> {
    sqlx::query_as::<_, RefreshRecord>(
        "SELECT t.id, t.session_id, t.user_id, t.expires_at, t.revoked_at,
                s.revoked_at AS session_revoked_at, s.expires_at AS session_expires_at
         FROM refresh_tokens t
         JOIN auth_sessions s ON s.id = t.session_id
         WHERE t.token_hash = $1",
    )
    .bind(token_hash)
    .fetch_optional(pool)
    .await
}

/// Rotasi: cabut token lama dan terbitkan penggantinya secara atomik.
///
/// `UPDATE ... WHERE revoked_at IS NULL` adalah penegaknya — dua permintaan
/// refresh yang berlomba dengan token sama hanya menghasilkan satu pemenang,
/// dan yang kalah mendapat 0 baris sehingga ditolak, bukan diberi token kedua.
pub async fn rotate_refresh_token(
    pool: &PgPool,
    current_token_id: Uuid,
    session_id: Uuid,
    user_id: Uuid,
    new_token_hash: &str,
    new_expires_at: DateTime<Utc>,
) -> sqlx::Result<bool> {
    let (_window, mut tx) = crate::commit_isolation::begin(pool).await?;

    let revoked = sqlx::query("UPDATE refresh_tokens SET revoked_at = now() WHERE id = $1 AND revoked_at IS NULL")
        .bind(current_token_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();

    if revoked == 0 {
        tx.rollback().await?;
        return Ok(false);
    }

    insert_refresh_token(&mut tx, session_id, user_id, new_token_hash, new_expires_at).await?;

    sqlx::query("UPDATE auth_sessions SET last_seen_at = now() WHERE id = $1")
        .bind(session_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(true)
}

/// Cabut session beserta seluruh refresh token-nya (logout, atau respons
/// terhadap pemakaian ulang token yang sudah dicabut).
pub async fn revoke_session(pool: &PgPool, session_id: Uuid) -> sqlx::Result<()> {
    let (_window, mut tx) = crate::commit_isolation::begin(pool).await?;

    sqlx::query("UPDATE auth_sessions SET revoked_at = now() WHERE id = $1 AND revoked_at IS NULL")
        .bind(session_id)
        .execute(&mut *tx)
        .await?;

    sqlx::query(
        "UPDATE refresh_tokens SET revoked_at = now()
         WHERE session_id = $1 AND revoked_at IS NULL",
    )
    .bind(session_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await
}

async fn insert_refresh_token(
    tx: &mut Transaction<'_, Postgres>,
    session_id: Uuid,
    user_id: Uuid,
    token_hash: &str,
    expires_at: DateTime<Utc>,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO refresh_tokens (session_id, user_id, token_hash, expires_at)
         VALUES ($1, $2, $3, $4)",
    )
    .bind(session_id)
    .bind(user_id)
    .bind(token_hash)
    .bind(expires_at)
    .execute(&mut **tx)
    .await?;

    Ok(())
}
