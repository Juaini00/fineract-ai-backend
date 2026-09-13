# Siklus hidup dataset

Status: disepakati struktur lifecycle, 2026-09-13. Pemilik handle dataset, storage, snapshot/freshness, pagination, completeness, expiry, dan otorisasi baca. Angka pasti (ukuran chunk, TTL, cap) adalah **nilai awal** dari [runtime.md](../operations/runtime.md) §4 dan menunggu uji kapasitas — dokumen ini menetapkan *aturan lifecycle*, bukan hasil tuning.

Sumber kebenaran: definisi tabel `datasets`/`dataset_chunks` (#11) dan batas transaksi ada di [database-design.md](database-design.md); invarian I4 (ketidakpastian terlihat) dan I7 (otorisasi tidak dari state turunan) berlaku di sini.

---

## 1. Model: handle + chunk

Satu dataset = satu baris `datasets` (handle) + nol-atau-lebih baris `dataset_chunks`.

| Konsep | Tabel | Peran |
| --- | --- | --- |
| handle | `datasets` | identitas, grain, scope, provenance, completeness, status, TTL |
| chunk | `dataset_chunks` | payload baris, `(dataset_id, chunk_index)` PK, CASCADE |

- **Immutable setelah `ready`.** Koreksi/refresh = **dataset baru**, bukan UPDATE (C5: `job_node_runs.dataset_id` menunjuk satu handle; reproduksi menuntut snapshot tidak berubah).
- **Purge tidak menghapus baris handle** — hanya chunk-nya; `status` berpindah ke `purged`. Baris tetap ada karena `session_memory.dataset_id` (C13) dan `job_node_runs.dataset_id` (NO ACTION) merujuknya agar `handle_state` dapat dinyatakan.
- `row_count_total = NULL` berarti **tidak diketahui**, bukan nol (I4).

---

## 2. Storage chunk

| Parameter | Nilai awal | Aturan |
| --- | --- | --- |
| `CHUNK_ROWS` | 1.000 | tutup chunk pada baris ini |
| `CHUNK_MAX_BYTES` | 1 MB | …atau pada byte ini, mana lebih dulu |
| `format` | `json` | diskriminator sejak awal |
| `encoding_version` | 1 | agar pindah ke `BYTEA` terkompresi = nilai baru, chunk lama tetap terbaca |

Alasan dua dimensi (baris + byte) dan ambang TOAST dicatat di runtime.md §4; ringkasnya: chunk kecil = overhead tuple tanpa untung (chunk selalu dibaca utuh), chunk besar = de-TOAST mahal untuk satu halaman UI. `JSONB`→`BYTEA` adalah **optimasi kelak, bukan koreksi**.

---

## 3. Penciptaan dan snapshot

1. Sebuah node menghasilkan baris (di luar transaksi commit — I1).
2. Baris ditulis sebagai chunk `dataset_chunks`; `datasets` diberi `schema_json`, `grain_json`, `scope_json`, `provenance_json`, `sort_key_json`, `completeness`, `truncated`, `row_count_*`.
3. `status` berpindah `building` → `ready` (atau `failed`); `byte_size`/`chunk_count` dicatat.
4. `provenance_json` merekam capability/kontrak + `catalog_version_id` + `catalog_content_hash` + `as_of` + `exchange_rate_id` bila ada (C18/C19).

**Snapshot vs freshness**: dataset adalah materialisasi pada saat pembuatan; `as_of` menyatakan titik datanya. Kesegaran *sumber* terpisah dari konsistensi *transaksi aplikasi* (design-review.md): query paralel/resume tidak inheren berbagi snapshot sumber. Dataset yang kedaluwarsa/kadaluwarsa adalah **retrieval baru**, bukan sambungan diam-diam (PRD §8).

---

## 4. Pagination dan otorisasi baca

- Urutan stabil dinyatakan `sort_key_json`; pagination keyset di atas `(dataset_id, chunk_index)`.
- **Otorisasi dicek ulang pada setiap pembacaan dataset** (I7): `owner_user_id` difilter, dan scope sumber diverifikasi ulang. Handle bukan token otorisasi — ia referensi, bukan grant.
- Hasil besar **tidak** disalin wholesale ke checkpoint JSON, pesan model, atau frame SSE (PRD §6).

---

## 5. Completeness, truncated, preview — tiga hal berbeda (I4)

| Dimensi | Tempat | Makna |
| --- | --- | --- |
| `truncated` | `datasets` | set yang **tersimpan** dibatasi (cap baris/byte) |
| `completeness` | `datasets` + `job_responses` | kelengkapan **analitik** (`Complete/Partial/Unknown`) |
| preview | `job_responses` blok `table` | pemotongan **tampilan** sisi response |

Ketiganya tidak boleh disamakan: total lengkap bisa hidup bersama preview terpotong; `truncated=true` boleh tetap `completeness=Complete` bila batasan dinyatakan dan tidak memengaruhi jawaban; `completeness=Partial/Unknown` karena input hilang/terpotong mencegah klaim `Complete` untuk output turunan yang terpengaruh (PRD §6, responses.md §3). `completeness_reason` wajib bila bukan `Complete`.

---

## 6. Expiry dan purge

- `DATASET_TTL_SECS` (awal 24 jam) ≥ `CLARIFICATION_WAIT_LIMIT + JOB_TTL_RUNNING` (K5). `expires_at` di-set saat `ready`.
- Reaper (T11, idempoten): dataset melewati TTL → hapus chunk, pertahankan baris, `status='purged'`, `purged_at`.
- Eviction kuota (`SESSION_RETAINED_BYTES_QUOTA`, LRU) **dilarang** menyentuh dataset milik job nonterminal berapa pun umurnya (#11). Eviction pernah berjalan → selidiki reaper dulu (runtime.md §4).
- `JOB_RETAINED_BYTES_CAP` diverifikasi **saat plan** (K16): plan yang melebihi ditolak sebelum dijalankan, bukan saat sudah terlanjur menyimpan.

---

## 7. Pengungkapan handle kedaluwarsa (tanpa penghilangan senyap — I5)

Bila blok `table` merujuk dataset yang `expired`/`purged`, response wajib menyatakan "detail data sudah kedaluwarsa" dan angka ringkas tetap terbaca dengan `as_of` (responses.md §7, #11 aturan 6). Pertanyaan lanjutan menjadi **retrieval baru** — tidak ada auto-reuse dari handle mati, tidak ada `pinned` dataset (K13).

---

## 8. Skenario acceptance

- `truncated=true` + `completeness=Complete` sah hanya bila batasan dinyatakan dan tidak mengubah jawaban.
- Dataset `purged` masih terbaca statusnya (handle_state non-optional) dan dinyatakan kedaluwarsa di response.
- Pagination stabil: halaman berbeda tidak mengubah urutan; `sort_key_json` menentukan.
- Baca ulang dataset oleh user lain/scope berbeda ditolak walau handle-nya diketahui.
- Chunk `BYTEA` kelak ditambahkan tanpa migrasi (cukup `format`/`encoding_version` baru).
- Eviction tidak menyentuh dataset job yang masih berjalan.

## 9. Terbuka

- Format payload chunk final (JSONB vs BYTEA) — menunggu uji kapasitas (database-design.md §8).
- Nilai `CHUNK_ROWS/MAX_BYTES`, `DATASET_MAX_ROWS/BYTES`, `SESSION_RETAINED_BYTES_QUOTA` — menunggu ukuran data Fineract nyata.
- Konsistensi/snapshot policy lintas query paralel (design-review.md).
- Retensi/kuota per-session yang terukur setelah rilis terbatas.
