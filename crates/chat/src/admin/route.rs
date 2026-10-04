//! Read-only settings HTTP surface. All responses use the common envelope.

use std::sync::Arc;

use axum::{
    Extension, Json, Router,
    extract::{Path, Query, State, rejection::QueryRejection},
    routing::get,
};
use foundation::{AuthUser, Envelope, error::ApiError, state::Foundation};
use serde_json::Value;
use uuid::Uuid;

use super::{
    model::{AuditDetail, AuditPage, AuditQuery},
    service,
};
use crate::catalog::Catalog;

#[derive(Clone)]
struct ModelSnapshot {
    provider: Option<String>,
    model: Option<String>,
}

fn safe_name(key: &str) -> Option<String> {
    let value = std::env::var(key).ok()?;
    let value = value.trim();
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-/".contains(&c))
    {
        return None;
    }
    Some(value.to_owned())
}

pub fn router() -> Router<Foundation> {
    let snapshot = ModelSnapshot {
        provider: safe_name("LLM_PROVIDER"),
        model: safe_name("LLM_MODEL"),
    };
    Router::new()
        .route("/settings/models", get(models))
        .route("/settings/connections", get(connections))
        .route("/settings/catalog", get(catalog))
        .route("/settings/access", get(access))
        .route("/settings/runtime", get(runtime))
        .route("/settings/audit", get(audit_page))
        .route("/settings/audit/{id}", get(audit_event))
        .route("/settings/observability", get(observability))
        .layer(Extension(snapshot))
}

async fn models(
    State(f): State<Foundation>,
    user: AuthUser,
    Extension(snapshot): Extension<ModelSnapshot>,
) -> Result<Json<Envelope<Value>>, ApiError> {
    service::require_admin(&f, &user).await?;
    Ok(Json(Envelope::ok(
        service::models(&f, snapshot.provider.as_deref(), snapshot.model.as_deref()).await?,
    )))
}

async fn connections(
    State(f): State<Foundation>,
    user: AuthUser,
) -> Result<Json<Envelope<Value>>, ApiError> {
    service::require_admin(&f, &user).await?;
    Ok(Json(Envelope::ok(service::connections(&f).await?)))
}

async fn catalog(
    State(f): State<Foundation>,
    user: AuthUser,
    Extension(c): Extension<Arc<Catalog>>,
) -> Result<Json<Envelope<Value>>, ApiError> {
    service::require_admin(&f, &user).await?;
    Ok(Json(Envelope::ok(service::catalog(&f, &c).await?)))
}

async fn access(
    State(f): State<Foundation>,
    user: AuthUser,
) -> Result<Json<Envelope<Value>>, ApiError> {
    service::require_admin(&f, &user).await?;
    Ok(Json(Envelope::ok(service::access(&f, &user).await?)))
}

async fn runtime(
    State(f): State<Foundation>,
    user: AuthUser,
) -> Result<Json<Envelope<Value>>, ApiError> {
    service::require_admin(&f, &user).await?;
    Ok(Json(Envelope::ok(service::runtime(&f))))
}

async fn observability(
    State(f): State<Foundation>,
    user: AuthUser,
) -> Result<Json<Envelope<Value>>, ApiError> {
    service::require_admin(&f, &user).await?;
    Ok(Json(Envelope::ok(service::observability())))
}

async fn audit_page(
    State(f): State<Foundation>,
    user: AuthUser,
    query: Result<Query<AuditQuery>, QueryRejection>,
) -> Result<Json<Envelope<AuditPage>>, ApiError> {
    service::require_admin(&f, &user).await?;
    let Query(query) = query.map_err(|_| ApiError::Unprocessable("invalid audit query".into()))?;
    Ok(Json(Envelope::ok(service::audit_page(&f, query).await?)))
}

async fn audit_event(
    State(f): State<Foundation>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<Envelope<AuditDetail>>, ApiError> {
    service::require_admin(&f, &user).await?;
    let id =
        Uuid::parse_str(&id).map_err(|_| ApiError::Unprocessable("invalid audit id".into()))?;
    Ok(Json(Envelope::ok(service::audit_event(&f, id).await?)))
}
