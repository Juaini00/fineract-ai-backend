# Jarvis

Asisten analisis data berbahasa alami di atas core perbankan Apache Fineract. Admin bertanya dalam bahasa biasa; Jarvis menjawab dengan hasil terstruktur, penjelasan tergrounding, dan bukti yang dapat ditelusuri.

**Read-only terhadap Fineract.** Jarvis tidak pernah menulis ke Fineract dan tidak menjalankan simulasi — itu urusan engine Fineract. State aplikasi sendiri tersimpan di database terpisah.

> **Status: implementasi berjalan.** Fondasi (`core`), autentikasi, session dan penerimaan job (T1) sudah berjalan dan terverifikasi terhadap PostgreSQL nyata. Engine, klarifikasi, dataset, SSE dan response document belum ada. Lihat [docs/checklist.md](docs/checklist.md) sebelum menganggap sebuah bagian selesai.

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

- Engine: worker lease/fencing (T2), plan (T3), eksekusi node (T4), klarifikasi (T5–T6), response commit (T7–T8), reaper (T11).
- SSE `/chat/jobs/{id}/events`, dataset/handle, memori session, dan integrasi LLM/embedding.
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
