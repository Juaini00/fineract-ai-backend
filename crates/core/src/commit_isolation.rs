//! Instrumentation isolasi commit — I1, OVR-6.7.
//!
//! Transaksi commit aplikasi (T1–T12, `docs/data/database-design.md` §6) hanya
//! boleh berisi tulisan PostgreSQL lokal. Modul ini menjadikan aturan itu
//! terukur, bukan prosa:
//!
//! - [`begin`] adalah satu-satunya pembuka transaksi commit (ditegakkan
//!   `clippy.toml` → `disallowed-methods`). Ia menandai **task** pemanggil
//!   sebagai "di dalam transaksi" sampai [`CommitWindow`] dilepas — setelah
//!   commit, rollback, atau early return.
//! - Setiap titik keluar ke dunia luar — query Fineract ([`crate::db::FineractDb`]),
//!   HTTP embedding ([`crate::embedding::EmbeddingClient`]), Redis
//!   ([`crate::redis::Notifier`]) — memanggil [`guard`]. Klien LLM/HTTP baru
//!   wajib melakukan hal yang sama.
//! - Panggilan saat jendela terbuka adalah pelanggaran: dihitung
//!   ([`violations`]), di-log `error!` dengan penanda [`VIOLATION_MARKER`], dan
//!   pada build debug langsung panic. `scripts/integration-test.sh` gagal bila
//!   penanda itu muncul di log app.
//!
//! Penandanya per task, bukan global: transaksi yang terbuka di task A tidak
//! boleh membuat query Fineract di task B tampak melanggar. Di luar task Tokio
//! (`block_on` pada `main`, mis. `app catalog`) identitasnya adalah thread.

use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex, MutexGuard, PoisonError};
use std::thread::{self, ThreadId};

use sqlx::{PgPool, Postgres, Transaction};
use tracing::error;

/// Penanda stabil di log dan pesan panic; dicari oleh runner integrasi.
pub const VIOLATION_MARKER: &str = "commit_isolation_violation";

/// Jenis panggilan eksternal yang dilarang selama transaksi commit (I1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalCall {
    Fineract,
    Embedding,
    Redis,
}

impl ExternalCall {
    fn label(self) -> &'static str {
        match self {
            Self::Fineract => "fineract",
            Self::Embedding => "embedding",
            Self::Redis => "redis",
        }
    }
}

/// Panggilan eksternal yang terjadi saat transaksi commit task yang sama
/// masih terbuka.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Violation(pub ExternalCall);

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{VIOLATION_MARKER}: panggilan {} saat transaksi commit terbuka (I1, OVR-6.7)",
            self.0.label()
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Owner {
    Task(tokio::task::Id),
    Thread(ThreadId),
}

fn current() -> Owner {
    match tokio::task::try_id() {
        Some(id) => Owner::Task(id),
        None => Owner::Thread(thread::current().id()),
    }
}

static OPEN: LazyLock<Mutex<HashMap<Owner, u32>>> = LazyLock::new(Mutex::default);
static VIOLATIONS: AtomicU64 = AtomicU64::new(0);

fn open_windows() -> MutexGuard<'static, HashMap<Owner, u32>> {
    OPEN.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Jendela transaksi commit milik task pemanggil; tertutup saat di-drop.
#[must_use = "jendela tertutup seketika bila tidak disimpan"]
#[derive(Debug)]
pub struct CommitWindow {
    owner: Owner,
}

impl CommitWindow {
    pub fn open() -> Self {
        let owner = current();
        *open_windows().entry(owner).or_insert(0) += 1;
        Self { owner }
    }
}

impl Drop for CommitWindow {
    fn drop(&mut self) {
        let mut open = open_windows();
        if let Some(depth) = open.get_mut(&self.owner) {
            *depth -= 1;
            if *depth == 0 {
                open.remove(&self.owner);
            }
        }
    }
}

