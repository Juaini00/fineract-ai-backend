# Status implementasi Jarvis

Diperbarui: **2026-09-15**, branch `feat/core-foundation`, commit `5aefc85`.

Dokumen ini menjawab satu pertanyaan: **apa yang sudah benar-benar berjalan, dan
apa yang berikutnya.** Ia berbeda dari [checklist.md](checklist.md), yang
melacak kelengkapan **dokumentasi** dan tidak pernah dimaksudkan sebagai status
build.

Aturan membaca:

- ✅ **Berjalan** — ada kodenya, ada testnya, dan dibuktikan terhadap Fineract
  lokal yang berisi data (8 office, 43 klien, 15.607 transaksi).
- 🟡 **Sebagian** — ada, tetapi dengan batasan yang dinyatakan di kolom catatan.
- ⬜ **Belum ada** — belum dibangun. Desainnya mungkin sudah tertulis; itu bukan
  hal yang sama.

Jangan menghitung persentase dari jumlah baris. Bobotnya tidak sama.

---

## 1. Ringkas

| | |
| --- | --- |
| Permukaan HTTP | 16 route / **17 operasi** (auth 4, session 4, job 5, klarifikasi 2, SSE 1, health 1) |
| Unit test | 93 pass (`cargo test --workspace`) |
| Integration test | 108 request / 165 test hijau (Bruno CLI, tiga tahap) |
| Lint | `cargo clippy --workspace -- -D warnings` bersih |
| Schema | 6 migrasi, 23 tabel, `tests/schema_smoke.sql` lulus |
| Katalog | 48 capability, 48 query manifest, 11 dataset, 69 file SQL — 0 error, 4 warning |

