//! Current settings projections; all database reads live in `repository`.

use std::collections::BTreeMap;

use chrono::Utc;
use foundation::{auth::AuthUser, error::ApiError, state::Foundation};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{catalog::Catalog, engine::dataset};

use super::{
    model::{AuditDetail, AuditPage, AuditQuery, Principal},
    repository,
};

fn db_error(error: sqlx::Error) -> ApiError {
    ApiError::Internal(error.into())
}

/// Revalidate both the user and the exact auth session for privileged requests.
pub async fn require_admin(foundation: &Foundation, user: &AuthUser) -> Result<(), ApiError> {
    principal(foundation, user).await.map(|_| ())
}

async fn principal(foundation: &Foundation, user: &AuthUser) -> Result<Principal, ApiError> {
    repository::current_principal(
        foundation.app_db().pool(),
        user.user_id,
        user.auth_session_id,
    )
    .await
    .map_err(db_error)?
    .ok_or(ApiError::Forbidden)
}

pub async fn models(
    foundation: &Foundation,
    provider: Option<&str>,
    model: Option<&str>,
) -> Result<Value, ApiError> {
    let c = foundation.config();
    Ok(json!({
        "generation": { "provider": provider, "model": model, "configuration_source": "startup_environment", "operational": false, "implementation": "not_integrated", "connection_checked": false },
        "planning": { "implementation": "not_integrated" },
        "narration": { "implementation": "not_integrated" },
        "summarization": { "implementation": "not_integrated" },
        "embedding": { "model": c.embedding_model, "dimensions": c.embedding_dimensions,
            "document_input_type": c.embedding_input_type_document, "query_input_type": c.embedding_input_type_query,
            "credential_configured": c.embedding_api_key.as_ref().is_some_and(|key| !key.trim().is_empty()),
            "timeout_ms": c.embedding_timeout_ms, "similarity_cutoff": c.embedding_similarity_cutoff,
            "implementation": "integrated", "connection_checked": false }
    }))
}

pub async fn connections(foundation: &Foundation) -> Result<Value, ApiError> {
    let checked_at = Utc::now();
    let app = foundation.app_db().ping().await.is_ok();
    let fineract = foundation.fineract_db().ping().await.is_ok();
    let redis_current = foundation.notifier().ping().await;
    let vector = if app {
        repository::vector_info(foundation.app_db().pool())
            .await
            .ok()
    } else {
        None
    };
    Ok(json!({
        "checked_at": checked_at,
        "app_postgresql": { "status": if app { "ok" } else { "unavailable" } },
        "fineract_postgresql": { "status": if fineract { "ok" } else { "unavailable" }, "session_default_read_only_configured": true },
        "redis_live_coordination": { "startup_status": foundation.notifier().status(), "current_check": redis_current },
        "pgvector": { "status": if vector.is_some() { "checked" } else { "unknown" },
            "installed": vector.map(|v| v.0), "column_dimensions": vector.and_then(|v| v.1) }
    }))
}