/// Buka transaksi commit aplikasi beserta jendelanya. Simpan jendelanya
/// selama transaksi hidup: `let (_window, mut tx) = begin(pool).await?;`.
#[allow(clippy::disallowed_methods)]
pub async fn begin(pool: &PgPool) -> sqlx::Result<(CommitWindow, Transaction<'static, Postgres>)> {
    let window = CommitWindow::open();
    let tx = pool.begin().await?;
    Ok((window, tx))
}

/// Periksa satu panggilan eksternal. Pelanggaran dihitung dan di-log di sini.
pub fn check(call: ExternalCall) -> Result<(), Violation> {
    if !open_windows().contains_key(&current()) {
        return Ok(());
    }
    let violation = Violation(call);
    let total = VIOLATIONS.fetch_add(1, Ordering::Relaxed) + 1;
    error!(call = call.label(), total, "{violation}");
    Err(violation)
}

/// Titik cek di setiap choke point eksternal. Build debug (termasuk runner
/// integrasi) gagal keras; build release tetap menghitung dan me-log.
pub fn guard(call: ExternalCall) {
    if let Err(violation) = check(call)
        && cfg!(debug_assertions)
    {
        panic!("{violation}");
    }
}

/// Jumlah pelanggaran sejak proses mulai.
pub fn violations() -> u64 {
    VIOLATIONS.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Satu test untuk seluruh skenario: penghitung bersifat global per proses,
    // dan test lain yang berjalan paralel tidak boleh mengganggu selisihnya.
    #[tokio::test]
    async fn ovr_6_7_external_call_is_refused_only_inside_the_callers_commit_window() {
        let before = violations();

        // Di luar transaksi mana pun: diizinkan, tidak dihitung.
        let outside = tokio::spawn(async { check(ExternalCall::Fineract) });
        assert_eq!(outside.await.unwrap(), Ok(()));
        assert_eq!(violations(), before);

        // Transaksi terbuka di task lain tidak menodai task ini.
        let (opened_tx, opened_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel::<()>();
        let holder = tokio::spawn(async move {
            let _window = CommitWindow::open();
            opened_tx.send(()).unwrap();
            let _ = release_rx.await;
        });
        opened_rx.await.unwrap();
        let other = tokio::spawn(async { check(ExternalCall::Embedding) });
        assert_eq!(other.await.unwrap(), Ok(()));
        assert_eq!(violations(), before);
        release_tx.send(()).unwrap();
        holder.await.unwrap();

        // Di dalam jendela task sendiri: ditolak dan dihitung.
        let inside = tokio::spawn(async {
            let _window = CommitWindow::open();
            check(ExternalCall::Fineract)
        });
        assert_eq!(
            inside.await.unwrap(),
            Err(Violation(ExternalCall::Fineract))
        );
        assert_eq!(violations(), before + 1);

        // Setelah commit (jendela dilepas) task yang sama boleh keluar lagi.
        let after_commit = tokio::spawn(async {
            drop(CommitWindow::open());
            check(ExternalCall::Redis)
        });
        assert_eq!(after_commit.await.unwrap(), Ok(()));
        assert_eq!(violations(), before + 1);

        // Di luar task Tokio (`block_on`, mis. `app catalog`) identitasnya thread.
        {
            let _window = CommitWindow::open();
            assert!(check(ExternalCall::Embedding).is_err());
        }
        assert_eq!(check(ExternalCall::Embedding), Ok(()));
        assert_eq!(violations(), before + 2);

        // Choke point gagal keras pada build debug, dengan penanda yang dicari
        // runner integrasi.
        let hard = tokio::spawn(async {
            let _window = CommitWindow::open();
            guard(ExternalCall::Fineract);
        });
        let panic = hard.await.unwrap_err().into_panic();
        let message = panic.downcast_ref::<String>().unwrap();
        assert!(message.starts_with(VIOLATION_MARKER), "{message}");
        assert_eq!(violations(), before + 3);
    }
}
