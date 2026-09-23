//! Handle dataset + chunk (L3, [dataset-lifecycle.md](../../../../../docs/data/dataset-lifecycle.md)).
//!
//! Modul ini memuat **aturan** lifecycle-nya dan tidak menyentuh database:
//! pemotongan chunk, batas simpan (`truncated`), status handle yang dibaca
//! pembaca (`handle_state`), otorisasi ulang tiap baca (I7), cursor paginasi,
//! diskriminator encoding, dan penjaga eviction. Semua yang butuh `sqlx` ada di
//! [`repository`].
//!
//! Tiga hal yang mudah dikacaukan dan sengaja dipisah (I4, §5 dokumen):
//! `truncated` (set yang **tersimpan**), `completeness` (kelengkapan
//! **analitik**), dan preview (pemotongan **tampilan** pada response). Modul ini
//! hanya memiliki dua yang pertama; preview milik `compose`.

pub mod repository;
pub mod route;
pub mod service;

use serde::Serialize;
use serde_json::{Map, Value};
use uuid::Uuid;

// Nilai awal dari docs/operations/runtime.md §4 — bukan hasil tuning. Tiap
// angka punya pemicu revisi terukur di sana; jangan ubah tanpa memenuhinya.
pub const CHUNK_ROWS: usize = 1_000;
pub const CHUNK_MAX_BYTES: usize = 1_048_576;
pub const DATASET_MAX_ROWS: usize = 100_000;
pub const DATASET_MAX_BYTES: usize = 67_108_864;
const DATASET_TTL_SECS: i64 = 86_400;

/// Diskriminator payload chunk (#11). `format` + `encoding_version` ada sejak
/// awal justru supaya pindah ke `BYTEA` terkompresi kelak **tidak** menuntut
/// migrasi: cukup nilai baru, dan chunk lama tetap terbaca oleh [`decode`].
pub const FORMAT_JSON: &str = "json";
pub const ENCODING_VERSION: i32 = 1;

/// Lifecycle job yang belum terminal. Dataset miliknya tidak boleh disentuh
/// eviction berapa pun umurnya (#11, §6 dokumen).
const NONTERMINAL: [&str; 4] = ["Queued", "Running", "WaitingForUser", "Cancelling"];

/// TTL efektif satu dataset.
///
/// **K5** — `DATASET_TTL ≥ CLARIFICATION_WAIT_LIMIT + JOB_TTL_RUNNING`. Dihitung
/// di sini, bukan dipercayakan pada dua konstanta yang kebetulan cocok: batas
/// tunggu klarifikasi adalah salah satu angka yang paling mungkin dinaikkan, dan
/// saat itu terjadi handle tidak boleh mati lebih dulu daripada job yang
/// menunggunya.
pub fn ttl_secs(clarification_wait_limit_secs: i64, job_ttl_running_secs: i64) -> i64 {
    DATASET_TTL_SECS.max(clarification_wait_limit_secs + job_ttl_running_secs)
}

/// Status handle sebagaimana **dibaca** pembaca (C13).
///
/// Non-optional dan tertutup: tidak ada varian "tidak tahu", sehingga tidak ada
/// jalur baca yang dapat melewatkan bahwa sebuah handle sudah mati.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HandleState {
    Live,
    Expired,
    Purged,
    None,
}

impl HandleState {
    /// Turunkan dari `datasets.status`. Total: setiap status punya satu jawaban.
    ///
    /// `building`/`failed` menjadi `None` — bukan `Live`: keduanya berarti tidak
    /// pernah ada snapshot yang boleh dibaca. Jalur tulis kami membuat handle
    /// dan chunk-nya dalam satu transaksi, jadi keduanya tidak bertahan.
    pub fn from_status(status: &str) -> Self {
        match status {
            "ready" => Self::Live,
            "expired" => Self::Expired,
            "purged" => Self::Purged,
            _ => Self::None,
        }
    }

