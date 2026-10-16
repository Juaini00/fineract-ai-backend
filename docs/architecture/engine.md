# Engine lifecycle

Status: agreed lifecycle direction and clarification/transport integration,
recorded 2026-09-08; transition matrix, lease/fencing and recovery added later.

> **Status implementasi (2026-09-15, dikoreksi 2026-10-12).** Sudah berjalan:
> klaim/lease/fencing (T2), reaper (T11), eksekusi capability (T4), commit
> response (T7), klarifikasi (T5/T6), skip (T8), `job.resumed`. Tahap **plan
> (T3) saat ini diisi pencocokan leksikal/vector — itu pengganti sementara,
> bukan desain**: perencana adalah LLM, Engine memverifikasi
> ([build-order.md](../build-order.md) "Correction 2026-10-12", FIN-161).
> **Belum**: planner LLM, re-plan dan `plan_version` > 1, plan multi-node dan
> fan-in, reuse output lintas plan, dan D4 (kesegaran katalog pada batas
> verifikasi–eksekusi).

## One owner

One Engine owns context assembly, planning, verification, node scheduling, deterministic composition, response assembly and persistence. The LLM is the planner (it understands the request, selects the capability, binds slots and proposes the plan); the Engine verifies every model output before execution and never lets the model write SQL or decide authorization. Job API handlers accept/read commands and delegate. Executors and LLM clients never introduce separate orchestration loops.

```text
authenticate/validate → persist accepted job → acquire work
→ assemble context → plan/verify → execute ready nodes
→ compose deterministic evidence → assemble/validate response
→ persist response + memory promotion + completion event
```

Missing information suspends the same job through [clarification](../contracts/clarifications.md). Live facts may trigger a bounded, versioned re-plan. All paths use the same budgets and policy guards. Public progress is a projection through [SSE](../contracts/sse.md), not an alternate execution flow.

## Conversational understanding (FIN-164)

The owner approved the [conversational understanding design](conversation.md)
on 2026-10-16. It extends this Engine's planning boundary, persistence and
interaction semantics without introducing another agent loop. The FIN-162
schema below remains the analytical proposal contract; it is not a contract
for classifying every user message as analysis. Conversation implementation
requires the approved documentation change and real-provider readiness gate.

## LLM planner contract (FIN-162)

**Owner decision, 2026-10-12:** use a compact typed graph, not a model-authored
execution graph; keep the existing lifecycle/outcome vocabulary and distinguish
planner failures with `failure_code`. This section defines the contract, not
implemented behaviour. FIN-163 owns client wiring; L2 remains unproven until
the acceptance scenarios below pass through the real Engine.

### Input and retrieval

The Engine supplies the original request, accepted clarification bindings,
relevant bounded context, the declared calendar/timezone and `business_today`,
current authorization constraints, remaining job budgets, and a candidate
shortlist from one immutable catalog version. Each candidate contains its
approved ID, meaning, supported/unsupported intents, parameter types/defaults,
guards, output contract and approved resolver argument contract. SQL, provider
credentials and unrestricted source-schema access are never prompt inputs.
Context and catalog prose are data, not instructions or authorization grants.
Accepted bindings and authorization remain Engine-owned even when shown to the
model.

Lexical/vector retrieval may rank candidates, but must not select the executed
capability, bind slots, trigger clarification or infer out-of-scope from a score
or empty shortlist. The LLM makes the proposal for every admissible analysis
request; existing authorization/read-only/policy preflight rejections remain
Engine decisions. Missing or inadequate candidates produce a planning failure,
not a silent lexical execution path. Retrieval failure must be recorded as such
in the internal diagnostic; it cannot become evidence of unsupported scope.
A model `no_fit` means no supplied candidate fit, not that the entire approved
catalog or product scope was searched exhaustively.

### Output shape, version 1

The output is exactly one JSON object. Markdown fences, leading/trailing prose,
duplicate keys, unknown fields and unknown enum values are invalid. Native
provider JSON/schema modes are aids, never a replacement for runtime validation.

| Field | Type and rule |
| --- | --- |
| `schema_version` | Integer, exactly `1`. |
| `decision` | `planned`, `abstain` or `no_fit`. |
| `confidence` | `low`, `medium` or `high`; diagnostic only, never an authorization, resolver-cardinality or execution threshold. |
| `nodes` | Array of compact nodes. Nonempty for `planned`; empty for `abstain` and `no_fit`. |

Every node has exactly `id` (nonempty string unique within the proposal),
`capability_id` (an approved ID from the supplied shortlist), `depends_on`
(array of distinct node IDs) and `slots` (object keyed by declared parameter
name). The dependency graph must be acyclic; a node cannot depend on itself.
The Engine derives query/probe IDs, node kinds, SQL, output fields, retention,
timeouts and execution bindings from approved contracts. The model never
supplies these execution details, free-form joins or authorization fields.

