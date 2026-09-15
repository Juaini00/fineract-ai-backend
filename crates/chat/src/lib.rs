//! Fitur pelaporan berbasis percakapan: session, job, plan, node ledger,
//! klarifikasi, dataset, memori, dan response document.
//!
//! Lihat docs/architecture/engine.md untuk lifecycle dan
//! docs/contracts/responses.md untuk aturan validasi response.
//!
//! Batas persistence: `route → service → repository → database`. `sqlx` hanya
//! muncul di modul `repository` (dan `audit`/`settings` yang juga repository).

pub mod audit;
pub mod catalog;
pub mod engine;
pub mod job;
pub mod session;
pub mod settings;

use axum::Router;
use foundation::state::Foundation;

/// Seluruh route fitur chat, dirakit oleh `app`.
pub fn router() -> Router<Foundation> {
    Router::new()
        .merge(session::route::router())
        .merge(job::route::router())
}