pub async fn catalog(foundation: &Foundation, catalog: &Catalog) -> Result<Value, ApiError> {
    let pool = foundation.app_db().pool();
    let version = repository::catalog_version(pool, &catalog.content_hash)
        .await
        .map_err(db_error)?;
    let latest = repository::latest_catalog_hash(pool)
        .await
        .map_err(db_error)?;
    let (index, sources) = if let Some(v) = &version {
        (
            Some(
                repository::index_counts(pool, v.id)
                    .await
                    .map_err(db_error)?,
            ),
            repository::index_source_counts(pool, v.id)
                .await
                .map_err(db_error)?,
        )
    } else {
        (None, Vec::new())
    };
    let c = foundation.config();
    let compatible = version.as_ref().is_some_and(|v| {
        v.embedding_model.as_deref() == Some(c.embedding_model.as_str())
            && v.embedding_dimensions == i32::try_from(c.embedding_dimensions).ok()
            && v.embedding_input_type.as_deref() == Some(c.embedding_input_type_document.as_str())
    });
    let lexical = index.as_ref().is_some_and(|i| i.capabilities > 0);
    let credential_configured = c
        .embedding_api_key
        .as_ref()
        .is_some_and(|key| !key.trim().is_empty());
    let semantic = credential_configured
        && compatible
        && index.as_ref().is_some_and(|i| i.embedded_capabilities > 0);
    let mut capability_statuses = BTreeMap::<String, usize>::new();
    for item in &catalog.capabilities {
        *capability_statuses
            .entry(
                item.entry
                    .status
                    .clone()
                    .unwrap_or_else(|| "unspecified".into()),
            )
            .or_default() += 1;
    }
    let mut inventory_statuses = BTreeMap::<String, usize>::new();
    for status in catalog.inventory.values() {
        *inventory_statuses.entry(status.clone()).or_default() += 1;
    }
    Ok(json!({
        "running_content_hash": catalog.content_hash,
        "catalog_version_id": version.as_ref().map(|v| v.id),
        "catalog_status": version.as_ref().map(|v| v.status.as_str()),
        "catalog_document_count": version.as_ref().map(|v| v.document_count),
        "last_synced_at": version.as_ref().and_then(|v| v.synced_at),
        "latest_indexed_content_hash": latest,
        "restart_required_for_latest": latest.as_ref().is_some_and(|h| h != &catalog.content_hash),
        "definitions": { "capabilities": catalog.capabilities.len(), "queries": catalog.queries.len(),
            "dataset_definitions": catalog.datasets.len(), "domains": catalog.domains.len(),
            "data_scope_areas": catalog.areas.len(), "sql_files": catalog.sql_files.len() },
        "capability_statuses": capability_statuses,
        "inventory_statuses": inventory_statuses,
        "index": { "total": index.as_ref().map(|i| i.total).unwrap_or(0),
            "capabilities": index.as_ref().map(|i| i.capabilities).unwrap_or(0),
            "embedded": index.as_ref().map(|i| i.embedded).unwrap_or(0),
            "embedded_capabilities": index.as_ref().map(|i| i.embedded_capabilities).unwrap_or(0),
            "missing_embeddings": index.as_ref().map(|i| i.missing).unwrap_or(0),
            "by_source": sources,
            "dataset_definitions_vectorized": false,
            "retained_analytical_datasets_are_separate": true },
        "retrieval": { "lexical_available": lexical, "semantic_available": semantic,
            "embedding_credential_configured": credential_configured,
            "embedding_provider_connection_checked": false,
            "embedding_metadata_compatible": compatible,
            "indexed_model": version.as_ref().and_then(|v| v.embedding_model.as_deref()),
            "indexed_dimensions": version.as_ref().and_then(|v| v.embedding_dimensions),
            "indexed_document_input_type": version.as_ref().and_then(|v| v.embedding_input_type.as_deref()) },
        "coverage_proves_business_correctness": false
    }))
}

pub async fn access(foundation: &Foundation, user: &AuthUser) -> Result<Value, ApiError> {
    let p = principal(foundation, user).await?;
    let rows = repository::current_pii(foundation.app_db().pool())
        .await
        .map_err(db_error)?;
    let mut enabled = false;
    let mut mode = "withhold";
    let mut setting_version = 0;
    for row in &rows {
        match row.key.as_str() {
            "pii.enabled" => {
                enabled = row.value_json.as_bool().unwrap_or(false);
                setting_version = row.version;
            }
            "pii.mode" if row.value_json.as_str() == Some("withhold") => {
                mode = "withhold";
            }
            "pii.mode" => {
                enabled = false;
            }
            _ => {}
        }
    }
    let offices = if p.entitlements_source == "admin_projection" {
        match repository::office_ids(foundation.fineract_db()).await {
            Ok(offices) => Some(offices),
            Err(error) => {
                tracing::warn!(error = %error, "settings office entitlement probe unavailable");
                None
            }
        }
    } else {
        None
    };
    let office_ids_status = if offices.is_some() {
        "current"
    } else {
        "unavailable"
    };
    Ok(json!({
        "principal": { "user_id": p.user_id, "username": p.username, "role": p.role,
            "auth_session_id": p.auth_session_id, "session_expires_at": p.session_expires_at },
        "entitlements": { "source": p.entitlements_source, "office_ids": offices,
            "office_ids_status": office_ids_status,
            "tenant": foundation.config().fineract_tenant,
            "session_snapshot_present": !p.entitlements_json.is_null() },
        "pii": { "enabled": enabled, "mode": mode, "setting_version": setting_version,
            "supported_mode": "withhold", "scope": "global" },
        "materialized_result_authorization": "owner_bound"
    }))
}

