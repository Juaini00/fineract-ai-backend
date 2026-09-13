# Engine lifecycle

Status: agreed lifecycle direction and clarification/transport integration, recorded 2026-09-08. The full transition matrix, recovery mechanics and database constraints remain design work; this is not implementation-ready.

## One owner

One Engine owns context assembly, planning, verification, node scheduling, deterministic composition, response assembly and persistence. Job API handlers accept/read commands and delegate. Executors and LLM clients never introduce separate orchestration loops.

```text
authenticate/validate → persist accepted job → acquire work
→ assemble context → plan/verify → execute ready nodes
→ compose deterministic evidence → assemble/validate response
→ persist response + memory promotion + completion event
```

Missing information suspends the same job through [clarification](../contracts/clarifications.md). Live facts may trigger a bounded, versioned re-plan. All paths use the same budgets and policy guards. Public progress is a projection through [SSE](../contracts/sse.md), not an alternate execution flow.

## State dimensions

| Dimension | Proposed agreed vocabulary |
| --- | --- |
| Job lifecycle | `Queued`, `Running`, `WaitingForUser`, `Cancelling`, `Completed`, `Failed`, `Cancelled`, `Expired` |
| Outcome | `Answered`, `Empty`, `NotFound`, `Unsupported`, `BlockedByPolicy`, `Invalid`, `OperationalFailure` |
| Analysis completeness | `Complete`, `Partial`, `Unknown` |

WaitingForUser is suspended, not terminal. Completed means a validated response is durable; it says nothing about delivery to the browser or analytical completeness. The complete allowed status/outcome combinations and cancellation/expiry race precedence remain open.

Response assembly is an Engine stage after data-plan execution, not a mandatory Respond graph node. A simple request can therefore have one data node while still receiving validation, audit and response assembly. This supersedes the old PRD's conflicting node-count terminology. Clarification forms are interaction state managed around resolver/data nodes, not a second engine.

## Agreed interaction transitions

| Trigger | State/action | Durable/public effect |
| --- | --- | --- |
| New request accepted | Queued | Persist job and job.accepted before HTTP 202 |
| Worker claims work | Running | Record execution ownership; emit actual phase/node progress |
| Missing user input | Stop new admissions, drain bounded running work, then WaitingForUser | Retain completed outputs and persist active form before clarification.required |
| Invalid/stale answer | Remain waiting | Field/conflict error; no resume |
| Valid answer commits | Queued for continuation | Persist answer/bindings/audit and clarification.accepted before HTTP 202 |
| Worker takes continuation | Running | job.resumed; run resolver or remaining nodes |
| New live ambiguity | Another form in same job | Retain prior accepted answers and explain new question |
| Response commits | Completed | Persist response, memory promotion and terminal event atomically where applicable |
| Client disconnects | No lifecycle transition | Work continues; client can replay or fetch state |

Scheduling follows fan-in: all required dependencies must be complete before a node runs. Node completion and output completeness are checked separately. For a plan requiring some failed inputs, fail-policy decides which independent results can still be presented with explicit gaps.

Initial session concurrency direction: at most one nonterminal job per session, including WaitingForUser. A competing new request returns an authorized conflict; the user can wait, cancel or use another session. Within-job and cross-session concurrency remain budgeted. Database enforcement is pending design.

## Verification and recovery boundaries

Static plan/contract/authorization checks precede source-data execution; DB-assisted validation is a separate bounded step before executing the query. Application persistence is not prohibited by source read-only policy.

Re-plans create a version and consume the same job budget. Reuse an output only if bindings, scope, contract version and freshness requirements remain valid. Persisted completed outputs are not rerun. An external read/model call that finishes before its completion commit can have an uncertain outcome after a crash; bounded retry policy must explicitly handle it rather than promise exactly-once external execution.

Worker lease/fencing, cancellation settlement, terminal races, clarification expiry and numeric budgets must be resolved with database/runtime design. No source transaction stays open during human waiting. Required audit failure blocks the protected transition; telemetry-export failure has a separate policy.

## Required follow-up design

