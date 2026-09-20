//! SQL untuk `chat_sessions`.

use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, Clone, FromRow, serde::Serialize)]
pub struct Session {
    pub id: Uuid,
    pub owner_user_id: Uuid,
    pub title: Option<String>,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub async fn insert(pool: &PgPool, owner_user_id: Uuid, title: Option<&str>) -> sqlx::Result<Session> {
    sqlx::query_as::<_, Session>("INSERT INTO chat_sessions (owner_user_id, title) VALUES ($1, $2) RETURNING id, owner_user_id, title, status, created_at, updated_at"
    )
    .bind(owner_user_id)
    .bind(title)
    .fetch_one(pool)
    .await
}

/// Keyset pagination: `updated_at DESC, id DESC`. Offset akan melewatkan atau
/// menggandakan baris ketika daftar berubah di antara dua halaman.
pub async fn list_for_owner(
    pool: &PgPool,
    owner_user_id: Uuid,
    before: Option<(DateTime<Utc>, Uuid)>,
    limit: i64,
) -> sqlx::Result<Vec<Session>> {
    match before {
        Some((updated_at, id)) => {
            sqlx::query_as::<_, Session>(
                "SELECT id, owner_user_id, title, status, created_at, updated_at FROM chat_sessions
                 WHERE owner_user_id = $1 AND (updated_at, id) < ($2, $3)
                 ORDER BY updated_at DESC, id DESC LIMIT $4",
            )
            .bind(owner_user_id)
            .bind(updated_at)
            .bind(id)
            .bind(limit)
            .fetch_all(pool)
            .await
        }
        None => {
            sqlx::query_as::<_, Session>(
                "SELECT id, owner_user_id, title, status, created_at, updated_at FROM chat_sessions
                 WHERE owner_user_id = $1
                 ORDER BY updated_at DESC, id DESC LIMIT $2",
            )
            .bind(owner_user_id)
            .bind(limit)
            .fetch_all(pool)
            .await
        }
    }
}

/// Satu turn riwayat. Indeksnya tipis (§3 database-design): konten tidak
/// disalin ke sini. `request_text` dibaca kembali dari `chat_jobs` — rumah
/// aslinya, immutable — dan hanya untuk baris `user`; pada baris `assistant`
/// teks itu adalah pertanyaan yang sama, bukan jawabannya. Isi jawaban diambil
/// frontend lewat `GET /chat/jobs/{job_id}/response`.
#[derive(Debug, Clone, FromRow, serde::Serialize)]
pub struct Message {
    pub id: Uuid,
    pub job_id: Uuid,
    pub role: String,
    pub request_text: Option<String>,
    pub response_version: Option<i32>,
    pub clarification_id: Option<Uuid>,
    pub clarification_revision: Option<i32>,
    pub created_at: DateTime<Utc>,
}

/// Keyset `created_at DESC, id DESC` — arah yang sama dengan daftar session,
/// sehingga frontend memakai satu pola cursor untuk keduanya.
pub async fn list_messages(
    pool: &PgPool,
    session_id: Uuid,
    before: Option<(DateTime<Utc>, Uuid)>,
    limit: i64,
) -> sqlx::Result<Vec<Message>> {
    match before {
        Some((created_at, id)) => {
            sqlx::query_as::<_, Message>(
                "SELECT m.id, m.job_id, m.role,
                        CASE WHEN m.role = 'user' THEN j.request_text END AS request_text,
                        m.response_version, m.clarification_id, m.clarification_revision, m.created_at
                 FROM chat_messages m JOIN chat_jobs j ON j.id = m.job_id
                 WHERE m.session_id = $1 AND (m.created_at, m.id) < ($2, $3)
                 ORDER BY m.created_at DESC, m.id DESC LIMIT $4",
            )
            .bind(session_id)
            .bind(created_at)
            .bind(id)
            .bind(limit)
            .fetch_all(pool)
            .await
        }
        None => {
            sqlx::query_as::<_, Message>(
                "SELECT m.id, m.job_id, m.role,
                        CASE WHEN m.role = 'user' THEN j.request_text END AS request_text,
                        m.response_version, m.clarification_id, m.clarification_revision, m.created_at
                 FROM chat_messages m JOIN chat_jobs j ON j.id = m.job_id
                 WHERE m.session_id = $1
                 ORDER BY m.created_at DESC, m.id DESC LIMIT $2",
            )
            .bind(session_id)
            .bind(limit)
            .fetch_all(pool)
            .await
        }
    }
}

pub async fn find(pool: &PgPool, session_id: Uuid) -> sqlx::Result<Option<Session>> {
    sqlx::query_as::<_, Session>("SELECT id, owner_user_id, title, status, created_at, updated_at FROM chat_sessions WHERE id = $1"
    )
    .bind(session_id)
    .fetch_optional(pool)
    .await
}

/// Versi transaksional: dipakai T1 supaya kepemilikan dan status session
/// dibaca pada snapshot yang sama dengan penulisan job.
pub async fn find_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    session_id: Uuid,
) -> sqlx::Result<Option<Session>> {
    sqlx::query_as::<_, Session>("SELECT id, owner_user_id, title, status, created_at, updated_at FROM chat_sessions WHERE id = $1"
    )
    .bind(session_id)
    .fetch_optional(&mut **tx)
    .await
}

pub async fn touch(tx: &mut Transaction<'_, Postgres>, session_id: Uuid) -> sqlx::Result<()> {
    sqlx::query("UPDATE chat_sessions SET updated_at = now() WHERE id = $1")
        .bind(session_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}
