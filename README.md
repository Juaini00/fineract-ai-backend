# Jarvis

Asisten analisis data berbahasa alami di atas core perbankan Apache Fineract. Admin bertanya dalam bahasa biasa; Jarvis menjawab dengan hasil terstruktur, penjelasan tergrounding, dan bukti yang dapat ditelusuri.

**Read-only terhadap Fineract.** Jarvis tidak pernah menulis ke Fineract dan tidak menjalankan simulasi — itu urusan engine Fineract. State aplikasi sendiri tersimpan di database terpisah.

> **Status: menjawab pertanyaan nyata.** Fondasi, autentikasi, session, penerimaan job (T1), validator katalog, siklus hidup job (T2/T7/T11), planner deterministik (T3), dan eksekusi capability yang disetujui ke Fineract (T4) sudah berjalan dan terverifikasi terhadap data nyata. Pertanyaan yang tercakup capability dijawab dengan angka + provenance; yang tidak tercakup ditolak sebagai `Unsupported` dengan sebab yang dinyatakan. Klarifikasi bertipe (T5–T6) juga berjalan: input yang kurang ditanyakan pada job yang sama, bukan ditolak. Resolver opsi, dataset berchunk, memori session, SSE dan integrasi LLM belum ada. Lihat [docs/checklist.md](docs/checklist.md) sebelum menganggap sebuah bagian selesai.

## Dokumen

Baca berurutan:

| Dokumen | Isi |
| --- | --- |
| [Product requirements](docs/product/prd.md) | Batasan produk, scope, dan kriteria keberhasilan |
| [Engine](docs/architecture/engine.md) | Lifecycle job, status, dan recovery |
| [Database design](docs/data/database-design.md) | **Invarian, matriks koneksi, ERD, batas transaksi** |
| [Carry-over](docs/migration/carry-over.md) | Keputusan #1–#15 dan alasannya |
| [Runtime](docs/operations/runtime.md) | Parameter operasional + pemicu revisi |
| [API](docs/contracts/api.md) · [SSE](docs/contracts/sse.md) · [Klarifikasi](docs/contracts/clarifications.md) · [Responses](docs/contracts/responses.md) | Kontrak antarmuka |
| [Checklist](docs/checklist.md) | Apa yang sudah dan belum selesai |

Kalau hanya sempat membaca satu, baca **§1 Invarian** dan **§2 Matriks koneksi** pada `database-design.md`. Keduanya adalah aturan yang membuat bagian-bagian sistem ini tetap terhubung.

## Setup lokal

```bash
cp .env.example .env          # lalu isi LLM_API_KEY dan EMBEDDING_API_KEY
docker compose up -d          # Redis + PostgreSQL (pgvector)
cargo check --workspace

# Pasang schema
sqlx migrate run --database-url "$APP_DATABASE_URL"

# Buktikan schema berperilaku benar, bukan sekadar terpasang
psql -v ON_ERROR_STOP=1 -d "$APP_DATABASE_URL" -f tests/schema_smoke.sql
```

Sudah punya PostgreSQL sendiri? Jalankan `docker compose up -d redis` saja, lalu sesuaikan `APP_DATABASE_URL` dan `FINERACT_DATABASE_URL`. Ekstensi `vector` diperlukan.

`.env` **tidak pernah** di-commit. Hanya `.env.example` yang masuk repo.

### `tests/schema_smoke.sql`

Menguji **perilaku**, bukan sekadar DDL berhasil di-parse. Ia gagal keras bila salah satu ini rusak: satu job nonterminal per session, `Completed` tanpa `outcome`, teks bebas menjadi binding identitas, dua form terbuka, memori tanpa response durable (K4), dua ActiveScope aktif, audit yang bisa di-UPDATE atau tidak bisa dipurge, **penghapusan session yang gagal karena referential action yang salah**, seed PII fail-closed, dan koreksi kurs yang menimpa alih-alih menambah baris.

Jalankan setiap kali migrasi berubah.

## Integration test

