//! Fondasi bersama: config, tracing, pool database, Redis, envelope API,
//! ekstraktor tervalidasi, error, dan autentikasi.
//!
//! Lihat docs/data/database-design.md untuk invarian yang berlaku lintas crate.

pub mod config;
pub mod db;
pub mod envelope;
pub mod error;
pub mod redis;
pub mod state;
pub mod telemetry;

pub use config::{AppEnv, Config};
pub use envelope::{Envelope, ErrorBody};
pub use error::ApiError;
pub use state::Foundation;