- Full state/outcome matrix and transactional preconditions, including failure, cancel and expiry races.
- Lease claim, renewal, fencing and uncertain-attempt recovery.
- Exact reuse rules and retry/re-plan budgets.
- Atomic response/memory promotion and session concurrency constraints.
- Node-kind/status schema and version compatibility.

## Skip sebagai jalur terminal (disepakati 2026-09-12)

`outcome` bertambah satu nilai: **`SkippedByUser`**.

| Trigger | State/action | Durable/public effect |
| --- | --- | --- |
| Pengguna skip saat `WaitingForUser` | Berhenti menerima node baru, susun response | `lifecycle=Completed`, `outcome=SkippedByUser`, `completeness=Partial/Unknown`; persist response + memory promotion + terminal event secara atomik |

Skip adalah **trigger action**, bukan penghentian seketika: engine tetap menghasilkan response document (menyatakan pengguna memilih tidak melanjutkan, menyertakan hasil parsial yang valid dengan gap eksplisit) dan tetap melakukan memory promotion. Setelah itu job terminal dan tidak dapat dilanjutkan. Detail interaksi dimiliki [clarifications.md](../contracts/clarifications.md).

`cancel` tetap berbeda: berlaku pada job berjalan, berakhir `Cancelled`, tanpa kewajiban response document dan tanpa memory promotion.

**Memory promotion tetap satu titik: response commit** (kini termasuk skip). Promosi inkremental tidak digunakan. Konsekuensi yang diterima: crash tanpa commit tidak menyimpan context ke session memory. Output node dan checkpoint tetap durable untuk **resume** — job dapat dilanjutkan worker dan, bila kemudian selesai, memory dipromosikan secara normal. Bila job tidak pernah selesai, pertanyaan terkait berikutnya dijawab sebagai retrieval baru, bukan sambungan diam-diam.

---

## Matriks transisi lengkap (lifecycle × outcome × completeness)

`outcome` dan `completeness` adalah keputusan terminal. Selama lifecycle nonterminal, keduanya **harus** `NULL`; progress parsial hidup di `job_node_runs`/`datasets`, bukan di ringkasan job.

| Lifecycle | Outcome legal | Completeness legal | Catatan |
| --- | --- | --- | --- |
| `Queued` | `NULL` | `NULL` | Menunggu klaim, atau kelanjutan setelah jawaban klarifikasi. |
| `Running` | `NULL` | `NULL` | Engine masih dapat menghasilkan node output parsial. |
| `WaitingForUser` | `NULL` | `NULL` | Suspensi; output node yang sudah `Completed` tetap durable. |
| `Cancelling` | `NULL` | `NULL` | Barrier: tidak ada admission node baru atau response commit biasa. |
| `Completed` | `Answered` | `Complete`, `Partial`, `Unknown` | Response tervalidasi dan durable. |
| `Completed` | `Empty`, `NotFound` | `Complete`, `Unknown` | `Partial` dilarang: pencarian parsial tidak boleh menyatakan populasi kosong/tidak ditemukan. |
| `Completed` | `Unsupported`, `BlockedByPolicy`, `Invalid` | `Unknown` | Tidak ada klaim kelengkapan analitis atas data sumber. |
| `Completed` | `SkippedByUser` | `Partial`, `Unknown` | Wajib memakai response `kind='skipped'`; tidak boleh `Complete`. |
| `Failed` | `OperationalFailure` | `Partial`, `Unknown` | Tidak ada response sukses; `Partial` hanya merangkum output durable yang sah sebelum gagal. |
| `Cancelled` | `OperationalFailure` * | `Unknown` | Tanpa response/memory promotion. *Nilai sementara — lihat Keputusan terbuka di bawah. |
| `Expired` | `OperationalFailure` * | `Partial`, `Unknown` | Tanpa response/memory promotion. *Nilai sementara — lihat Keputusan terbuka di bawah. |

Kombinasi lain dilarang, khususnya: lifecycle nonterminal dengan `outcome`/`completeness` terisi; `Completed` dengan `OperationalFailure`; `Failed` dengan outcome selain `OperationalFailure`; `Cancelled`/`Expired` dengan outcome semantik (`Answered`, `NotFound`, `SkippedByUser`); `Empty`/`NotFound` dengan `Partial`; `SkippedByUser` dengan `Complete`.

