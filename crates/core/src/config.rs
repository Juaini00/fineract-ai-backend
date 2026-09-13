//! Konfigurasi runtime, dibaca sekali saat startup dari environment.
//!
//! Sumber nilai: `.env.example` (dokumentasi) dan `docs/operations/runtime.md`
//! (alasan + pemicu revisi tiap angka). Aturan di sini:
//!
//! - **Hanya variabel yang sudah punya konsumen** yang dimuat. Memuat seluruh
//!   `.env.example` sekarang berarti memelihara field yang tidak dibaca siapa
//!   pun; tambahkan saat kodenya ada.
//! - **Rahasia tidak pernah punya default produksi.** `JWT_*_SECRET` wajib ada;
//!   tidak ada nilai cadangan yang diam-diam dipakai di production.
//! - **Fail fast**: konfigurasi salah menggagalkan startup dengan pesan yang
//!   menyebut nama variabelnya, bukan menunggu request pertama.

use serde::Deserialize;

/// Environment deployment. Dipakai untuk keputusan yang tidak boleh bergantung
/// pada flag sendiri-sendiri — mis. migrasi otomatis (lihat [`Config::may_migrate_on_startup`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AppEnv {
    Local,
    Staging,
    Production,
}

/// Konfigurasi aplikasi yang sudah tervalidasi.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    // ---- Aplikasi ----
    #[serde(default = "default_app_env")]
    pub app_env: AppEnv,
    #[serde(default = "default_host")]
    pub app_host: String,
    #[serde(default = "default_port")]
    pub app_port: u16,

    // ---- PostgreSQL aplikasi (writable, sumber kebenaran durable) ----
    pub app_database_url: String,
    #[serde(default = "default_app_db_max_connections")]
    pub app_database_max_connections: u32,
    /// Hanya berlaku di `AppEnv::Local`; lihat [`Config::may_migrate_on_startup`].
    #[serde(default)]
    pub app_database_migrate_on_startup: bool,

    // ---- Fineract (read-only; credentials terpisah) ----
    pub fineract_database_url: String,
    #[serde(default = "default_fineract_db_max_connections")]
    pub fineract_database_max_connections: u32,
    /// Identitas deployment Fineract, direkam pada scope job untuk audit (#7).
    pub fineract_tenant: String,

    // ---- Redis (koordinasi live saja, bukan sumber kebenaran) ----
    #[serde(default = "default_true")]
    pub redis_enabled: bool,
    #[serde(default = "default_redis_url")]
    pub redis_url: String,

    // ---- Autentikasi ----
    pub jwt_access_secret: String,
    pub jwt_refresh_secret: String,
    #[serde(default = "default_access_expiry_secs")]
    pub jwt_access_token_expiry_seconds: u64,
    #[serde(default = "default_refresh_expiry_secs")]
    pub jwt_refresh_token_expiry_seconds: u64,
    /// Config, bukan konstanta kode (#7): pindah ke SSO tidak boleh menuntut
    /// perubahan kode.
    #[serde(default = "default_jwt_issuer")]
    pub jwt_issuer: String,
    #[serde(default = "default_jwt_audience")]
    pub jwt_audience: String,
}

impl Config {
    /// Muat `.env` (bila ada) lalu deserialisasi dari environment.
    ///
    /// `.env` tidak menimpa variabel yang sudah diset di environment proses —
    /// environment nyata menang atas file pengembangan.
    pub fn from_env() -> anyhow::Result<Self> {
        let _ = dotenvy::dotenv();
        Self::from_builder(
            config::Config::builder().add_source(config::Environment::default().try_parsing(true)),
        )
    }

    fn from_builder(
        builder: config::ConfigBuilder<config::builder::DefaultState>,
    ) -> anyhow::Result<Self> {
        let config: Self = builder.build()?.try_deserialize()?;
        config.validate()?;
        Ok(config)
    }

    /// Alamat bind server HTTP.
    pub fn bind_address(&self) -> String {
        format!("{}:{}", self.app_host, self.app_port)
    }

    /// Migrasi otomatis saat startup hanya sah di `local`.
    ///
    /// AGENTS.md: "Startup aplikasi tidak pernah membuat atau mengubah tabel."
    /// Flag sendirian tidak cukup — flag yang keliru aktif di production adalah
    /// tepat cara invarian itu jebol tanpa suara.
    pub fn may_migrate_on_startup(&self) -> bool {
        self.app_database_migrate_on_startup && self.app_env == AppEnv::Local
    }

