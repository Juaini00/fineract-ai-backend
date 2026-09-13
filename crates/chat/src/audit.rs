//! Penulisan `audit_events`.
//!
//! Audit ditulis **di dalam** transaksi yang sama dengan transisi state (I1:
//! tulisan lokal boleh, panggilan eksternal tidak). Ia append-only dan tidak
//! punya FK (I8) — `job_id` menggantung setelah purge adalah normal.

use serde_json::Value;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

/// Satu baris audit. Field yang tidak relevan pada sebuah tahap dibiarkan
/// `None`; tidak ada nilai pengganti yang dikarang.
#[derive(Debug, Default)]
pub struct AuditEvent<'a> {
    pub actor_kind: &'a str,
    pub actor_user_id: Option<Uuid>,
    pub session_id: Option<Uuid>,
    pub job_id: Option<Uuid>,
    pub stage: &'a str,
    pub action: &'a str,
    pub result: &'a str,
    pub failure_code: Option<&'a str>,
    /// Scope TEREDAKSI: metadata, bukan nilai filter.
    pub scope_json: Option<Value>,
    /// Keputusan terstruktur tersanitasi. SQL/prompt/stack tidak pernah masuk.
    pub detail_json: Option<Value>,
}

pub async fn insert(
    tx: &mut Transaction<'_, Postgres>,
    event: AuditEvent<'_>,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO audit_events
            (actor_kind, actor_user_id, session_id, job_id, stage, action, result,
             failure_code, scope_json, detail_json)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8,
                 COALESCE($9, '{}'::jsonb), COALESCE($10, '{}'::jsonb))",
    )
    .bind(event.actor_kind)
    .bind(event.actor_user_id)
    .bind(event.session_id)
    .bind(event.job_id)
    .bind(event.stage)
    .bind(event.action)
    .bind(event.result)
    .bind(event.failure_code)
    .bind(event.scope_json)
    .bind(event.detail_json)
    .execute(&mut **tx)
    .await?;

    Ok(())
}