### Transisi lifecycle legal dan precedence race

```text
Queued ─claim/T2→ Running
Running ─clarification/T5→ WaitingForUser
WaitingForUser ─jawaban valid/T6→ Queued
WaitingForUser ─skip/T8→ Completed(SkippedByUser)
Running ─response commit/T7→ Completed
Running|Queued|WaitingForUser ─cancel request/T9→ Cancelling
Cancelling ─settlement→ Cancelled
Queued|Running|WaitingForUser ─TTL reaper/T11→ Expired
Running ─terminal operational failure→ Failed
```

Lifecycle terminal bersifat immutable. Semua transisi memakai update kondisional pada baris `chat_jobs` yang dikunci; pemenang race adalah transisi yang lebih dahulu berhasil mengubah kondisi secara serializable.

Precedence operasional:

1. `Completed` menang bila T7/T8 sudah commit sebelum cancel atau reaper memperoleh row lock.
2. `Cancelling` menang atas completion biasa bila T9 sudah commit; T7 tidak boleh commit dari `Cancelling`.
3. `Expired` menang bila reaper lebih dahulu memindahkan job nonterminal ke `Expired`; completion setelahnya wajib gagal kondisi.
4. Bila `Cancelling` sudah tercatat, expiry tidak mengubahnya menjadi `Expired`; reaper menyelesaikannya menjadi `Cancelled`.
5. Worker yang kalah fencing atau melihat lifecycle terminal/barrier wajib berhenti tanpa mencoba transition alternatif.

## Matriks node kind × status

Semua `node_kind` dapat memiliki status berikut bila node itu ada di graph; `Respond` tetap opsional karena response assembly dapat dilakukan Engine tanpa node graph.

| Node kind | Pending | Runnable | Running | Completed | Failed | Skipped | Abandoned |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `Probe` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| `CuratedQuery` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| `AnalyticalQuery` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| `Clarify` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| `Compose` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| `Respond` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |

Semantik status:

- `Pending`: fan-in belum terpenuhi atau node belum boleh diadmisikan karena plan/version belum aktif.
- `Runnable`: dependency wajib sudah selesai menurut fail-policy, tetapi node menunggu slot concurrency/budget.
- `Running`: attempt telah diadmisikan dan dapat sedang menjalankan external read/model call.
- `Completed`: output/checkpoint immutable dan durable.
- `Failed`: kegagalan deterministik/terklasifikasi pasti; retry untuk logical node itu dilarang (`NODE_ATTEMPT_CAP_DETERMINISTIC=1`).
- `Skipped`: node tidak diperlukan branch/plan aktif, atau fail-policy memutuskan node tidak boleh berjalan.
- `Abandoned`: outcome attempt tidak pasti — worker/lease hilang setelah dispatch external call tetapi sebelum checkpoint commit.

Transisi attempt: `Pending → Runnable → Running → Completed|Failed|Skipped|Abandoned`. Baris attempt terminal tidak dibuka kembali; recovery membuat attempt baru dengan `attempt` lebih besar, tanpa mengubah baris historis.

## Lease, fencing, dan recovery

**Klaim** (T2): hanya job `Queued` dengan lease kosong/kedaluwarsa yang dapat diklaim lewat `FOR UPDATE SKIP LOCKED`. Klaim menetapkan `lease_owner`, `lease_token`, `lease_claimed_at`, `heartbeat_at`, `lease_expires_at`, dan memindahkan lifecycle ke `Running`.

**Renewal**: lease awal 60 s, heartbeat 10 s, **wajib** dari task Tokio independen (K1), bukan batas node. Renewal hanya memperpanjang lease bila token/lifecycle cocok:

```sql
UPDATE chat_jobs
SET heartbeat_at = now(), lease_expires_at = now() + WORKER_LEASE_DURATION
WHERE id = $job_id AND lease_token = $token AND lifecycle = 'Running';
```

**Fencing (C16)**: setiap tulisan durable milik worker — plan pointer, node completion, budget, transisi lifecycle, event, dan audit terkait — memakai `AND lease_token = $token`. Update yang menyentuh 0 baris berarti worker sudah dipagari dan wajib berhenti tanpa commit lanjutan. External call yang sudah berjalan tidak dapat dibatalkan fencing, tetapi hasilnya tidak boleh dipersist oleh pemegang token basi.