    fn validate(&self) -> anyhow::Result<()> {
        // Dua pool berbeda hak akses (overview §2). URL identik berarti pool
        // "read-only" sebenarnya memegang kredensial writable.
        if self.app_database_url == self.fineract_database_url {
            anyhow::bail!(
                "APP_DATABASE_URL dan FINERACT_DATABASE_URL tidak boleh sama: \
                 pool Fineract wajib memakai kredensial read-only terpisah"
            );
        }

        if self.app_env != AppEnv::Local {
            // Rahasia contoh pada .env.example tidak boleh ikut terbawa keluar local.
            for (name, secret) in [
                ("JWT_ACCESS_SECRET", &self.jwt_access_secret),
                ("JWT_REFRESH_SECRET", &self.jwt_refresh_secret),
            ] {
                if secret.contains("change-me") {
                    anyhow::bail!("{name} masih memakai nilai contoh di env {:?}", self.app_env);
                }
            }
        }

        if self.jwt_access_secret == self.jwt_refresh_secret {
            anyhow::bail!(
                "JWT_ACCESS_SECRET dan JWT_REFRESH_SECRET tidak boleh sama: \
                 access token yang bocor akan lolos sebagai refresh token"
            );
        }

        if self.jwt_access_token_expiry_seconds >= self.jwt_refresh_token_expiry_seconds {
            anyhow::bail!(
                "JWT_ACCESS_TOKEN_EXPIRY_SECONDS harus lebih pendek daripada \
                 JWT_REFRESH_TOKEN_EXPIRY_SECONDS"
            );
        }

        Ok(())
    }
}

fn default_app_env() -> AppEnv {
    AppEnv::Local
}
fn default_host() -> String {
    "127.0.0.1".to_string()
}
fn default_port() -> u16 {
    3007
}
fn default_app_db_max_connections() -> u32 {
    20
}
fn default_fineract_db_max_connections() -> u32 {
    10
}
fn default_true() -> bool {
    true
}
fn default_redis_url() -> String {
    "redis://127.0.0.1:6380/0".to_string()
}
fn default_access_expiry_secs() -> u64 {
    900
}
fn default_refresh_expiry_secs() -> u64 {
    604_800
}
fn default_jwt_issuer() -> String {
    "jarvis".to_string()
}
fn default_jwt_audience() -> String {
    "jarvis-api".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Set minimal yang sah; test memodifikasi salinannya.
    fn minimal() -> Vec<(&'static str, String)> {
        vec![
            ("app_database_url", "postgres://app/jarvis".into()),
            ("fineract_database_url", "postgres://ro/fineract".into()),
            ("fineract_tenant", "default".into()),
            ("jwt_access_secret", "access-secret".into()),
            ("jwt_refresh_secret", "refresh-secret".into()),
        ]
    }

    fn build(entries: Vec<(&'static str, String)>) -> anyhow::Result<Config> {
        // `config::Environment` membaca environment proses — tidak dapat dipakai
        // di test paralel. Override eksplisit memberi input yang sama tanpa
        // menyentuh global state. Override terakhir untuk sebuah key menang,
        // sehingga test cukup menambahkan entri untuk menimpa `minimal()`.
        let mut builder = config::Config::builder();
        for (key, value) in entries {
            builder = builder.set_override(key, value)?;
        }
        Config::from_builder(builder)
    }

    #[test]
    fn defaults_fill_optional_fields() {
        let config = build(minimal()).unwrap();

        assert_eq!(config.app_env, AppEnv::Local);
        assert_eq!(config.bind_address(), "127.0.0.1:3007");
        assert_eq!(config.jwt_issuer, "jarvis");
        assert!(config.redis_enabled);
        assert!(!config.app_database_migrate_on_startup);
    }

    #[test]
    fn missing_required_secret_fails_startup() {
        let entries = minimal()
            .into_iter()
            .filter(|(key, _)| *key != "jwt_access_secret")
            .collect();

        let error = build(entries).unwrap_err().to_string();
        assert!(error.contains("jwt_access_secret"), "pesan tidak menyebut variabelnya: {error}");
    }

    #[test]
    fn shared_database_url_is_rejected() {
        let mut entries = minimal();
        entries.push(("fineract_database_url", "postgres://app/jarvis".into()));

        let error = build(entries).unwrap_err().to_string();
        assert!(error.contains("read-only"), "{error}");
    }

    #[test]
    fn identical_jwt_secrets_are_rejected() {
        let mut entries = minimal();
        entries.push(("jwt_refresh_secret", "access-secret".into()));

        assert!(build(entries).is_err());
    }

    #[test]
    fn access_token_must_expire_before_refresh_token() {
        let mut entries = minimal();
        entries.push(("jwt_access_token_expiry_seconds", "604800".into()));

        assert!(build(entries).is_err());
    }

    #[test]
    fn example_secret_rejected_outside_local() {
        let mut entries = minimal();
        entries.push(("app_env", "production".into()));
        entries.push(("jwt_access_secret", "local-access-secret-change-me".into()));

        let error = build(entries).unwrap_err().to_string();
        assert!(error.contains("JWT_ACCESS_SECRET"), "{error}");
    }

    #[test]
    fn migrate_on_startup_is_ignored_outside_local() {
        let mut entries = minimal();
        entries.push(("app_env", "production".into()));
        entries.push(("app_database_migrate_on_startup", "true".into()));
        entries.push(("jwt_access_secret", "produksi-access".into()));
        entries.push(("jwt_refresh_secret", "produksi-refresh".into()));

        let config = build(entries).unwrap();
        assert!(config.app_database_migrate_on_startup);
        assert!(!config.may_migrate_on_startup(), "migrasi startup bocor ke production");
    }

    #[test]
    fn migrate_on_startup_allowed_in_local() {
        let mut entries = minimal();
        entries.push(("app_database_migrate_on_startup", "true".into()));

        assert!(build(entries).unwrap().may_migrate_on_startup());
    }
}
