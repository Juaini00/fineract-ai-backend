//! Health endpoint operasional.
//!
//! Berada di `app` karena ini urusan proses, bukan domain pelaporan. Aturan
//! yang ditegakkan di sini: **ketidakpastian terlihat** (invarian I4). Redis
//! mati bukan berarti sistem mati — ia hanya koordinasi live — jadi statusnya
//! dilaporkan apa adanya, tidak dibulatkan menjadi "ok".

use axum::{Json, extract::State, http::StatusCode, routing::get};
use foundation::{Envelope, state::Foundation};
use serde::Serialize;
use tracing::error;

#[derive(Debug, Serialize)]
pub struct Health {
    /// `ok` bila seluruh dependency wajib sehat, selain itu `degraded`.
    status: &'static str,
    app_database: &'static str,
    fineract_database: &'static str,
    redis: &'static str,
}

pub fn router() -> axum::Router<Foundation> {
    axum::Router::new().route("/health", get(health))
}

async fn health(State(foundation): State<Foundation>) -> (StatusCode, Json<Envelope<Health>>) {
    let app_database = probe("app_database", foundation.app_db().ping().await);
    let fineract_database = probe("fineract_database", foundation.fineract_db().ping().await);

    // Kedua PostgreSQL wajib; Redis tidak (overview §5).
    let healthy = app_database == "ok" && fineract_database == "ok";
    let status = if healthy {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (
        status,
        Json(Envelope::ok(Health {
            status: if healthy { "ok" } else { "degraded" },
            app_database,
            fineract_database,
            redis: foundation.notifier().status(),
        })),
    )
}

fn probe(component: &'static str, result: anyhow::Result<()>) -> &'static str {
    match result {
        Ok(()) => "ok",
        Err(error) => {
            // Detail kegagalan hanya ke log, tidak ke response (error.rs).
            error!(component, error = %error, "health probe gagal");
            "unavailable"
        }
    }
}