    /// Hanya handle hidup yang masih punya baris untuk dibaca.
    pub fn is_readable(self) -> bool {
        self == Self::Live
    }
}

/// Penolakan baca. Dibedakan karena artinya berbeda bagi pemanggil — dan
/// keduanya tetap menjadi error publik tersanitasi di route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Denied {
    /// Handle milik pengguna lain. Keberadaannya tidak diungkap.
    NotOwner,
    /// Pemilik yang sama, tetapi otorisasi sekarang lebih sempit daripada scope
    /// saat dataset dibuat.
    ScopeNarrowed,
}

/// **I7** — otorisasi dicek ulang pada setiap pembacaan dataset.
///
/// Handle bukan token otorisasi; ia referensi. `scope_json` yang tersimpan
/// adalah konteks keputusan untuk audit, **bukan** grant yang boleh dipakai
/// ulang — karena itu ia dibandingkan dengan otorisasi yang berlaku sekarang,
/// bukan dipercaya apa adanya.
pub fn authorize(
    handle_owner: Uuid,
    handle_offices: &[i64],
    caller: Uuid,
    caller_offices: &[i64],
) -> Result<(), Denied> {
    if handle_owner != caller {
        return Err(Denied::NotOwner);
    }

    if handle_offices
        .iter()
        .any(|office| !caller_offices.contains(office))
    {
        return Err(Denied::ScopeNarrowed);
    }

    Ok(())
}

/// Satu chunk siap tulis.
#[derive(Debug, PartialEq, Eq)]
pub struct Chunk {
    pub chunk_index: i32,
    pub row_from: i64,
    pub row_to: i64,
    pub row_count: i32,
    pub byte_size: i64,
    pub payload: Value,
}

/// Hasil materialisasi: chunk + apa yang harus dinyatakan tentangnya.
#[derive(Debug)]
pub struct Materialized {
    pub chunks: Vec<Chunk>,
    /// Baris yang benar-benar tersimpan.
    pub row_count_available: i64,
    /// Baris yang dilihat node. `None` berarti **tidak diketahui**, bukan nol (I4).
    pub row_count_total: Option<i64>,
    /// Set yang tersimpan dibatasi cap — bukan pernyataan analitik. `Some`
    /// membawa cap mana yang tercapai; `truncated` diturunkan darinya, sehingga
    /// "terpotong tanpa alasan" tidak dapat direpresentasikan (I5).
    pub truncation: Option<&'static str>,
    pub byte_size: i64,
}

impl Materialized {
    pub fn truncated(&self) -> bool {
        self.truncation.is_some()
    }
}

/// Handle yang sudah `ready` dan tertaut ke node run, beserta apa yang wajib
/// dinyatakan response tentangnya (FIN-43, DS-8.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Retained {
    pub dataset_id: Uuid,
    /// `Some` bila set tersimpan dibatasi cap — response yang merujuk handle
    /// ini wajib menyatakannya (I5).
    pub truncation: Option<Truncation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Truncation {
    pub reason: &'static str,
    pub row_count_available: i64,
}

/// Cap baris yang berlaku untuk satu materialisasi.
///
/// `local_override` adalah seam `LOCAL_DATASET_MAX_ROWS` (FIN-46) yang hanya
/// diterima config di `APP_ENV=local`. Ia hanya **menyempitkan**: nilai di atas
/// [`DATASET_MAX_ROWS`] tidak pernah melebarkan batas makna runtime.md §4.
pub fn row_cap(local_override: Option<usize>) -> usize {
    local_override.map_or(DATASET_MAX_ROWS, |cap| cap.min(DATASET_MAX_ROWS))
}

