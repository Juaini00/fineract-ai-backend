//! Entrypoint biner dan composition root: merangkai fondasi `core`
//! dengan fitur `chat`.
//!
//! `app` hanya merakit dan menjalankan proses — tidak ada orchestration domain
//! di sini (overview §3).

mod health;

use foundation::{Config, Foundation, auth, telemetry};

use std::net::SocketAddr;
use tokio::{net::TcpListener, signal};
use tracing::{info, warn};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    telemetry::init();

    let config = Config::from_env()?;
    let bind_address = config.bind_address();
    let app_env = config.app_env;

    let foundation = Foundation::connect(config).await?;

    if foundation.config().may_migrate_on_startup() {
        // Hanya di local. Di lingkungan lain schema dipasang lewat
        // `sqlx migrate run` sebagai langkah deploy tersendiri (AGENTS.md).
        info!("menjalankan migrasi (local)");
        foundation.app_db().migrate().await?;
    }

    if foundation.config().may_bootstrap_admin() {
        auth::service::bootstrap_admin(&foundation).await?;
    }

    let router = health::router()
        .merge(auth::route::router())
        .with_state(foundation);

    let listener = TcpListener::bind(&bind_address).await?;
    info!(%bind_address, ?app_env, "jarvis listening");

    // `into_make_service_with_connect_info` diperlukan agar alamat klien dapat
    // direkam pada auth_sessions.ip_address.
    axum::serve(
        listener,
        router.into_make_service_with_connect_info::<SocketAddr>(),
    )
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    info!("jarvis berhenti");
    Ok(())
}

/// Tunggu SIGINT atau SIGTERM. SIGTERM penting karena container dihentikan
/// dengan sinyal itu; tanpa menanganinya, shutdown selalu berupa kill paksa.
async fn shutdown_signal() {
    let interrupt = async {
        if let Err(error) = signal::ctrl_c().await {
            warn!(error = %error, "gagal memasang handler SIGINT");
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match signal::unix::signal(signal::unix::SignalKind::terminate()) {
            Ok(mut stream) => {
                stream.recv().await;
            }
            Err(error) => warn!(error = %error, "gagal memasang handler SIGTERM"),
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = interrupt => info!("SIGINT diterima"),
        _ = terminate => info!("SIGTERM diterima"),
    }
}
