//! Entrypoint biner dan composition root: merangkai fondasi `core`
//! dengan fitur `chat`.
//!
//! `app` hanya merakit dan menjalankan proses — tidak ada orchestration domain
//! di sini (overview §3).

mod catalog_command;
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

    // Subcommand dijalankan tanpa membuka listener: pemeriksaan katalog adalah
    // alat operator, bukan endpoint.
    if let Some(command) = std::env::args().nth(1) {
        return run_command(&foundation, &command).await;
    }

    if foundation.config().may_migrate_on_startup() {
        // Hanya di local. Di lingkungan lain schema dipasang lewat
        // `sqlx migrate run` sebagai langkah deploy tersendiri (AGENTS.md).
        info!("menjalankan migrasi (local)");
        foundation.app_db().migrate().await?;
    }

    if foundation.config().may_bootstrap_admin() {
        auth::service::bootstrap_admin(&foundation).await?;
    }

    // Satu pemuatan katalog untuk seluruh proses: worker dan resolver opsi wajib
    // melihat isi yang sama, jika tidak plan dan opsi dapat merujuk versi yang
    // berbeda tanpa satu pun sinyal kegagalan.
    let (catalog, catalog_version_id) = chat::catalog::prepare(&foundation).await?;

    let engine =
        chat::engine::Background::spawn(&foundation, catalog.clone(), catalog_version_id).await?;

    // Hub notifikasi hidup selama proses, terlepas dari worker: instance yang
    // tidak menjalankan worker tetap harus menstream kemajuan job yang
    // dikerjakan instance lain.
    let hub = chat::events::Hub::spawn(&foundation, engine.shutdown_token());

    let router = health::router()
        .merge(auth::route::router())
        .merge(chat::router(catalog, hub))
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

    // Worker dihentikan SETELAH server berhenti menerima request: job yang
    // sudah diterima tetap punya kesempatan diklaim dan diselesaikan.
    engine.shutdown().await;

    info!("jarvis berhenti");
    Ok(())
}

/// Jalankan subcommand CLI lalu berhenti.
async fn run_command(foundation: &Foundation, command: &str) -> anyhow::Result<()> {
    match command {
        "catalog" => {
            let arguments: Vec<String> = std::env::args().skip(2).collect();
            let passed = catalog_command::run(
                foundation,
                arguments.iter().any(|argument| argument == "--sync"),
                arguments.iter().any(|argument| argument == "--no-probe"),
            )
            .await?;

            if !passed {
                // Exit code non-nol supaya CI dan skrip dapat memakainya.
                std::process::exit(1);
            }
            Ok(())
        }
        other => {
            anyhow::bail!("subcommand tidak dikenal: {other} (tersedia: catalog [--sync] [--no-probe])")
        }
    }
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
