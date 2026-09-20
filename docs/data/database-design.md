# Desain database Jarvis

Status: menerjemahkan keputusan yang dikunci di [carry-over.md](../migration/carry-over.md) (#1–#15, K1–K5) menjadi ERD, constraint, index dan batas transaksi. Angka operasional ada di [runtime.md](../operations/runtime.md). Dokumen ini **bukan** otorisasi implementasi; ia adalah kontrak schema yang harus dipatuhi migrasi pertama.

**Sumber kebenaran**: bila dokumen ini bertentangan dengan carry-over.md, carry-over.md menang dan dokumen ini yang diperbaiki.

---

## 1. Invarian lintas-keputusan

Aturan-aturan ini berlaku di **seluruh** schema. Ia dinyatakan di sini **sekali** justru karena kegagalan sistem lama adalah tiap bagian diperbaiki sendiri-sendiri tanpa melihat aturan bersama. Setiap perubahan schema di masa depan wajib diperiksa terhadap daftar ini.

### I1 — Tidak ada panggilan eksternal di dalam transaksi commit
Transaksi PostgreSQL hanya boleh berisi tulisan PostgreSQL lokal. Panggilan LLM, HTTP, atau query ke Fineract **tidak pernah** berada di dalamnya.

Konsekuensi yang sudah dikunci: audit ditulis **di dalam** transaksi (#4, tulisan lokal, milidetik), sedangkan summary memori dihitung **di luar** transaksi (#5, butuh panggilan LLM). Menahan transaksi terbuka selama panggilan eksternal memblokir autovacuum dan menahan lock selama detik-detik.

### I2 — Setiap referential action diperiksa terhadap rantai CASCADE session
Rantai `chat_sessions → chat_jobs → {job_plans, job_node_runs, datasets, job_responses, clarification_*, job_events, chat_messages}` semuanya CASCADE. PostgreSQL **tidak menjamin urutan** antar jalur cascade dari satu induk yang sama.

Karena itu: FK dari tabel yang **sendirinya anak CASCADE** menuju sesama anak CASCADE **tidak boleh `RESTRICT`** — gunakan `NO ACTION` (diperiksa di akhir statement) atau `DEFERRABLE INITIALLY DEFERRED`. Juga dilarang: `ON DELETE SET NULL` menuju tabel append-only, karena itu adalah UPDATE.

> Titik buta ini terbukti berulang: ditemukan pada usulan `session_memory` (`RESTRICT`) **dan** pada skema audit lama (`SET NULL` + trigger anti-UPDATE yang membuat penghapusan session gagal). Matriks §5 wajib diperiksa ulang setiap kali FK ditambahkan.

### I3 — Sequence per-entitas dialokasikan lewat row lock, bukan sequence PostgreSQL
`job_events.sequence` dan `session_memory.session_seq` harus **strictly increasing tanpa lubang**, dan sequence PostgreSQL meninggalkan lubang saat rollback. Alokasi: `UPDATE <induk> SET <counter> = <counter> + 1 … RETURNING`, di dalam transaksi yang sama dengan insert anaknya. **Row lock induknya adalah alokatornya.**

### I4 — Ketidakpastian harus terlihat di data
Tidak pernah diasumsikan, tidak pernah dikompresi menjadi status sukses/gagal biner:
- `job_node_runs.status = 'Abandoned'` ≠ `'Failed'` — "mungkin sudah berjalan" berbeda dari "gagal dan diketahui".
- `completeness` (`Complete`/`Partial`/`Unknown`) terpisah dari `status` di node, dataset dan response.
- `datasets.truncated` (set tersimpan) ≠ `completeness` (analitik) ≠ preview (sisi response).
- `row_count_total = NULL` berarti **tidak diketahui**, bukan nol.

### I5 — Tidak ada penghilangan senyap
Setiap pembatasan wajib dinyatakan pada response: truncation (#11), handle kedaluwarsa (#11 aturan 6), kurs tidak tersedia (#14), PII dimatikan (#15), slot yang di-auto-bind (K5), dan skip pengguna (K2).

### I6 — Setiap hand-off antar tahap ditegakkan constraint, bukan konvensi
Lihat matriks §2. Bila sebuah koneksi hanya hidup sebagai aturan prosa, ia dicatat eksplisit di §2.2 sebagai utang, bukan dianggap selesai.

### I7 — Otorisasi tidak pernah dibaca dari state turunan
Summary, session memory, dan baris audit **tidak pernah** menjadi dasar izin. `owner_user_id` difilter pada setiap baca, dan otorisasi dicek ulang pada setiap pembacaan dataset. Yang disnapshot (`scope_json`) adalah **konteks keputusan untuk audit**, bukan grant yang boleh dipakai ulang.

### I8 — Audit hidup lebih lama daripada isinya, dan tidak punya integritas referensial
`audit_events` tidak memiliki satu pun FK. `job_id`/`session_id`/`dataset_id` yang menggantung setelah purge adalah **kondisi normal**, bukan korupsi, dan **tidak boleh** "diperbaiki" menjadi FK oleh migrasi berikutnya.

---

## 2. Matriks koneksi

### 2.1 Hand-off yang ditegakkan struktur

| # | Hand-off | Penegak | Kegagalan bila dilanggar |
| --- | --- | --- | --- |
| C1 | request → job | baris `idempotency_keys` `completed` + baris job, **satu transaksi** | rollback; tidak ada job tanpa acknowledgement dan sebaliknya |
| C2 | session → job aktif | partial unique `chat_jobs(session_id) WHERE lifecycle nonterminal` | unique violation → 409 |
| C3 | job → plan aktif | `UNIQUE (job_id, plan_version)`; `chat_jobs.plan_version` menunjuk yang aktif | mustahil dua plan aktif untuk satu job |
| C4 | plan → node | `UNIQUE (job_id, plan_version, node_id, attempt)`; scheduler hanya membaca `plan_version` aktif | node dari plan lama tidak dapat dieksekusi |
| C5 | node → dataset | FK `job_node_runs.dataset_id → datasets(id)` | handle yatim mustahil |
| C6 | dataset → chunk | PK `(dataset_id, chunk_index)` + FK CASCADE | chunk yatim mustahil |
| C7 | node → reuse lintas plan | FK `reused_from_node_run_id` + `input_binding_hash` | reuse tanpa jejak binding mustahil |
| C8 | job → form klarifikasi | FK `clarification_forms.job_id` + `plan_version`; partial unique satu form `open` per job | dua form terbuka mustahil |
| C9 | form → opsi terbit | PK `(form_id, field_id, option_id)` | option ID dari form/job lain gagal lookup |
| C10 | jawaban → binding | PK `(form_id, field_id)`; `answer_kind`/`raw_text`/`binding_json` **terpisah** | teks bebas tidak dapat menjadi binding identitas |
| C11 | job → response | `UNIQUE (job_id, response_version)`, baris immutable | versi tidak dapat ditimpa |
| C12 | **response → memory** | **`source_response_version` NOT NULL + FK komposit `(source_job_id, source_response_version)`** | **baris memori mustahil tanpa response durable (K4)** |
| C13 | memory → dataset kedaluwarsa | FK `session_memory.dataset_id` + `handle_state` **non-optional** pada tipe hasil repository | tidak ada jalur baca yang dapat melewatkan status handle |
| C14 | semua transisi → event | `sequence` dialokasikan row lock `chat_jobs` (I3); insert event **satu transaksi** dengan perubahan state | sequence tidak mungkin berlubang; cursor selalu konsisten dengan snapshot |
| C15 | semua keputusan → audit | INSERT audit **satu transaksi** dengan efek yang dijelaskannya | audit gagal ⇒ rollback ⇒ tidak ada kemajuan |
| C16 | worker → tulisan durable | `AND lease_token = $token` pada setiap UPDATE | worker basi mengenai 0 baris dan wajib berhenti |
| C17 | konfigurasi → job | `scope_json` menyimpan nilai efektif + `setting_version` | laporan lama tidak berubah makna saat konfigurasi diubah |
| C18 | katalog → plan/node | `catalog_version_id` + `catalog_content_hash` pada `job_plans` dan `job_node_runs`; katalog append-only | prosa kontrak yang dilihat planner selalu dapat ditemukan kembali |
| C19 | kurs → analisis | `exchange_rate_id` direkam di `provenance_json` dan evidence response | reproduksi tetap eksak walau registry dikoreksi |
| C20 | session → seluruh isi | rantai CASCADE (§5) | penghapusan session tidak meninggalkan baris yatim |

### 2.2 Koneksi yang masih berupa aturan, bukan constraint (utang eksplisit)

Ini **bukan** daftar yang boleh dilupakan — ia persis kelas masalah yang membuat sistem lama tidak konsisten.

| # | Koneksi | Status | Pemilik penyelesaian |
| --- | --- | --- | --- |
| D1 | **`completeness` node → dataset → response** | ✅ **DITUTUP** oleh [responses.md](../contracts/responses.md) §3: validator **menghitung ulang** completeness dari ledger; klaim composer yang **lebih baik** daripada hitungan ditolak (satu arah) | selesai |
| D2 | Kewajiban mengungkap auto-bind (K5) | ✅ **DITUTUP** oleh responses.md §5: himpunan slot auto-bind dari `input_binding_json` dibandingkan dengan himpunan yang diungkap; tidak sama → validasi gagal | selesai |
| D3 | Kelengkapan evidence lineage | ✅ **DITUTUP** oleh responses.md §4: numeral pada narasi wajib cocok dengan blok ber-evidence atau dengan entri `derivation`; selain itu ditolak | selesai |
| D4 | Kesegaran katalog saat eksekusi | `catalog_version_id` direkam saat verifikasi; belum ada pemeriksaan katalog tidak berubah antara verifikasi dan eksekusi | `architecture/engine.md` |
| D5 | Penegakan office scope di SQL | ✅ DITUTUP (design) oleh analytical-contracts.md §6 — compiler/validator tetap pekerjaan implementasi | `data/analytical-contracts.md` |
| D6 | `guards.snapshot_only` ditegakkan mekanisme | 24 kapabilitas snapshot mendeklarasikan `guards.snapshot_only: true` (knowledge/VERIFICATION.md §8.3) tetapi **tidak ada validator/guard yang menolak atau menganotasi** permintaan yang memperlakukannya sebagai as-of (§7). Deklarasi tanpa penegakan = persis drift yang I6 larang. **Keputusan owner (L1.8/FIN-39):** dicatat sebagai utang di sini, ditegakkan saat mekanisme as-of hadir — penegakan butuh layer eksekusi (bandingkan `as_of` yang diminta vs kapabilitas snapshot), di luar scope L1 katalog. | layer eksekusi (as-of), setelah `architecture/engine.md` |

---

## 3. Inventaris tabel

23 tabel dalam enam kelompok.

```
AUTH & CONFIG            CHAT                     JOB
  users                    chat_sessions            chat_jobs
  auth_sessions            chat_messages            job_plans
  refresh_tokens           session_memory           job_node_runs
  system_settings                                   job_events
                                                    job_responses
KLARIFIKASI              DATA                     PENDUKUNG
  clarification_forms      datasets                 idempotency_keys
  clarification_answers    dataset_chunks           audit_events
  clarification_options                             audit_evidence
                                                    knowledge_catalog_versions
                                                    knowledge_index
                                                    exchange_rates
```

### ERD (relasi utama)

```
users ─┬─< auth_sessions ─< refresh_tokens
       └─< chat_sessions ─┬─< chat_messages
                          ├─< session_memory ──> job_responses (komposit)
                          └─< chat_jobs ─┬─< job_plans
                                         ├─< job_node_runs ──> datasets ─< dataset_chunks
                                         ├─< job_events
                                         ├─< job_responses
                                         ├─< clarification_forms ─┬─< clarification_answers
                                         │                        └─< clarification_options
                                         └─< datasets

idempotency_keys ··> chat_jobs            (target_job_id, tanpa CASCADE)
audit_events ─< audit_evidence            (audit TANPA FK ke mana pun)
knowledge_catalog_versions ─< knowledge_index
system_settings, exchange_rates           (berdiri sendiri, append-only)
```

`─<` = FK dengan CASCADE · `··>` = referensi tanpa CASCADE · `──>` = FK non-CASCADE

---

## 4. Definisi tabel

Tipe: `id` = UUID (v7 untuk tabel append-only bervolume tinggi), waktu = `TIMESTAMPTZ`, uang/kurs = `NUMERIC` (**tidak pernah** floating point).

### 4.1 `users`
`id` PK · `username` UNIQUE NOT NULL · `email` UNIQUE · `password_hash` NOT NULL · `full_name` · `role` NOT NULL CHECK IN (`admin`) · **`external_subject` UNIQUE NULL** (engsel SSO) · `is_active` NOT NULL · `created_at` · `updated_at` · `last_login_at`.
Index: UNIQUE `username`, `email`, `external_subject`. Index eksplisit di atas UNIQUE **dibuang** (redundan).

### 4.2 `auth_sessions` *(rename dari `user_sessions`)*
`id` PK · `user_id` FK → `users` CASCADE · `user_agent` · `ip_address` · `entitlements_json` · `entitlements_derived_at` · `entitlements_source` CHECK IN (`admin_projection`,`fineract_derived`) · `created_at` · `last_seen_at` · `expires_at` · `revoked_at`.
Index: `(user_id)` · `(revoked_at)` · parsial `(expires_at) WHERE revoked_at IS NULL`.

### 4.3 `refresh_tokens`
`id` PK · `session_id` FK → `auth_sessions` CASCADE · `user_id` FK → `users` CASCADE · `token_hash` UNIQUE NOT NULL (SHA-256) · `created_at` · `expires_at` · `revoked_at`.
Index: UNIQUE `token_hash` · `(session_id)`.

### 4.4 `system_settings` — append-only (#15)
`id` PK · `key` NOT NULL · `value_json` NOT NULL · `version` INT NOT NULL · **UNIQUE (key, version)** · `effective_from` NOT NULL · `changed_by_user_id` FK → `users` (NO ACTION) · `change_reason` · `created_at`.
Nilai berlaku = `version` tertinggi per `key`. Perubahan = baris baru; **tidak pernah** UPDATE.
Index: UNIQUE `(key, version)` · `(key, version DESC)`.

### 4.5 `chat_sessions`
`id` PK · `owner_user_id` FK → `users` NOT NULL · `title` · `status` NOT NULL CHECK IN (`active`,`archived`,`expired`) · `memory_summary_text` · `memory_summary_version` INT NOT NULL DEFAULT 0 · `memory_summary_watermark` BIGINT NOT NULL DEFAULT 0 · `memory_summary_status` CHECK IN (`current`,`stale`,`failed`) · `memory_summary_updated_at` · **`memory_seq_last` BIGINT NOT NULL DEFAULT 0** (alokator I3) · `created_at` · `updated_at` · `expires_at` · `archived_at`.
`context_json` lama **dihapus**.
Index: PK · `(owner_user_id, updated_at DESC)` · `(status)` · parsial `(expires_at)`.

### 4.6 `chat_messages` — indeks tipis (#6)
`id` PK · `session_id` FK CASCADE · `job_id` FK → `chat_jobs` CASCADE · `role` NOT NULL CHECK IN (`user`,`assistant`,`clarification`) · `response_version` INT · `clarification_id` UUID · `clarification_revision` INT · `created_at`.
CHECK: `role<>'assistant' OR response_version IS NOT NULL` · `role<>'clarification' OR (clarification_id IS NOT NULL AND clarification_revision IS NOT NULL)` · `role<>'user' OR (response_version IS NULL AND clarification_id IS NULL)`.
**Tanpa kolom `content`/`metadata_json`.**
Index: PK · **`(session_id, created_at, id)`** (keyset) · `(job_id)`.

### 4.7 `session_memory` (#5)
`id` PK · `session_id` FK CASCADE · `owner_user_id` NOT NULL · `session_seq` BIGINT NOT NULL · **UNIQUE (session_id, session_seq)** · `kind` NOT NULL CHECK IN (`PriorResult`,`ResolvedEntity`,`ActiveScope`) · `entity_key` · `label` *(PII)* · `fact_json` NOT NULL · `source_job_id` NOT NULL · `source_plan_version` · `source_node_id` · **`source_response_version` NOT NULL** · `provenance_json` · `as_of` · `completeness` NOT NULL · `completeness_reason` · `dataset_id` FK → `datasets` · `status` NOT NULL CHECK IN (`valid`,`superseded`,`invalidated`,`evicted`) · `superseded_by_id` self-FK · `invalidation_reason` · `invalidated_at` · `created_at`.
**FK komposit `(source_job_id, source_response_version)` → `job_responses (job_id, response_version)` dengan `NO ACTION`** (I2).
CHECK: `kind='ResolvedEntity' ⇒ entity_key IS NOT NULL` · `kind='ActiveScope' ⇒ dataset_id IS NULL AND entity_key IS NULL` · `status<>'valid' ⇒ invalidated_at IS NOT NULL AND invalidation_reason IS NOT NULL`.
Index: PK · UNIQUE `(session_id, session_seq)` · parsial `(session_id, kind) WHERE status='valid'` · parsial UNIQUE `(session_id) WHERE kind='ActiveScope' AND status='valid'` · parsial UNIQUE `(session_id, entity_key) WHERE kind='ResolvedEntity' AND status='valid'` · parsial `(dataset_id)` · `(source_job_id)`.

### 4.8 `chat_jobs` (#1) — `fillfactor = 85`
**Identitas**: `id` PK · `session_id` FK CASCADE · `owner_user_id` NOT NULL · `scope_json` NOT NULL.
**Permintaan**: `request_text` · `request_json`.
**Status**: `lifecycle` NOT NULL CHECK IN (`Queued`,`Running`,`WaitingForUser`,`Cancelling`,`Completed`,`Failed`,`Cancelled`,`Expired`) · `outcome` CHECK IN (`Answered`,`Empty`,`NotFound`,`Unsupported`,`BlockedByPolicy`,`Invalid`,`OperationalFailure`,`SkippedByUser`) · `completeness` CHECK IN (`Complete`,`Partial`,`Unknown`) · `completeness_reason` · `failure_code`.
**CHECK**: `lifecycle IN ('Completed','Failed','Cancelled','Expired') ⇒ outcome IS NOT NULL`.
**Pointer**: `plan_version` INT · `final_response_version` INT · `last_event_sequence` BIGINT NOT NULL DEFAULT 0.
**Budget**: `query_count` · `model_call_count` · `token_cost` · `replan_count` (semua NOT NULL DEFAULT 0).
**Lease**: `lease_owner` · `lease_token` UUID · `lease_expires_at` · `lease_claimed_at` · `heartbeat_at` · `cancel_requested_at`.
**Waktu**: `created_at` · `started_at` · `waiting_since` · `terminal_at` · `expires_at` · `updated_at`.
`scope_json` minimal: `{"pii":{"enabled":bool,"setting_version":N},"source":"admin_projection|fineract_derived","office_ids":[…],"fineract_tenant":"…"}`.
Index: PK · **partial unique `(session_id) WHERE lifecycle IN ('Queued','Running','WaitingForUser','Cancelling')`** · parsial `(lifecycle) WHERE lifecycle='Queued'` · `(owner_user_id, created_at DESC)` · parsial `(expires_at) WHERE lifecycle NOT IN (terminal)`.
**`lease_expires_at` sengaja TIDAK di-index** — meng-index kolom yang berubah tiap perpanjangan lease mematikan HOT update (#1).

### 4.9 `job_plans` (#12)
`id` PK · `job_id` FK CASCADE · `plan_version` INT NOT NULL · **UNIQUE (job_id, plan_version)** · `graph_json` NOT NULL (nodes + edges + bindings, immutable) · `graph_hash` NOT NULL · `contract_versions_json` NOT NULL (capability/contract + **`catalog_version_id` + `catalog_content_hash`**) · `verified_at` · `supersedes_plan_version` · `replan_reason` · `created_at` · `superseded_at`.
Index: PK · UNIQUE `(job_id, plan_version)`.

### 4.10 `job_node_runs` (#2) *(rename dari `chat_workflow_node_runs`)*
`id` PK · `job_id` FK CASCADE · `plan_version` INT NOT NULL · `node_id` NOT NULL · `node_kind` NOT NULL CHECK IN (`Probe`,`CuratedQuery`,`AnalyticalQuery`,`Clarify`,`Compose`,`Respond`) · `attempt` INT NOT NULL · **UNIQUE (job_id, plan_version, node_id, attempt)** · `status` NOT NULL CHECK IN (`Pending`,`Runnable`,`Running`,`Completed`,`Failed`,`Skipped`,`Abandoned`) · `completeness` + `completeness_reason` · `failure_code` · `input_binding_json` · `input_binding_hash` · `dataset_id` FK → `datasets` NO ACTION · `output_json` · `provenance_json` (contract + `catalog_version_id` + `catalog_content_hash` + `as_of` + `exchange_rate_id` bila relevan) · `reused_from_node_run_id` self-FK NO ACTION · `rows_returned` · `duration_ms` · `started_at` · `finished_at` · `created_at`.
Index: UNIQUE di atas · `(job_id, plan_version, status)` (query scheduler) · `(dataset_id)`.

### 4.11 `job_events` (#3)
**PK `(job_id, sequence)`** — tanpa kolom `id`. `job_id` FK CASCADE · `sequence` BIGINT NOT NULL · `schema_version` · `event_type` NOT NULL · `occurred_at` NOT NULL · `plan_version` · `node_id` · `node_attempt` · `clarification_id` · `clarification_revision` · `response_version` · `payload_json` · `payload_truncated` NOT NULL DEFAULT false.
Append-only: tanpa UPDATE, tanpa DELETE satuan.
Index: PK `(job_id, sequence)` · **BRIN `(occurred_at)`**.

### 4.12 `job_responses` (#10)
`id` PK · `job_id` FK CASCADE · `response_version` INT NOT NULL · **UNIQUE (job_id, response_version)** · `schema_version` · `plan_version` · `kind` NOT NULL CHECK IN (`analysis`,`skipped`,`limitation`) · `outcome` · `completeness` + `completeness_reason` · `blocks_json` NOT NULL · `evidence_json` NOT NULL · `validation_status` NOT NULL CHECK IN (`passed`,`failed`,`fallback`) · `validation_report_json` · `superseded_by_version` · `response_hash` NOT NULL · `composed_at` · `created_at`.
Immutable setelah insert (kecuali `superseded_by_version` sekali).
Index: PK · UNIQUE `(job_id, response_version)`.

### 4.13 `clarification_forms` (#8)
`id` PK · `job_id` FK CASCADE · `clarification_id` UUID NOT NULL · `revision` INT NOT NULL · **UNIQUE (job_id, clarification_id, revision)** · `plan_version` · `schema_version` · `purpose` · `stage_label` · `fields_json` NOT NULL · `state` NOT NULL CHECK IN (`open`,`answered`,`superseded`,`skipped`,`expired`,`invalidated`) · `resolved_by_user_id` · `resolved_at` · `resolution_reason` · `superseded_by_revision` · `expires_at` · `created_at`.
Index: UNIQUE di atas · **partial unique `(job_id) WHERE state='open'`**.

### 4.14 `clarification_answers` (#8, K5)
**PK `(form_id, field_id)`** · `form_id` FK → `clarification_forms` CASCADE · `answer_kind` NOT NULL CHECK IN (`option_id`,`typed_value`,`refine_search`,`change_intent`) · `raw_text` · `binding_json` · **`provenance` NOT NULL CHECK IN (`user_confirmed`,`resolver_unique`,`deterministic_parse`)** · `resolver_ref` · `option_set_ref` · `answered_by_user_id` · `answered_at`.
CHECK: `answer_kind IN ('refine_search','change_intent') ⇒ binding_json IS NULL`.
Hanya jawaban **diterima**; yang ditolak masuk audit.

### 4.15 `clarification_options` (#8)
**PK `(form_id, field_id, option_id)`** · `form_id` FK CASCADE · `binding_json` NOT NULL · `label` *(PII)* · `attributes_json` · `resolver_ref` · `page_cursor` · `issued_at` · `expires_at` **diturunkan dari `clarification_forms.expires_at`** (K4).
Hanya halaman yang **benar-benar dikirim** ke klien.
Index: PK · parsial `(expires_at)` untuk purge.

### 4.16 `datasets` (#11)
`id` PK (handle) · `job_id` FK CASCADE · `node_id` · `plan_version` · `session_id` FK CASCADE · `owner_user_id` NOT NULL · `schema_json` · `grain_json` · `scope_json` · `provenance_json` · `row_count_available` · `row_count_total` (NULL = tidak diketahui) · `completeness` + `completeness_reason` · `truncated` NOT NULL · `sort_key_json` NOT NULL · `byte_size` · `chunk_count` · `status` NOT NULL CHECK IN (`building`,`ready`,`failed`,`expired`,`purged`) · `created_at` · `expires_at` · `purged_at`.
Immutable setelah `ready`; refresh = dataset baru. **Baris tidak dihapus saat purge** — hanya chunk-nya; status berpindah ke `purged` (syarat C13).
Index: PK · `(session_id, created_at)` · parsial `(expires_at) WHERE status='ready'` · `(job_id)`.

### 4.17 `dataset_chunks` (#11)
**PK `(dataset_id, chunk_index)`** · `dataset_id` FK CASCADE · `row_from` · `row_to` · `payload` JSONB · `row_count` · `byte_size` · **`format`** · **`encoding_version`**.

### 4.18 `idempotency_keys` (#9)
`id` PK · `owner_user_id` NOT NULL · `operation` NOT NULL CHECK IN (`job.create`,`job.respond`,`job.skip`) · `idempotency_key` NOT NULL · **UNIQUE (owner_user_id, operation, idempotency_key)** · `request_fingerprint` NOT NULL (hash) · `target_job_id` UUID (**tanpa FK**, tidak ikut CASCADE) · `status` NOT NULL CHECK IN (`in_progress`,`completed`) · `response_status` · `response_body_json` · `created_at` · `completed_at` · `expires_at`.
Index: UNIQUE di atas · parsial `(expires_at)`.

### 4.19 `audit_events` (#4) — append-only, **tanpa FK**
`id` PK (**UUIDv7**) · `schema_version` · `occurred_at` NOT NULL · `duration_ms` · `request_id` · `actor_kind` NOT NULL CHECK IN (`user`,`worker`,`reaper`,`system`) · `actor_user_id` · `session_id` · `job_id` · `plan_version` · `node_id` · `node_attempt` · `query_attempt_id` · `model_call_id` · `clarification_id` · `clarification_revision` · `response_version` · `response_hash` · `dataset_id` · `graph_hash` · `stage` NOT NULL · `action` NOT NULL · `result` NOT NULL CHECK IN (`ok`,`denied`,`invalid`,`failed`,`deferred`) · `failure_code` · `job_outcome` · `job_completeness` · `completeness_reason` · `catalog_version_id` · `catalog_content_hash` · `contract_refs_json` NOT NULL DEFAULT `'{}'` · `scope_json` NOT NULL DEFAULT `'{}'` (redacted) · `detail_json` NOT NULL DEFAULT `'{}'` CHECK `jsonb_typeof='object'` · `has_evidence` NOT NULL DEFAULT false.
`job_outcome`/`job_completeness` **hanya** terisi pada `stage IN ('commit','settle')`.
Penegakan: `REVOKE UPDATE, DELETE FROM <app_role>`; peran `audit_purge` terpisah memegang DELETE; trigger `BEFORE UPDATE` sebagai jaring kedua — **tidak untuk DELETE**.
Index: PK · parsial `(job_id, occurred_at) WHERE job_id IS NOT NULL` · parsial `(actor_user_id, occurred_at DESC) WHERE actor_user_id IS NOT NULL` · **BRIN `(occurred_at)`**.

### 4.20 `audit_evidence` (#4)
`id` PK · `audit_event_id` FK → `audit_events` CASCADE · `kind` NOT NULL · `payload_json` **NULL** · `redaction_level` · `created_at` · `expires_at` · `purged_at`.
Purge = `payload_json := NULL, purged_at := now()`; baris tetap ada sebagai bukti evidence pernah ada.
Index: `(audit_event_id)` · parsial `(expires_at) WHERE purged_at IS NULL`.

### 4.21 `knowledge_catalog_versions` + `knowledge_index` (#7)
`knowledge_catalog_versions`: `id` PK · `version` · `content_hash` UNIQUE NOT NULL · `status` · `document_count` · `embedding_model` · `embedding_dimensions` · **`embedding_input_type`** · `metadata_json` · `created_at` · `synced_at`. **Append-only**: baris yang dirujuk `job_plans`/`job_node_runs` tidak boleh dihapus.
`knowledge_index`: `id` PK · `catalog_version_id` FK CASCADE · `source_type` CHECK IN (`data_area`,`domain`,`capability`,`query`,`schema`,`metric`,`policy`,`response`,**`analytical_contract`**,**`measure`**) · `source_id` · `source_path` · `title` · `retrieval_text` NOT NULL · `metadata_json` · `content_hash` · `embedding vector(1024)` · `embedding_model` · `embedded_at` · `created_at` · **UNIQUE (catalog_version_id, source_type, source_id)**.
Index: UNIQUE di atas · `(catalog_version_id)` · `(source_type, source_id)` · GIN `(metadata_json)`.
**Dibuang**: index ivfflat (pakai exact search) · `(content_hash)`.

### 4.22 `exchange_rates` (#14)
`id` PK · `from_currency_code` · `to_currency_code` · `rate` **NUMERIC(20,10)** · `rate_type` · `effective_date` DATE · `source` · `captured_at` · `captured_by_user_id` · `notes` · `created_at` · **UNIQUE (from_currency_code, to_currency_code, rate_type, effective_date, captured_at)**.
Immutable; koreksi = baris baru. Lookup exact-match; bila banyak baris untuk tanggal sama, ambil `captured_at` **terbaru** dan **rekam `exchange_rate_id` terpilih**. **Tidak ada** fungsi "kurs terdekat".
Index: UNIQUE di atas · `(from_currency_code, to_currency_code, rate_type, effective_date DESC)`.

---

## 5. Matriks referential action (wajib diperiksa saat menambah FK — I2)

| FK | Action | Alasan |
| --- | --- | --- |
| `chat_sessions.owner_user_id → users` | NO ACTION | user tidak dihapus saat masih punya session |
| `chat_jobs.session_id → chat_sessions` | **CASCADE** | rantai utama |
| `job_plans/job_node_runs/job_events/job_responses/clarification_forms/datasets/chat_messages .job_id → chat_jobs` | **CASCADE** | anak job |
| `clarification_answers/options .form_id → clarification_forms` | **CASCADE** | |
| `dataset_chunks.dataset_id → datasets` | **CASCADE** | |
| `session_memory.session_id → chat_sessions` | **CASCADE** | |
| **`session_memory.(source_job_id, source_response_version) → job_responses`** | **NO ACTION** | ⚠️ **bukan RESTRICT** — keduanya anak CASCADE dari session; RESTRICT akan menggagalkan penghapusan session (I2) |
| `session_memory.dataset_id → datasets` | NO ACTION | baris dataset tetap ada saat `purged` |
| `job_node_runs.dataset_id → datasets` | NO ACTION | idem |
| `job_node_runs.reused_from_node_run_id` | NO ACTION | self-FK lintas plan_version |
| `audit_evidence.audit_event_id → audit_events` | CASCADE | evidence tidak bermakna tanpa auditnya |
| **`audit_events.*`** | **tanpa FK** | I8 — audit hidup lebih lama; `SET NULL` dilarang karena itu UPDATE |
| **`idempotency_keys.target_job_id`** | **tanpa FK** | punya TTL sendiri, tidak ikut CASCADE session |
| `knowledge_index.catalog_version_id` | CASCADE | tetapi versi yang dirujuk audit/plan tidak pernah dihapus (aturan retensi) |

---

## 6. Batas transaksi

Setiap blok di bawah adalah **satu transaksi**. Tidak ada panggilan eksternal di dalamnya (I1).

**T1 — Create job** (`POST /chat/jobs`)
`INSERT idempotency_keys (in_progress) ON CONFLICT DO NOTHING` → bila konflik: bandingkan fingerprint, replay atau 409 · baca `system_settings` nilai berlaku · INSERT `chat_jobs` (`Queued`, `scope_json` termasuk PII + setting_version) · alokasi `last_event_sequence` + INSERT `job_events(job.accepted)` · INSERT `chat_messages(role='user')` · INSERT `audit_events(stage='accept')` · UPDATE `idempotency_keys` → `completed` + acknowledgement. **Commit → 202.** Notifikasi Redis **setelah** commit (#3).

**T2 — Klaim job oleh worker**
`UPDATE chat_jobs SET lease_owner, lease_token=gen_random_uuid(), lease_expires_at, lease_claimed_at, heartbeat_at, lifecycle='Running', started_at, expires_at = now() + JOB_TTL_RUNNING WHERE id = (SELECT … WHERE lifecycle='Queued' AND (lease_expires_at IS NULL OR lease_expires_at < now()) FOR UPDATE SKIP LOCKED) RETURNING lease_token` · INSERT event + audit.

**T3 — Plan diverifikasi**
INSERT `job_plans` (`graph_json`, `graph_hash`, `contract_versions_json` dengan `catalog_version_id`+hash) · UPDATE `chat_jobs.plan_version` (+ `AND lease_token=$token`) · INSERT node `Pending`/`Runnable` · event + audit. Verifikasi retensi per node dilakukan **di sini** (amandemen K16): plan yang melebihi plafon **ditolak sebelum dijalankan**.

**T4 — Node selesai**
UPDATE `job_node_runs` (status, completeness, dataset_id, provenance) `AND` job masih dipegang token · UPDATE `datasets.status='ready'` bila ada · UPDATE budget counter pada `chat_jobs` · event `node.status_changed` · audit `stage='node_execute'`/`source_query'`. Eksekusi query/LLM terjadi **di luar** transaksi ini.

**T5 — Klarifikasi dibutuhkan**
INSERT `clarification_forms` (`open`) · INSERT `clarification_options` untuk halaman yang dikirim · UPDATE `chat_jobs` → `WaitingForUser`, `waiting_since=now()`, **`expires_at = waiting_since + CLARIFICATION_WAIT_LIMIT`** (amandemen K3) · INSERT `chat_messages(role='clarification')` · event `clarification.required` · audit.
Slot yang **auto-bind** (K5) tidak membuat baris form — hanya `input_binding_json` pada node + audit `action='slot.auto_bound'`.

**T6 — Jawaban diterima** (`POST /chat/jobs/{id}/responses`)
Urutan cek: **idempotency → replay bila cocok → ownership → lifecycle → revision → validasi field**. Lalu: INSERT `clarification_answers` · UPDATE form → `answered` · UPDATE `chat_jobs` → `Queued`, **`expires_at = now() + JOB_TTL_RUNNING`** · event `clarification.accepted` · audit · `idempotency_keys` → `completed`. **Commit → 202.** `job.resumed` dipancarkan saat worker benar-benar melanjutkan, bukan di sini.

**T7 — Response commit** (titik promosi memori tunggal, K4)
INSERT `job_responses` · UPDATE `chat_jobs` (`lifecycle` terminal, `outcome`, `completeness`, `final_response_version`, `terminal_at`) · **INSERT `session_memory`** (alokasi `memory_seq_last` lewat row lock `chat_sessions`) · INSERT `chat_messages(role='assistant')` · event terminal · audit `stage='commit'` (mengisi `job_outcome`/`job_completeness`) · `idempotency_keys` → `completed` bila dipicu skip.
**Summary memori dihitung SETELAH commit ini, di luar transaksi** (I1); gagal → `memory_summary_status='stale'`, watermark tidak bergerak.

**T8 — Skip** (K2): sama dengan T7, `kind='skipped'`, `outcome='SkippedByUser'`.

**T9 — Cancel**: UPDATE → `Cancelling` + `cancel_requested_at` + event + audit. Worker menuntaskan dan commit `Cancelled`; bila lease kedaluwarsa, reaper yang memindahkannya.

**T10 — Hapus session (FORCE, #6)**
Cek job nonterminal di service layer → tulis `Cancelling` · `DELETE FROM chat_sessions` (CASCADE membersihkan seluruh rantai) · INSERT audit `stage='admin'`. Worker pemegang lease **tidak diberi tahu**: heartbeat berikutnya mengenai 0 baris dan ia wajib berhenti (C16). `idempotency_keys` dan `audit_events` **tidak** ikut terhapus.

**T11 — Reaper** (idempoten, berulang): lease kedaluwarsa → attempt `Abandoned` + attempt baru · job melewati `expires_at` → `Expired` · `Cancelling` tanpa lease → `Cancelled` · dataset melewati TTL → `expired`/`purged` (hapus chunk, **pertahankan baris dataset**) · purge `clarification_options` pada form terminal · purge `idempotency_keys` kedaluwarsa · purge `job_events` berbasis usia · purge `audit_evidence`.

**T12 — Perubahan konfigurasi**: INSERT `system_settings` versi baru + audit `stage='admin'`. Job berjalan **tidak** terpengaruh — nilainya sudah disnapshot (C17).

---

## 7. Urutan migrasi

`users` → `auth_sessions` → `refresh_tokens` → `system_settings` → `knowledge_catalog_versions` → `knowledge_index` → `exchange_rates` → `chat_sessions` → `chat_jobs` → `job_plans` → `datasets` → `dataset_chunks` → `job_node_runs` → `job_events` → `job_responses` → `session_memory` → `clarification_forms` → `clarification_answers` → `clarification_options` → `chat_messages` → `idempotency_keys` → `audit_events` → `audit_evidence` → grant/revoke peran audit.

Catatan: `job_node_runs.dataset_id` dan `session_memory.(source_job_id, source_response_version)` menciptakan ketergantungan maju; keduanya ditambahkan sebagai `ALTER TABLE … ADD CONSTRAINT` setelah tabel tujuan ada. Startup aplikasi **tidak pernah** membuat atau mengubah tabel.

---

## 8. Terbuka

- **D4** (§2.2) — kesegaran katalog saat eksekusi, menunggu `architecture/engine.md`. D1–D3 ditutup `contracts/responses.md`; D5 ditutup (design) `data/analytical-contracts.md`.
- **K1** ([runtime.md](../operations/runtime.md)) — heartbeat wajib task independen, kalau tidak lease harus ≥180 s.
- Penerbit token bila SSO dipakai (HS256 dilarang saat itu) — `security/access-data-policy.md`.
- Retensi audit — keputusan compliance.
- Format payload `dataset_chunks` final (JSONB vs BYTEA terkompresi) — menunggu uji kapasitas.
