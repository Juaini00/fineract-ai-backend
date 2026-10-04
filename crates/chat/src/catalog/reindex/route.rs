use std::sync::Arc;

use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
};
use foundation::{AuthUser, Envelope, error::ApiError, state::Foundation};
use uuid::Uuid;

use super::{
    model::{ReindexRequest, RunView},
    service,
};
use crate::{admin::service::require_admin, catalog::Catalog};

pub fn router() -> Router<Foundation> {
    Router::new()
        .route("/settings/catalog/reindex", post(start).get(recent))
        .route("/settings/catalog/reindex/{id}", get(read))
}

async fn start(
    State(foundation): State<Foundation>,
    Extension(running): Extension<Arc<Catalog>>,
    user: AuthUser,
    Json(request): Json<ReindexRequest>,
) -> Result<(StatusCode, Json<Envelope<RunView>>), ApiError> {
    require_admin(&foundation, &user).await?;
    let checked = service::checked_current(&foundation).await?;
    let admission = service::admit(
        &foundation,
        checked,
        Some(&running),
        request.rebuild_embeddings,
        Some(user.user_id),
    )
    .await?;
    let accepted = admission.run().clone();
    let worker_foundation = foundation.clone();
    tokio::spawn(async move {
        if let Err(error) = admission.execute(&worker_foundation).await {
            tracing::error!(error=%error, "catalog reindex failed");
        }
    });
    let view = service::view(&foundation, &running, accepted).await?;
    Ok((StatusCode::ACCEPTED, Json(Envelope::ok(view))))
}

async fn read(
    State(foundation): State<Foundation>,
    Extension(running): Extension<Arc<Catalog>>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Envelope<RunView>>, ApiError> {
    require_admin(&foundation, &user).await?;
    Ok(Json(Envelope::ok(
        service::get(&foundation, &running, id).await?,
    )))
}

async fn recent(
    State(foundation): State<Foundation>,
    Extension(running): Extension<Arc<Catalog>>,
    user: AuthUser,
) -> Result<Json<Envelope<Vec<RunView>>>, ApiError> {
    require_admin(&foundation, &user).await?;
    Ok(Json(Envelope::ok(
        service::recent(&foundation, &running).await?,
    )))
}