**Recovery attempt tak pasti** (reaper T11, idempoten): (1) kunci job dan pastikan lease masih kedaluwarsa; (2) ubah attempt `Running` terkait menjadi `Abandoned` + event/audit; (3) jangan simpulkan external call gagal/sukses; (4) bila logical node masih diperlukan dan `attempt + 1 < NODE_ATTEMPT_CAP`, insert attempt baru `Pending`/`Runnable` sesuai fan-in; (5) bila cap tercapai, selesaikan menurut fail-policy (`Failed`/`OperationalFailure`) atau response parsial eksplisit bila kontrak mengizinkan.

`NODE_ATTEMPT_CAP=3` hanya untuk `Abandoned`/ketidakpastian transien; `Failed` deterministik bercap 1 tanpa retry. Budget job tetap absolut lintas attempt dan lintas plan version (K9).

## Invalidasi, re-plan, dan reuse output

Re-plan membuat `plan_version` baru dan men-supersede plan aktif sebelumnya. Scheduler hanya membaca node dari `chat_jobs.plan_version` aktif (C4); node plan lama tidak dapat diadmisikan kembali.

Output lama hanya boleh direuse lewat node run baru yang mereferensikan `reused_from_node_run_id` bila **semua** syarat berikut valid: (1) `input_binding_hash` cocok dengan binding efektif baru (C7); (2) `scope_json` efektif — tenant, office scope, PII setting/version, batas otorisasi — ekuivalen; (3) versi capability/contract di provenance kompatibel dengan `contract_versions_json` plan baru; (4) requirement freshness/`as_of` terpenuhi; (5) dataset/output sumber masih tersedia, belum expired/purged, dan otorisasi baca tetap lolos; (6) provenance/catalog konsisten dengan aturan kesegaran katalog (D4, di bawah).

Perubahan binding, scope, contract, atau freshness membuat output **tidak eligible** untuk reuse. Output lama tetap immutable untuk audit/reproducibility; bila perlu kerja ulang, engine membuat node run baru pada plan version baru. Attempt `Completed` tidak pernah di-rerun atau dibuka ulang.

## Kesegaran katalog pada batas verifikasi–eksekusi (D4)

1. Saat verifikasi plan (T3), Engine merekam `catalog_version_id` + `catalog_content_hash` yang diverifikasi ke `job_plans.contract_versions_json`.
2. Tepat sebelum node diadmisikan, Engine membaca pasangan katalog aktif yang akan dipakai executor.
3. Bila berbeda dari pasangan plan → node **tidak boleh** dispatch external query/model call; plan diverifikasi ulang (plan version baru + pasangan katalog baru, memakai budget re-plan).
4. Bila sama → executor merekam pasangan itu ke `job_node_runs.provenance_json` bersama contract/`as_of`/`exchange_rate_id` yang relevan.
5. Perubahan katalog setelah node diadmisikan tidak mengubah attempt berjalan; provenance attempt menyatakan katalog yang benar-benar dipakai saat admission. Reuse berikutnya tetap melewati pemeriksaan ini.

## Keputusan terbuka

- **Enum `outcome` belum punya nilai `Cancelled`/`Expired`.** CHECK `chat_jobs_terminal_has_outcome` mewajibkan semua lifecycle terminal ber-`outcome`, tetapi `Cancelled` (user) dan `Expired` (TTL) bukan kegagalan operasional maupun hasil analisis. Sampai schema memutuskan, matriks memakai `OperationalFailure` sebagai penanda teknis sementara. Dua arah perbaikan: (a) tambah `Cancelled`/`Expired` ke enum `outcome`, atau (b) relaksasi CHECK menjadi hanya `Completed`/`Failed` yang wajib ber-`outcome` (membiarkan `Cancelled`/`Expired` `NULL`). Ini perubahan schema, bukan dokumen.
- Precedence lengkap cancel-vs-expiry dan penyelesaian node aktif saat reaper vs worker masih perlu matriks final di dokumen ini bila ditemukan kasus yang belum tertutup.
