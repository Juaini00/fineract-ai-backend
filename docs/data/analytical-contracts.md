# Kontrak analitik (analytical contracts)

Status: disepakati struktur kontrak, 2026-09-13. Pemilik lapisan semantik Mode 2 dan penegakan **D5** ([database-design.md](database-design.md) §2.2). Skema field/measure konkret per domain **belum final** — ia menunggu verifikasi deployment (dataset-scope-decisions.md) dan **bukan** bagian yang dokumen ini klaim selesai. Yang dokumen ini tetapkan adalah *bentuk* kontrak, *aturan validasi*, *grain*, *siklus katalog*, dan *penegakan scope* — agar kontrak pertama yang ditulis kelak punya satu rumah aturan, bukan aturan per domain.

Sumber kebenaran: bila bertentangan dengan [database-design.md](database-design.md) atau [carry-over.md](../migration/carry-over.md), dokumen itu yang menang dan dokumen ini diperbaiki.

---

## 1. Dua mode eksekusi, satu lapisan keselamatan

PRD §5 menetapkan urutan pilihan:

1. **Mode 1 — curated capability**: query pre-authored yang sudah disetujui (`queries/*.sql`), dipilih lewat retrieval atas `capabilities/*.yaml` + `queries/*.yaml`.
2. **Mode 2 — analytical contract**: kontrak deklaratif (entity, field, measure, relasi) yang dikompilasi engine menjadi SQL berparameter.
3. Bila keduanya tidak ada → **`Unsupported`**, tanpa fallback ke schema bebas.

Keduanya melewati **guard yang sama** di §4. Mode 2 bukan "jalan bebas"; ia adalah Mode 1 yang query-nya dirangkai pada runtime dari kontrak yang sudah diverifikasi. Tidak ada SQL yang dikarang model (PRD §2, AGENTS.md).

---

## 2. Kosakata kontrak

Sebuah kontrak menyatakan, minimal:

| Elemen | Isi | Catatan |
| --- | --- | --- |
| `entity` | jenis entitas + `id_field` + `label_fields` | contoh carried-over: `savings_account` |
| `field` | nama, tipe, kelas sensitivitas | kelas tunggal dari §2.1 |
| `usage` | per field: `selectable` / `filterable` / `groupable` / `aggregatable` | tidak semua field boleh semua operasi |
| `relationship` | antar entitas + cardinality | mencegah duplikasi measure (§5) |
| `measure` | agregasi berdeklarasi **grain** + currency + rounding | §5 |
| `office_scope_path` | jalur dari entitas ke `office_id` | penegakan §6 |
| `cap` | batas resource per kontrak | §4.4 |

Aset carried-over `knowledge/datasets/*.yaml`, `knowledge/schema/fineract/*.yaml`, dan `knowledge/metrics/*.yaml` adalah **calon** kosakata ini. Ia dibawa apa adanya dan **belum direview** ([knowledge/CARRY-OVER.md](../../knowledge/CARRY-OVER.md)): memuat YAML membuktikan sintaks, bukan kebenaran grain/relasi/sensitivitas.

### 2.1 Kelas sensitivitas