/// Pecah baris menjadi chunk, dengan cap per-dataset ditegakkan di sini.
///
/// Dua dimensi (baris + byte) karena lebar baris Fineract bervariasi: chunk
/// kecil hanya menambah overhead tuple (chunk selalu dibaca utuh), chunk besar
/// membuat de-TOAST mahal untuk satu halaman UI (runtime.md §4).
pub fn materialize(rows: &[Map<String, Value>], max_rows: usize) -> Materialized {
    let total = rows.len() as i64;
    let mut chunks: Vec<Chunk> = Vec::new();
    let mut current: Vec<Value> = Vec::new();
    let mut current_bytes = 0usize;
    let mut row_from = 0i64;
    let mut stored = 0i64;
    let mut byte_size = 0i64;
    let mut truncation = None;

    for row in rows {
        let value = Value::Object(row.clone());
        let width = value.to_string().len();

        // Batas yang tercapai dinyatakan apa adanya (DS-8.1): menyebut cap
        // baris saat yang habis adalah cap byte adalah pernyataan yang keliru.
        if stored as usize >= max_rows {
            truncation = Some(ROW_CAP_REASON);
            break;
        }
        if byte_size as usize + width > DATASET_MAX_BYTES {
            truncation = Some(BYTE_CAP_REASON);
            break;
        }

        if current.len() >= CHUNK_ROWS || (!current.is_empty() && current_bytes + width > CHUNK_MAX_BYTES)
        {
            push_chunk(&mut chunks, &mut current, current_bytes, &mut row_from);
            current_bytes = 0;
        }

        current.push(value);
        current_bytes += width;
        stored += 1;
        byte_size += width as i64;
    }

    if !current.is_empty() {
        push_chunk(&mut chunks, &mut current, current_bytes, &mut row_from);
    }

    Materialized {
        chunks,
        row_count_available: stored,
        row_count_total: Some(total),
        truncation,
        byte_size,
    }
}

fn push_chunk(chunks: &mut Vec<Chunk>, current: &mut Vec<Value>, bytes: usize, row_from: &mut i64) {
    let rows = std::mem::take(current);
    let count = rows.len() as i64;

    chunks.push(Chunk {
        chunk_index: chunks.len() as i32,
        row_from: *row_from,
        // Setengah terbuka: `row_to` adalah baris pertama chunk berikutnya,
        // supaya cursor tidak pernah bergantung pada aritmetika ±1 pemanggil.
        row_to: *row_from + count,
        row_count: count as i32,
        byte_size: bytes as i64,
        payload: Value::Array(rows),
    });

    *row_from += count;
}

/// Klaim kelengkapan atas satu dataset yang **sah dinyatakan** (§5, I4).
///
/// `truncated=true` boleh hidup bersama `completeness=Complete` — tetapi hanya
/// bila batasnya dinyatakan. Tanpa alasan tertulis, "lengkap" atas set yang
/// dipotong adalah penghilangan senyap (I5).
pub fn claim_is_stated(truncated: bool, completeness: &str, reason: Option<&str>) -> bool {
    let stated = reason.is_some_and(|reason| !reason.trim().is_empty());

    match completeness {
        "Complete" => !truncated || stated,
        // `completeness_reason` wajib bila bukan `Complete`.
        "Partial" | "Unknown" => stated,
        _ => false,
    }
}

/// Alasan yang dipakai jalur tulis saat cap simpan tercapai — satu per batas.
pub const ROW_CAP_REASON: &str = "dataset_row_cap_reached";
pub const BYTE_CAP_REASON: &str = "dataset_byte_cap_reached";

/// Cursor keyset paginasi (§4): `(chunk_index, row)`.
///
/// `chunk_index` adalah komponen utama karena chunk adalah unit penyimpanan dan
/// PK-nya; `row` adalah ordinal absolut baris berikutnya di dalam urutan yang
/// dibekukan `sort_key_json`. Dituliskan `chunk:row` supaya klien
/// memperlakukannya sebagai token buram, bukan offset yang boleh diaritmetikakan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    pub chunk_index: i32,
    pub row: i64,
}

impl Cursor {
    pub const START: Self = Self { chunk_index: 0, row: 0 };