**Tanpa test integrasi di dalam Rust — ini keputusan, bukan kebetulan.**
Permukaan HTTP diuji sebagai HTTP lewat [Bruno CLI](https://docs.usebruno.com)
terhadap aplikasi yang benar-benar berjalan dan PostgreSQL yang benar-benar
dimigrasi. `cargo test` hanya untuk logika murni (config, token, hashing,
binding parameter, komposisi blok). Aturan lengkap beserta format berkasnya ada
di [AGENTS.md](AGENTS.md#integration-test-bruno-cli-bukan-test-integrasi-di-dalam-rust).

```bash
npm install -g @usebruno/cli     # sekali saja
./scripts/integration-test.sh    # katalog, lalu dua tahap Bruno
./scripts/integration-test.sh auth            # satu folder saja
./scripts/integration-test.sh engine          # tahap engine saja
PORT=3210 ./scripts/integration-test.sh       # port lain bila 3107 dipakai
KEEP_RUNNING=1 ./scripts/integration-test.sh  # biarkan app hidup untuk debug
```

Koleksi ada di `fineract-assistant-api/` (format OpenCollection 1.0):
`health/`, `auth/`, `chat/`, `engine/`, `clarification/`. Request di dalam satu folder
**berurutan dan saling bergantung** — rotasi refresh token hanya dapat diuji
setelah login, dan deteksi pemakaian ulang hanya setelah rotasi.

Runner menjalankannya dalam **dua tahap**. `health`/`auth`/`chat` berjalan
dengan `WORKER_ENABLED=false` karena folder `chat` menguji semantik penerimaan
(job tetap `Queued`, satu job nonterminal per session); dengan worker menyala,
job selesai dalam milidetik dan hasil test bergantung pada balapan, bukan pada
perilaku yang diuji. `engine`, `clarification`, `resolver` dan `sse` berjalan dengan worker
menyala untuk membuktikan job bergerak sampai terminal tanpa campur tangan
klien, dan bahwa job yang ditangguhkan melanjutkan setelah dijawab.

Runner menunggu `/health` benar-benar `200`, bukan sekadar port terbuka: port
yang sudah menerima koneksi sementara PostgreSQL belum terjangkau menghasilkan
kegagalan test yang menyesatkan. Ia juga mematikan app dengan SIGTERM, sehingga
jalur graceful shutdown ikut terlatih setiap run.

## Struktur

```
crates/app      entrypoint biner dan composition root
crates/core     fondasi: config, tracing, pool DB, Redis, envelope, auth
crates/chat     fitur pelaporan: job, plan, ledger, klarifikasi, dataset, memori, response
knowledge/      katalog YAML  — DIBAWA APA ADANYA, belum direview
queries/        SQL approved  — DIBAWA APA ADANYA, belum direview
docs/           paket desain
```

Tiga crate, dan jumlahnya tetap tiga. Nama singkat, tanpa awalan `ai_report_*`.

`knowledge/` dan `queries/` disalin dari repo lama dan **belum menjadi sumber kebenaran** — baca `CARRY-OVER.md` di masing-masing folder sebelum memercayai isinya.

## Yang belum ada

- Resolver untuk `account_number`: slot itu bersumber `transient_sensitive_input` dan katalog belum punya shape kandidat rekening, jadi permintaan yang memerlukannya dijawab `Unsupported` dengan alasan `identity_slot_without_resolver` — bukan ditanyakan sebagai teks bebas (K1).
- Klarifikasi bertahap: form kedua setelah sebuah slot terjawab.
- Plan multi-node dan fan-in: planner menghasilkan tepat satu node `CuratedQuery`.
- Analytical contract (Mode 2), dataset berchunk/handle, memori session.
- Integrasi LLM/embedding (narasi additive), dataset berchunk, dan session memory.
- `docs/security/access-data-policy.md`, `docs/operations/observability.md`, `docs/verification/acceptance.md`.

## Aturan yang tidak boleh dilanggar

Dirangkum dari `database-design.md` §1:

1. Tidak ada panggilan eksternal di dalam transaksi commit.
2. Setiap referential action diperiksa terhadap rantai CASCADE session.
3. Sequence dialokasikan lewat row lock, bukan sequence PostgreSQL.
4. Ketidakpastian harus terlihat di data.
5. Tidak ada penghilangan senyap — truncation, expiry, PII mati, auto-bind, skip: semuanya wajib dinyatakan.
6. Setiap hand-off antar tahap ditegakkan constraint, bukan konvensi.
7. Otorisasi tidak pernah dibaca dari state turunan.
8. Audit hidup lebih lama daripada isinya dan tidak punya FK.

Startup aplikasi tidak pernah membuat atau mengubah tabel. Schema hanya berubah lewat `migrations/*.sql`.