Slot proposals are a closed tagged union:

| `kind` | Other fields | Meaning |
| --- | --- | --- |
| `value` | `value`, `source` | A request-derived non-identity value of the declared parameter type. Monetary values use decimal strings, not floating-point JSON numbers. |
| `date_phrase` | `source` | A date expression to resolve under the declared calendar contract; the model does not calculate relative dates. |
| `probe` | `arguments` | Arguments for the resolver declared on this identity slot. Each argument has exactly `value` and `source`; names/types come from the approved resolver contract. Empty arguments are legal only when that contract permits an unfiltered, scoped probe. |
| `node_output` | `node_id`, `output_slot` | A binding from a declared output slot of a dependency node, only where the consuming approved contract permits that composition. Not an arbitrary row selector or identity auto-bind. |

`source` has exactly `quote` (nonempty verbatim text from the original request)
and `occurrence` (zero-based integer identifying its left-to-right occurrence).
The Engine locates the quote in the original, unnormalized request and records
the matching half-open UTF-8 byte span `[start, end)` with its request/message
reference. The model need not count byte offsets. A nonexistent occurrence,
invalid source quote or value not supported by that quote fails verification.
Normalization is limited to the declared parameter/calendar contract; an
invented value cannot be justified by an unrelated quote.

Omitted slots are filled only from accepted bindings, current authorization or
declared defaults, with their actual provenance. They are not permission to
invent values or ignore a constraint stated in the request. Accepted identity
bindings cannot be replaced by model output. Missing required user inputs and
ambiguous date phrases follow the existing clarification/calendar contracts;
the model does not emit a form or a clarification decision.

For `Total setoran tabungan bulan ini`, a compact proposal is:

```json
{
  "schema_version": 1,
  "decision": "planned",
  "confidence": "high",
  "nodes": [
    {
      "id": "n1",
      "capability_id": "savings_deposit_total",
      "depends_on": [],
      "slots": {
        "from_date": {
          "kind": "date_phrase",
          "source": {"quote": "bulan ini", "occurrence": 0}
        },
        "to_date": {
          "kind": "date_phrase",
          "source": {"quote": "bulan ini", "occurrence": 0}
        }
      }
    }
  ]
}
```

The Engine resolves the period and supplies `office_ids` from authorization.
The example is a contract illustration, not a recorded successful model call.
The graph contract permits dependencies; multi-node scheduling/fan-in remains
the L8 gate `OVR-6.2`, not a claim made by the single-capability L2 proof.

### Static verification before T3

The Engine, not another model call, performs all of these checks before
persisting/admitting a plan:

1. Parse the closed versioned output; check decision/node consistency, unique
   IDs, dependency existence, acyclicity and binding dependencies.
2. Resolve every capability and resolver reference against the exact catalog
   version/hash supplied to the call. Check approval, shortlist membership,
   supported intent/output, parameter names/types and dependency-output
   compatibility. Never load SQL or contracts named by the model.
3. Check source spans, value provenance, accepted-binding precedence, declared
   defaults, missing required inputs and deterministic calendar resolution.
   Reject fabricated identities and undeclared parameter/coercion paths.
4. Recheck current principal/tenant authorization, office scope, PII policy,
   reference-data permissions and capability guards. Model/context values may
   narrow an authorized scope only through an approved path, never widen it.
5. Check remaining shared query/model/token/cost/re-plan budgets, timeouts,
   parameter caps, date-range limits, fan-in and per-node retention bounds.
   Limits and disclosures remain owned by the existing runtime/data contracts.
6. Expand the verified compact proposal into the Engine-owned execution graph,
   preserving slot provenance; commit its graph/hash, catalog version/hash,
   effective bindings, event and audit through T3 with lease fencing. No model,
   embedding or Fineract call occurs inside that transaction. Catalog freshness
   at admission still follows D4.

Model syntax/schema errors may receive bounded corrective calls using the same
request/catalog snapshot and sanitized validation diagnostics. An initial call
plus repairs/transient retries may never exceed `LLM_MAX_RETRIES` additional
calls, the absolute shared model/token budgets, timeout or job TTL. Every call,
including failed attempts, counts. These are planner-call repairs, not resets of
the re-plan/node-attempt budgets. Authorization/policy violations are not
repairable by asking the model to grant permission. No invalid proposal is
persisted as a verified T3 plan or dispatched to source execution.

### Failure outcomes and public boundary

Before any source node has run, all four cases settle as `Failed`,
`OperationalFailure`, `Unknown`, with these distinct sanitized `failure_code`
values in the job snapshot and durable failure event/audit:

