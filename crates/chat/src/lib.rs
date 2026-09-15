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
pub mod clarification;
pub mod engine;
pub mod job;
pub mod session;
pub mod settings;

use std::sync::Arc;

use axum::{Extension, Router};
use foundation::state::Foundation;

/// Seluruh route fitur chat, dirakit oleh `app`.
///
/// Katalog dibawa sebagai `Extension`, bukan dimuat per request: isinya tetap
/// selama proses hidup, dan resolver opsi wajib memakai katalog yang **sama**
/// dengan yang dilihat worker — dua pemuatan berarti dua kebenaran.
pub fn router(catalog: Arc<catalog::Catalog>) -> Router<Foundation> {
    Router::new()
        .merge(session::route::router())
        .merge(job::route::router())
        .merge(clarification::route::router())
        .layer(Extension(catalog))
}