Bukti dijalankan ulang dengan:

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo run -p app -- catalog
psql -v ON_ERROR_STOP=1 -d "$APP_DATABASE_URL" -f tests/schema_smoke.sql
PORT=3107 ./scripts/integration-test.sh
./scripts/docs-check.sh            # link mati + endpoint yang tidak terdokumentasi
```

`scripts/docs-check.sh` memeriksa dua hal yang paling cepat membusuk: link
antar-dokumen yang menunjuk file tidak ada, dan endpoint yang terdaftar di kode
tetapi tidak muncul di `contracts/api-reference.md`. Ia tidak memeriksa
kebenaran prosa — tidak ada yang bisa.

---

## 2. Yang berjalan

### Fondasi

| Bagian | Status | Catatan |
| --- | --- | --- |
| Config tervalidasi | ✅ | Fail fast dengan nama variabelnya; rahasia tanpa default produksi |
| Dua pool PostgreSQL terpisah | ✅ | `APP_DATABASE_URL` writable, `FINERACT_DATABASE_URL` read-only; URL identik ditolak saat boot |
| Redis opsional | ✅ | Mati = degraded, bukan gagal boot |
| Envelope `{success,data,error}` | ✅ | Error publik tersanitasi |
| Tracing | ✅ | — |
| `/health` | ✅ | Melaporkan status tiap dependency terpisah |

### Autentikasi

| Bagian | Status | Catatan |
| --- | --- | --- |
| Login, logout, `/auth/me` | ✅ | Argon2 |
| JWT HS256 | ✅ | Access di body, refresh sebagai cookie `HttpOnly` |
| Rotasi refresh + deteksi reuse | ✅ | Reuse mematikan seluruh rantai token pada auth session itu |
| Pesan 401 seragam | ✅ | Password salah dan user tak dikenal dibandingkan byte-per-byte di test |
| SSO / identitas dashboard | ⬜ | HS256 hanya sah selama Jarvis penerbit sekaligus verifier. `docs/security/access-data-policy.md` belum ada |

### Session dan job

| Bagian | Status | Catatan |
| --- | --- | --- |
| Session CRUD + keyset pagination | ✅ | — |
| T1 penerimaan job | ✅ | Idempotency, snapshot scope+PII, audit, event — satu transaksi |
| Satu job nonterminal per session | ✅ | Ditegakkan partial unique index, bukan pemeriksaan aplikasi |
| Cancel (T9) | ✅ | Cancel berulang no-op, bukan transisi kedua |
| Riwayat pesan | ✅ | `GET /chat/sessions/{id}/messages`, keyset terbaru-dulu. Indeks tipis: `request_text` dibaca dari `chat_jobs`, isi jawaban/form tetap di endpointnya sendiri |

### Engine

| Bagian | Status | Catatan |
| --- | --- | --- |
| Klaim + lease + fencing (T2, C16) | ✅ | `FOR UPDATE SKIP LOCKED`; heartbeat dari task Tokio independen (K1) |
| Reaper (T11) | ✅ | Expired → cancelling terbengkalai → requeue lease hilang, dalam urutan itu |
| Planner deterministik (T3) | 🟡 | Retrieval leksikal atas `knowledge_index`; **satu node `CuratedQuery`** per plan. Tanpa model |
| Eksekusi capability (T4) | ✅ | SQL dari `queries/`, parameter terikat, timeout dua sisi, di luar transaksi (I1) |
| Komposisi deterministik | 🟡 | Blok `metrics`/`table`/`narrative`/`provenance`/`limitation`. Belum ada `chart`/`findings`/`comparison`/`suggestions` |
| Commit response (T7) | ✅ | Response + lifecycle + fakta memori + pesan + event + audit, satu transaksi |
| Promosi `session_memory` (C12/K4) | 🟡 | `ActiveScope`, `PriorResult`, `ResolvedEntity` ditulis pada T7; seq lewat row lock (I3), fakta lama di-supersede, ringkasan → `stale`. **Belum ada konsumennya**: seleksi konteks menunggu integrasi LLM |
| Re-plan / multi-node / fan-in | ⬜ | `plan_version` selalu 1 |

### Klarifikasi

| Bagian | Status | Catatan |
| --- | --- | --- |
| Form bertipe (T5/T6) | ✅ | Immutable per revision; stale revision ditolak 409 |
| Default tanggal relatif | ✅ | `business_today - 12m` dihitung, bukan ditanyakan |
| Resolver opsi `single_choice` | ✅ | Berpaginasi, terikat form+field; hanya halaman yang dikirim yang dicatat |
| Auto-bind `resolver_unique` (K5) | ✅ | Dibedakan dari `user_confirmed`; diungkap sebagai blok `slots_auto_bound` |
| Cek keanggotaan + scope saat submit | ✅ | Dua pemeriksaan terpisah; keanggotaan bukan otorisasi |
| Skip (T8) | ✅ | `Completed`/`SkippedByUser`, `kind='skipped'`, `Complete` dilarang |
| Klarifikasi bertahap | ⬜ | Form kedua sesudah slot pertama terjawab |
| `refine_search`, `change_intent` | ⬜ | Kolom `answer_kind` sudah mengizinkannya; jalurnya belum ada |

### SSE

| Bagian | Status | Catatan |
| --- | --- | --- |
| `GET /chat/jobs/{id}/events` | ✅ | Bearer dari header; tidak pernah dari URL |
| Replay dari cursor | ✅ | `Last-Event-ID` atau `?cursor=`; cursor tidak sah gagal eksplisit |
| Duplikat aman | ✅ | Diuji: pembacaan ulang menghasilkan id yang sama persis |
| Notifikasi | ✅ | `pg_notify` di dalam `append_event` (transaksional) + Redis untuk lintas instance |
| Fallback polling | ✅ | 2s→15s. Dibuktikan dengan `SSE_NOTIFICATIONS_ENABLED=false`: assertion identik |
| Disconnect ≠ cancel | ✅ | Diuji terhadap job `WaitingForUser` |
| `job.resumed` | ✅ | Dipancarkan saat worker mengklaim kelanjutan, bukan saat jawaban diterima |
| Auth expiry di tengah stream | ⬜ | Token kedaluwarsa belum menghentikan delivery |

### Katalog

| Bagian | Status | Catatan |
| --- | --- | --- |
| Loader + `content_hash` | ✅ | Identitas katalog adalah hash isi, bukan string versi |
| Validator statis | ✅ | SELECT-only, single statement, placeholder, office binding, sensitivity, resolver, probe |
| Probe SQL ke Fineract | ✅ | `cargo run -p app -- catalog` menyiapkan tiap SQL terhadap schema nyata |
| `knowledge_index` untuk retrieval | ✅ | Leksikal; arm embedding belum ada |
| Review isi katalog | ⬜ | `knowledge/` dan `queries/` masih carry-over. Memuat YAML membuktikan bentuk, bukan kebenaran angka |

---

## 3. Yang belum ada

Urut menurut apa yang paling menghalangi integrasi frontend penuh.

| # | Bagian | Kenapa penting | Pemilik desain |
| --- | --- | --- | --- |
| 1 | **Konsumsi session memory** | Fakta sudah dipromosikan, tetapi belum ada yang membacanya: seleksi konteks per model call (memory-context.md §5) menunggu integrasi LLM | `architecture/memory-context.md` |
| 2 | **Integrasi LLM** | Planner dan composer deterministik. Narasi additive belum ada | `architecture/tech-stack.md` |
| 3 | **Plan multi-node + fan-in** | Setiap pertanyaan menjadi tepat satu query. Pertanyaan komparatif tidak dapat direncanakan | `architecture/engine.md` |
| 4 | **Dataset berchunk + handle** | Hasil besar belum punya jalur; tidak ada pagination hasil | `data/dataset-lifecycle.md` |
| 5 | **Analytical contract (Mode 2)** | Hanya capability tetap yang dapat dijalankan | `data/analytical-contracts.md` |
| 6 | **Blok response lanjutan** | `chart`, `findings`, `comparison`, `suggestions` belum dipancarkan | `contracts/responses.md` |
| 7 | **Validator response (D1–D3)** | Hitung ulang completeness dan lineage evidence belum ditegakkan runtime | `contracts/responses.md` |
| 8 | **Klarifikasi bertahap** | Form kedua sesudah slot pertama terjawab | `contracts/clarifications.md` |
| 9 | **Embedding retrieval** | Retrieval masih leksikal; fail-closed ke leksikal sudah dirancang | `migration/carry-over.md` #7 |
| 10 | **Security/identity final** | SSO, tenant model, izin PII per pengguna | `security/access-data-policy.md` (belum ada) |
| 11 | **Observability** | Metrics, traces, alerting, exporter | `operations/observability.md` (belum ada) |
| 12 | **Acceptance matrix** | Requirement → skenario → hasil terukur | `verification/acceptance.md` (belum ada) |
| 13 | **OpenAPI** | Schema formal; FE masih memakai `contracts/api-reference.md` | `contracts/api.md` |

---

## 4. Utang yang diketahui

Bukan "belum sempat" — ini keputusan sadar yang punya alasan dan pemicu revisi.

| Utang | Keadaan sekarang | Kapan wajib diselesaikan |
| --- | --- | --- |
| Resolver tanpa konsumen | `organization.office_candidates`, `savings.charge_type_candidates`, `group.group_candidates` tidak ditunjuk satu pun `probe:`. Validator melaporkannya sebagai warning | Saat ada capability yang benar-benar memerlukan slot itu. Jangan mengarang `probe:` hanya agar warning hilang |
| `account_number` tanpa resolver | Dijawab `Unsupported` dengan `identity_slot_without_resolver` | Saat katalog punya shape kandidat rekening yang disetujui |
| Shape `role: resolver` tanpa manifest | `savings.products/products_by_client`, `savings.transactions/activity_rows` | Review carry-over katalog: apakah `role`-nya memang salah label |
| Paginasi resolver di memori | Dibatasi `RESOLVER_MAX_CANDIDATES=500`, kelebihannya dinyatakan `truncated` | Saat sebuah resolver rutin menyentuh batas itu pada halaman pertama → beri resolver itu keyset SQL-side, jangan naikkan angkanya |
| Notifikasi memakai `pg_notify`, bukan Redis | Redis dipakai untuk fan-out lintas instance saja. `pg_notify` dipilih karena transaksional dan tidak dapat terlupakan | Bila deployment memakai pooler mode transaction: jalankan `SSE_NOTIFICATIONS_ENABLED=false` (didukung, diuji) |
| Idempotency untuk `job.respond`/`job.skip` | Header divalidasi tetapi belum disimpan. Yang menjaga adalah `WHERE state='open'` di database | Saat klien mulai melakukan retry otomatis pada endpoint itu |
| D4 kesegaran katalog | `catalog_version_id` direkam saat plan; belum ada pemeriksaan katalog tidak berubah antara verifikasi dan eksekusi | Saat katalog dapat berubah tanpa restart |
| Angka runtime | Seluruhnya nilai awal, bukan hasil tuning | Tiap angka punya pemicu revisi terukur di `operations/runtime.md` |

---

## 5. Urutan yang disarankan berikutnya

Dependensi, bukan prioritas produk.

1. **Validator response (D1–D3).** Sebelum blok bertambah banyak — menambahkan
   validator sesudahnya berarti memvalidasi permukaan yang sudah menyebar.
2. **Plan multi-node + fan-in**, lalu **dataset berchunk**. Keduanya mengubah
   arti `completeness`, jadi keduanya menunggu validator di atas.
3. **Integrasi LLM** sebagai lapisan additive: kegagalannya tidak boleh
   menghapus structured output. Ia sekaligus konsumen pertama
   `session_memory` — fakta sudah ada, yang belum ada adalah seleksi
   konteksnya.
4. **Security/identity final**, sebelum deployment nyata.

---

## 6. Menjalankan lokal

```bash
cd fineract-ai-backend
docker compose up -d                                    # Redis + PostgreSQL
sqlx migrate run --database-url "$APP_DATABASE_URL"
APP_PORT=3107 cargo run -p app                          # 3007 dipakai repo lama
```

Port 3007 dan Redis 6380 dipakai backend lama pada mesin pengembangan ini;
jalankan dengan `APP_PORT=3107`.

Frontend: mulai dari [contracts/api-reference.md](contracts/api-reference.md) —
itu permukaan yang benar-benar ada, dengan payload yang disalin dari aplikasi
berjalan.