pub fn runtime(foundation: &Foundation) -> Value {
    let c = foundation.config();
    json!({
        "app_version": env!("CARGO_PKG_VERSION"),
        "environment": format!("{:?}", c.app_env).to_lowercase(),
        "build_sha": option_env!("BUILD_SHA"),
        "http_envelope": { "fields": ["success", "data", "error"], "version": null },
        "response_block_schema_version": crate::engine::compose::BLOCK_SCHEMA_VERSION,
        "configured": {
            "app_database_max_connections": c.app_database_max_connections,
            "fineract_database_max_connections": c.fineract_database_max_connections,
            "worker_enabled": c.worker_enabled,
            "worker_lease_duration_secs": c.worker_lease_duration_secs,
            "worker_lease_heartbeat_interval_secs": c.worker_lease_heartbeat_interval_secs,
            "worker_poll_interval_ms": c.worker_poll_interval_ms,
            "reaper_interval_secs": c.reaper_interval_secs,
            "job_ttl_running_secs": c.job_ttl_running_secs,
            "node_attempt_cap_uncertain": c.node_attempt_cap,
            "clarification_wait_limit_secs": c.clarification_wait_limit_secs,
            "resolver_page_size": c.resolver_page_size,
            "resolver_page_size_max": c.resolver_page_size_max,
            "resolver_max_candidates": c.resolver_max_candidates,
            "sse_fallback_poll_interval_secs": c.sse_fallback_poll_interval_secs,
            "sse_fallback_poll_max_interval_secs": c.sse_fallback_poll_max_interval_secs,
            "sse_outgoing_buffer_frames": c.sse_outgoing_buffer_frames,
            "sse_transport_comment_interval_secs": c.sse_transport_comment_interval_secs,
            "sse_notifications_enabled": c.sse_notifications_enabled,
            "idempotency_ttl_secs": c.idempotency_ttl_secs,
            "idempotency_key_min_length": c.idempotency_key_min_length,
            "idempotency_key_max_length": c.idempotency_key_max_length,
            "dataset_max_rows": c.local_dataset_max_rows.unwrap_or(dataset::DATASET_MAX_ROWS),
            "dataset_max_bytes": dataset::DATASET_MAX_BYTES,
            "dataset_chunk_max_bytes": dataset::CHUNK_MAX_BYTES,
            "dataset_ttl_secs": dataset::ttl_secs(c.clarification_wait_limit_secs, c.job_ttl_running_secs),
            "response_inline_max_rows": crate::engine::compose::INLINE_MAX_ROWS,
            "response_inline_max_bytes": crate::engine::compose::INLINE_MAX_BYTES,
            "jwt_access_token_expiry_seconds": c.jwt_access_token_expiry_seconds,
            "jwt_refresh_token_expiry_seconds": c.jwt_refresh_token_expiry_seconds
        },
        "unimplemented_configuration": ["generation_llm_budget", "generation_llm_retry", "response_retention", "model_call_concurrency"]
    })
}

pub fn observability() -> Value {
    json!({ "sinks": [
        { "kind": "operational_logs", "destination": "process_output", "status": "implemented" },
        { "kind": "audit_events", "destination": "application_postgresql", "status": "implemented" }
    ], "traces_exporter": "not_implemented", "metrics_exporter": "not_implemented" })
}

pub async fn audit_page(foundation: &Foundation, query: AuditQuery) -> Result<AuditPage, ApiError> {
    let before = match (query.before_occurred_at, query.before_id) {
        (Some(at), Some(id)) => Some((at, id)),
        (None, None) => None,
        _ => {
            return Err(ApiError::Unprocessable(
                "before_occurred_at and before_id must be supplied together".into(),
            ));
        }
    };
    if query.from.zip(query.to).is_some_and(|(from, to)| from > to) {
        return Err(ApiError::Unprocessable(
            "from must be at or before to".into(),
        ));
    }
    if query.stage.as_deref().is_some_and(|s| {
        !matches!(
            s,
            "accept"
                | "authorize"
                | "context"
                | "plan"
                | "plan_verify"
                | "clarify"
                | "node_execute"
                | "source_query"
                | "model_call"
                | "compose"
                | "response_validate"
                | "commit"
                | "settle"
                | "data_access"
                | "admin"
        )
    }) {
        return Err(ApiError::Unprocessable("invalid stage".into()));
    }
    if query
        .result
        .as_deref()
        .is_some_and(|s| !matches!(s, "ok" | "denied" | "invalid" | "failed" | "deferred"))
    {
        return Err(ApiError::Unprocessable("invalid result".into()));
    }
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let events = repository::audit_page(
        foundation.app_db().pool(),
        limit,
        before,
        query.from,
        query.to,
        query.stage.as_deref(),
        query.result.as_deref(),
        query.job_id,
    )
    .await
    .map_err(db_error)?;
    let last = events.last();
    Ok(AuditPage {
        next_before_occurred_at: last.map(|e| e.occurred_at),
        next_before_id: last.map(|e| e.id),
        events,
        controlled_evidence: "unavailable_on_settings_api",
    })
}

pub async fn audit_event(foundation: &Foundation, id: Uuid) -> Result<AuditDetail, ApiError> {
    let event = repository::audit_event(foundation.app_db().pool(), id)
        .await
        .map_err(db_error)?
        .ok_or(ApiError::NotFound)?;
    Ok(AuditDetail {
        event,
        controlled_evidence: "unavailable_on_settings_api",
    })
}