| Cause | `failure_code` | Public meaning |
| --- | --- | --- |
| Generation provider unavailable, timeout or exhausted transient retries | `planner_unavailable` | The planner could not be reached or did not complete. |
| Malformed/schema-invalid output after bounded repair, or a proposal rejected by static verification | `planner_output_invalid` | The planner could not produce a verifiable plan. |
| Valid `abstain` decision | `planner_abstained` | The planner could not decide reliably. |
| Valid `no_fit`, empty shortlist or no usable candidate shortlist | `planner_no_fit` | The planner could not fit this request to the available candidates. |

Abstention and no-fit do not trigger model-output repair or an identity form.
They must not become `Unsupported`, `out_of_scope` or `NotFound`. Unsupported
scope requires an Engine-verified approved-surface/catalog coverage reason,
independent of retrieval scores or model confidence. A confirmed policy denial
still uses the existing policy outcome, not a fabricated planning diagnosis.
No deterministic capability-selection fallback runs when the model fails.

Settlement is fenced and atomic with its terminal event/audit, has no successful
analytical response or memory promotion, and never claims an empty population.
Public diagnostics contain neither prompts, raw model output, SQL nor provider
secrets. For failure during a later re-plan, durable outputs and completeness
follow the existing lifecycle matrix; do not overwrite valid evidence or force
`Unknown` where the ledger supports `Partial`. Cancellation/expiry precedence
and shared budgets are unchanged.

### K5 and small-model reliability

