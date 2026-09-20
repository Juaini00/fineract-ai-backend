//! Inisialisasi tracing.
//!
//! Detail internal (SQL, prompt, sumber error) hanya boleh terlihat di sini —
//! tidak pernah di response publik (lihat [`crate::error`]). Exporter
//! observability final menunggu `docs/operations/observability.md`; sampai itu
//! ada, keluaran adalah stdout terstruktur dengan filter dari `RUST_LOG`.

use tracing_subscriber::{EnvFilter, fmt, prelude::*};

/// Pasang subscriber global. Aman dipanggil lebih dari sekali: pemanggilan
/// kedua tidak menimpa subscriber pertama dan tidak panic.
pub fn init() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,tower_http=debug,sqlx=warn"));

    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer())
        .try_init();
}