    pub fn parse(raw: &str) -> Option<Self> {
        let (chunk, row) = raw.split_once(':')?;
        let chunk_index = chunk.parse().ok()?;
        let row = row.parse().ok()?;

        if chunk_index < 0 || row < 0 {
            return None;
        }

        Some(Self { chunk_index, row })
    }

    pub fn encode(self) -> String {
        format!("{}:{}", self.chunk_index, self.row)
    }
}

/// Payload chunk yang tidak dapat dibaca runtime ini.
#[derive(Debug, PartialEq, Eq)]
pub struct UnsupportedEncoding {
    pub format: String,
    pub encoding_version: i32,
}

/// Baca payload satu chunk sesuai diskriminatornya.
///
/// Encoding yang tidak dikenal **ditolak dengan menyebut dirinya**, bukan
/// dianggap kosong: chunk yang ditulis versi lebih baru harus terbaca sebagai
/// "belum dapat dibaca di sini", bukan sebagai dataset yang kehilangan baris.
pub fn decode(
    format: &str,
    encoding_version: i32,
    payload: &Value,
) -> Result<Vec<Value>, UnsupportedEncoding> {
    match (format, encoding_version) {
        (FORMAT_JSON, ENCODING_VERSION) => Ok(payload.as_array().cloned().unwrap_or_default()),
        _ => Err(UnsupportedEncoding {
            format: format.to_string(),
            encoding_version,
        }),
    }
}

/// Kandidat yang dilihat reaper/eviction: handle beserta lifecycle job pemiliknya.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub dataset_id: Uuid,
    pub job_lifecycle: String,
}

