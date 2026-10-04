use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Only columns explicitly approved for the settings surface.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AuditEvent {
    pub id: Uuid,
    pub occurred_at: DateTime<Utc>,
    pub duration_ms: Option<i64>,
    pub stage: String,
    pub action: String,
    pub result: String,
    pub actor_kind: String,
    pub actor_user_id: Option<Uuid>,
    pub job_id: Option<Uuid>,
    pub session_id: Option<Uuid>,
    pub node_id: Option<String>,
    pub request_id: Option<Uuid>,
    pub model_call_id: Option<Uuid>,
    pub catalog_version_id: Option<Uuid>,
    pub catalog_content_hash: Option<String>,
    pub has_evidence: bool,
}

#[derive(Debug, Deserialize)]
pub struct AuditQuery {
    pub limit: Option<i64>,
    pub before_occurred_at: Option<DateTime<Utc>>,
    pub before_id: Option<Uuid>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub stage: Option<String>,
    pub result: Option<String>,
    pub job_id: Option<Uuid>,
}

#[derive(Debug, Serialize)]
pub struct AuditPage {
    pub events: Vec<AuditEvent>,
    pub next_before_occurred_at: Option<DateTime<Utc>>,
    pub next_before_id: Option<Uuid>,
    pub controlled_evidence: &'static str,
}

#[derive(Debug, Serialize)]
pub struct AuditDetail {
    pub event: AuditEvent,
    pub controlled_evidence: &'static str,
}

#[derive(Debug, sqlx::FromRow)]
pub struct Principal {
    pub user_id: Uuid,
    pub username: String,
    pub role: String,
    pub auth_session_id: Uuid,
    pub session_expires_at: DateTime<Utc>,
    pub entitlements_source: String,
    pub entitlements_json: serde_json::Value,
}

#[derive(Debug, sqlx::FromRow)]
pub struct CatalogVersion {
    pub id: Uuid,
    pub status: String,
    pub document_count: i32,
    pub embedding_model: Option<String>,
    pub embedding_dimensions: Option<i32>,
    pub embedding_input_type: Option<String>,
    pub synced_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct IndexSourceCount {
    pub source_type: String,
    pub count: i64,
}

#[derive(Debug, sqlx::FromRow)]
pub struct IndexCounts {
    pub total: i64,
    pub capabilities: i64,
    pub embedded: i64,
    pub missing: i64,
    pub embedded_capabilities: i64,
}

#[derive(Debug, sqlx::FromRow)]
pub struct PiiRow {
    pub key: String,
    pub value_json: serde_json::Value,
    pub version: i32,
}
