# Migration & schema carry-over — keputusan (draft)

Tanggal: 2026-09-12. Status: DRAFT untuk dibahas per keputusan. Belum final, belum di-commit sebagai kontrak. Dasar: struktur `migrations/*.sql` repo lama (ai_report) dinilai terhadap desain Engine baru ([engine.md](../architecture/engine.md), [prd.md](../product/prd.md), contracts). Repo baru mendapat **migration set fresh** — dokumen ini menentukan struktur mana yang diport, diubah, atau dibuang; **bukan** menjalankan migration lama.

Prinsip: PRD §11/tech-stack — jangan apply migration lama otomatis; adaptasi berdasarkan schema baru. Kolom lama = bukti referensi, bukan kontrak.

## Ringkasan klasifikasi

| Tabel lama | Peran | Verdict |
| --- | --- | --- |
| `api_keys`, `users`, `permissions`, `role_permissions`, `user_sessions`, `refresh_tokens` | Auth/identity | **CARRY** |
| `knowledge_catalog_versions`, `knowledge_index` | Vector/catalog (pgvector) | **CARRY** (upgrade ringan) |
| `chat_sessions` | Sesi + history | **UPGRADE** |
| `chat_messages` | Riwayat pesan | **UPGRADE** |
| `chat_jobs` | Job lifecycle | **REDESIGN** (upgrade terberat) |
| `chat_job_checkpoints` | Checkpoint | **UPGRADE** |
| `chat_job_events` | Public event / SSE | **UPGRADE** |
| `chat_workflow_node_runs` | Node run | **UPGRADE** → node-status ledger |
| `assistant_job_memory` | JobMemory ad-hoc | **DROP** → unified job state |
| `assistant_session_memory` | Session memory | **REDESIGN** → tier session-memory |
| `assistant_graph_checkpoints` | Checkpoint graph | **DROP** → node ledger |
| `assistant_llm_traces` | Trace model | **REDESIGN** → observability |
| `chat_job_audit_events`, `management_audit_events`, `management_audit_outbox`, `management_telemetry_counters` | Audit + telemetry (terpecah) | **REDESIGN/CONSOLIDATE** |
| `assistant_original_intents`, `_fact_observations`, `_effective_constraints`, `_planner_input_snapshots` | Canonical gateway | **SUDAH DIHAPUS** (20260906) |

---