/// Saring kandidat yang benar-benar boleh dilepas.
///
/// Dataset milik job **nonterminal** tidak pernah ikut, berapa pun umurnya
/// (#11, §6): job yang masih berjalan atau masih menunggu jawaban pengguna akan
/// membaca handle-nya kembali, dan membuang chunk-nya berarti membuat job hidup
/// menjawab atas data yang sudah tidak ada.
pub fn releasable(candidates: &[Candidate]) -> Vec<Uuid> {
    candidates
        .iter()
        .filter(|candidate| !NONTERMINAL.contains(&candidate.job_lifecycle.as_str()))
        .map(|candidate| candidate.dataset_id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rows(count: usize) -> Vec<Map<String, Value>> {
        (0..count)
            .map(|index| {
                let mut row = Map::new();
                row.insert("id".into(), json!(index));
                row
            })
            .collect()
    }

    #[test]
    fn ds_8_1_truncated_complete_is_only_legal_when_stated() {
        // DS-8.1 — `truncated=true` + `completeness=Complete` sah HANYA bila
        // batasannya dinyatakan.
        assert!(!claim_is_stated(true, "Complete", None));
        assert!(!claim_is_stated(true, "Complete", Some("   ")));
        assert!(claim_is_stated(true, "Complete", Some(ROW_CAP_REASON)));

        // Tidak terpotong: tidak ada yang perlu dinyatakan.
        assert!(claim_is_stated(false, "Complete", None));

        // Bukan `Complete` selalu menuntut alasan.
        assert!(!claim_is_stated(false, "Partial", None));
        assert!(claim_is_stated(false, "Partial", Some("input_truncated")));
        assert!(!claim_is_stated(false, "Unknown", None));
    }

    #[test]
    fn ds_8_1_truncated_is_not_completeness_is_not_preview() {
        // DS-8.1 — tiga dimensi terpisah: set tersimpan penuh, tetapi klaim
        // analitiknya boleh `Partial` (mis. input hilir hilang), dan sebaliknya.
        let materialized = materialize(&rows(10), DATASET_MAX_ROWS);
        assert!(!materialized.truncated());
        assert_eq!(materialized.row_count_available, 10);
        assert_eq!(materialized.row_count_total, Some(10));
        // `truncated=false` tidak dengan sendirinya mengesahkan `Complete`.
        assert!(claim_is_stated(false, "Partial", Some("upstream_partial")));
    }

    #[test]
    fn ds_8_1_row_cap_names_itself_and_only_narrows() {
        // DS-8.1 — cap yang tercapai dinyatakan dengan namanya, dan jumlah
        // aslinya tetap diketahui: `available < total` adalah buktinya.
        let materialized = materialize(&rows(5), 2);
        assert_eq!(materialized.truncation, Some(ROW_CAP_REASON));
        assert_eq!(materialized.row_count_available, 2);
        assert_eq!(materialized.row_count_total, Some(5));

        // Tepat pada cap bukan pemotongan.
        assert!(!materialize(&rows(2), 2).truncated());

        // Seam lokal hanya menyempitkan; tidak pernah melebarkan batas makna.
        assert_eq!(row_cap(None), DATASET_MAX_ROWS);
        assert_eq!(row_cap(Some(1)), 1);
        assert_eq!(row_cap(Some(DATASET_MAX_ROWS * 10)), DATASET_MAX_ROWS);
    }

    #[test]
    fn ds_8_2_handle_state_is_total_and_keeps_purged_readable() {
        // DS-8.2 — dataset `purged` masih terbaca STATUSNYA; tidak ada varian
        // "tidak tahu", jadi tidak ada jalur baca yang dapat melewatkannya.
        assert_eq!(HandleState::from_status("ready"), HandleState::Live);
        assert_eq!(HandleState::from_status("expired"), HandleState::Expired);
        assert_eq!(HandleState::from_status("purged"), HandleState::Purged);
        assert_eq!(HandleState::from_status("building"), HandleState::None);
        assert_eq!(HandleState::from_status("failed"), HandleState::None);
        // Status yang belum pernah ada pun punya jawaban, bukan panik.
        assert_eq!(HandleState::from_status("sesuatu"), HandleState::None);

        assert!(!HandleState::Purged.is_readable());
        assert!(!HandleState::Expired.is_readable());
        assert!(HandleState::Live.is_readable());

        // Dinyatakan pada response sebagai nilai, bukan sebagai ketiadaan field.
        assert_eq!(serde_json::to_value(HandleState::Purged).unwrap(), json!("purged"));
    }

    #[test]
    fn ds_8_3_chunking_freezes_a_stable_order() {
        // DS-8.3 — paginasi stabil: chunk menutupi seluruh baris sekali,
        // berurutan, tanpa tumpang tindih dan tanpa lubang.
        let materialized = materialize(&rows(CHUNK_ROWS * 2 + 5), DATASET_MAX_ROWS);
        assert_eq!(materialized.chunks.len(), 3);

        let mut expected_from = 0i64;
        for (index, chunk) in materialized.chunks.iter().enumerate() {
            assert_eq!(chunk.chunk_index, index as i32);
            assert_eq!(chunk.row_from, expected_from);
            assert_eq!(chunk.row_to, expected_from + i64::from(chunk.row_count));
            expected_from = chunk.row_to;
        }
        assert_eq!(expected_from, materialized.row_count_available);
    }

    #[test]
    fn ds_8_3_cursor_is_keyset_over_chunk_and_row() {
        // DS-8.3 — cursor menentukan posisi, bukan halaman ke-n.
        let cursor = Cursor::parse("2:2048").unwrap();
        assert_eq!(cursor.chunk_index, 2);
        assert_eq!(cursor.row, 2048);
        assert_eq!(cursor.encode(), "2:2048");
        assert_eq!(Cursor::parse(&Cursor::START.encode()), Some(Cursor::START));

        // Cursor rusak ditolak, bukan ditebak menjadi awal — menebak berarti
        // diam-diam mengulang halaman pertama.
        assert_eq!(Cursor::parse("2"), None);
        assert_eq!(Cursor::parse("-1:0"), None);
        assert_eq!(Cursor::parse("0:-5"), None);
        assert_eq!(Cursor::parse("a:b"), None);
    }

    #[test]
    fn ds_8_4_authorization_is_rechecked_not_inherited_from_the_handle() {
        // DS-8.4 — handle yang diketahui tidak mengesahkan apa pun.
        let owner = Uuid::new_v4();
        let other = Uuid::new_v4();

        assert_eq!(authorize(owner, &[1, 2], owner, &[1, 2, 3]), Ok(()));
        assert_eq!(authorize(owner, &[1], other, &[1]), Err(Denied::NotOwner));
        // Pemilik yang sama, tetapi otorisasinya menyempit sejak dataset dibuat.
        assert_eq!(
            authorize(owner, &[1, 9], owner, &[1, 2]),
            Err(Denied::ScopeNarrowed)
        );
        // Scope kosong pada handle tidak boleh menjadi celah "cocok dengan apa pun".
        assert_eq!(authorize(owner, &[], owner, &[]), Ok(()));
        assert_eq!(authorize(owner, &[1], owner, &[]), Err(Denied::ScopeNarrowed));
    }

    #[test]
    fn ds_8_5_encoding_discriminator_keeps_old_chunks_readable() {
        // DS-8.5 — `BYTEA` kelak cukup menambah nilai `format`/`encoding_version`
        // baru: chunk lama tetap terbaca, dan yang baru ditolak dengan menyebut
        // dirinya alih-alih dibaca sebagai dataset kosong.
        let payload = json!([{ "id": 1 }]);
        assert_eq!(decode(FORMAT_JSON, ENCODING_VERSION, &payload).unwrap().len(), 1);

        assert_eq!(
            decode("bytea_zstd", 2, &payload),
            Err(UnsupportedEncoding {
                format: "bytea_zstd".to_string(),
                encoding_version: 2,
            })
        );
        // Versi baru pada format yang sama juga bukan asumsi.
        assert!(decode(FORMAT_JSON, 2, &payload).is_err());
    }

    #[test]
    fn ds_8_6_release_never_touches_a_running_job() {
        // DS-8.6 — eviction/purge tidak menyentuh dataset job yang masih hidup,
        // berapa pun umurnya.
        let running = Uuid::new_v4();
        let waiting = Uuid::new_v4();
        let done = Uuid::new_v4();

        let candidates = vec![
            Candidate { dataset_id: running, job_lifecycle: "Running".into() },
            Candidate { dataset_id: waiting, job_lifecycle: "WaitingForUser".into() },
            Candidate { dataset_id: done, job_lifecycle: "Completed".into() },
        ];

        assert_eq!(releasable(&candidates), vec![done]);
    }

    #[test]
    fn ttl_never_dips_below_a_job_that_is_still_waiting() {
        // K5 — batas bawah keras, bukan kebetulan dua konstanta cocok.
        assert_eq!(ttl_secs(7_200, 1_800), DATASET_TTL_SECS);
        assert_eq!(ttl_secs(86_400, 1_800), 88_200);
    }

    #[test]
    fn a_chunk_closes_on_bytes_before_it_closes_on_rows() {
        let wide: Vec<Map<String, Value>> = (0..8)
            .map(|index| {
                let mut row = Map::new();
                row.insert("id".into(), json!(index));
                row.insert("blob".into(), json!("x".repeat(CHUNK_MAX_BYTES / 4)));
                row
            })
            .collect();

        let materialized = materialize(&wide, DATASET_MAX_ROWS);
        assert!(
            materialized.chunks.len() > 1,
            "chunk tidak ditutup oleh byte"
        );
        for chunk in &materialized.chunks {
            assert!((chunk.row_count as usize) < CHUNK_ROWS);
        }
    }

    #[test]
    fn empty_result_is_a_handle_without_chunks() {
        // Nol baris tetap menghasilkan handle: "tidak ada baris" adalah jawaban,
        // dan ia harus dapat dirujuk seperti jawaban lain.
        let materialized = materialize(&[], DATASET_MAX_ROWS);
        assert!(materialized.chunks.is_empty());
        assert_eq!(materialized.row_count_available, 0);
        assert_eq!(materialized.row_count_total, Some(0));
        assert!(!materialized.truncated());
    }
}