The LLM supplies identity search/probe arguments; the Engine executes the
declared bounded, authorized resolver. Only its cardinality/completeness
decides identity interaction: zero means no-match/refine-search, exactly one
complete result auto-binds without a form, and multiple results require options.
A truncated singleton never auto-binds. Model confidence, direct guessed IDs
and retrieval rank cannot bypass this rule. Bindings retain `resolver_unique`,
`user_confirmed` or the declared calendar provenance, are audited and disclosed
in the response. See [K5](../migration/carry-over.md#k5--klarifikasi-dipicu-kardinalitas-resolver-dikunci-2026-09-13).

The current cost-conscious local model is retained, but reliability is earned
by the scenarios below. Keep prompts bounded, expose only the shortlist's
typed contracts, require the compact closed output, and reject rather than
guess when verification fails. Provider reachability and JSON-mode support are
not planner-conformance evidence.

### Planner acceptance scenarios

These scenarios belong to L2. Contract publication names them; it does not mark
them passed. Pure verifier cases may use unit tests, but job outcomes, audit,
binding disclosures and real planning must be proven through HTTP/Bruno.

- `PLN-1` — An Indonesian request with an existing approved capability is planned by a real LLM call; the Engine verifies the selected ID, slot spans and concrete calendar bindings, and the executed result/provenance agrees with direct SQL.
- `PLN-2` — With multiple candidates, retrieval order/scores alone cannot select the executed capability or trigger identity clarification. Empty/failed retrieval and valid model no-fit are not reported as unsupported scope; a separately verified scope rejection remains distinguishable.
- `PLN-3` — Unknown/unapproved capability or probe, malformed source span/value, fabricated identity, widened scope, unknown parameter, invalid dependency/cycle and over-budget proposal are rejected before source dispatch and cannot become a verified T3 plan.
- `PLN-4` — A forced generation-provider outage/timeout ends the accepted job with `planner_unavailable`; snapshot, durable terminal event and audit agree, no source query runs and no deterministic planner fallback answers.
- `PLN-5` — Malformed output, including fenced JSON, remains invalid until a valid bounded repair passes verification. Exhaustion ends with `planner_output_invalid`; all attempts count against the same job budget and no source query runs.
- `PLN-6` — A valid `abstain` ends with `planner_abstained`, no identity form, no source query and no `Unsupported`/`NotFound` substitution.
- `PLN-7` — A valid `no_fit` ends with `planner_no_fit`, not `Unsupported`; no source query or hidden capability-selection fallback runs.
- `PLN-8` — LLM-supplied probe arguments lead to the K5 zero/one/many result paths; a truncated singleton is not auto-bound, high confidence does not bypass a form, and unique auto-binding is disclosed/audited without a fictitious form.
- `PLN-9` — Planner/provider calls, retries and terminal audit are correlated with the effective provider/model, prompt/schema version, catalog version/hash and observed usage. Every external call is outside T1–T12 (`OVR-6.7`); invalid proposals leave no executable plan and planner failure promotes no session facts.

## State dimensions

| Dimension | Proposed agreed vocabulary |
| --- | --- |
| Job lifecycle | `Queued`, `Running`, `WaitingForUser`, `Cancelling`, `Completed`, `Failed`, `Cancelled`, `Expired` |
| Outcome | `Answered`, `Empty`, `NotFound`, `Unsupported`, `BlockedByPolicy`, `Invalid`, `OperationalFailure`, `Cancelled`, `Expired` |
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
| `Cancelled` | `Cancelled` | `Unknown` | Tanpa response/memory promotion. Cancel adalah abort (K3), bukan kegagalan operasional. |
| `Expired` | `Expired` | `Partial`, `Unknown` | Tanpa response/memory promotion. Expiry adalah stop TTL, bukan kegagalan operasional. |

Kombinasi lain dilarang, khususnya: lifecycle nonterminal dengan `outcome`/`completeness` terisi; `Completed` dengan `OperationalFailure`; `Failed` dengan outcome selain `OperationalFailure`; `Cancelled` dengan outcome selain `Cancelled`; `Expired` dengan outcome selain `Expired`; `Empty`/`NotFound` dengan `Partial`; `SkippedByUser` dengan `Complete`.

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

Transisi attempt: `Pending → Runnable → Running → Completed|Failed|Skipped|Abandoned`. Attempt `Pending`/`Runnable` yang tidak lagi diperlukan — termasuk karena job menjadi terminal (`Expired`/`Cancelled`) sebelum attempt itu diadmisikan — ditutup langsung `Skipped` dalam transaksi settlement yang sama; bukan `Abandoned`, karena attempt itu pasti tidak pernah berjalan dan tidak ada outcome yang tidak pasti. Baris attempt terminal tidak dibuka kembali; recovery membuat attempt baru dengan `attempt` lebih besar, tanpa mengubah baris historis.

## Lease, fencing, dan recovery

**Klaim** (T2): hanya job `Queued` dengan lease kosong/kedaluwarsa yang dapat diklaim lewat `FOR UPDATE SKIP LOCKED`. Klaim menetapkan `lease_owner`, `lease_token`, `lease_claimed_at`, `heartbeat_at`, `lease_expires_at`, dan memindahkan lifecycle ke `Running`.

**Renewal**: lease awal 60 s, heartbeat 10 s, **wajib** dari task Tokio independen (K1), bukan batas node. Renewal hanya memperpanjang lease bila token/lifecycle cocok:

```sql
UPDATE chat_jobs
SET heartbeat_at = now(), lease_expires_at = now() + WORKER_LEASE_DURATION
WHERE id = $job_id AND lease_token = $token AND lifecycle = 'Running';
```

**Fencing (C16)**: setiap tulisan durable milik worker — plan pointer, node completion, budget, transisi lifecycle, event, dan audit terkait — memakai `AND lease_token = $token`. Update yang menyentuh 0 baris berarti worker sudah dipagari dan wajib berhenti tanpa commit lanjutan. External call yang sudah berjalan tidak dapat dibatalkan fencing, tetapi hasilnya tidak boleh dipersist oleh pemegang token basi.

**Recovery attempt tak pasti** (reaper T11, idempoten): (1) kunci job dan pastikan lease masih kedaluwarsa; (2) ubah attempt `Running` terkait menjadi `Abandoned` + event/audit; (3) jangan simpulkan external call gagal/sukses; (4) bila logical node masih diperlukan dan `attempt < NODE_ATTEMPT_CAP`, insert attempt baru (`attempt + 1`) `Pending`/`Runnable` sesuai fan-in — `attempt` dimulai dari 1, sehingga `NODE_ATTEMPT_CAP=3` berarti attempt 1, 2, 3 (percobaan awal + dua pemulihan, [runtime.md](../operations/runtime.md) §1) dan attempt 4 tidak pernah dibuat; (5) bila cap tercapai, selesaikan menurut fail-policy (`Failed`/`OperationalFailure`) atau response parsial eksplisit bila kontrak mengizinkan.

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

- **Diselesaikan 2026-09-20 (FIN-29, §6.4).** Diambil arah (a): `outcome` kini punya nilai `Cancelled`/`Expired`. Migration `20260920000001_outcome_cancel_expire.sql` melebarkan CHECK `chat_jobs_outcome_check` dan mem-backfill baris lama (`outcome = lifecycle` untuk `Cancelled`/`Expired`). CHECK `chat_jobs_terminal_has_outcome` tidak berubah: semua lifecycle terminal tetap wajib ber-`outcome` non-null, dan kini terpenuhi tanpa memakai `OperationalFailure` sebagai penanda palsu.
- Precedence lengkap cancel-vs-expiry dan penyelesaian node aktif saat reaper vs worker masih perlu matriks final di dokumen ini bila ditemukan kasus yang belum tertutup.