> **AMANDEMEN KLASIFIKASI (2026-09-13)** — klasifikasi awal di bawah sudah direvisi oleh keputusan yang dikunci belakangan:
> - `chat_job_checkpoints`: UPGRADE → **DROP** (#2 — batas commit bermakna **adalah** baris node run yang selesai + commit response).
> - Auth: **6 tabel → 3** (#7). CARRY: `users`, `user_sessions` **→ rename `auth_sessions`**, `refresh_tokens`. **DROP**: `permissions`, `role_permissions`, `api_keys`.
> - Rename: `chat_workflow_node_runs` → `job_node_runs` (#2) · `chat_job_events` → `job_events` (#3).
> - Tabel baru di luar daftar semula: `system_settings` (#15).
> - `published_reports` (gap G) **DITUNDA** (#14); `exchange_rates` tetap dibangun.

## CARRY (dibawa apa adanya)

### Auth/identity — 6 tabel
`api_keys`, `users`, `permissions`, `role_permissions`, `user_sessions`, `refresh_tokens`. Terbukti; fondasi bearer/session. **Yang perlu dibahas:** apakah tenant model / issuer-audience token dashboard mengubah `users`/`user_sessions` (mengait ke security/access-data-policy). Ownership job pindah dari `api_key_id` → `user_id` (lihat chat_jobs).

### Vector/catalog — `knowledge_catalog_versions`, `knowledge_index`
pgvector + content-hash dedup. **Upgrade ringan yang mungkin:** penyesuaian bila model catalog berubah untuk analytical-contract Mode 2 (indeks juga kontrak/analytical view, bukan hanya capability), dan perbaikan parser embedding Voyage. **Dibahas:** apakah retrieval index perlu kolom baru untuk contract/measure.

---

## UPGRADE (tabel dipertahankan, schema diadaptasi)

### `chat_sessions`
Sekarang: `id, api_key_id→(user_id), title, status(active/archived/expired), context_json, created/updated/expires/archived_at`.
Tambah/ubah: owner `user_id` (bukan api_key); pemisahan **durable history** vs **tier session-memory terstruktur** (ResolvedEntity/PriorResult/ActiveScope) — kemungkinan tabel `session_memory` terpisah (lihat REDESIGN memory) daripada `context_json` bebas. Concurrency: dukung aturan "1 job nonterminal/session".
Dibahas: session-memory sebagai tabel terpisah vs JSONB pada session; retensi/pagination history.

### `chat_messages`
Sekarang: `id, session_id, job_id, role(user/assistant/system/tool/clarification), content, metadata_json, created_at`.
Tambah/ubah: kemungkinan cukup; selaraskan `role`/metadata dengan response-document & clarification. Riwayat panjang → pagination (bukan load penuh).
Dibahas: apakah "message" tetap unit history utama atau turunan dari job/response.

### `chat_jobs` (REDESIGN — terberat)
Sekarang: satu `status` + `current_step`(vocab lama) + `resume_from_step` + `state_json`+`state_revision` + `workflow_id/contract_version/revision/current_node_id` + `api_key_id` + timestamps.
Ubah ke desain baru (engine.md):
- **Pisah 3 dimensi**: `lifecycle` (Queued/Running/WaitingForUser/Cancelling/Completed/Failed/Cancelled/Expired), `outcome` (Answered/Empty/NotFound/Unsupported/BlockedByPolicy/Invalid/OperationalFailure), `completeness` (Complete/Partial/Unknown). Ganti `status` tunggal.
- **Owner** `user_id` (bearer identity), bukan `api_key_id`.
- **`idempotency_key`** untuk create (api.md).
- **`plan_version`** (re-plan berversi); node identity pindah ke ledger.
- **`current_step`** lama → fase SSE = projection (bukan kolom sumber-kebenaran); kemungkinan disederhanakan/di-drop.
- Tambah `Cancelling` (belum ada) + waktu terminal yang tepat.
Dibahas: kolom relasional vs JSONB untuk plan; representasi outcome/completeness; retensi.

### `chat_job_checkpoints`
Sekarang: `id, job_id, step, checkpoint_type(...), state_json, created_at`.
Tambah/ubah: selaraskan boundary commit dengan node-run + response/memory-promotion (engine.md "persist response + memory promotion + completion event"). Tipe checkpoint disederhanakan ke boundary bermakna.
Dibahas: apakah checkpoint terpisah masih perlu vs status pada node-ledger + job.

### `chat_job_events` (SSE)
Sekarang: `id, job_id, event_type(...), step, payload_json, created_at` — **tanpa sequence monotonik**.
Tambah/ubah (WAJIB, api.md/sse.md): **`sequence` BIGINT strictly-increasing per job** + cursor opaque; `schema_version`; `occurred_at`; `plan_version`; kosakata event baru (`job.accepted/phase_changed`, `node.status_changed`, `clarification.required/accepted`, `job.resumed/notice/completed/failed/cancelled/expired`). Redis hanya notifikasi; PG durable + sequence.
Dibahas: threshold inline-size payload vs referensi; retensi replay.

### `chat_workflow_node_runs` → node-status ledger
Sekarang: `id, job_id, workflow_id, node_id, attempt, status(runnable/running/completed/failed/skipped/waiting), output_json, provenance_json, rows_returned, duration_ms, started/finished_at, UNIQUE(job,workflow,node,attempt)`.
Tambah/ubah: `node_kind` (Probe/CuratedQuery/AnalyticalQuery/Clarify/Compose/Respond); `plan_version`; `completeness` per node (Complete/Partial/Unknown, terpisah dari status); referensi `dataset_handle` (bukan salin hasil besar ke output_json); representasi dependensi/edge (di sini atau tabel plan-graph terpisah). Status `Pending` (fan-in belum siap) vs `runnable`.
Dibahas: simpan plan-graph (nodes+edges) sebagai kolom job JSONB atau tabel `chat_workflow_nodes`/`_edges` terpisah; di mana completeness dihitung.

---

## REDESIGN / CONSOLIDATE

### Memory: `assistant_job_memory` + `assistant_session_memory` → satu kontrak state + 3-tier
Lama: dua tabel memory ad-hoc terpisah (sumber inkonsistensi). Baru:
- **Job/request state**: pada `chat_jobs.state_json` + node-ledger (intent, plan-graph, hasil per-node, state klarifikasi).
- **Session tier (context-window)**: tabel `session_memory` terstruktur (PriorResult/ResolvedEntity/ActiveScope + provenance/completeness/handle ref) dengan summary incremental berversi.
- **Global**: deferred (slot `assistant_user_memory` didesain, tak dibangun).
`assistant_job_memory` → **DROP** (diganti job state). `assistant_session_memory` → **REDESIGN** ke `session_memory`.
Dibahas: skema `session_memory`; promotion/invalidation; watermark summary.

### Audit + telemetry: 4 tabel → satu model audit + telemetry terpisah
Lama terpecah: `chat_job_audit_events` (audit per stage), `management_audit_events` + `management_audit_outbox` (audit + outbox), `management_telemetry_counters` (telemetry), `assistant_llm_traces` (trace model). design-review: **audit ≠ logs/traces/metrics = kontrak terpisah**.
Baru:
- **Audit** (append-only, controlled evidence): satu tabel event audit dengan correlation IDs (request/session/job/node/query-attempt/model-call), contract/version, redacted scope, outcome/completeness, timing. Outbox hanya bila butuh delivery terjamin ke sistem eksternal — dibahas apakah masih perlu.
- **Model-call trace** (`assistant_llm_traces`) → bagian **observability** (provider/model/purpose/usage/correlation), bukan audit.
- **Telemetry counters** → observability/metrics (bukan tabel audit).
`chat_job_audit_events` + `management_audit_*` → **CONSOLIDATE** ke satu audit. `assistant_llm_traces` + `management_telemetry_counters` → **observability**.
Dibahas: satu tabel audit vs beberapa; perlukah outbox; retention & akses investigasi.

---

## DROP (engine lama; ada penggantinya)

| Drop | Pengganti |
| --- | --- |
| `assistant_job_memory` | `chat_jobs.state_json` + node-ledger (job state terpadu) |
| `assistant_graph_checkpoints` | `chat_workflow_node_runs` (node-status ledger) + checkpoint boundary |
| `assistant_original_intents`, `_fact_observations`, `_effective_constraints`, `_planner_input_snapshots` | sudah dihapus (canonical gateway) — tidak dihidupkan |

---

## Titik bahas (urutan usulan)
1. `chat_jobs` redesign (3 dimensi status, owner, idempotency, plan_version) — inti.
2. Node-ledger `chat_workflow_node_runs` (node_kind, completeness, plan-graph storage).
3. `chat_job_events` sequence + kosakata SSE.
4. Konsolidasi audit (4→1) + observability (traces/telemetry).
5. Memory (`session_memory` schema + job state).
6. `chat_sessions`/`chat_messages` + retensi/pagination.
7. CARRY: penyesuaian auth (tenant) & knowledge index.

Setiap keputusan menghasilkan definisi tabel/kolom/FK final di `data/database-design.md`. Dokumen ini menutup pertanyaan "port/ubah/buang"; ERD lengkap menyusul.

---

## Tabel baru yang dibutuhkan (tanpa padanan lama)

Ditemukan 2026-09-12. Klasifikasi di atas menjawab "tabel lama mau diapakan", tetapi karena review digerakkan oleh `migrations/` lama, **kebutuhan baru yang tidak punya padanan lama tidak terlihat**. Tujuh kelompok berikut wajib ada agar desain yang sudah disepakati benar-benar tertutup.

### A. Clarification forms
Sumber: [clarifications.md](../contracts/clarifications.md). Form **berversi milik job**: `clarification_id`, `revision`, `plan_version`, purpose, fields, suggestions, expiry, continuation bindings; jawaban dengan provenance `user_confirmed`; **option set terbitan server** (bounded + paginated resolver). Di skema lama semua ini terkubur di JSONB `chat_jobs`/`assistant_job_memory`.
Butuh: `clarification_forms`, `clarification_answers`, (opsional) `clarification_options`.

### B. Idempotency store
Sumber: [api.md](../contracts/api.md). `Idempotency-Key` wajib untuk **create job DAN submit klarifikasi (termasuk skip)**; harus **menyimpan acknowledgement asli** untuk replay; 409 bila payload berbeda; konvergensi ditegakkan DB; ada retention horizon. Kolom `idempotency_key` pada `chat_jobs` **tidak cukup**.
Butuh: `idempotency_keys` (key, principal, operation, target, payload_fingerprint, stored_response, status, created_at, expires_at) + unique constraint.

### C. Response documents
Sumber: PRD §9 + `contracts/responses.md` (planned). Dokumen **berversi** dari blok ter-approve + evidence lineage + completeness; `api.md` dan `sse.md` merujuk "final response reference" / "response version ... atau authorized retrieval reference". Lama hanya blob `chat_jobs.result_json`.
Butuh: `job_responses` (job_id, response_version, schema_version, blocks, completeness, outcome, created_at) + kemungkinan baris blok/evidence.

### D. Dataset handles + retained datasets
Sumber: PRD §6 + [tech-stack.md](../architecture/tech-stack.md) ("PostgreSQL-backed chunked retained datasets"). Ini **tulang punggung hasil besar**: node merujuk handle, butuh storage, schema/grain, row count, completeness, **stable pagination**, expiry. Klasifikasi lama hanya menyebut "referensi `dataset_handle`" tanpa storage.
Butuh: `datasets` + `dataset_chunks`.

### E. Plan storage / versioning
Sumber: [engine.md](../architecture/engine.md) — re-plan **berversi** dan diverifikasi ulang; audit harus menelusuri plan. Sebelumnya dibiarkan OPEN (JSONB vs tabel).
Butuh: `job_plans` (job_id, plan_version, graph, verified_at, contract versions) + representasi nodes/edges.

### F. Worker lease / fencing + concurrency
Sumber: engine.md + checklist §4 ("Enforcement idempotency, satu job per session, lease/fencing dan event sequence"). Sebelumnya **tidak disebut sama sekali**.
Butuh: kolom lease atau `job_leases` (worker_id, fence_token, lease_expires_at) + partial unique index untuk "1 job nonterminal per session".

### G. Kebutuhan durable dari keputusan dataset (D01, D03)
D01: laporan penutupan harus dapat **ditampilkan ulang persis seperti diterbitkan** → implikasi **snapshot laporan tersimpan** + retention/reproducibility. D03: **kurs yang dipakai harus dapat ditelusuri** → kurs yang digunakan harus tersimpan (Fineract core tidak menyediakan tabel FX umum).
Butuh: keputusan penyimpanan laporan terbit + `exchange_rates` (atau kurs direkam bersama laporan).

### Minor
Penyimpanan *controlled evidence* audit (akses terbatas, terpisah dari baris audit umum); referensi **contract/catalog version** pada node dan audit untuk reproducibility; kuota session/pengguna.

## Keputusan interaksi yang mempengaruhi schema (disepakati 2026-09-12)

### K1 — Input manual saat clarification
Mengetik manual **didukung** (tipe `text` + jalur no-match/refined search sudah ada). Aturan keamanan yang ditegaskan: **teks bebas untuk slot identitas adalah istilah pencarian, bukan binding** — ia memicu resolver ulang; hanya **option ID terbitan server** yang mengikat identitas.
Form perlu **jenis jawaban eksplisit** `answer_kind`: `option_id` (mengikat) · `typed_value` (field non-identitas) · `refine_search` · `change_intent` (ubah permintaan → re-plan, bukan isi field).
Schema: `clarification_answers` menyimpan `answer_kind`, teks mentah, dan binding hasil resolusi secara terpisah.

### K2 — Skip (aksi baru)
Skip **bukan abort**, melainkan **trigger action** yang menandai job selesai. Valid **hanya saat `WaitingForUser`**. Engine memperlakukannya sebagai jalur terminal normal: berhenti menerima kerja baru → susun response → **commit response + memory promotion + terminal event secara atomik** (jalur commit yang sama dengan jawaban normal).
- `lifecycle = Completed` · `outcome = SkippedByUser` (nilai enum **baru**) · `completeness = Partial/Unknown`.
- Response tetap dikembalikan ("Anda sudah memilih untuk tidak melanjutkan proses ini…") dan **menyertakan hasil parsial yang sudah valid** dengan gap ditandai eksplisit.
- **Context tersimpan ke memory pada commit itu**, sehingga follow-up berikutnya terjawab.
- Setelah skip job **terminal**; pertanyaan terkait berikutnya adalah **job baru** yang membaca session memory.
- Dikirim lewat `POST /chat/jobs/{id}/responses` dengan `action: "skip"` (bukan endpoint baru) → ikut jalur commit dan **idempotency store** (bagian B).
Schema: nilai outcome baru; `clarification_forms` terminal state `skipped` (+ aktor/waktu); `job_responses` berjenis skipped. Tidak ada tabel khusus skip.

### K3 — Cancel berbeda dari Skip
`cancel` (saat job `Running`) = abort → `lifecycle = Cancelled`, **tanpa** kewajiban response document dan **tanpa** memory promotion. `skip` = graceful finish → `Completed` + response + memory.

### K4 — Memory promotion tetap pada response commit
Promosi memory **tidak** dibuat inkremental (usulan itu ditarik). Titik promosi tunggal tetap **response commit**, yang kini mencakup skip. Konsekuensi yang diterima: **crash tanpa commit tidak menyimpan context** — tidak ada mekanisme khusus untuk itu. Yang tetap durable adalah output node + checkpoint, yaitu untuk **resume** (job dapat dilanjutkan worker; bila kemudian selesai, memory promosi normal). Bila job tidak pernah selesai, pertanyaan terkait berikutnya dijawab sebagai **retrieval baru**, bukan sambungan diam-diam — sesuai aturan PRD.

## Titik bahas (revisi 2026-09-12 — menggantikan daftar sebelumnya)

Tabel lama:
1. `chat_jobs` redesign (3 dimensi status, owner, idempotency, plan_version).
2. Node-ledger `chat_workflow_node_runs` (node_kind, completeness, penyimpanan plan-graph).
3. `chat_job_events` sequence + kosakata SSE.
4. Konsolidasi audit (4→1) + observability.
5. Memory (`session_memory` + job state).
6. `chat_sessions`/`chat_messages` + retensi/pagination.
7. CARRY: penyesuaian auth (tenant) dan knowledge index.

Tabel baru:
8. Clarification forms/answers/options (A) — termasuk `answer_kind` (K1) dan terminal `skipped` (K2).
9. Idempotency store (B) — mencakup create, jawaban klarifikasi, dan skip.
10. Response documents (C) — termasuk response berjenis skipped.
11. Datasets + dataset_chunks (D) — paling berdampak.
12. Job plans/versioning (E).
13. Job leases/fencing + concurrency constraint (F).
14. Snapshot laporan terbit + exchange rates (G).

---

## Keputusan #11 — `datasets` + `dataset_chunks` (DIKUNCI 2026-09-12)

**Chunk** = hasil satu node dipecah menjadi potongan berukuran tetap; tiap potongan = satu baris `dataset_chunks`. Tujuannya: pagination presisi (baca satu chunk, bukan scan), memori engine terbatas, urutan stabil, serta budget/purge per potongan.

`datasets` (satu baris = satu **handle**): `id` (handle), `job_id`/`node_id`/`plan_version`, `session_id`/`owner_user_id`, `schema_json`, `grain_json` (anti-fanout), `scope_json`, `provenance_json` (capability/contract + versi, as-of), `row_count_available`, `row_count_total` (NULL = tidak diketahui), `completeness` + `completeness_reason`, `truncated`, `sort_key_json` (syarat pagination stabil), `byte_size`, `chunk_count`, `status` (building/ready/failed/expired/purged), `created_at`/`expires_at`/`purged_at`.

`dataset_chunks`: PK `(dataset_id, chunk_index)`, `row_from`/`row_to`, payload, `row_count`, `byte_size`, **`format`** (mis. `json`) + **`encoding_version`**.

**Aturan yang dikunci**
1. Dataset **immutable** setelah `ready`; refresh = dataset baru (menjaga result identity).
2. Binding node hilir memakai **handle**, bukan daftar ID yang dibentangkan.
3. Tidak semua output diretensi: hasil kecil cukup inline di output node; retensi hanya bila dipakai node lain, menopang tabel berpaginasi, atau dirujuk session memory.
4. `completeness` (analitik) ≠ `truncated` (set tersimpan) ≠ preview (sisi response).
5. Otorisasi dicek ulang pada setiap baca (`owner_user_id` + scope); tidak pernah lintas user/session.
6. Expiry eksplisit: `expired`/`purged` → `session_memory` yang merujuk **wajib menyatakan** handle kedaluwarsa; pertanyaan lanjutan menjadi retrieval baru (konsisten K4).
7. `node_runs.dataset_id` (nullable FK) menghubungkan ledger ↔ dataset.

**Format payload**: mulai dengan **JSONB** (sederhana, tervalidasi, bisa diinspeksi; memungkinkan operasi ringan atas dataset kecil yang sudah lengkap). Kolom `format`+`encoding_version` ditambahkan sejak awal sebagai diskriminator, sehingga pindah ke **BYTEA terkompresi** kelak cukup menambah kolom + nilai format baru tanpa membongkar desain; chunk lama tetap terbaca. Keputusan final menunggu uji kapasitas dengan data nyata (tech-stack melarang mengasumsikan penyimpanan tak terbatas di PostgreSQL).

**Batas "SQL mengintip baris"**: query ke **sumber Fineract** tetap sering dan di awal (probe-first, pertanyaan bertingkat — didukung penuh). Yang dihindari adalah **menyaring ulang salinan retained kita**: dataset adalah snapshot, menyaring di dalamnya berisiko menjawab atas populasi basi/terpotong. Filter/agregasi diekspresikan ulang ke query sumber yang approved; engine membaca chunk secara utuh.

**Kepemilikan & penghapusan**: rantai `session → job → dataset → chunks`. Menghapus session **meng-CASCADE** dataset dan chunk (otorisasi diturunkan dari session; baris hasil bisa memuat PII; mencegah storage tumbuh tanpa batas). **Audit TIDAK ikut terhapus** — audit menyimpan metadata/provenance (bukan baris mentah) dan harus hidup lebih lama untuk investigasi. Menghapus session yang masih punya job nonterminal harus ditolak atau dibatalkan lebih dulu.

**Kuota berlapis**: per-dataset (max byte/baris → `truncated` + reason, tak pernah diam-diam) · per-job (total byte retained) · **per-session (kuota)** dengan eviction dataset terlama yang tidak dipakai job aktif → `expired`/`purged` · TTL `expires_at` + purge background. Eviction **tidak boleh** menyentuh dataset yang dibutuhkan plan berjalan. Angka pasti (byte, TTL, kuota) ditetapkan di dokumen operasional setelah uji kapasitas.

**Catatan**: engram/mem0 adalah tooling asisten pengembangan, **bukan** komponen Jarvis. Padanan konseptualnya di Jarvis adalah `session_memory` (fakta ringkas + handle), bukan `dataset_chunks` (baris data).

---

## Keputusan #12 — `job_plans`: penyimpanan & versioning plan-graph (DIKUNCI 2026-09-12)

Sekaligus menutup pertanyaan terbuka pada #2 (plan-graph disimpan di mana).

**Dipilih: satu dokumen JSONB immutable**, bukan tabel `plan_nodes`/`plan_edges` ternormalisasi. Alasan: plan kecil (1–20 node) dan **selalu dibaca utuh** oleh scheduler (fan-in dihitung di engine, bukan lewat SQL); **immutable = reproducibility eksak** atas dokumen yang benar-benar diverifikasi; menghindari dua tabel dan kerumitan join untuk struktur yang tak pernah di-query per-bagian. Bila kelak butuh query graph, tambahkan tabel proyeksi **tanpa** mengubah sumber kebenaran.

`job_plans`: `id` · `job_id` · `plan_version` (**UNIQUE (job_id, plan_version)**) · `graph_json` (nodes + edges + bindings, immutable) · `graph_hash` (integritas/audit) · `contract_versions_json` (capability/analytical-contract + **`catalog_version_id` (UUID) + `catalog_content_hash`** saat diverifikasi — **diamandemen #7**: teks `catalog_version` ditolak karena nilainya literal selalu `"local"`) · `verified_at` · `supersedes_plan_version` (NULL untuk plan pertama) · `replan_reason` · `created_at` · `superseded_at`.

Kaitan ledger: `chat_workflow_node_runs` menyimpan `(job_id, plan_version)` + `node_id` + `node_kind` secara **denormalisasi** agar bisa di-index/di-query tanpa membuka JSONB.

Terbuka: retensi versi plan lama (mengikuti retensi audit/job) dan batas jumlah re-plan per job (budget, angka di dokumen operasional).

---

## Keputusan #13 — Worker lease/fencing + concurrency session (DIKUNCI 2026-09-12)

**Lease sebagai kolom di `chat_jobs`** (bukan tabel terpisah; lease selalu 1:1 dengan job): `lease_owner` · `lease_token` (UUID, **fence token**, diregenerasi tiap klaim) · `lease_expires_at` · `lease_claimed_at` · `heartbeat_at`.

**Klaim atomik** lewat satu UPDATE bersyarat (`WHERE lifecycle='Queued' AND (lease_expires_at IS NULL OR lease_expires_at < now())` … `RETURNING lease_token`); pemilihan job antre memakai `SELECT … FOR UPDATE SKIP LOCKED`.

**Fencing (aturan inti)**: setiap tulisan durable worker menyertakan `AND lease_token = $token`. Bila lease sudah direbut, tulisan basi mengenai 0 baris dan worker wajib berhenti — inilah pencegah split-brain.

**Recovery attempt tak pasti**: bila lease kedaluwarsa saat node `Running`, worker baru menandai attempt itu gagal dan memulai **attempt baru** (`attempt+1`), tidak pernah mengasumsikan node itu tak berjalan. Karena Jarvis **hanya membaca** Fineract, mengulang eksekusi **aman**; biayanya hanya belanja query/model ganda yang dibatasi budget. Ini menyederhanakan recovery dibanding workflow engine umum yang harus menjaga efek samping.

**Constraint "1 job nonterminal per session"**: partial unique index `chat_jobs (session_id) WHERE lifecycle IN ('Queued','Running','WaitingForUser','Cancelling')`. Create yang bentrok → pelanggaran unique → dipetakan ke **409**. Catatan: daftar state pada predikat index bersifat tetap; menambah state nonterminal kelak memerlukan migrasi index.

**Cancellation settlement**: `cancel` menulis `Cancelling` + `cancel_requested_at`; worker pemegang lease melihatnya saat heartbeat/checkpoint, berhenti menerima node, menuntaskan attempt berjalan, lalu commit `Cancelled`. Bila lease sudah kedaluwarsa, reaper memindahkan `Cancelling` → `Cancelled`.

**Reaper periodik** (idempoten): mengedaluwarsakan lease, menandai job `Expired` (termasuk `WaitingForUser` yang melewati batas klarifikasi), dan purge dataset kedaluwarsa. Presedensi race: **state terminal menang**; karena semua transisi adalah UPDATE satu baris ber-token, pemenangnya deterministik.

Terbuka (angka → dokumen operasional): durasi lease, interval heartbeat, TTL job, batas tunggu klarifikasi.

---

## Keputusan #8 — Tabel klarifikasi: `clarification_forms` / `clarification_answers` / `clarification_options` (DIKUNCI 2026-09-12)

**Tiga tabel, options termasuk (bukan opsional).** Validasi keanggotaan option saat submit harus server-side; satu-satunya alternatif tanpa tabel adalah *signed option token* (kripto + masalah revocation) yang lebih mahal daripada satu tabel kecil.

### `clarification_forms` — satu baris per **revision** (immutable)

`id` · `job_id` (FK → `chat_jobs`, CASCADE) · `clarification_id` (stabil per stage, berulang lintas revision) · `revision` · **UNIQUE (job_id, clarification_id, revision)** · `plan_version` (form terikat pada plan yang memverifikasinya, #12) · `schema_version` · `purpose` · `stage_label` (label deskriptif, **bukan** "step 2 of 5") · `fields_json` (definisi field: semantic_type, required, constraints, dependencies, suggestions non-identitas) · `state` (`open` | `answered` | `superseded` | `skipped` | `expired` | `invalidated`) · `resolved_by_user_id` · `resolved_at` · `resolution_reason` · `superseded_by_revision` · `expires_at` · `created_at`.

Revision = **baris baru**, bukan update in-place: form adalah apa yang **dilihat pengguna saat menjawab**. Update in-place menghapus bukti tampilan dan membuat cek *stale revision* → 409 menjadi tebakan. Biayanya murah (form kecil, jumlah round dibatasi).

Terminal `skipped` (K2) memakai kolom resolusi generik yang sama — **tidak ada kolom maupun tabel khusus skip**.

Partial unique index: maksimal **satu form `open` per job**.

### `clarification_answers` — hanya jawaban yang **diterima**

PK `(form_id, field_id)` (form_id sudah mencakup revision) · `answer_kind` (`option_id` | `typed_value` | `refine_search` | `change_intent`, K1) · `raw_text` (teks apa adanya; NULL untuk pilihan murni) · `binding_json` (hasil resolusi bertipe; NULL untuk `refine_search`/`change_intent`; `multiple_choice` = array dalam satu baris, bukan multi-baris) · `provenance` (`user_confirmed`) · `resolver_ref` · `option_set_ref` · `answered_by_user_id` · `answered_at`.

Pemisahan `answer_kind` / `raw_text` / `binding_json` adalah inti aturan keamanan K1: teks bebas pada slot identitas tersimpan sebagai teks dan **tidak pernah** naik menjadi binding.

Jawaban yang **ditolak tidak masuk tabel ini** — masuk audit (#4). Bila dicampur, "accepted facts" tak bisa dibaca lurus dan setiap konsumen harus ingat memfilter.

### `clarification_options` — hanya option yang **benar-benar diterbitkan**

PK `(form_id, field_id, option_id)` · `binding_json` (binding bertipe yang diotorisasi saat terbit) · `label` · `attributes_json` (atribut pembeda yang diizinkan; berpotensi PII) · `resolver_ref` · `page_cursor` · `issued_at` · `expires_at`.

Yang dipersist hanya halaman yang **sudah dikirim** ke klien, bukan seluruh hasil resolver → pertumbuhan terbatas. Option ID otomatis ter-scope per form+field, sehingga ID dari job lain gagal lookup tanpa cek tambahan.

**Keanggotaan ≠ otorisasi.** Lookup di tabel ini hanya membuktikan "opsi ini pernah kami terbitkan untuk form ini". Otorisasi dan eksistensi tetap dicek ulang ke sumber saat submit (sesuai clarifications.md).

**Retensi**: options memuat nama nasabah → purge saat form terminal/kedaluwarsa (reaper #13). Audit menyimpan **referensi** option-set, bukan label mentah; pilihan sensitif masuk controlled evidence storage.

### Sengaja TIDAK dibuat
- Tabel skip — state form + outcome job sudah cukup (K2).
- Tabel `clarification_rounds` — jumlah round = `COUNT(revision)` per job.
- Idempotency di sini — milik #9.
- Normalisasi `fields_json` menjadi tabel field/constraint — selalu dibaca utuh (alasan sama dengan #12).

Terbuka (angka → dokumen operasional): page size resolver, TTL option, batas revision/round per job, batas panjang `raw_text`.

---

## Keputusan #9 — `idempotency_keys` (DIKUNCI 2026-09-12)

**Pola**: *idempotency key pattern* (at-least-once delivery + idempotent consumer = effectively-once). Ini **bukan fitur produk**, melainkan pelindung integritas tulis: tabel ini tidak menyimpan data analisis apa pun, hanya "request ini sudah pernah diproses, dan inilah acknowledgement yang diberikan saat itu".

**Masalah yang ditangani**: klik ganda, timeout jaringan (server sudah memproses, balasan hilang, klien retry), dan retry otomatis proxy/HTTP client. Ketiganya tampak identik di sisi server; tanpa penanda, server tidak dapat membedakan "dua permintaan" dari "satu permintaan yang sampai dua kali".

### Satu tabel generik untuk tiga operasi tulis

`POST /chat/jobs` (`job.create`) · `POST /chat/jobs/{id}/responses` dengan `action:"answer"` (`job.respond`) dan `action:"skip"` (`job.skip`). Satu tabel dengan kolom `operation` — mekanikanya identik, hanya labelnya berbeda.

`idempotency_keys`: `id` · `owner_user_id` · `operation` (`job.create` | `job.respond` | `job.skip`) · `idempotency_key` · **UNIQUE (owner_user_id, operation, idempotency_key)** · `request_fingerprint` (hash payload kanonik + path param, **bukan** body mentah) · `target_job_id` (NULL saat create belum commit) · `status` (`in_progress` | `completed`) · `response_status` · `response_body_json` (acknowledgement kecil: job_id + lifecycle, bukan hasil analisis) · `created_at` · `completed_at` · `expires_at`.

### Mekanika
1. `INSERT … ON CONFLICT DO NOTHING` dengan `status='in_progress'` — berhasil insert = pemilik operasi. Ini sekaligus pengganti lock.
2. Konflik + fingerprint **beda** → **409** (kunci sama, payload lain; bug klien dilaporkan eksplisit, tidak disembunyikan).
3. Konflik + fingerprint **sama** + `completed` → **replay `response_body_json` apa adanya**, tidak mengeksekusi ulang. Retry wajib mendapat `job_id` yang sama, jika tidak frontend kehilangan handle untuk memantau job yang sebenarnya berjalan.
4. Konflik + fingerprint sama + `in_progress` → **409 retryable**, bukan menunggu. Menunggu berarti menahan koneksi HTTP selama commit request lain dan berisiko menumpuk.

### Aturan inti: completion menyatu dengan efeknya
Baris `completed` + acknowledgement ditulis **dalam transaksi yang sama** dengan efek durable-nya (job row untuk create; answer + slot resolusi + state form + job runnable untuk respond/skip). Bila terpisah: crash setelah menandai key tetapi sebelum membuat job = permintaan user **hilang diam-diam** (retry ditolak sebagai duplikat padahal job tak pernah ada); urutan sebaliknya = job ganda. Ini jalur commit atomik yang sama dengan clarifications.md dan K2. Partial unique index #13 (1 job nonterminal per session) adalah jaring pengaman **kedua** untuk create, bukan jaminan yang sama — ia menolak berdasarkan session, bukan berdasarkan "request ini sudah diproses".

### Kenapa PostgreSQL, bukan cache in-memory/Redis
Multi-worker (request pertama di instance A, retry di instance B) · restart/deploy menghapus memori · dan yang terpenting: catatan completion **harus bisa ikut dalam transaksi PostgreSQL**.

### Fingerprint & scoping
Hash, bukan teks asli: payload klarifikasi dapat memuat PII; yang perlu diketahui hanya "sama atau tidak". Path param (`job_id`) ikut di-hash sehingga kunci yang dipakai ulang untuk job lain otomatis terdeteksi sebagai mismatch tanpa aturan tambahan. Scope **per `owner_user_id`**, tidak pernah global — kunci dibuat klien, sehingga kunci global memungkinkan user B menerima replay acknowledgement milik user A (kebocoran lintas user).

### Urutan cek di endpoint responses
`idempotency lookup` → replay bila cocok → `ownership` → `lifecycle` → `revision` → validasi field. Bila revision dicek lebih dulu, retry jaringan atas jawaban yang **sudah diterima** akan memperoleh 409 stale-revision (karena form sudah maju) padahal user tidak melakukan kesalahan.

### Sengaja TIDAK dibuat
- Tabel terpisah per endpoint — kolom `operation` cukup.
- Idempotency untuk GET dan `cancel` — cancel idempoten secara alami (state terminal menang, #13).
- Distributed lock — `ON CONFLICT` sudah menjadi mutual exclusion-nya.

Terbuka (angka → dokumen operasional): TTL `expires_at` (deduplication window; purge lewat reaper #13), batas panjang `idempotency_key`, apakah `response_body_json` dipurge lebih awal daripada barisnya.

---

## Keputusan #10 — `job_responses` (DIKUNCI 2026-09-12)

**Masalah pada desain lama** (`chat_jobs.result_json` sebagai blob): (a) tidak dapat ditelusuri — PRD §10 menuntut investigator dapat menelusuri "respons sukses yang salah" sampai plan/node/provenance/validasi, dan blob tanpa identitas/versi tidak menyimpan jejak itu; (b) tidak ada tempat bagi **kegagalan validasi response** (skenario wajib di PRD), karena menimpa in-place menghapus versi yang justru perlu diinvestigasi; (c) baris job bersifat mutable (lifecycle, lease, heartbeat) sedangkan dokumen yang dilihat pengguna harus menjadi **snapshot beku**.

**Dipilih: satu tabel, blok sebagai dokumen JSONB** — alasan identik dengan #12: dokumen kecil, **selalu dibaca utuh**, immutability = reproducibility. Tidak ada tabel `response_blocks`/`response_evidence` terpisah.

`job_responses`: `id` · `job_id` (FK → `chat_jobs`, CASCADE) · `response_version` · **UNIQUE (job_id, response_version)** · `schema_version` · `plan_version` (#12) · `kind` (`analysis` | `skipped` | `limitation`) · `outcome` · `completeness` + `completeness_reason` · `blocks_json` (blok terurut: metric, table, chart_spec, comparison, finding, limitation, suggestion) · `evidence_json` (lineage: dataset handle, node_id, contract+catalog version, as-of) · `validation_status` (`passed` | `failed` | `fallback`) · `validation_report_json` · `superseded_by_version` · `response_hash` (identitas untuk audit) · `composed_at` · `created_at`.

**Versi = baris baru, immutable.** Normalnya satu job = satu baris. Versi kedua muncul **hanya** ketika versi pertama gagal validasi dan sistem menyusun versi fallback yang lebih konservatif. Versi yang ditolak **tetap disimpan** — itu bahan investigasi.

**`outcome`/`completeness` disalin ke sini; `chat_jobs` tetap otoritatif untuk lifecycle.** Response adalah snapshot "apa yang diklaim saat komposisi". Bila render dokumen lama harus join ke job yang mutable, makna dokumen bisa berubah di kemudian hari.

**Tabel besar tidak disalin ke dalam blok**: blok tabel merujuk **dataset handle** + info pagination (#11); baris mentah tetap di `dataset_chunks`. Hasil kecil boleh inline (aturan 3 pada #11). Tanpa aturan ini hasil besar tersimpan dua kali dan preview/truncation menjadi ambigu.

**`kind = skipped`**, bukan tabel terpisah (K2): skip menghasilkan response document biasa berisi hasil parsial yang valid + gap eksplisit.

**Narasi LLM** tersimpan di dalam blok (bukan kolom sendiri), ditandai aditif dan tergrounding evidence — data tetap otoritatif (PRD §9); narasi tidak di-stream sebelum tervalidasi (sse.md).

**Suggestion** tersimpan sebagai blok tetapi **bukan aksi**: menjalankannya adalah job baru dengan otorisasi baru, tidak ada jalur resume dari suggestion.

**Retensi/PII**: dokumen memuat nilai terformat termasuk PII → **CASCADE bersama session** (sama seperti datasets). Audit menyimpan `response_hash` + identitas versi, **bukan** isinya, sehingga jejak investigasi tetap hidup setelah konten dihapus.

### Sengaja TIDAK dibuat
- `response_blocks`/`response_evidence` sebagai tabel — tak pernah di-query per bagian. Bila kelak perlu ("respons mana yang memakai dataset X"), tambahkan **GIN index** pada `evidence_json` atau tabel proyeksi, tanpa mengubah sumber kebenaran.
- Tabel skip — `kind` sudah cukup.
- Kolom hasil streaming parsial — progress adalah event (#3), bukan dokumen.

Terbuka (ops doc): retensi response vs retensi audit, batas ukuran `blocks_json`, apakah versi `failed` dipurge lebih cepat daripada versi yang disajikan.

---

## Keputusan #1 — `chat_jobs` redesign (DIKUNCI 2026-09-12)

Tabel lama mencampur lima peran dalam satu baris: status, posisi eksekusi (`current_step`/`resume_from_step`/`current_node_id`), state bebas (`state_json` + `state_revision`), identitas workflow, dan kepemilikan via `api_key_id`. Keputusan #8–#13 sudah memindahkan empat di antaranya keluar (plan → `job_plans`, node → ledger, klarifikasi → `clarification_forms`, hasil → `job_responses`), sehingga #1 sebagian besar adalah **membuang**, bukan menambah.

### Kolom

**Identitas & kepemilikan**: `id` · `session_id` (FK → `chat_sessions`, CASCADE) · `owner_user_id` (bearer identity, **bukan `api_key_id`**) · `scope_json` (snapshot office scope/otorisasi saat accept).

**Permintaan**: `request_text` (pertanyaan asli apa adanya) · `request_json` (interpretasi terstruktur).

**Status — 3 dimensi terpisah**: `lifecycle` NOT NULL (`Queued`|`Running`|`WaitingForUser`|`Cancelling`|`Completed`|`Failed`|`Cancelled`|`Expired`) · `outcome` NULL (`Answered`|`Empty`|`NotFound`|`Unsupported`|`BlockedByPolicy`|`Invalid`|`OperationalFailure`|`SkippedByUser`) · `completeness` NULL (`Complete`|`Partial`|`Unknown`) + `completeness_reason` · `failure_code` NULL (machine-readable, tersanitasi; detail ke audit).

**Pointer (denormalisasi baca)**: `plan_version` (aktif; `job_plans` otoritatif) · `final_response_version` (NULL sampai commit; `job_responses` otoritatif) · `last_event_sequence`.

**Budget (kolom eksplisit)**: `query_count` · `model_call_count` · `token_cost` · `replan_count`.

**Lease/fencing (#13)**: `lease_owner` · `lease_token` · `lease_expires_at` · `lease_claimed_at` · `heartbeat_at` · `cancel_requested_at`.

**Waktu**: `created_at` · `started_at` · `waiting_since` · `terminal_at` · `expires_at` · `updated_at`.

### Keputusan
- **Tiga dimensi = tiga kolom + CHECK**: `lifecycle` selalu terisi; `outcome`/`completeness` NULL sampai terminal; CHECK `lifecycle` terminal ⇒ `outcome` NOT NULL. Mencegah kelas bug lama "Completed padahal kosong/gagal" (engine.md: Completed hanya berarti response tervalidasi sudah durable).
- **DIBUANG**: `current_step`, `resume_from_step`, `current_node_id`, `workflow_id` — posisi eksekusi adalah turunan node ledger (#2); menyimpannya sebagai kolom = dua sumber kebenaran yang bisa berbeda setelah crash. Fase SSE adalah **proyeksi**, bukan kolom.
- **DIBUANG**: `state_revision` — optimistic locking digantikan `lease_token` (#13) yang lebih kuat (mencegah split-brain, bukan sekadar lost update).
- `state_json` menyusut menjadi `request_json` saja.
- **Budget sebagai kolom, bukan JSON**: dibandingkan terhadap limit di setiap admission dan butuh increment atomik murah; di dalam JSONB berarti read-modify-write penuh tiap node.
- **`idempotency_key` TIDAK ada di sini** — #9 sudah memilikinya via `target_job_id`; menyalinnya = dua sumber kebenaran untuk satu jaminan.
- **`scope_json` disnapshot saat accept** karena entitlement dapat berubah di tengah job dan audit harus tahu scope yang berlaku saat keputusan dibuat. Ini **bukan** pengganti pengecekan ulang otorisasi saat baca (#11 aturan 5).
- **`expires_at`** memberi reaper #13 target eksplisit, termasuk `WaitingForUser` yang melewati batas tunggu klarifikasi.
- **`scope_json` merekam nilai PII efektif**, bukan hanya office scope: `{"pii": {"enabled": …, "setting_version": N}, "source": "admin_projection"|"fineract_derived", "office_ids": […], "fineract_tenant": "…"}` (#15).

### AMANDEMEN (2026-09-13) — `expires_at` dihitung ulang per fase (menutup konflik K3)
Satu kolom `expires_at` harus memuat **dua TTL berbeda**: TTL job berjalan (nilai awal 30 menit) dan batas tunggu klarifikasi (nilai awal 2 jam). Satu kolom tidak dapat memuat keduanya sekaligus, dan tanpa aturan ini **setiap klarifikasi yang dijawab lebih dari 30 menit akan di-`Expired` reaper di tengah percakapan yang sah**.

**Aturan tulis (bukan angka)**: `expires_at` **dihitung ulang pada setiap transisi fase** — saat masuk `WaitingForUser` menjadi `waiting_since + CLARIFICATION_WAIT_LIMIT`, dan dihitung ulang lagi saat resume menjadi `now() + JOB_TTL_RUNNING`. Ditulis dalam transaksi transisi yang sama.

### AMANDEMEN (2026-09-13) — kebutuhan retensi dinyatakan saat verifikasi plan (menutup konflik K16)
`JOB_RETAINED_BYTES_CAP` ÷ `DATASET_MAX_BYTES` hanya menyisakan beberapa dataset berukuran maksimum, sementara plan boleh berisi hingga 20 node (#12). Bila lebih banyak node menghasilkan dataset besar, hasilnya terpotong **bergantung urutan eksekusi node** — truncation non-deterministik, yang dilarang semangat #11 ("tak pernah diam-diam") dan #2 (`completeness` harus bermakna).
**Aturan**: kebutuhan retensi per node dinyatakan **eksplisit saat verifikasi plan**, sehingga plan yang membutuhkan lebih banyak dataset besar daripada yang diizinkan **ditolak sebelum dijalankan**, bukan terpotong di tengah. Sejalan dengan #12 (plan diverifikasi sebelum eksekusi).

### Alokasi `last_event_sequence` (aturan, bukan sekadar kolom)
Sequence per job yang **strictly increasing dan tanpa lubang** (#3) dialokasikan lewat `UPDATE chat_jobs SET last_event_sequence = last_event_sequence + 1 … RETURNING`, **dalam transaksi yang sama** dengan insert event. **Row lock baris job adalah alokatornya.** Sequence PostgreSQL tidak dipakai karena meninggalkan lubang saat rollback; tabel terpisah berarti satu lock tambahan untuk jaminan yang sama. Ini penting karena jalur HTTP (`job.accepted`, `clarification.accepted`) dan worker sama-sama memancarkan event dan membutuhkan alokator yang sama. Kolom ini juga yang membuat `GET /chat/jobs/{id}` dapat mengembalikan snapshot **beserta** cursor yang konsisten dengannya (api.md), tanpa celah antara membaca state dan membaca cursor.

### Satu tabel: analisis dan penyesuaian (ditinjau ulang atas permintaan, 2026-09-12)

Kriteria pemisahan adalah **pola tulis**, bukan jumlah kolom. Rasionya ekstrem: identitas/permintaan ditulis 1×, sedangkan `heartbeat_at` ditulis tiap beberapa detik (~60 versi baris untuk job 5 menit, ~90 termasuk budget + status). PostgreSQL menulis **versi baris baru** setiap UPDATE → risiko bloat, amplifikasi WAL, beban autovacuum, dan amplifikasi tulis index.

Hasil analisis: **satu tabel cukup**, dengan tiga penyesuaian yang diputuskan sekarang:
1. **`fillfactor = 85`** pada `chat_jobs` agar tersedia ruang page untuk **HOT update** (versi baru tinggal di page yang sama, index tidak ditulis ulang).
2. **Index klaim dipersempit**: parsial pada `lifecycle` saja (`WHERE lifecycle = 'Queued'`), `lease_expires_at` **tidak** di-index dan cukup menjadi predikat filter. HOT update batal bila ada kolom ter-index yang berubah — meng-index `lease_expires_at` akan mematikan HOT tepat pada operasi paling sering (perpanjangan lease).
3. Aturan alokasi sequence di atas.

Alasan menolak pemisahan:
- **JSONB besar bukan masalah**: nilai ter-TOAST **tidak ditulis ulang** bila kolomnya tidak berubah — versi baris baru menyalin pointer TOAST. Ini menghapus argumen terkuat untuk memisahkan `request_json`/`scope_json`; memisahkannya hanya menambah join pada setiap pembacaan job.
- **Tidak ada kontensi antar-worker**: satu job hanya punya satu pemegang lease (#13) dan semua tulisan durable berpagar `lease_token`, sehingga node paralel tetap menulis dari worker yang sama. `job_budgets` terpisah tidak menghindari kontensi apa pun, malah membuat transaksi menyentuh dua baris.
- **`job_status_history` ditolak**: riwayat transisi sudah tercatat di event (#3) dan audit (#4); tabel ketiga = sumber kebenaran ketiga.

**Jalur upgrade (ponytail: plafon + tombol pertama)**: bila churn heartbeat terbukti mengganggu pada uji kapasitas, pisahkan **hanya `job_leases` 1:1** (baris sempit, tanpa kolom besar, index sendiri). Migrasi aditif — tidak ada semantik baca yang berubah, hanya jalur klaim/heartbeat. **Pemicu terukur**: bloat tabel `chat_jobs` atau latensi autovacuum melewati ambang operasional. Tidak dibangun sekarang: volume Jarvis adalah alat internal admin (puluhan–ratusan job/hari), jauh di bawah ambang mana pun; membangunnya di awal adalah over-engineering.

### Index
- Partial unique `(session_id) WHERE lifecycle IN ('Queued','Running','WaitingForUser','Cancelling')` — dari #13.
- Partial `(lifecycle) WHERE lifecycle = 'Queued'` untuk klaim worker (`FOR UPDATE SKIP LOCKED`).
- `(owner_user_id, created_at DESC)` untuk daftar job pengguna.
- `(expires_at)` parsial pada lifecycle nonterminal untuk reaper.

### Tumpang tindih yang ditandai — DIKOREKSI oleh #6 (2026-09-13)
Catatan semula menyebut `request_text` sebagai "duplikasi yang disengaja antara job dan `chat_messages`". **Alasan itu salah.** #6 menjadikan `chat_messages` indeks tipis **tanpa kolom `content`**, sehingga **tidak ada duplikasi fisik**, dan `chat_messages` selalu hidup-mati bersama job lewat CASCADE — tidak ada skenario retensi terpisah di antara keduanya.
Pemisahan retensi yang sesungguhnya berlaku antara **job dan audit** (#4): audit hidup lebih lama, tanpa FK, dan menyimpan hash/scope tersanitasi. Kesimpulan strukturalnya tetap berlaku: `request_text` tetap di `chat_jobs`.

### AMANDEMEN — `fillfactor = 85` wajib diverifikasi, bukan diasumsikan (konflik K17)
Ruang bebas 15% dibagi **seluruh baris dalam satu page**; dengan baris `chat_jobs` yang lebar, satu page 8 KB menyisakan ruang untuk beberapa versi baris saja sebelum HOT chain putus. Ini tidak dapat ditebak. **Pemicu terukur**: `n_tup_hot_upd / n_tup_upd` < **0.80** atau `n_dead_tup/n_live_tup` > **20%** di antara dua autovacuum. Urutan respons **wajib berurutan**: (1) turunkan `fillfactor` ke 70, (2) **baru** jalankan pemisahan `job_leases` 1:1. Jangan melompat ke (2).

Terbuka (ops doc): retensi job vs audit, TTL default, batas `replan_count` dan budget, apakah `request_json` dipurge lebih awal (berpotensi PII).

---

## Keputusan #2 — node ledger `job_node_runs` (DIKUNCI 2026-09-12)

**Rename**: `chat_workflow_node_runs` → **`job_node_runs`**. `workflow_id` dibuang pada #1 dan plan kini berversi (#12), sehingga nama lama menyesatkan. Repo baru = migrasi baru, rename tanpa biaya.

`job_node_runs`: `id` · `job_id` (FK → `chat_jobs`, CASCADE) · `plan_version` (denormalisasi dari #12, untuk index) · `node_id` (stabil di dalam `graph_json`) · `node_kind` (`Probe`|`CuratedQuery`|`AnalyticalQuery`|`Clarify`|`Compose`|`Respond`) · `attempt` (mulai 1) · **UNIQUE (job_id, plan_version, node_id, attempt)** · `status` (`Pending`|`Runnable`|`Running`|`Completed`|`Failed`|`Skipped`|`Abandoned`) · `completeness` + `completeness_reason` · `failure_code` (tersanitasi; detail ke audit) · `input_binding_json` + `input_binding_hash` · `dataset_id` (FK → `datasets`, nullable, #11 aturan 7) · `output_json` (hanya hasil kecil) · `provenance_json` (capability/contract + versi, **`catalog_version_id` + `catalog_content_hash`** — **diamandemen #7**, bukan teks `catalog_version`; as-of; untuk node konsolidasi multi-currency juga `exchange_rate_id` yang dipakai, #14) · `reused_from_node_run_id` (nullable) · `rows_returned` · `duration_ms` · `started_at` · `finished_at` · `created_at`.

### Keputusan
- **`Abandoned` adalah status tersendiri, bukan `Failed`.** #13 menetapkan lease kedaluwarsa saat node `Running` → attempt ditutup dan attempt+1 dimulai **tanpa** mengasumsikan node tidak berjalan. "Gagal karena query error" (deterministik, diketahui) berbeda fundamental dari "tidak diketahui, mungkin sudah berjalan". Menyamakan keduanya menghapus kemampuan audit membedakannya — padahal PRD eksplisit menolak menjanjikan exactly-once pada eksekusi eksternal. **Ketidakpastian harus terlihat di data.**
- **`completeness` terpisah dari `status`**: node dapat `Completed` **dan** `Partial` (query sukses tetapi kena batas baris). Menggabungkannya adalah persis mekanisme lama yang menghasilkan "sukses tetapi jawabannya salah".
- **`Pending` vs `Runnable` dipisah**: `Pending` = fan-in belum terpenuhi; `Runnable` = dependensi selesai, menunggu slot. Tanpa pemisahan ini scheduler tidak dapat membedakan "belum boleh" dari "boleh tetapi antre", dan diagnosa job macet menjadi tebakan.
- **Edge/dependensi tidak disimpan di sini** (final pada #12): `graph_json` memiliki nodes + edges, fan-in dihitung di engine. Ledger hanya mendenormalisasi `node_id`/`node_kind` agar dapat di-index.
- **`input_binding_hash` untuk kelayakan reuse**: engine.md mewajibkan output dipakai ulang hanya bila binding, scope, versi kontrak dan kesegaran masih valid. Tanpa merekam binding yang **benar-benar dikonsumsi**, kelayakan reuse hanya ditebak dari plan — sedangkan plan dapat berubah.
- **Reuse lintas re-plan = baris baru + `reused_from_node_run_id`**: ketika plan v2 memakai ulang output node dari v1, ledger v2 memperoleh baris `Completed` yang menunjuk baris v1 tanpa eksekusi ulang. Scheduler cukup membaca ledger pada `plan_version` aktif — satu query, tanpa logika lintas-versi. Alternatifnya (tanpa baris baru, scheduler menyapu semua versi) memindahkan kerumitan ke setiap pembacaan.

### `chat_job_checkpoints` → **DROP** (merevisi klasifikasi awal UPGRADE)
Setelah #12 (plan immutable) dan ledger ini, tidak ada data yang disimpan checkpoint namun tidak ada di tempat lain: batas commit yang bermakna **adalah** baris node run yang selesai, ditambah commit response (#10). Tabel checkpoint hanya menduplikasinya dengan skema lebih longgar.

Yang hilang: resume **di tengah** satu node (mis. fetch paginasi berhenti di halaman 7). Itu memang tidak dibutuhkan — #13 sudah menetapkan mengulang node **aman** karena Jarvis read-only, biayanya hanya belanja query yang dibatasi budget. Membangun resume mid-node demi penghematan itu adalah over-engineering.

### Sengaja TIDAK dibuat
- Tabel `chat_workflow_nodes`/`_edges` — ditutup #12.
- Kolom biaya per node — akuntansi budget ada di `chat_jobs` (#1), rincian per panggilan di observability (#4).
- `progress_json` mid-node — lihat alasan drop checkpoint.

### Index
UNIQUE `(job_id, plan_version, node_id, attempt)` · `(job_id, plan_version, status)` (query utama scheduler) · `(dataset_id)` (purge/eviction #11).

Terbuka (ops doc): batas `attempt` per node, ambang ukuran `output_json` inline sebelum wajib menjadi dataset, retensi ledger vs audit.

---

## Keputusan #3 — `job_events` (DIKUNCI 2026-09-12)

**Rename** `chat_job_events` → **`job_events`**, konsisten dengan #2.

`job_events`: **PRIMARY KEY (job_id, sequence)** — tanpa kolom `id` surrogate · `job_id` (FK → `chat_jobs`, CASCADE) · `sequence` BIGINT (dialokasikan lewat row lock job, #1) · `schema_version` · `event_type` (11 nilai sse.md) · `occurred_at` (waktu commit boundary) · `plan_version` (NULL pada `job.accepted`) · `node_id` + `node_attempt` (NULL kecuali `node.status_changed`) · `clarification_id` + `clarification_revision` (NULL kecuali event klarifikasi) · `response_version` (NULL kecuali `job.completed`) · `payload_json` (bounded) · `payload_truncated`.

### Keputusan
- **PK `(job_id, sequence)` tanpa surrogate `id`**: satu-satunya query replay adalah `WHERE job_id = ? AND sequence > cursor ORDER BY sequence`, yaitu index scan langsung pada PK. Kolom `id` hanya menambah index kedua yang tak pernah dipakai.
- **Referensi bertipe menjadi kolom, bukan sekadar isi payload.** sse.md mengizinkan "bounded payload **atau** retrieval reference" tetapi membiarkan ambangnya terbuka. Dengan `clarification_id`/`revision`, `response_version`, `node_id` sebagai kolom, referensi **selalu** tersedia berapa pun ambangnya; `payload_truncated` menandai klien harus mengambil via endpoint. Keputusan ambang ukuran karena itu dapat diubah **tanpa** migrasi skema.
- **Append-only**: tidak pernah UPDATE, tidak pernah DELETE satuan — penghapusan hanya purge massal berbasis usia. Inilah yang membuat `sequence` dapat dipercaya sebagai cursor.
- **Publish Redis HANYA setelah commit.** Bila notifikasi dikirim sebelum/di dalam transaksi, subscriber dapat terbangun, membaca PostgreSQL, **tidak menemukan** event tersebut, lalu menyimpulkan tidak ada yang baru. Urutan wajib: commit → notifikasi. Notifikasi yang hilang dipulihkan polling fallback terbatas (sse.md: Redis bukan sumber event durable).
- **Heartbeat tidak disimpan** (sse.md eksplisit): ia komentar transport, bukan progres. Menyimpannya membanjiri tabel dan membuat "job masih hidup" tampak seperti "job masih maju" — padahal deteksi worker macet adalah tugas lease (#13).
- **`phase` tidak menjadi kolom di mana pun**, hanya field dalam payload `job.phase_changed` — konsisten dengan #1 (fase = proyeksi pengalaman, bukan state machine kedua).
- **Cursor = `sequence` terenkode opaque**, divalidasi server terhadap job terotorisasi, dan **bukan** kredensial otorisasi (sse.md). PK sudah `(job_id, sequence)`, tidak perlu struktur cursor lain.
- **Validitas cursor diturunkan, bukan disimpan**: tidak ada kolom `expires_at`. Purge berbasis usia; bila cursor yang diminta lebih kecil dari `sequence` terkecil yang tersisa untuk job itu, server menjawab "cursor kedaluwarsa, ambil snapshot baru" (sse.md butir 5) — riwayat yang hilang tidak pernah disembunyikan.

### Index
- PK `(job_id, sequence)` menutup seluruh jalur replay.
- **BRIN pada `occurred_at`** untuk purge: tabel append-only dengan waktu berkorelasi urutan fisik adalah kasus penggunaan BRIN yang tepat — jauh lebih kecil daripada B-tree dan tidak membebani jalur insert terpanas.

### Sengaja TIDAK dibuat
- Tabel payload terpisah — referensi bertipe sudah menggantikannya.
- `created_at` terpisah dari `occurred_at` — event ditulis pada commit, satu stempel waktu cukup.
- **Partisi tabel** — ini tabel dengan volume tulis tertinggi (satu baris tiap transisi node), tetapi untuk alat admin internal masih kecil. **Jalur upgrade**: partisi per rentang waktu bila biaya purge atau ukuran tabel melewati ambang operasional; tidak mengubah semantik baca.

Terbuka (ops doc): jendela retensi replay, ambang ukuran payload inline, interval dan batas polling fallback, interval heartbeat transport.

---

## Keputusan #4 — Konsolidasi audit (4→1) + observability (DIKUNCI 2026-09-13)

Empat tabel lama menjawab satu pertanyaan dengan empat bentuk berbeda dan tak satu pun menjawabnya penuh. **Bukti yang menutup pembahasan** (ditemukan saat riset, bukan asumsi):

1. **Outbox lama tidak pernah punya sistem eksternal.** `crates/chat/src/management/outbox.rs:99` — `publish()` berisi `INSERT INTO management_audit_events`, tabel PostgreSQL sebelah, database dan transaksi yang sama. Polanya degenerate sejak awal.
2. **Skema audit lama punya bug fungsional.** `management_audit_events` memasang trigger `BEFORE UPDATE OR DELETE … RAISE EXCEPTION` **sekaligus** FK `session_id … ON DELETE SET NULL`. Menghapus session memicu cascade SET NULL → trigger raise → **penghapusan session gagal**. Trigger yang sama membuat purge retensi mustahil.
3. **Audit asinkron bertentangan dengan gerbang yang sudah dikunci**: api.md dan engine.md menuntut audit persist **sebelum** transisi terlindungi maju; outbox menjamin kebalikannya.

### Dua tabel: `audit_events` + `audit_evidence`. Nol outbox, nol tabel trace, nol tabel counter.

`audit_events`: `id` UUID PK (**UUIDv7**, urutan fisik ≈ urutan waktu) · `schema_version` · `occurred_at` · `duration_ms` · **korelasi**: `request_id`, `actor_kind` (`user`|`worker`|`reaper`|`system`), `actor_user_id`, `session_id`, `job_id`, `plan_version`, `node_id`, `node_attempt`, `query_attempt_id`, `model_call_id` · **referensi bertipe** (pola #3): `clarification_id`, `clarification_revision`, `response_version`, `response_hash`, `dataset_id`, `graph_hash` · `stage` (`accept`|`authorize`|`context`|`plan`|`plan_verify`|`clarify`|`node_execute`|`source_query`|`model_call`|`compose`|`response_validate`|`commit`|`settle`|`data_access`|`admin`) · `action` · `result` (`ok`|`denied`|`invalid`|`failed`|`deferred`) · `failure_code` · `job_outcome` + `job_completeness` + `completeness_reason` (**hanya pada baris `commit`/`settle`**) · `catalog_version_id` + `catalog_content_hash` · `contract_refs_json` · `scope_json` (redacted) · `detail_json` · `has_evidence`.

`audit_evidence`: `id` · `audit_event_id` FK CASCADE · `kind` · `payload_json` **NULL** · `redaction_level` · `created_at` · `expires_at` · `purged_at`.

### Keputusan
- **`result` dipisah dari `job_outcome`** — menyelesaikan tabrakan kosakata: #1 sudah mengunci `outcome` sebagai kosakata **job-level**, sedangkan audit butuh hasil **per-langkah**. Satu nama untuk dua kosakata adalah jebakan. `job_outcome`/`job_completeness` memakai kosakata #1 persis dan hanya terisi pada baris commit/settle.
- **Evidence wajib tabel, bukan kolom.** Alasannya **bukan** akses (PostgreSQL bisa `GRANT SELECT (kolom)`) melainkan **retensi**: pilihan sensitif harus dipurge lebih cepat daripada baris auditnya, dan memurge isi kolom berarti **UPDATE baris audit** — persis yang dilarang append-only. Di tabel terpisah, purge = `payload_json := NULL, purged_at := now()`, dan barisnya tetap menjadi bukti "evidence pernah ada lalu dipurge".
- **FK dari evidence → audit**, ditambah `has_evidence` yang diset saat insert, sehingga `audit_events` tidak punya satu pun kolom yang perlu di-UPDATE setelah insert.
- **Append-only via GRANT**: `REVOKE UPDATE, DELETE ON audit_events FROM <app_role>` (peran `audit_purge` terpisah memegang DELETE), ditambah trigger `BEFORE UPDATE` sebagai jaring kedua — **tidak untuk DELETE** (klausa DELETE pada trigger lama justru yang membuat purge mustahil).
- **Kegagalan tulis audit tidak butuh kebijakan**, karena tidak ada jalur tulis audit yang berdiri sendiri: setiap baris ditulis **dalam transaksi yang sama** dengan efek yang dijelaskannya. INSERT gagal → rollback → tidak ada acknowledgement, tidak ada kemajuan. **Gerbangnya adalah transaksi.** Fencing #13 ikut gratis: tulisan worker basi mengenai 0 baris, transaksi rollback, audit basi tak pernah tertulis. **Pengecualian tunggal**: penolakan jalur HTTP tanpa efek durable lain (401/403/409, stale revision, idempotency replay) — audit best-effort, dan request tetap gagal tertutup.
- **Audit tidak punya satu pun FK** ke session/job/user/dataset; semuanya UUID polos. Inilah mekanisme yang membuat aturan #11 ("audit tidak ikut terhapus") benar-benar berlaku. **`job_id` menggantung setelah purge adalah KONDISI NORMAL, bukan korupsi** — ditulis eksplisit agar migrasi berikutnya tidak "memperbaikinya" menjadi FK. `ON DELETE SET NULL` gaya lama ditolak: itu UPDATE pada baris audit.
- **`assistant_llm_traces` tidak diport**: audit sudah wajib mencatat panggilan model (PRD §10), jadi `stage='model_call'` memuat provider/model/prompt_version/purpose/token usage. Tabel trace = sumber kebenaran kedua. **Akuntansi budget tetap di `chat_jobs` (#1)** dan tidak bergantung observability — metrics boleh hilang/di-sample, budget tidak boleh. `management_telemetry_counters` dibuang tanpa pengganti (metrik tentang pipeline metrik).
- **Tidak ada sequence global**: alokator row-lock #1 tidak tersedia untuk baris tanpa `job_id` (admin, penolakan auth, reaper). UUIDv7 + keyset `(occurred_at, id)` cukup; audit bukan kontrak publik (SSE yang kontrak publik).
- **`stage` bukan state machine kedua** — hanya melabeli baris, tidak pernah dibaca balik engine. Konsisten dengan membuang `current_step` (#1) dan `phase` (#3). `layer`/`blueprint_step` dibuang, bukan diganti nama.

### Sengaja TIDAK dibuat
`audit_outbox` (tabel audit append-only **adalah** outbox-nya bila sink eksternal muncul; cukup tambah satu baris cursor) · `llm_traces`/`model_calls` · `telemetry_counters` · `audit_access_log` (regresi tak terbatas; akses dicatat grant/log DB) · kolom `payload`/`prompt`/`sql_text` mentah · kolom `severity` (properti log, bukan audit) · normalisasi `detail_json`/`scope_json` · partisi.

### Index
PK `(id)` UUIDv7 · parsial `(job_id, occurred_at) WHERE job_id IS NOT NULL` (jalur investigasi utama PRD §10) · parsial `(actor_user_id, occurred_at DESC)` · **BRIN `(occurred_at)`**. Dibuang dari desain lama: `(stage, created_at)`, `(blueprint_step, …)`, `(api_key_id, …)`, feed global.
**Tidak dipartisi**: ~15–40 baris/job → ribuan baris/hari, **lebih rendah** daripada `job_events` yang sudah diputuskan tidak dipartisi (#3). Jalur upgrade: RANGE partition per bulan; pemicu = biaya purge atau ukuran tabel melewati ambang operasional.

Terbuka (ops doc): horizon retensi audit (**keputusan compliance, bukan engineering**) dan horizon evidence yang lebih pendek · nama & pemisahan peran DB · ambang ukuran `detail_json` sebelum pindah ke evidence · kosakata `action` final · retensi baris `stage='data_access'`.

---

## Keputusan #5 — `session_memory` (DIKUNCI 2026-09-13)

`assistant_session_memory` lama: satu baris per session berisi `summary` + `entities_json` array yang di-update in-place, tanpa provenance, tanpa completeness, tanpa referensi handle, plus `pending_clarification_json` yang merupakan state job bocor ke tier session.

**Satu tabel `session_memory` + diskriminator `kind`.** Kriterianya bukan "tiga konsep" melainkan **pola tulis dan baca identik**: ketiganya ditulis **hanya** pada response commit, dibaca **selalu bersama** saat context assembly, berbagi kolom provenance/completeness/validitas yang sama, tunduk kuota dan CASCADE yang sama.

`session_memory`: `id` · `session_id` (FK → `chat_sessions`, CASCADE) · `owner_user_id` NOT NULL · `session_seq` BIGINT (**UNIQUE (session_id, session_seq)**, alokator = row lock `chat_sessions`) · `kind` (`PriorResult`|`ResolvedEntity`|`ActiveScope`) · `entity_key` · `label` (**berpotensi PII**) · `fact_json` · `source_job_id` · `source_plan_version` · `source_node_id` · **`source_response_version` NOT NULL** · `provenance_json` · `as_of` · `completeness` + `completeness_reason` · `dataset_id` · `status` (`valid`|`superseded`|`invalidated`|`evicted`) · `superseded_by_id` · `invalidation_reason` · `invalidated_at` · `created_at`.

Summary sebagai **kolom pada `chat_sessions`**: `memory_summary_text` · `memory_summary_version` · `memory_summary_watermark` · `memory_summary_status` (`current`|`stale`|`failed`) · `memory_summary_updated_at` · `memory_seq_last`. `chat_sessions.context_json` **DIBUANG**.

### Keputusan
- **K4 ditegakkan struktur, bukan disiplin kode**: `source_response_version` NOT NULL dengan **FK KOMPOSIT `(source_job_id, source_response_version)` → `job_responses (job_id, response_version)`** membuat baris memori **mustahil ada** tanpa response document durable. Tidak ada `status='pending'`, tidak ada jalur tulis inkremental. Crash tanpa commit = tidak ada baris. *(FK komposit, bukan dua FK terpisah — dua FK terpisah memungkinkan baris menunjuk job A dengan versi milik job B.)*
- **`source_job_id` memakai `NO ACTION` (atau DEFERRABLE INITIALLY DEFERRED), BUKAN `RESTRICT`.** `session_memory` dan `chat_jobs` sama-sama anak CASCADE dari `chat_sessions`, dan PostgreSQL **tidak menjamin urutan jalur cascade**; dengan RESTRICT, bila baris `chat_jobs` terhapus lebih dulu, pemeriksaan langsung menemukan baris `session_memory` yang belum terhapus dan **seluruh penghapusan session gagal**. `NO ACTION` diperiksa di **akhir statement**, saat kedua sisi sudah terhapus — niat aslinya tetap terjaga karena menghapus job sendirian tetap gagal.
- **Expiry handle tidak disalin.** `datasets.status` otoritatif. Baca session_memory **selalu** `LEFT JOIN datasets`, dan tipe hasil repository memiliki field `handle_state` **non-optional** (`live`|`expired`|`purged`|`none`) — tidak ada varian "tidak tahu", sehingga tidak ada jalur baca yang bisa melewatkannya. Handle kedaluwarsa **tidak** membatalkan faktanya: angka ringkasnya tetap terjawab dengan `as_of` eksplisit; yang hilang hanya drill-down/pagination.
- **Invalidasi = supersede, tidak pernah hard delete.** Baris yang dihapus menghilangkan penjelasan "kenapa jawaban berubah antar-turn" — persis pertanyaan investigasi PRD §10. Pemicu: fakta baru untuk `entity_key` sama → `superseded`; `ActiveScope` baru → scope lama `superseded` + `PriorResult` di bawahnya `invalidated (scope_changed)`; hasil bertentangan pada grain sama → `invalidated (contradicted)`; kuota terlampaui → `evicted`.
- **Dua hal sengaja BUKAN pemicu invalidasi**: (a) entitlement berubah — tidak ada event yang bisa dipercaya, dan otorisasi memang **tidak pernah** dibaca dari memori; (b) staleness — `as_of` dinilai saat baca terhadap ambang ops doc, sehingga ambang bisa berubah tanpa migrasi dan tanpa job background yang membusukkan baris.
- **Summary di LUAR transaksi commit, best-effort** (penajaman K4, bukan pelanggaran — K4 mengunci **promosi fakta**, yang dipatuhi penuh). Alasan mengikat: summary butuh panggilan LLM, dan **menahan transaksi PostgreSQL terbuka selama panggilan eksternal memblokir vacuum dan menahan lock selama detik-detik**. Gagal → `status='stale'`, **watermark tidak bergerak**, percobaan berikutnya melipat semuanya. Tidak ada konteks hilang: faktanya sudah durable dan otoritatif. **Summary adalah cache lossy, bukan sumber kebenaran** — bila bertentangan dengan baris fakta, fakta menang; summary tidak pernah dibaca untuk angka maupun izin.
- **Otorisasi dibuat mustahil, bukan dilarang**: nol kolom entitlement di tabel ini. `ActiveScope.fact_json` berisi `scope_kind: "requested_filter"` — penyempitan yang **diminta pengguna**, bukan yang **diizinkan sistem**. `owner_user_id` NOT NULL difilter di setiap baca. Jalan dari "memori bilang 4.312 baris" ke baris sungguhan **melewati** pemeriksaan otorisasi dataset (#11 aturan 5). `provenance_json` menyimpan **input** untuk re-check, bukan **hasil** pemeriksaan.
- **`ActiveScope` = satu dokumen**, bukan satu baris per filter; ditegakkan partial unique `(session_id) WHERE kind='ActiveScope' AND status='valid'`.
- **Tier global/user tetap deferred**; kolom `tier` sengaja TIDAK ditambahkan karena akan berisi satu nilai.

### Sengaja TIDAK dibuat
Tiga tabel per kind · tabel `session_summaries` berversi · kolom status handle · `pending_clarification_json` (milik #8) · `revision` optimistic locking (penulis tunggal berpagar `lease_token`, dan baris ini insert-only) · `relevant_jobs_json`/`context_warnings_json` · embedding/pencarian semantik atas fakta (**pemicu**: jumlah fakta `valid` per session rutin melewati ambang sehingga context window tertekan → tambah kolom embedding + top-k, aditif) · GIN pada `fact_json` · `updated_at`.

### Index
PK · UNIQUE `(session_id, session_seq)` · parsial `(session_id, kind) WHERE status='valid'` · parsial UNIQUE `(session_id) WHERE kind='ActiveScope' AND status='valid'` · parsial UNIQUE `(session_id, entity_key) WHERE kind='ResolvedEntity' AND status='valid'` · parsial `(dataset_id) WHERE dataset_id IS NOT NULL` · `(source_job_id)`. Tanpa `fillfactor` khusus: insert-only dengan update status yang jarang.

Terbuka (ops doc): kuota fakta `valid` per session per `kind` · ambang staleness `as_of` per domain · retensi baris non-`valid` · batas panjang `memory_summary_text` · apakah `label` dipurge lebih awal.

---

## Keputusan #6 — `chat_sessions` + `chat_messages` (DIKUNCI 2026-09-13)

`chat_sessions`: `id` · `owner_user_id` NOT NULL (FK → `users`; `api_key_id` **dibuang**, bukan dibuat nullable) · `title` · `status` (`active`|`archived`|`expired`) · `created_at` · `updated_at` · `expires_at` · `archived_at` · kolom summary memori dari #5. **`context_json` DIHAPUS** — tempatnya sudah didefinisikan ulang di #5; mempertahankannya = dua tempat untuk satu jaminan.

`chat_messages` — **indeks riwayat tipis, bukan pemilik konten**: `id` · `session_id` (FK CASCADE) · `job_id` (FK CASCADE) · `role` (`user`|`assistant`|`clarification`) · `response_version` · `clarification_id` + `clarification_revision` · `created_at`, dengan CHECK per role. **Tanpa kolom `content`/`metadata_json`.** Rendering menyusun teks lewat join tertarget: `user` → `chat_jobs.request_text`; `assistant` → `job_responses`; `clarification` → `clarification_forms`.

### Keputusan
- **"Message" bukan lagi unit utama** — konten sudah punya rumah masing-masing, semuanya immutable/berversi. Salinan teks kedua akan menciptakan kelas bug yang baru ditutup #10.
- **Tabel tipis, bukan VIEW.** Setiap baris memang dapat diturunkan penuh, tetapi pagination riwayat adalah operasi paling sering di UI, dan keyset atas **satu** tabel jauh lebih sederhana dan cepat daripada UNION tiga sumber setiap scroll.
- **Vokabulari `role` diperkecil**: `system` dan `tool` dibuang karena tidak ada penulisnya — lifecycle job disampaikan lewat SSE (#3), eksekusi node internal di ledger (#2). Enum yang tak pernah ditulis adalah spekulasi. Pemicu penambahan: produk benar-benar butuh notifikasi sistem sebagai baris riwayat.
- **Jawaban klarifikasi tidak punya baris sendiri** — sudah lengkap di `clarification_answers` (#8), tertaut lewat `job_id`+`clarification_id`+`revision`.
- **Pagination keyset `(session_id, created_at, id)`**, bukan alokator sequence ala #3: volume rendah, dan constraint "1 job nonterminal per session" (#13) mencegah penulisan message yang benar-benar konkuren.
- **KOREKSI terhadap catatan #1**: catatan "duplikasi `request_text` yang disengaja antara job dan message" **salah alasannya**. Dengan desain thin-pointer **tidak ada duplikasi fisik**, dan `chat_messages` selalu hidup-mati bersama job lewat CASCADE. Pemisahan retensi yang sesungguhnya berlaku antara **job dan audit** (#4). Kesimpulan strukturalnya tetap: `request_text` tetap di `chat_jobs`.
- **Penghapusan session = FORCE (cabang "dibatalkan lebih dulu" dari #11).** Hapus session menulis `Cancelling` + menghapus barisnya dalam satu transaksi; CASCADE membersihkan job → plans → node_runs → datasets/chunks → responses → clarifications → events → messages. Worker pemegang lease **tidak diberi tahu**: heartbeat berikutnya (`UPDATE … WHERE lease_token = $token`) mengenai **0 baris** → worker wajib berhenti (fencing #13). Kerja terbuang terbatas satu interval heartbeat, dan karena Jarvis read-only tidak ada efek samping tertinggal di Fineract. `idempotency_keys` (#9) **tidak** ikut CASCADE (punya TTL sendiri); audit (#4) **tidak pernah** ikut terhapus.

### Sengaja TIDAK dibuat
Kolom `content`/`metadata_json` · role `system`/`tool` · baris message untuk jawaban klarifikasi · sequence/cursor khusus · TTL message independen · trigger DB untuk aturan penghapusan (cek service layer + partial index #13 cukup) · `context_json`.

### Index
`chat_sessions`: PK · `(owner_user_id, updated_at DESC)` · `(status)` · parsial `(expires_at)`.
`chat_messages`: PK · **`(session_id, created_at, id)`** · `(job_id)`.

---

## Keputusan #7 — CARRY: auth/identity + knowledge index (DIKUNCI 2026-09-13)

### Bagian A — auth: 6 tabel → 3

**CARRY**: `users` · `user_sessions` **RENAME → `auth_sessions`** · `refresh_tokens`.
**DROP**: `permissions`, `role_permissions`, `api_keys`.

- **Rename `user_sessions` → `auth_sessions`**: repo baru punya `chat_sessions` (percakapan) dan `user_sessions` (sesi login); keduanya "session" dan keduanya punya `expires_at`. Tabrakan nama ini menghasilkan bug ownership yang **dibaca benar oleh mata dan salah oleh kode**.
- **DROP `permissions` + `role_permissions`**: grep seluruh `crates/` → **nol rujukan**, dan CHECK membatasi role ke satu nilai. Pemicu penambahan kembali: role kedua yang benar-benar dibutuhkan produk; backfill sepele karena semua user hari ini `admin`.
- **DROP `api_keys`**: seluruh kolom kebijakannya sudah inert (`allowed_capabilities` ditimpa proyeksi admin, `can_view_pii` dipaksa `true`, office hanya menyempit untuk pemanggil kooperatif). Struktur bernama `allowed_*` yang **tidak mengikat** adalah jebakan bagi reviewer berikutnya — ini membuang batas **palsu**, bukan batas nyata. #1 sudah mengganti perannya dengan `owner_user_id` + `scope_json`. Penggantinya: parameter **`office_ids[]`** pada `POST /chat/jobs`, divalidasi ⊆ scope tenant bearer, lalu di-snapshot ke `scope_json` — literal PRD §10 "request filters may narrow but cannot widen". **Pemicu kembalinya**: pemanggil non-interaktif pertama yang nyata; saat itu `api_keys` lahir sebagai **tabel kredensial murni** yang mengotentikasi **sebagai seorang user**, dengan entitlement tetap menempel pada user. Kesalahan tabel lama adalah mencampur kredensial dan entitlement dalam satu baris.
- **Jalur verifikasi bearer CARRY apa adanya dan jangan "dioptimasi" menjadi JWT-only**: `verify_access_token` memvalidasi `iss`/`aud`/`exp` dengan `leeway = 0`, lalu `authenticate_access_token` **tetap memukul DB** (`is_active AND revoked_at IS NULL AND expires_at > now()`), sehingga pencabutan berlaku pada request berikutnya.

**TENANT: single-tenant (opsi 1). `users` adalah satu-satunya jangkar.** Alasan penentu: **Fineract sendiri multi-tenant lewat database terpisah**, dan Jarvis menyambung ke satu database lewat config. Menambah `tenant_id` berarti membangun **mekanisme tenancy kedua yang lebih lemah** (filter baris di kode aplikasi) di samping yang sudah ada dan lebih kuat. Asimetri biaya: salah memilih opsi 1 = satu kolom aditif + backfill satu konstanta, predikat menempel di **satu tempat**; salah memilih opsi 2 = kolom yang tak pernah bervariasi di setiap tabel yang akhirnya **dipercaya sebagai batas**. **Aturan yang dikunci: `tenant_id` tidak pernah ditaruh di `chat_jobs`/`chat_sessions`/`datasets`** — fakta itu diturunkan dari `owner_user_id`. Biaya nol sekarang: identitas deployment Fineract direkam di `chat_jobs.scope_json`. Pemicu upgrade: satu proses Jarvis harus melayani **dua database tenant sekaligus** (pelanggan kedua = deployment kedua, tetap opsi 1).

**AUTENTIKASI — sekarang: Jarvis penerbit token, username/password (tidak berubah).** Sisi Fineract tidak dapat diubah, jadi sistem baru yang menyesuaikan. HS256 sah **karena penerbit = pemverifikasi**. Tiga tabel carry as-is.
Dua perubahan kecil yang wajib sekarang:
- **`iss`/`aud` menjadi config, bukan konstanta kode** — supaya perpindahan ke SSO tidak butuh perubahan kode.
- **`users.external_subject TEXT NULL UNIQUE`** — engsel SSO. Tanpanya, pindah ke SSO berarti mencocokkan user lewat `username`/`email`, string yang bisa berubah: cara klasik memberikan sesi milik orang lain.
> **ATURAN KEAMANAN**: bila kelak dashboard/SSO yang menerbitkan token, **HS256 dilarang** — secret simetris bersama berarti masing-masing layanan dapat mencetak token milik yang lain. Opsi itu mewajibkan asimetris (RS256/EdDSA + JWKS + rotasi `kid`).

`users.fineract_user_id` **tidak** ditambahkan sekarang; pemicunya adalah saat derivasi entitlement dari Fineract benar-benar dipakai. **Derivasi entitlement dari Fineract DITUNDA** — PII kini konfigurasi global (#15) dan office scope ditangani parameter request, sehingga tidak ada yang tersisa untuk diderivasi. Membangunnya sekarang = mekanisme tanpa pembaca.

**PII**: `users.can_view_pii` **TIDAK ditambahkan** — setiap baris akan `true`, dan kolom bernilai seragam di jalur keamanan **lebih buruk daripada tidak ada** karena reviewer memercayainya. Lihat #15.

Index: UNIQUE `users(username)` · UNIQUE `users(email)` · UNIQUE `users(external_subject)` · `auth_sessions(user_id)` · `auth_sessions(revoked_at)` · parsial `auth_sessions(expires_at) WHERE revoked_at IS NULL` · UNIQUE `refresh_tokens(token_hash)` · `refresh_tokens(session_id)`. **Dibuang**: index eksplisit yang redundan di atas UNIQUE (`users(username)`, `refresh_tokens(token_hash)`).

### Bagian B — knowledge index

**Mode 2 = baris baru, bukan kolom baru.** Perluas CHECK `source_type` dengan **`analytical_contract`** dan **`measure`**.
- **`metric` ≠ `measure`, jangan disatukan**: `metric` adalah dokumen pengetahuan Mode 1 (dipakai `METRIC_BOOST`), `measure` adalah agregasi yang dideklarasikan kontrak Mode 2 dengan grain. Menumpuknya membuat boost Mode 1 memenangkan baris Mode 2 secara kebetulan, **dan tidak ada yang gagal keras**.
- **Yang sesungguhnya memblokir Mode 2 bukan schema melainkan query**: `build_hybrid_sql` mematok `AND source_type = $2` (satu tipe per pencarian). Ubah menjadi `= ANY($2::text[])` agar `capability` dan `analytical_contract` diperingkat dalam **satu daftar** berbasis skor. Nol migrasi.
- **Identitas kontrak/measure tetap di `metadata_json`** (sudah GIN, sudah punya jalur filter). Tidak ada kolom `contract_id`/`measure_id` yang dipromosikan; bila kelak terbukti perlu, tambahkan **expression index** pada `(metadata_json->>'contract_id')`.

**BUANG index ivfflat; pakai exact search.** `lists = 100` di atas katalog beberapa ratus baris dengan `ivfflat.probes` default `1` berarti pencarian hanya menyentuh **±1/100 ruang vektor** (~3 vektor per list) — tetangga yang benar dapat hilang **tanpa error**, dan pada volume ini exact scan lebih cepat. Ini sumber "capability yang bekerja menjadi tak terjangkau". **Pemicu upgrade**: >10.000 baris terindeks **atau** p95 latensi retrieval melewati ambang operasional — dan penggantinya **HNSW**, bukan ivfflat. Juga dibuang: `knowledge_index(content_hash)` (nol pembaca).

**CACAT EMBEDDING VOYAGE — akar masalah: DUA klien embedding untuk SATU index.** Sisi tulis `knowledge/embedding.rs` (klien Voyage native, mengirim `input_type:"document"` + `output_dimension`); sisi baca `assistant/llm/provider.rs:225` membangun **klien OpenAI rig** yang diarahkan ke base URL Voyage. Lima cacat:
| | Cacat | Status |
| --- | --- | --- |
| a | Respons Voyage ditolak tipe rig OpenAI (`usage` tanpa `prompt_tokens`) → HTTP 200 valid menjadi error (finding 837) | call site sudah diperbaiki, **penyebab masih hidup** |
| b | Sisi baca tidak mengirim `input_type`/`output_dimension`; model Voyage **asimetris** sehingga vektor query dan dokumen dihasilkan parameter berbeda | **masih hidup, gagal diam-diam** |
| c | `.unwrap_or_default()` → embedding hilang menjadi `vec![]`, bukan error | masih hidup |
| d | `zip` posisional mengabaikan field `index` → embedding menempel ke dokumen yang salah **tanpa jejak** | masih hidup |
| e | Tidak ada verifikasi model/dimensi baris vs query saat baca | masih hidup |

Yang WAJIB pada implementasi baru: **satu provider, satu klien, satu jalur kode** untuk dokumen dan query, dengan `input_type` `document`/`query` sebagai **satu-satunya** perbedaan (memperbaiki a dan b sekaligus, diff terkecil) · parse berdasarkan `index` dengan assert himpunan `0..n` · assert panjang vektor == dimensi terkonfigurasi sebelum INSERT · **tanpa `unwrap_or_default`** · **fail-closed** pada ketidakcocokan model/dimensi/input_type → degradasi ke arm leksikal. Konsekuensi schema: **`knowledge_catalog_versions.embedding_input_type`**, tanpanya aturan fail-closed tidak punya pembanding.

**KOREKSI terhadap #12 dan #2 — `catalog_version` literal selalu `"local"`.** `KnowledgeSyncService::sync_documents` memanggil `replace_indexed_catalog_version("local", …)`; identitas nyata satu-satunya adalah `content_hash` UNIQUE. Karena itu **#12 `contract_versions_json` dan #2 `provenance_json` menyimpan `catalog_version_id` (UUID) + `content_hash`, bukan teks `version`.** Bila tidak, setiap baris audit menyimpan token yang sama untuk semua katalog yang pernah ada dan pertanyaan "prosa kontrak mana yang dilihat planner" menjadi tidak terjawab — persis kemampuan yang dituntut PRD §10.
**Aturan yang dikunci**: `knowledge_catalog_versions` **append-only**; baris versi yang dirujuk `job_plans.contract_versions_json` atau `job_node_runs.provenance_json` **tidak boleh pernah dihapus**. Retensi = N versi terbaru **plus** semua yang masih dirujuk. Catatan turunan: karena `content_hash` adalah identitas, kembali ke katalog sebelumnya **memakai ulang baris yang sama** (`synced_at` bergerak, `created_at` tidak) — audit wajib merujuk `id`/`content_hash`, **jangan pernah** "versi terbaru menurut `synced_at`".

Terbuka (ops doc): TTL access/refresh token dan apakah refresh dirotasi tiap pakai · retensi `auth_sessions` kedaluwarsa · bentuk final parameter `office_ids[]` (kontrak API) · jumlah versi katalog yang diretensi di luar yang dirujuk audit · ambang pemicu HNSW · bobot hibrida setelah `analytical_contract`+`measure` ikut diperingkat.

---

## Keputusan #14 — `exchange_rates` DIBANGUN, `published_reports` DITUNDA (DIKUNCI 2026-09-13)

### Analisis: apakah Jarvis pernah MENERBITKAN laporan?

Berdasarkan scope yang dikunci sejak awal — Jarvis adalah asisten analisis **murni baca**; simulasi dan penerbitan adalah urusan engine Fineract — jawabannya **tidak**. D01 dapat dibaca dua cara: (a) Jarvis menyimpan & menampilkan ulang laporan **yang ia terbitkan** → butuh `published_reports`; (b) Jarvis **menjawab pertanyaan** tentang periode lampau dengan semantik as-of yang benar, sedangkan catatan penutupan tetap milik Fineract → **tidak butuh tabel baru**. Pembacaan (b) jauh lebih konsisten dengan "Jarvis tidak pernah menulis". Menerbitkan laporan adalah tindakan **otoritatif** milik core banking.

**Keputusan: `published_reports` DITUNDA.** Jangan bangun arsip untuk tindakan yang produknya belum lakukan. Sebagai gantinya dikunci: **semantik as-of adalah urusan kontrak query, bukan storage** — setiap analytical contract yang menjawab pertanyaan periode wajib mendeklarasikan semantik cutoff-nya dan merekam `as_of` di provenance (sudah ada di #2 dan #11).

**Pemicu penambahan**: produk memperoleh aksi "terbitkan laporan" yang eksplisit dengan pemilik proses yang jelas. Saat itu `published_reports` datang sebagai tabel aditif dengan **aturan self-contained**: blok tabelnya berisi baris inline dan **tidak boleh merujuk dataset handle sama sekali** (menyalin `content_json` tidak menyalin baris; laporan yang merujuk handle akan rusak saat dataset kedaluwarsa). Laporan yang isinya tidak muat di bawah batas `blocks_json` **ditolak saat publikasi**, bukan terbit lalu rusak diam-diam.

**Konsekuensi: K13 selesai tanpa kelas dataset baru.** Response biasa tetap merujuk handle dan setelah TTL menyatakan "detail data sudah kedaluwarsa" (#11 aturan 6). Tidak ada kelas `pinned`, tidak ada TTL bersyarat, tidak ada pengecualian eviction — dan tidak ada tabel konten permanen berisi PII, sehingga pertanyaan kebijakan penghapusan data nasabah tidak perlu dijawab.

### `exchange_rates` — DIBANGUN SEKARANG

Tidak bergantung pada penerbitan: Fineract core tidak punya tabel FX umum, sehingga konsolidasi multi-currency apa pun yang Jarvis lakukan **hari ini** membutuhkan ketertelusuran (D03).

`exchange_rates`: `id` · `from_currency_code` · `to_currency_code` · `rate` **NUMERIC(20,10)** · `rate_type` · `effective_date` DATE · `source` · `captured_at` · `captured_by_user_id` · `notes` · `created_at` · **UNIQUE (from_currency_code, to_currency_code, rate_type, effective_date, captured_at)**.

**Registry dan record dalam satu tabel** dengan baris **immutable**: koreksi = baris baru ber-`captured_at` lebih baru, **bukan** UPDATE. Konsumen menyimpan **`exchange_rate_id`**, sehingga reproduksi tetap eksak walau registry terus tumbuh/dikoreksi. Dua tabel terpisah akan menjadi dua sumber kebenaran untuk fakta yang sama.

**Aturan lookup (menutup ambiguitas koreksi)**: exact-match `(from, to, rate_type, effective_date)` — **tidak boleh `<=`, tidak boleh "ambil terdekat"**. Bila ada beberapa baris untuk tanggal yang sama (akibat koreksi), ambil **`captured_at` terbaru**, dan `exchange_rate_id` yang terpilih **wajib direkam** di `provenance_json` (#2) serta evidence response (#10). Tanpa aturan tie-break ini, larangan substitusi senyap hanya berlaku untuk **tanggal** dan tidak untuk **koreksi**.

**Kurs tidak tersedia harus eksplisit**: tanpa baris yang cocok, hasilnya NULL/tanpa baris — **bukan nol dan bukan kurs hari lain** — dan composer **wajib** menerjemahkannya menjadi `completeness='Partial'` + blok `limitation` yang menyebut pasangan currency dan tanggal yang hilang (konsisten D11: data kosong ≠ nol).

**Presisi**: `NUMERIC(20,10)`, **tidak pernah floating point**. Konversi memakai tipe Decimal; pembulatan dilakukan **sekali** di titik penyajian akhir ke skala mata uang target, bukan bertahap tiap langkah (mencegah akumulasi error).

**Retensi**: permanen (append-only, volume kecil, kurs lama tetap dibutuhkan untuk mereproduksi analisis lama). Audit menyimpan `exchange_rate_id`, bukan nilainya.

Index: UNIQUE di atas · `(from_currency_code, to_currency_code, rate_type, effective_date DESC)` (telusur/administrasi registry, **bukan** untuk fallback otomatis).

Sengaja TIDAK dibuat: tabel provider FX · junction `report_exchange_rates` · kolom rantai koreksi (`captured_at` terbaru sudah cukup) · **fungsi/view "ambil kurs terdekat" — justru harus tidak ada**.

Terbuka (ops doc): mode pembulatan pasti (half-up vs mengikuti Fineract) · sumber kurs definitif (manual admin entry vs provider vs Currency Configuration Fineract).

---

## Keputusan #15 — `system_settings`: konfigurasi global (DIKUNCI 2026-09-13)

**PII menjadi konfigurasi global, bukan berbasis role.** Alasannya konkret: hari ini setiap user adalah `admin`, sehingga PII berbasis role menghasilkan kolom bernilai sama untuk semua orang — persis "hiasan di jalur keamanan" yang ditolak #7. Satu sakelar global jujur menggambarkan kenyataannya.

`system_settings` — **append-only, satu baris = satu perubahan**: `id` · `key` (mis. `pii.enabled`, `pii.mode`) · `value_json` · `version` (**UNIQUE (key, version)**) · `effective_from` · `changed_by_user_id` · `change_reason` · `created_at`. Nilai berlaku = `version` tertinggi per `key`; perubahan = **baris baru**, bukan UPDATE.

Append-only karena audit harus dapat menjawab **"siapa menyalakan PII, kapan, dan apa alasannya"** — di-update in-place, jawaban itu hilang. Pola yang sama dengan #10 dan #12.

### Empat aturan
1. **Setiap job menyimpan nilai efektifnya, bukan hanya merujuk konfigurasi.** `chat_jobs.scope_json` (#1) merekam `{"pii": {"enabled": true, "setting_version": 7}, "source": "admin_projection", "office_ids": […], "fineract_tenant": "…"}`. Tanpa ini, laporan minggu lalu **berubah maknanya** ketika dibaca setelah PII dimatikan, dan audit tidak lagi dapat menjelaskan kenapa kolom nama ada di sana. Field `source` membedakan `admin_projection` dari `fineract_derived` sehingga baris lama tetap terbaca ketika derivasi Fineract kelak dipakai — dan membedakan **"diizinkan"** dari **"tidak pernah diperiksa"**.
2. **Fail closed.** Baris konfigurasi hilang/tidak terbaca/korup → PII dianggap **mati**.
3. **Klasifikasi per-kolom tetap di `knowledge/`.** Sakelar global memutuskan **apakah** kolom berkelas `pii` dilepas; **kolom mana** yang berkelas `pii` tetap dideklarasikan YAML. Satu sakelar, klasifikasi yang sudah ada, tanpa model izin baru.
4. **PII mati = kolom identitas DITAHAN, bukan dimasking, dan tidak pernah senyap.** Response menyatakannya lewat blok `limitation`; jawaban agregat tidak terpengaruh. Masking ditolak karena pola unik masih dapat membocorkan identitas; bila kelak dibutuhkan, ia menjadi nilai `pii.mode` tanpa mengubah apa pun.

**Tanpa cache**: pada puluhan–ratusan job/hari, membaca satu baris ter-index saat job diterima sudah cukup. Cache + invalidasi hanya menambah satu kelas bug (konfigurasi basi di satu worker) demi menghemat query yang tidak terasa.

Index: UNIQUE `(key, version)` · parsial/urut `(key, version DESC)` untuk membaca nilai berlaku.

Sengaja TIDAK dibuat: tabel permission baru · konfigurasi lewat env var (butuh deploy, tanpa jejak siapa mengubah) · tabel riwayat terpisah (tabel ini **sudah** riwayatnya).

---

## K5 — Klarifikasi dipicu kardinalitas resolver (DIKUNCI 2026-09-13)

Masalah pada sistem lama: sistem bertanya "rekening tabungan yang mana?" walau hasilnya hanya **satu**, dan meminta rentang tanggal walau pengguna sudah mengatakan "bulan ini".

**Aturan: klarifikasi dipicu oleh KARDINALITAS HASIL RESOLVER, bukan oleh tebakan planner.**

| Hasil resolver | Perilaku |
| --- | --- |
| 0 | jalur no-match → `refine_search`. **Bukan** form kosong |
| **tepat 1** | **auto-bind, tidak ada form** |
| >1 | form klarifikasi dengan opsinya |

Keputusan untuk bertanya dibuat **setelah** query terbatas dijalankan, bukan sebelum — prinsip probe-first yang sudah ada di #2 (`node_kind = Probe`).

**Parameter waktu**: "hari ini", "kemarin", "minggu ini", "bulan ini" **terurai deterministik** di bawah kontrak kalender/timezone yang dideklarasikan → langsung bind. Yang ditanyakan hanya frasa yang benar-benar ambigu ("belakangan ini"; "Q1" ketika tahun fiskal ≠ tahun kalender).

> **SYARAT KEAMANAN**: auto-bind pada satu hasil **hanya sah bila pencarian resolver `Complete`**. Satu hasil yang terlihat di satu halaman ≠ satu hasil di seluruh scope; bila resolver `truncated`, satu match yang terlihat **tidak boleh** di-auto-bind. Memakai distingsi `completeness` vs `truncated` dari #11 persis di sini.

**Dampak schema — hanya provenance.** `clarification_answers.provenance` bertambah: `resolver_unique` (satu-satunya match) dan `deterministic_parse` (frasa tanggal terurai kontrak), di samping `user_confirmed`. Ini penting karena #5 dan #4 sama-sama merekam provenance, dan **"pengguna mengonfirmasi nasabah ini" adalah klaim yang berbeda secara material dari "kebetulan hanya ada satu"**.

**Slot auto-bind TIDAK membuat baris `clarification_forms`** — form adalah "apa yang dilihat pengguna"; bila pengguna tidak melihat apa pun, baris form adalah fiksi. Binding direkam di `job_node_runs.input_binding_json` (#2) dan diaudit dengan `action='slot.auto_bound'`.

**Response WAJIB menyatakan auto-binding** ("Rekening tabungan X — satu-satunya yang cocok"; "Periode: 1–30 September 2026") lewat blok `note`/`limitation`. Tidak butuh kolom baru, tetapi wajib sebagai aturan: **auto-bind yang diam adalah cara menghasilkan jawaban salah dengan meyakinkan.**
