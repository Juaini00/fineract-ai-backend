use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReindexRequest {
    #[serde(default = "default_rebuild")]
    pub rebuild_embeddings: bool,
}

fn default_rebuild() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Run {
    pub id: Uuid,
    pub status: String,
    pub rebuild_embeddings: bool,
    pub requested_by_user_id: Option<Uuid>,
    pub catalog_version_id: Option<Uuid>,
    pub content_hash: String,
    pub running_content_hash: Option<String>,
    pub capability_count: i32,
    pub query_count: i32,
    pub dataset_definition_count: i32,
    pub resolver_shape_count: i32,
    pub lexical_row_count: i32,
    pub embedded_row_count: i32,
    pub processed_row_count: i32,
    pub finding_error_count: i32,
    pub finding_warning_count: i32,
    pub error_code: Option<String>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct RunView {
    #[serde(flatten)]
    pub run: Run,
    pub indexed_content_hash: Option<String>,
    pub indexed_catalog_version_id: Option<Uuid>,
    pub running_catalog_content_hash: String,
    pub running_catalog_version_id: Option<Uuid>,
    pub published_lexical_row_count: i64,
    pub published_embedded_row_count: i64,
    pub restart_required: bool,
}

#[derive(Debug, Clone)]
pub struct IndexDocument {
    pub source_type: &'static str,
    pub source_id: String,
    pub source_path: String,
    pub title: Option<String>,
    pub retrieval_text: String,
    pub metadata: serde_json::Value,
}