Dari `knowledge/schema/fineract/columns/sensitivity.yaml` (calon, belum direview ulang untuk kebijakan PII #15):

| Kelas | Perilaku default | Contoh |
| --- | --- | --- |
| `public_business` | izinkan bila capability/kontrak menyatakannya | `office_name`, `amount`, `currency_code` |
| `sensitive_business_identifier` | `exclude` | `account_no`, `external_id`, `ref_no` |
| `pii` | butuh `can_view_pii` + persetujuan kontrak | `display_name`, `mobile_no` |
| `security_sensitive` | `exclude` | `username`, `role`, `client_ip` |
| `secret_never_expose` | `never_expose` (tidak di schema prompt, output, log, response) | `password`, `token` |
| `free_text_sensitive` | `exclude` | `description`, `reason_for_block` |

Klasifikasi *kolom mana* masuk kelas mana adalah utang audit sebelum PII dinyalakan (#15). Dokumen ini menetapkan **aturan penerapan kelas**, bukan daftar kolom final.

### 2.2 Relasi dan cardinality

Relasi menyatakan cardinality (`1:1`, `1:N`, `N:1`). Aturan anti-fanout PRD §6: join `1:N` tidak boleh menggandakan measure sisi `1`. Kompiler wajib menolak plan yang menggabungkan measure pada grain berbeda tanpa menyatakan grain hasilnya — atau memisahkannya menjadi dua dataset bernode berbeda.

---

## 3. Siklus katalog (versioning + retrieval)

Tabel: `knowledge_catalog_versions` (append-only) + `knowledge_index` (#7). Identitas nyata katalog adalah **`content_hash`**, bukan kolom `version` yang di sistem lama selalu "local".

### 3.1 Append-only + retensi

- Versi yang dirujuk `job_plans.contract_versions_json` / `job_node_runs.provenance_json` / `audit_events` **tidak boleh pernah dihapus**.
- Purge hanya boleh menyentuh versi yang tidak lagi dirujuk (retensi "N terbaru + semua yang dirujuk").
- Setiap perubahan katalog = baris versi baru, bukan update.

### 3.2 Retrieval

- Arm embedding aktif hanya bila `embedding_model` + `embedding_dimensions` + `embedding_input_type` **sama** dengan yang terindeks; tidak cocok → **fail-closed ke arm leksikal** (exact search). Tidak ada ANN index pada volume ini (dibuang di #7); GIN `metadata_json` + exact scan.
- Satu klien embedding, satu jalur kode untuk dokumen vs query; beda hanya `input_type` (.env.example).

### 3.3 Verifikasi kontrak sebelum sah

Empat syarat dari `knowledge/CARRY-OVER.md`, diberlakukan validator katalog:

1. Contoh dijalankan ujung-ke-ujung dan angkanya diperiksa — memuat YAML + `PREPARE` **tidak** membuktikan apa pun.
2. Prosa (title/description) konsisten dengan SQL/relasi yang dirujuk.
3. Kelas sensitivitas kolom output sesuai kebijakan PII berlaku.
4. Semantik cutoff/`as_of` dideklarasikan (D4).

Entri yang belum lolos keempatnya boleh dipakai pengembangan lokal, **tidak** untuk menjawab pertanyaan yang dipercaya.

---

## 4. Kompilasi dan validasi

Alur: **spesifikasi analitik (keluaran model terstruktur) → validasi statis → validasi berbasis-DB → execution guard → eksekusi**. D02/D05 pada design-review.md menetapkan urutan ini; validasi statis mendahului eksekusi sumber, dan `PREPARE` terhadap DB adalah langkah terpisah yang dibatasi.

### 4.1 Spesifikasi analitik

Model menghasilkan spesifikasi terstruktur (bukan SQL): entitas + field yang dipilih + filter + grouping + measure + sort + limit. Spec ini dikompilasi engine menjadi SQL berparameter.

### 4.2 Validasi statis (sebelum menyentuh sumber)

- Setiap field/measure/relasi yang dirujuk harus ada di katalog versi aktif.
- `usage` tiap field dicocokkan dengan operasinya (mis. field non-`aggregatable` tidak boleh jadi measure).
- Semua filter wajib memakai operator yang diizinkan untuk tipe field itu.
- Kelas sensitivitas output dicocokkan dengan kebijakan PII berlaku (§2.1).
- Preservasi intent eksplisit admin (PRD §7): filter/field/sort/format yang diminta tidak boleh dihilangkan diam-diam; yang tidak didukung → `Unsupported`/limitation yang diungkap.

### 4.3 Validasi berbasis-DB (langkah terbatas, terpisah)

`PREPARE` query terhadap Fineract read-only untuk memvalidasi keberadaan kolom/sintaks. Ini **bukan** pengecekan semantik, bukan pengganti guard, dan tidak boleh dieksekusi penuh pada langkah ini.

### 4.4 Execution guard (sebelum setiap eksekusi sumber)

Wajib menegakkan **semua** ini, apa pun mode-nya:

1. `SELECT` tunggal, satu statement — tidak ada `;` selain opsional di akhir.
2. Token `INSERT/UPDATE/DELETE/TRUNCATE/DROP/ALTER/CREATE/GRANT/REVOKE/COPY/VACUUM/ANALYZE` ditolak.
3. Parameterisasi penuh — tidak ada interpolasi string; nilai runtime hanya lewat placeholder terikat.
4. Office scope sebagai **parameter terikat di dalam SQL** (§6), bukan filter Rust setelah fetch.
5. Predikat PII + sensitivitas diterapkan.
6. Read-only transaction + statement timeout (`PROBE_QUERY_TIMEOUT`/`ANALYTICAL_QUERY_TIMEOUT`).
7. Cap resource per node/job diperiksa terhadap konsumsi aktual, bukan hanya rencana (runtime.md §6).
8. Hanya surface/kontrak yang disetujui; model tidak dapat memperlebar otorisasi (PRD §5).

**Penting (D09 design-review):** AST parsing + `PREPARE` bukan jaminan keamanan mutlak; keduanya defense-in-depth di atas semantic policy check. Klaim keamanan berbunyi "parameterized + SELECT-only + scope-bound + allowlist", bukan "injection terbukti mustahil".

---

## 5. Measure dan grain

Setiap measure wajib menyatakan: grain, currency, satuan, pembulatan, dan perilaku NULL/tanggal/ties/periode kosong.

- **Grain** adalah baris yang dihitung measure (client vs account vs transaction). Dua measure pada grain berbeda tidak boleh dijumlah tanpa menyatakan grain hasil — dan bila menyatukan menggandakan baris, plan ditolak (§2.2).
- **Currency** (`NUMERIC`, tidak pernah float): total dasar per mata uang; konsolidasi lintas mata uang hanya lewat `exchange_rates` exact-match dan wajib merekam `exchange_rate_id` (C19, #14). Tidak ada "kurs terdekat".
- **Rounding**: toleransi tampilan per kelas measure dideklarasikan; ini menunggu bagian di bawah.
- **NULL ≠ 0**: field kosong tidak disamakan dengan nol; `row_count_total = NULL` berarti tidak diketahui (I4). Analisis kelengkapan (D11) mengelompokkan "belum tercatat" secara eksplisit, tidak membuang record diam-diam.
- **Tanggal/ties/periode kosong**: boundary timezone bisnis, aturan ties untuk sort, dan representasi periode tanpa data dideklarasikan per kontrak (PRD §7).

Nilai budget per job (query/model/token) milik runtime.md §6 dan counter-nya absolut lintas plan version (K9).

---

## 6. Office scope — D5 (penegakan di SQL)

`D5` ditutup oleh dokumen ini. Aturan:

1. Scope ditegakkan **di dalam** SQL lewat parameter terikat, contoh carried-over: `WHERE c.office_id = ANY($1::bigint[])`. Filter Rust setelah fetch **dilarang** — ia membocorkan baris keluar scope ke memori/log dan membuat "authorized" menjadi konvensi, bukan constraint.
2. `office_ids` berasal dari `source: authorized_scope`; `user_may_override = false`. Nilai yang diminta user adalah **subset** dari otorisasi (narrow, tidak pernah widen — PRD §10).
3. Setiap kontrak/query Fineract wajib mendeklarasikan `office_ids` + `require_office_filter = true`; validator katalog menolak yang tidak.
4. Jalur scope per entitas dinyatakan (`office_scope_path`). Bila entitas tidak punya `office_id` langsung (mis. `m_savings_account` → lewat `m_client.office_id`), jalurnya eksplisit di kontrak dan SQL-nya.
5. Otorisasi tidak pernah dibaca dari state turunan (I7); scope disnapshot ke `chat_jobs.scope_json` saat accept (C17).

---

## 7. Kesegaran katalog saat eksekusi — D4

`D4` (pemilik penyelesaian: `architecture/engine.md`) adalah pemeriksaan bahwa katalog tidak berubah antara **verifikasi plan** dan **eksekusi node**. Antarmuka data yang dokumen ini tetapkan: verifikasi merekam `catalog_version_id` + `catalog_content_hash` ke `job_plans.contract_versions_json`; eksekusi node merekam pasangan yang sama ke `job_node_runs.provenance_json`. Engine yang membandingkan keduanya; bila berbeda → plan dianggap stale dan diverifikasi ulang. Dokumen ini hanya menyediakan kolomnya, bukan alur pengecekannya.

---

## 8. Skenario acceptance

- `AC-8.1` — Plan yang menggabungkan measure pada grain berbeda tanpa grain hasil ditolak sebelum eksekusi.
- `AC-8.2` — Field `secret_never_expose` tidak pernah muncul di prompt schema, output, log, maupun response.
- `AC-8.3` — Query tanpa `office_ids` terikat ditolak validator; filter Rust-side ditolak review.
- `AC-8.4` — `Unsupported` dikembalikan saat tidak ada capability maupun kontrak yang disetujui.
- `AC-8.5` — Katalog diubah di tengah job → plan diverifikasi ulang atau node ditolak (D4).
- `AC-8.6` — Kontrak yang prosa-nya tidak cocok dengan SQL-nya gagal verifikasi katalog.
- `AC-8.7` — Kurs konsolidasi direproduksi persis lewat `exchange_rate_id` yang direkam.

## 9. Terbuka

- Kontrak field/measure/relasi konkret per domain (D01–D15) — menunggu verifikasi deployment + inventaris formal.
- Toleransi pembulatan tampilan per kelas measure (dipanggil responses.md §4).
- Parser YAML/SQL AST final (sqlparser kandidat, bukan validator keamanan).
- Daftar pengecualian numeral pada validasi narasi (responses.md §4) — direview bersama kontrak pertama.
- D4 penuh (engine.md), D5 implementasi compiler (bukan schema).
