//! Autentikasi: password, token, session, dan ekstraktor identitas.
//!
//! Penerbit token adalah Jarvis sendiri (#7). Jalur akses mengikuti
//! `route → service → repository → database`; `sqlx` hanya ada di
//! [`repository`].

pub mod extractor;
pub mod password;
pub mod repository;
pub mod route;
pub mod service;
pub mod token;

pub use extractor::AuthUser;
pub use service::Profile;
