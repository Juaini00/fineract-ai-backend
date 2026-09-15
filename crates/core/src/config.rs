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

    // ---- Bootstrap admin ----
    /// Seed admin pertama saat tabel `users` masih kosong. Tidak pernah
    /// menimpa user yang sudah ada.
    #[serde(default)]
    pub auth_bootstrap_admin_enabled: bool,
    #[serde(default = "default_bootstrap_username")]
    pub auth_bootstrap_admin_username: String,
    #[serde(default)]
    pub auth_bootstrap_admin_password: Option<String>,
    #[serde(default)]
    pub auth_bootstrap_admin_email: Option<String>,

    // ---- Katalog pengetahuan ----
    #[serde(default = "default_catalog_path")]
    pub catalog_path: String,
    #[serde(default = "default_query_path")]
    pub query_path: String,
    #[serde(default = "default_true")]
    pub catalog_validate_on_startup: bool,
    #[serde(default)]
    pub catalog_sync_on_startup: bool,

    // ---- Worker, lease dan recovery (runtime.md §1) ----
    #[serde(default = "default_true")]
    pub worker_enabled: bool,
    #[serde(default = "default_lease_duration_secs")]
    pub worker_lease_duration_secs: i64,
    #[serde(default = "default_lease_heartbeat_interval_secs")]
    pub worker_lease_heartbeat_interval_secs: u64,
    #[serde(default = "default_reaper_interval_secs")]
    pub reaper_interval_secs: u64,
    #[serde(default = "default_job_ttl_running_secs")]
    pub job_ttl_running_secs: i64,
    /// Jeda polling antrean. Sementara: notifikasi Redis belum dipakai, jadi
    /// worker memeriksa PostgreSQL secara berkala. Setelah notifikasi ada,
    /// polling menjadi fallback, bukan jalur utama (SSE §transport).
    #[serde(default = "default_worker_poll_interval_ms")]
    pub worker_poll_interval_ms: u64,

    // ---- Idempotency (runtime.md §3) ----
    #[serde(default = "default_idempotency_ttl_secs")]
    pub idempotency_ttl_secs: i64,
    #[serde(default = "default_idempotency_key_min_length")]
    pub idempotency_key_min_length: usize,
    #[serde(default = "default_idempotency_key_max_length")]
    pub idempotency_key_max_length: usize,

    // ---- Cookie refresh token ----
    #[serde(default = "default_refresh_cookie_name")]
    pub auth_refresh_cookie_name: String,
    #[serde(default = "default_true")]
    pub auth_refresh_cookie_secure: bool,
    #[serde(default = "default_refresh_cookie_same_site")]
    pub auth_refresh_cookie_same_site: String,
    #[serde(default = "default_refresh_cookie_path")]
    pub auth_refresh_cookie_path: String,
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

    /// Seed admin hanya sah di `local`, dengan alasan yang sama seperti
    /// [`Config::may_migrate_on_startup`]: kredensial seed yang tanpa sengaja
    /// aktif di production adalah pintu masuk yang tidak pernah diminta siapa pun.
    pub fn may_bootstrap_admin(&self) -> bool {
        self.auth_bootstrap_admin_enabled && self.app_env == AppEnv::Local
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

            // Cookie refresh tanpa `Secure` berarti token dikirim polos pada
            // hop HTTP mana pun.
            if !self.auth_refresh_cookie_secure {
                anyhow::bail!(
                    "AUTH_REFRESH_COOKIE_SECURE wajib true di env {:?}",
                    self.app_env
                );
            }
        }

        if self.jwt_access_secret == self.jwt_refresh_secret {
            anyhow::bail!(
                "JWT_ACCESS_SECRET dan JWT_REFRESH_SECRET tidak boleh sama: \
                 access token yang bocor akan lolos sebagai refresh token"
            );
        }

        // K1: lease diperpanjang task heartbeat independen. Interval yang tidak
        // lebih rapat daripada lease berarti worker sehat dipagari di tengah
        // kerja — kegagalan yang tampak seperti bug acak.
        if self.worker_lease_heartbeat_interval_secs as i64 * 3 > self.worker_lease_duration_secs {
            anyhow::bail!(
                "WORKER_LEASE_HEARTBEAT_INTERVAL_SECS ({}) terlalu longgar untuk \
                 WORKER_LEASE_DURATION_SECS ({}): butuh setidaknya tiga kesempatan renewal",
                self.worker_lease_heartbeat_interval_secs,
                self.worker_lease_duration_secs
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
fn default_lease_duration_secs() -> i64 {
    60
}
fn default_lease_heartbeat_interval_secs() -> u64 {
    10
}
fn default_reaper_interval_secs() -> u64 {
    30
}
fn default_job_ttl_running_secs() -> i64 {
    1_800
}
fn default_worker_poll_interval_ms() -> u64 {
    1_000
}
fn default_catalog_path() -> String {
    "knowledge".to_string()
}
fn default_query_path() -> String {
    "queries".to_string()
}
fn default_idempotency_ttl_secs() -> i64 {
    86_400
}
/// Batas panjang kunci mengikuti CHECK pada `idempotency_keys` — keduanya
/// wajib bergerak bersama; nilai yang lolos validasi tetapi ditolak database
/// menjadi 500, bukan 422.
fn default_idempotency_key_min_length() -> usize {
    16
}
fn default_idempotency_key_max_length() -> usize {
    255
}
fn default_bootstrap_username() -> String {
    "admin".to_string()
}
fn default_refresh_cookie_name() -> String {
    "refresh_token".to_string()
}
fn default_refresh_cookie_same_site() -> String {
    "strict".to_string()
}
fn default_refresh_cookie_path() -> String {
    "/".to_string()
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
