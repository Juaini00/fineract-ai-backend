# Build order and passing gates

**Status: working contract. Binding for humans and agents alike.**
Replaces `status.md`, deleted 2026-09-15.

> Written in English on purpose. Most other documents in `docs/` are in
> Indonesian; this one is read first by every agent, so it uses the language
> agents handle most reliably. When you edit the Indonesian documents, keep them
> Indonesian.

---

## Why `status.md` was deleted

`status.md` answered the question "what is **running**?". That question looks
useful and turned out to be dangerous: the only signal available to answer it is
**green tests**, so "running" slowly came to be read as "matches the docs".

What happened on 2026-09-15: three milestones were marked ✅ on the strength of
111 unit tests and 173 integration tests passing. An audit against the docs found
that all three departed from their own contracts ([§6](#6-deviations-already-found)).
Green tests prove the code runs. They have never proved the code is **correct
according to what was agreed**.

> **"The app runs" and "the app conforms" are two different things.**
> This repository is a rewrite precisely because those two were once confused.

This document replaces that with the right question: **which layer has passed its
gate, and what is allowed to be worked on next.**

---

## 1. Four binding rules for agents

### Rule 1 — Done means acceptance scenarios, not green tests

The docs already define "done" for every layer. There are **86 acceptance
scenarios** spread across eight documents:

| Document | Scenarios |
| --- | --- |
| [contracts/clarifications.md](contracts/clarifications.md) | 15 |
| [contracts/responses.md](contracts/responses.md) | 14 |
| [data/analytical-contracts.md](data/analytical-contracts.md) | 12 |
| [contracts/api.md](contracts/api.md) | 11 |
| [contracts/sse.md](contracts/sse.md) | 10 |
| [data/dataset-lifecycle.md](data/dataset-lifecycle.md) | 10 |
| [architecture/overview.md](architecture/overview.md) | 7 |
| [architecture/memory-context.md](architecture/memory-context.md) | 7 |

As of 2026-09-15, the number of tests that reference any of those 86 scenarios
is **zero**. That is the root cause — not incomplete docs.

**The rule**: every test names the scenario ID it proves. A status row may not be
✅ if its scenario has no test.

### Rule 2 — A layer may never be rated above its prerequisites

L7 cannot be ✅ while L1 is still ⬜. This is mechanical, not a judgement call.

The reason is concrete: `session_memory` promotion writes its rows correctly, but
the facts it promotes come from answers whose correctness has never been checked
(L1/L4), reference a `dataset_id` that cannot exist yet (L3), and a
`handle_state` that is not yet a column. Calling that "done" hides all of it.

### Rule 3 — Never change a contract document in the same commit as code

If the code cannot satisfy the docs, the agent **stops and asks**. Editing the
clause so it matches the code reverses the direction of authority: the docs are
research agreed before any code was written, and the code conforms to them.

Changing a contract is the repository owner's decision, in its own `docs:` commit,
with the reasoning recorded.

**Exception** — these two documents *must* travel with the code commit, because
they record "what exists" rather than "what was agreed":

- [contracts/api-reference.md](contracts/api-reference.md)
- this document (§5 status table)

### Rule 4 — Silence about mechanism is not permission to invent concepts

The docs deliberately say nothing about token management, vector creation, and
similar plumbing: those parts were already correct and did not need restating.

- Docs silent about **mechanism** → follow what already works; do not rebuild it.
- Docs speak about **concept** → binding, no interpretation.

Block vocabulary, the memory promotion point, lifecycle ordering, the direction
of D1 — all of these are concepts. Breaking one while believing you are deciding
a mechanism is exactly the failure that destroyed the previous application.

---

## 2. Status vocabulary

| | Meaning |
| --- | --- |
| ⬜ | Not built. A design may exist; that is not the same thing. |
| 🔨 | Built, but **not yet** checked against its acceptance scenarios. |
| 🧪 | Acceptance scenarios pass, but **prerequisites are not** ✅. This is the ceiling while prerequisites are unfinished. |
| ✅ | Scenarios pass **and** every prerequisite is ✅. |
| ❌ | Built and **proven to deviate** from its contract. Must be fixed, not extended. |

There is no "partial". If a qualifier is needed, put it in the notes column.

---

## 3. The ladder, L0–L8

This ordering is a **dependency chain**, not a product priority list. The numbers
cannot be swapped without editing this document first.

### L0 — Foundation, schema, auth, transport

**Goal**: migrations, invariants I1–I8, response envelope, auth, SSE, lease/fencing.
**Owning docs**: [data/database-design.md](data/database-design.md) §1 §2 §5 ·
[architecture/engine.md](architecture/engine.md) · [contracts/sse.md](contracts/sse.md)
**Prerequisites**: none
**Done when**: `tests/schema_smoke.sql` passes, and the SSE (10) and API (11)
scenarios each have a test carrying their ID.
**Status**: 🔨 — the mechanism runs and is tested, but not one scenario is mapped
to a test.

### L1 — A catalog that is actually correct

**Goal**: `knowledge/` and `queries/` stop being unreviewed carry-over.
**Owning docs**: [knowledge/CARRY-OVER.md](../knowledge/CARRY-OVER.md) ·
[queries/CARRY-OVER.md](../queries/CARRY-OVER.md)
**Prerequisites**: L0
**Done when** the four rules in `CARRY-OVER.md` hold for each entry:

1. Its example runs end to end and **its numbers are matched against direct SQL**
   on the local Fineract database (8 offices, 43 clients, 15,607 transactions).
2. The title and description prose agree with the SQL it points to.
3. Output column sensitivity classes match the PII policy.
4. Cutoff / as-of semantics are declared.

Plus the items `queries/CARRY-OVER.md` says nobody has checked yet: the two
timeout classes, grain and anti-fanout, column naming against the response
validation rules, and currency semantics.

**Status**: ⬜ — 0 of 48 capabilities have passed.
**Note**: `CARRY-OVER.md` states plainly that entries which have not cleared
those four points **must not be used to answer questions that are trusted**.
Every answer produced so far comes from entries that have not cleared them.

### L2 — Retrieval finds the capability that exists

**Goal**: a user's question reaches the capability the system actually has.
**Owning docs**: [architecture/tech-stack.md](architecture/tech-stack.md) ·
[migration/carry-over.md](migration/carry-over.md) #7
**Prerequisites**: L1
**Done when**: an Indonesian-language question whose capability exists finds it,
and "we failed to find it" is reported differently from "it is out of scope"
(see §6.5).

**Status**: ⬜
**Note**: all catalog prose is in English while the product serves Indonesian
users. `"berapa total portfolio aktif bulan ini"` produces **0 lexical matches**
even though the word `portfolio` appears in 4 catalog entries. The embedding arm
does not exist — `knowledge_index.embedding vector(1024)` is populated on 0 of
192 rows. Vector creation itself is not specified in the docs because it was
already correct (Rule 4).

### L3 — Dataset handles and chunks

**Goal**: large results get a storage path, result pagination exists, and an
expired handle can be stated as expired.
**Owning docs**: [data/dataset-lifecycle.md](data/dataset-lifecycle.md) (10 scenarios) ·
[migration/carry-over.md](migration/carry-over.md) #11
**Prerequisites**: L0
**Done when**: the 10 scenarios in §8 pass, including `truncated` ≠ `completeness`
≠ preview, stable pagination via `sort_key_json`, authorization re-checked on
every read (I7), and a `purged` dataset still readable as such.
**Status**: ⬜ — the `datasets` and `dataset_chunks` tables have existed since the
migration, and **no line of code touches them**. `handle_state` (C13) is not yet a
column. Query results are dumped inline into `job_node_runs.output_json`.

### L4 — Jobs answer correctly

**Goal**: the numbers coming out are proven right, not merely produced.
**Owning docs**: [architecture/engine.md](architecture/engine.md) ·
[architecture/overview.md](architecture/overview.md) §6
**Prerequisites**: L1, L3
**Done when**: for every capability in use, the job's result is compared against
direct SQL on Fineract and matches, and every L4-owned OVR scenario (OVR-6.1,
OVR-6.3…6.7) carries a test naming its ID. OVR-6.2 (multi-node fan-in) is owned
by L8 — owner decision 2026-09-25, see §5.1.
**Status**: 🔨 — the job machinery (8 lifecycle states, fencing, idempotency,
reaper) runs and is tested. **The correctness of the answers has never been
tested by anyone.**

### L5 — Response documents match the contract

**Goal**: document shape conforms to [contracts/responses.md](contracts/responses.md) §1–§2.
**Prerequisites**: L0 (can run in parallel with L1–L4; see §4)
**Done when**: every block carries `block_id`, `type`, `schema_version` and
`derived_from`; the vocabulary is limited to the 9 types in §2; lineage lives in
`evidence_json` rather than in a block.
**Status**: ❌ — deviates; see §6.2.

### L6 — The full D1–D3 validator

**Goal**: runtime enforcement that actually enforces the contract.
**Owning docs**: [contracts/responses.md](contracts/responses.md) §3–§6 (14 scenarios)
**Prerequisites**: L5
**Done when**: D1 is computed **per block** through `derived_from` and then
aggregated (§3 rules 2–3); D2 inspects the `note` block (§5); D3 uses the closed
exclusion list from §4; §1 and §2 are enforced, so a type outside the vocabulary
is rejected.
**Status**: ❌ — an approximation; see §6.3.

### L7 — Complete clarification and memory that is actually used

**Goal**: a conversation, rather than a series of unrelated questions.
**Owning docs**: [contracts/clarifications.md](contracts/clarifications.md) (15 scenarios) ·
[architecture/memory-context.md](architecture/memory-context.md) (7 scenarios)
**Prerequisites**: L1, L2, L3, L4, L5, L6
**Done when**: all 22 scenarios pass, including promotion on T8 skip (§7-3),
non-optional `handle_state` (§5, C13), budgeted context selection (§5), and
incremental summaries with a watermark (§4).
**Status**: ❌ / 🔨 — the memory write path is correct and proven, but T8 skip
deviates (§6.1), nothing reads memory back, the summary is never computed, and
`dataset_id` is always NULL because L3 does not exist.

### L8 — Everything above

Additive LLM narration, multi-node plans with fan-in (including scenario
OVR-6.2, moved here from L4 — §5.1), analytical contracts (Mode 2), final
security model, observability, OpenAPI.
**Prerequisites**: L7.
**Status**: ⬜ — **do not touch** until L1–L7 are ✅.

---

## 4. What can be worked on in parallel

Several agents at once is **allowed**, as long as no one skips a prerequisite.

| Lane | Work | Conflicts with |
| --- | --- | --- |
| **A — Catalog (L1)** | verify the 48 capabilities one at a time | nothing; easiest to parallelise, one agent per domain |
| **B — Datasets (L3)** | `datasets` / `dataset_chunks`, a pure storage layer | nothing |
| **C — Document shape (L5+L6)** | `compose.rs`, `validate.rs`, `evidence_json` | do not run alongside any other lane that touches `compose.rs` |
| **D — Scenario mapping** | give IDs to the 86 scenarios, write `scripts/acceptance-check.sh` | touches all of `docs/` — run it **alone** |

Lane D should be **done first and finished before the others start**: without the
scenario map, lanes A, B and C have no way to prove they are done.

L2 waits for L1. L4 waits for L1 and L3. L7 waits for all of them.

---

## 5. Status at a glance

| Layer | Status | Prerequisites met? |
| --- | --- | --- |
| L0 Foundation | 🔨 | — |
| L1 Correct catalog | 🧪 | L0 🔨 |
| L2 Retrieval | 🧪 | L1 🧪 |
| L3 Datasets | 🧪 | L0 🔨 |
| L4 Correct answers | 🔨 | L1 🧪, L3 🧪 |
| L5 Response shape | 🔨 | L0 🔨 |
| L6 Validator | 🔨 | L5 🔨 |
| L7 Clarification + memory | ❌ | six layers below are not ✅ |
| L8 Above | ⬜ | L7 ❌ |

**Not a single layer is ✅.** That is the real state on 2026-09-15, and it is more
useful than a list of ✅ marks nobody can stand behind.

Update after commits `fd425a8` (L1) and `d3a1535` (L3, L5, L6):

- **L1 ⬜ → 🔨.** All 48 capabilities were run end to end and compared against
  direct SQL (`knowledge/VERIFICATION.md`): 39 match, 9 do not. L1 is **not ✅**:
  its gate is the four `CARRY-OVER.md` rules, and 9 entries fail rule 1 (mixed
  currency, silent `LIMIT` truncation, double resolver grain). They are recorded,
  not yet fixed — fixing them changes capability contracts and waits on the owner.
- **L5 ❌ → 🔨 and L6 ❌ → 🔨.** The §6.2 and §6.3 deviations are fixed. All 10
  RESP scenarios now carry a test: 8.7 (chart→table downgrade) and 8.9 (expired
  dataset table) were the last two, proven by pure-logic unit tests in
  `compose.rs` (`chart_or_table`, `expired_dataset_table`). Both are composition
  logic not yet wired into the served response — no live path emits a `chart_spec`
  or a dataset-backed `table` today — so L5/L6 **cannot rise above 🔨**: not every
  scenario is proven to *conform*, and prerequisites remain unfinished.
- **L3 ⬜ → 🔨.** `datasets`/`dataset_chunks` are now written and read; all 6 DS
  scenarios are referenced by **unit** tests. It is **not** 🧪: DS-8.2 and DS-8.4
  are HTTP-surface behaviours and have no Bruno test yet, and the repo's own
  thesis is that a passing unit test is not proof of the acceptance scenario.

Update after L1.1–L1.8 (PR #2–#8, merged):

- **L1 🔨 → 🧪.** All nine rule-1 failures are fixed and merged, so the four
  `CARRY-OVER.md` rules now hold for all 48 capabilities and
  `knowledge/VERIFICATION.md` records **48 lulus / 0 gagal** with two adjacent
  numbers per fixed capability: currency-mixing split per currency (L1.1),
  silent `LIMIT` removed/bound with disclosure (L1.2), double resolver grain
  fixed 24 = 24 (L1.3); plus the two timeout classes enforced (L1.7), canonical
  `client_display_name` (L1.6), `snapshot_only` recorded as a §2.2 debt (L1.8),
  and result grain declared per query and enforced (L1.5). `cargo run -p app --
  catalog` → **0 error**; the validator now mechanically enforces grain,
  timeout class, column naming and output shape (constraint, not prose — I6). L1
  is marked **🧪** (mechanism + evidence complete, gate green). ✅ is defensible
  since L1 owns no separate acceptance scenario — its gate *is* these four rules
  — but the ✅ stamp is left to the owner at review.

Coverage moved from 0/59 to **16/59** (see §5.1) — the two RESP scenarios added
this cycle. Besides L1 (🧪), no other layer reaches 🧪 or ✅: the ceiling is
capped by unfinished prerequisites and by scenarios that still lack a test.

Update after L2 (FIN-40/41/42/7, PR #10–#12, #14) and FIN-43 (PR #16):

- **L2 ⬜ → 🧪.** The §3 gate holds at the HTTP surface: an Indonesian question
  whose capability exists finds it (lexical arm, FIN-40), a held-out Indonesian
  question reaches `savings_deposit_total` through the semantic arm alone
  (FIN-41; every catalog version's `knowledge_index` rows carry a
  `vector(1024)`, 0 NULL), and a retrieval failure is reported as
  `retrieval_miss` while an out-of-domain request with a healthy semantic arm is
  `out_of_scope` (FIN-42, §6.5). Proven by the `retrieval-unavailable`,
  `retrieval-vector` and `retrieval-healthy` Bruno stages. Ceiling is 🧪 because
  L1 is 🧪, not ✅.
- **L3 stays 🔨.** §6.6 #1 is resolved: `dataset_id` is discoverable from the
  response lineage and the handle/rows endpoints are exercised end to end. #2
  (purge seam) and the scope-narrowing half of #3 remain.
- The table rows for L1 and L4 are corrected to the already-recorded L1 🧪.

Coverage is **30/59** (see §5.1).

Update after FIN-46 (DS-8.1):

- **DS-8.1 is proven at the HTTP surface.** Local Fineract data (15,607
  transactions) never reaches `DATASET_MAX_ROWS` (100,000), so the truncated
  branch was unreachable. The owner-approved seam `LOCAL_DATASET_MAX_ROWS`
  (honoured only when `APP_ENV=local`; startup fails elsewhere; it only
  narrows the cap) drives the new `dataset-capped` Bruno stage: the stored
  handle is `truncated=true`, `Partial`, `dataset_row_cap_reached`,
  `row_count_available` 1 < `row_count_total`; the answer stays `Complete`
  over all N node rows and states the handle's cap in a `limitation` block
  `dataset_truncated`. Locally N = 3 (AED/EUR/USD), and the served numbers were
  cross-checked by hand against direct SQL on `fineract_default` (the Bruno
  stage itself asserts N > 1 stored-vs-answer, not the SQL values).
- The truncation reason now names the cap that was actually hit
  (`dataset_row_cap_reached` vs `dataset_byte_cap_reached`); before, a byte-cap
  cut was reported as a row-cap cut.
- L3 stays 🔨: DS-8.5 and DS-8.6 are still unit-level.

Update after FIN-50 (DS-8.5):

- **DS-8.5 now holds in the schema, not only in the discriminator.** The
  original `dataset_chunks.payload JSONB NOT NULL` meant a real `BYTEA` chunk
  still required a migration (as `migration/carry-over.md` #11 itself said:
  "add a column + a new format value"), contradicting DS-8.5's "without a
  migration". Owner decision: prepare the column now. Migration
  `20260923000001` makes `payload` nullable, adds `payload_bytes BYTEA`, and
  enforces exactly one payload per chunk — with deliberately no CHECK tying
  `format` to a column, so a future encoding is a value, not DDL.
- Proof is at the database, the surface DS-8.5 names: `tests/schema_smoke.sql`
  T13 writes a legacy JSONB chunk and a `bytea_zstd`/v2 chunk into the same
  dataset with no DDL, and rejects a chunk with zero or two payloads. The read
  path names an unreadable chunk (`chunk_encoding_unsupported`) instead of
  reading it as zero rows; `decode` now also rejects a `json` chunk whose
  payload is not an array, which it previously read as empty.
- `docs/data/database-design.md` §4.17 still lists only `payload JSONB`, and
  `docs/migration/carry-over.md` #11 still says a future BYTEA needs "a column
  + a new format value" — the column now exists. The owner updates both in a
  separate `docs:` commit (Rule 3).
- L3 stays 🔨 at this point: DS-8.6 is still unit-level.

Update after FIN-51 (DS-8.6):

- **The eviction guard is fail-closed.** `dataset::releasable` — the single
  predicate both release paths route through (T11 reaper `purge_expired` and
  the FIN-44 local seam `purge_now`) — now releases only datasets whose job is
  in an explicit **terminal** allowlist (`Completed`/`Failed`/`Cancelled`/
  `Expired`). Before, it excluded a denylist of the four nonterminal states,
  so a lifecycle added to the CHECK later would have been evicted by default.
- DS-8.6 is proven at the unit, as its ticket specifies:
  `ds_8_6_release_never_touches_a_running_job` (all four nonterminal states
  kept, all four terminal released) and
  `ds_8_6_unknown_lifecycle_is_kept_not_evicted`. The terminal control case is
  also exercised at HTTP by `engine/dataset-purge.yml` (FIN-44). The
  nonterminal branch has no HTTP trigger: a handle's id is surfaced only in the
  settled response's lineage, and terminal lifecycles are absorbing. A
  nonterminal job can still hold a `ready` handle — `retain_dataset` commits
  before settle, so a cancel or lease loss in between leaves it
  `Cancelling`/`Queued` until the reaper settles it — so the guard is
  load-bearing, not defence in depth. (A lease-loss re-run also writes a second
  handle for the same node; `datasets` has no unique key on job/node.)
- Quota eviction (`SESSION_RETAINED_BYTES_QUOTA`, LRU) does not exist yet; when
  it is built it must route through `releasable`.
- **L3 🔨 → 🧪.** All six DS scenarios pass: DS-8.1..8.4 at the HTTP surface,
  DS-8.5 at the schema, DS-8.6 at the release predicate. 🧪 is the ceiling
  while L0 is 🔨.

Update after FIN-52 (L4 answer gate) and the bugs it found (FIN-132, FIN-133, FIN-134):

- **The L4 correctness gate now runs on every integration run.** Stage
  `answers` (`scripts/integration-test.sh`) runs every reachable answer
  capability through the real job path (`POST /chat/jobs` → worker →
  validated response, with the clarification round-trip where a capability
  needs one), and compares the answer with direct SQL written separately from
  `queries/**` (`tests/answers/*.sql`). `scripts/answer-expectations.sh`
  recomputes that SQL live just before the stage, using the same UTC
  `business_today` the planner binds. `fineract-assistant-api/lib/answers.js`
  asserts four things: the capability id, bound parameters = the oracle's
  declared parameters (office scope included), column set, and rows in order.
  Row-capped capabilities also assert the `row_cap_reached` disclosure against
  an uncapped population count. Latest run: 219 requests, 245 tests, all green.
- **Coverage: 42 of 48 capabilities** are compared (41 row-for-row, 1 random
  sample as a subset of the population). The other six:
  - 4 are continuation-only resolvers/probes, never an answer node:
    `organization_office_identity_resolve`, `client_identity_resolve`,
    `savings_charge_type_identity_resolve` (a probe that nothing wires) and
    `group_identity_resolve` (`status: candidate`).
  - 2 can never be answered: `savings_account_identity_lookup` and
    `savings_account_terms_lookup`. Their `account_number` is
    `transient_sensitive_input` with no probe, so K1 returns
    `identity_slot_without_resolver`; the `answers` chain asserts exactly that.
- One comparison is vacuous today: `client_activation_top_n_offices` has 0
  activations this month, so 0 rows = 0 rows.
- **Wrong answers the gate caught and that are now fixed:**
  - FIN-132: `savings_activity_list` labelled type 17 as `withhold_tax`.
  - FIN-133: `limit: unbounded` ignored the declared caps (4114 rows instead of
    100 disclosed). Owner decision: cap + disclose.
- **Open findings (not fixed here):**
  - The planner never extracts dates, limits, currencies or names from the
    request text. It binds manifest defaults, and says nothing about the
    parameters the user stated.
  - Some phrasings are misrouted by retrieval, e.g. "Top withdrawals per month…"
    goes to `client_activation_top_n_offices`.
- **L4 stays 🔨.** The "every capability in use matches direct SQL" half of the
  §3 gate holds. The seven OVR scenarios still have no test (FIN-53…FIN-59).

Update after FIN-53 (OVR-6.1):

- **The Fineract pool is read-only at the server, not by convention.** Every
  session opens with `default_transaction_read_only=on`
  (`core::db::FineractDb::connect`); a write fails with `cannot execute UPDATE
  in a read-only transaction` even under the writable local `root` credential.
  Before, "read-only" rested only on a separate credential that local setups
  do not have. The query runs outside any application transaction
  (`worker.rs` → `executor::execute`, I1).
- **OVR-6.1 is proven at the HTTP surface** in the `answers` chain for
  `savings_balance_summary` (worker on): the 202 acknowledgement is `Queued`
  with `event_cursor` 1; the full replay starts with the durable T1 event
  `job.accepted` (Queued), carries one plan and no clarification, and ends
  exactly once in `job.completed` → response version 1, `validation_status`
  `passed`, `Complete` (T7); the answer is recomputed by direct SQL
  (`tests/answers/savings_balance_summary.sql`). It lives in `answers/`, not
  `engine/`, because only that stage has the direct-SQL oracle.
- L4 stays 🔨: OVR-6.2..6.7 (FIN-54…FIN-59) have no test.

Update after FIN-55 (OVR-6.3):

- **OVR-6.3 is proven at the HTTP surface** by the `clarification` chain
  (worker on), with no code change — the mechanism already held; it lacked a
  test naming the scenario. The job suspends to `WaitingForUser` with the same
  `job_id` and no `terminal_at`; a refresh returns the durable form
  byte-for-byte; a valid answer resumes the same job to a `passed`/`Complete`
  response; the full replay holds `job.accepted` → `clarification.required` →
  `clarification.accepted` → `job.resumed` → `job.completed` in order on one
  contiguous cursor; and a reconnect (`job-reconnect-event.yml`) with
  `Last-Event-ID` = the snapshot cursor taken while suspended returns exactly
  the tail of the full replay — nothing lost, nothing duplicated.
- Single clarification stage only; the multi-stage half belongs to CLR-3
  (FIN-75, L7), as for SSE-5.
- L4 stays 🔨: OVR-6.2, 6.4..6.7 have no test.

Update after FIN-57 (OVR-6.5):

- **OVR-6.5 is proven at the HTTP surface** by a new `redis-down` stage in
  `scripts/integration-test.sh`: the worker runs with `REDIS_URL` pointing at a
  port that never listens, so Redis is enabled but unreachable (`/health`
  `redis: "unavailable"` is asserted — a real outage, not
  `REDIS_ENABLED=false`). The client opens `GET /events`, reads the first frame
  and disconnects while the job is still nonterminal (asserted, so the proof
  cannot pass vacuously). The job still completes (`Completed`/`Answered`,
  response `passed`/`Complete`), is never `Cancelling`/`Cancelled`, and a
  replay from PostgreSQL returns exactly `1..last_event_sequence` ending in
  `job.completed` with the snapshot's `final_response_version`. No code change:
  the mechanism (pg_notify + polling fallback, optional Redis) already held.
- L4 stays 🔨: OVR-6.2, 6.4, 6.6, 6.7 have no test.

Update after FIN-58 (OVR-6.6) and the bug it found (FIN-139):

- **Found:** "Delete all clients.", "Run this SQL: DROP TABLE m_client" and
  "Show the password hash of every app user." were answered
  `Answered`/`Complete` through `client_list_recent` (lexical match on
  "client"). Nothing was written — SQL only comes from `queries/` and the pool
  is read-only since FIN-53 — but the source query ran and a wrong answer was
  served instead of a refusal. Office ids outside the caller's authorization
  were silently dropped and the job answered over the rest.
- **Owner decision (FIN-139): a deterministic write-intent guard.**
  `engine/write_intent.rs` refuses a request whose first meaningful token is a
  mutation verb (EN/ID) **and** names a Fineract entity (client, rekening,
  pinjaman, …) within the next three tokens, or that carries write/DDL SQL
  (`create|drop|alter|truncate` + object, `delete from`, `insert into`,
  `update … set`, `grant|revoke` + privilege). Domain nouns that are also verbs
  (deposit, transfer, tarik) and conversational follow-ups ("change the period
  to last quarter") stay reads; the reviewer pass caught those false positives
  in a first draft. It runs first in `run_job`, before scope, retrieval, plan
  and any source query.
- **Scope widening is refused, not trimmed:** any requested office outside the
  authorization settles the job `office_scope_not_authorized`.
- Both refusals: `Completed` + `BlockedByPolicy` + `Unknown`, `kind`
  `limitation`, `plan_version` null, no `node.status_changed`, no lineage.
  Proven by `engine/policy-{widen,write,sql,repeat}-*` plus a narrowing
  control (`policy-narrow-*`: `[1]` still answers over "1 authorized
  offices"). `repeat` asks again in the same session after a refusal and is
  refused again: session history grants nothing (I7). Memory itself (L7) does
  not exist yet, so the memory half of I7 is only as strong as that.
- **Still open (FIN-139):** a request naming an unapproved *field/entity*
  ("password hash of every app user") is still misrouted to a read capability;
  the guard catches write commands only. That half of OVR-6.6 is not proven.
- L4 stays 🔨: OVR-6.2, 6.4, 6.7 have no test; OVR-6.6 is partial.

Update after FIN-59 (OVR-6.7):

- **I1 is instrumented, not just stated.** `crates/core/src/commit_isolation.rs`:
  every app commit transaction opens through `commit_isolation::begin`, which
  marks the calling Tokio task (thread outside a task) as "inside a commit
  transaction" until the returned window drops. `clippy.toml`
  (`disallowed-methods = sqlx::Pool::begin`) makes that the only way to open
  one, so a new T-block cannot silently skip the instrumentation. The external
  choke points — `FineractDb::pool()`/`ping()` (every source query, office
  scope, resolver, probe), `EmbeddingClient::embed` (the only HTTP client) and
  `Notifier::publish` (Redis) — call `commit_isolation::guard`. A call inside
  the caller's own window is counted, logged `error!` with the marker
  `commit_isolation_violation`, and panics in debug builds. No LLM client
  exists yet; one must call `guard` too. The marking is per task: a
  transaction open in another task does not taint this one.
- **Proof:** unit test
  `ovr_6_7_external_call_is_refused_only_inside_the_callers_commit_window`
  (outside → allowed, uncounted; other task's window → allowed; own window →
  refused + counted; after commit → allowed; debug `guard` panics with the
  marker). `scripts/integration-test.sh` now fails the whole run when the app
  log of any stage contains the marker, so every full run asserts zero
  violations across all exercised T-blocks (T1–T9, T11). Bruno
  `engine/isolation-*` (`OVR-6.7: …`) runs one answered job through
  T1→T2→T3→T4→T7 and asserts one event per transition, contiguous sequence
  `1..N`, and `chat_jobs.last_event_sequence = N`.
- **Exposure choice:** no route exposes the counter — `/health`'s shape is
  fixed in `contracts/api-reference.md` and no doc backs a new surface, so the
  log gate is the integration assertion.
- **Audit/event atomicity** was checked, not changed: every transition writes
  `append_event*` and `audit::insert` through the same `&mut tx`
  (T1 `job::repository::create_job`, T2 `claim_next`, T3 `persist_plan`,
  T4 `complete_node`, T5 `clarification::repository::open_form`, T6
  `accept_answers`, T7 `settle_with_response`/`settle_failed`, T8 `skip`,
  T9 `request_cancel`/`settle_cancelled`, T11 `finish_sweep_row`); both helpers
  only accept a `Transaction`. T10 and T12 have no implementation yet.
- L4 stays 🔨: OVR-6.2, 6.4 have no test; OVR-6.6 is partial.

Update after FIN-139 (OVR-6.6 unapproved surface):

- **OVR-6.6 is now fully proven; FIN-58 criterion 1 ("a request naming an
  unapproved surface/field is rejected before any Fineract query runs") is
  closed.** "Show the password hash of every app user." and "List all app
  users." settle `Completed` + `BlockedByPolicy` + `Unknown`,
  `surface_not_approved`, `kind` `limitation`, `plan_version` null, no
  `node.status_changed`, no lineage (`engine/policy-surface-*`,
  `engine/policy-surface-entity-*`).
- **The vocabulary comes from `knowledge/`, not from Rust lists**
  (`catalog/surface.rs`, built once at catalog load):
  `unsupported_requests.yaml` `hard_reject` names secret fields and
  out-of-scope tables, and `hard_reject_maps_to_policy` forbids mapping them
  to an approved capability. Terms: the `secret_never_expose` examples of
  `schema/fineract/columns/sensitivity.yaml`; the `excluded_tables` of every
  `data-scope/areas/*.yaml`; every domain's `unsupported_intents`. Areas owned
  by a `deferred` domain (loans, accounting_gl, tax) are skipped: those domains
  say to answer `Unsupported` with a deferred reason (`domains/loan.yaml`
  `default_rules`), not a policy refusal.
- **Narrow by construction:** whole-token phrase match (plural `-s`
  normalized), never substring; a one-segment table (`m_role`,
  `m_permission`, `m_appuser`) matches only as its identifier or a split
  compound ("app user" → `appuser`), never as the plain word "role" or
  "permission"; and any term that the approved capabilities' own prose uses
  is dropped (e.g. `result`). A unit test runs every `request_text` in
  `fineract-assistant-api/` through the real catalog: only the
  `policy-surface-*` chains are refused.
- It runs in `run_job` right after the write guard, before scope, retrieval,
  plan and any source query.
- **Not covered (closed by FIN-140, see below):** the vocabulary was English
  because `knowledge/` was; an Indonesian paraphrase ("kata sandi") was not
  matched. Deferred-domain questions (loans, tax, accounting) had no route of
  their own and could fall through to the nearest read capability.
- L4 stays 🔨: OVR-6.2, 6.4 have no test.

Update after FIN-56 (OVR-6.4):

- **Found (by reading the code, then fixed):** lease-loss recovery requeued
  the job but never touched the node ledger. The node run was never `Running`
  (T3 wrote `Runnable`, T4 wrote the terminal status directly), so no attempt
  could ever become `Abandoned`; `attempt` was hard-coded to 1; the re-claimed
  worker re-inserted plan version 1 and failed on `job_plans_version_uniq`;
  and because `process` returned that error before cancelling the heartbeat,
  the orphaned heartbeat kept the lease alive, so the reaper never saw the job
  again and it sat `Running` until the 30-minute TTL expired it.
- **Now (engine.md "Recovery attempt tak pasti"):** the worker admits the
  attempt (`Runnable` → `Running`, fenced) right before the source query. The
  reaper (T11) locks each job whose lease expired while `Running`, marks its
  `Running` attempt `Abandoned` (`completeness` `Unknown`, no `failure_code`,
  event `node.status_changed` + audit `node.abandoned`), inserts attempt + 1
  `Runnable`, requeues the job and announces the retry on `job.notice`
  (`retry: [{node_id, attempt}]`). The re-claimed worker re-verifies the plan
  and **adopts** the stored plan version when graph and contract/catalog are
  identical (`plan.adopted`); if they differ it refuses to run the node
  (`plan_changed_on_recovery`, D4 — re-plan does not exist yet). An attempt
  that ends `Abandoned` still counts toward `query_count` (budget absolute
  across attempts, K9). If the lease was lost after T4 but before T7, the node
  is already `Completed`; it is not rerun ("persisted completed outputs are not
  rerun") and output reuse does not exist, so the re-claimed worker settles
  `completed_node_not_rerun` instead of requeueing forever.
- **Bound (`NODE_ATTEMPT_CAP`, now read by config, default 3):** once attempt
  3 is `Abandoned` the reaper does not retry again; the job settles `Failed` +
  `OperationalFailure` + `Unknown` with `failure_code`/reason
  `node_attempt_cap_reached` and a `limitation` response that says the outcome
  is unknown, not zero. Three attempts follows runtime.md §1 ("Tiga = percobaan
  awal + dua pemulihan", and its revision trigger names "attempt 3"). engine.md
  step (4) writes the condition as `attempt + 1 < NODE_ATTEMPT_CAP`, which with
  1-based attempts (`migration/carry-over.md` #2: "attempt (mulai 1)") would
  allow only two — the owner should reconcile the formula in a `docs:` commit.
- **OVR-6.4 is proven at the HTTP surface** by two new stages. The
  owner-approved seam pattern (FIN-44/FIN-46) gives
  `LOCAL_CRASH_AFTER_EXTERNAL_CALL=<n>` (honoured only when `APP_ENV=local`;
  startup fails elsewhere or at 0): the first `n` source calls of the process
  are abandoned right after the query returns and before T4 — heartbeat
  stopped, nothing written — as if the worker died there. The stages run with
  lease 6 s / heartbeat 2 s / reaper 2 s. `crash-recovery` (n = 1): replay shows
  attempt 1 `Abandoned`, then the retry notice, then attempt 2 `Completed`, one
  `job.completed`; the job is `Completed`/`Answered` on plan version 1 with no
  `failure_code`. `crash-exhausted` (n = 3): attempts 1–3 `Abandoned`, exactly
  two retries, no attempt 4, one `job.failed` `node_attempt_cap_reached`.
- Not proven by a test: the `completed_node_not_rerun` and
  `plan_changed_on_recovery` branches (no seam reaches them). Still open: a
  worker **error** (not a crash) returns before the heartbeat is cancelled, so
  the lease stays alive until the TTL expires the job; a crash **before**
  admission requeues without a bound because every claim recomputes
  `expires_at` (K3); and `Running` attempts of jobs settled `Expired` or
  `Cancelled` by the reaper stay `Running` in the ledger.
- L4 stays 🔨: OVR-6.2 has no test.

Update after FIN-141 (OVR-6.4 recovery bugs):

- **Classification: code bug, all three.** Each new assertion below was run
  against the pre-fix state machine (fix hunks reverted, seam kept) and failed:
  `worker-error` saw 0 reaper notices (heartbeat held the lease until TTL);
  with only the heartbeat fixed, the job never reached a terminal state within
  90 s (every re-claim reset `expires_at`); `crash-expired`/`crash-cancelled`
  replays had no `Abandoned` frame (attempt 1 stayed `Running`).
- **Heartbeat stops on every worker exit** (`engine/worker.rs` `process`): the
  heartbeat is cancelled and joined before the job result (including an `Err`)
  is propagated. A worker that gives up now lets the lease lapse, so the reaper
  recovers the job one lease later instead of at the TTL.
- **Pre-admission recovery is bounded by the original TTL.** The claim writes
  `expires_at = COALESCE(expires_at, now() + JOB_TTL_RUNNING)`: the first claim
  sets the deadline (T2), T6/auto-resolve already recompute it on resume (K3,
  `migration/carry-over.md` amendment "`expires_at` dihitung ulang per fase"),
  and a re-claim after a lost lease is the same `Running` phase, not a new one,
  so it keeps the deadline (owner decision, FIN-141). Expiry is checked before
  requeue, so a job that always fails before admission ends `Expired`.
- **Terminal settlement leaves no `Running` attempt.** The reaper's `Expired`
  and `Cancelled` settlements close `Running` attempts as `Abandoned` (+
  `node.status_changed` + `node.abandoned` audit) in the same transaction as
  the job transition, before the terminal event. There is no node `Cancelled`
  status in the contract, and the outcome of a dispatched query is unknown (I4).
- **Cap semantics unchanged and confirmed** (owner decision): `NODE_ATTEMPT_CAP=3`
  means attempts 1, 2, 3 — retry while the abandoned attempt is below the cap,
  never attempt 4. engine.md step (4)'s `attempt + 1 < cap` stays for the
  owner's separate `docs:` correction.
- **Proof** — seam `LOCAL_WORKER_ERROR_BEFORE_ADMISSION=<n>` (same rules as
  `LOCAL_CRASH_AFTER_EXTERNAL_CALL`: `APP_ENV=local` only, startup fails
  elsewhere or at 0): the first `n` claims of the process end in a worker error
  after T3 and before admission. Three new stages, lease 6 s / heartbeat 2 s /
  reaper 2 s: `worker-error` (n = 1000, TTL 24 s) — ≥ 2 reaper notices without
  `retry`, one re-claim per notice, no `node.status_changed`, `Expired` with
  `terminal_at − created_at` in [24 s, 30 s) and exactly one `job.expired`;
  `crash-expired` (crash n = 1, TTL 4 s < lease) — `Expired`, replay
  `[[1, "Abandoned"]]` before `job.expired`, no retry; `crash-cancelled`
  (crash n = 1) — cancel while attempt 1 is `Running`, reaper `Cancelled`,
  replay `[[1, "Abandoned"]]` before `job.cancelled`, no retry.
  `crash-recovery` and `crash-exhausted` stay green.
- L4 stays 🔨: OVR-6.2 has no test.

Update after FIN-136 (L2 lexical scoring bug):

- **Found:** `planner::best_capability`'s lexical arm ranked candidates by raw
  `overlap.matched_terms` (a bag-of-words count with `to_tsquery('simple', …)`,
  no stopword removal) before `ts_rank_cd`. Two failure shapes: (1) generic
  filler words ("the", "in", "last", "top", "month") counted the same as
  domain terms, so an unrelated capability could tie a genuine match on
  matched-term count and win the `source_id ASC` tie-break by alphabetical
  luck — e.g. "Top withdrawals per month in the last 12 months." selected
  `client_activation_top_n_offices` instead of
  `savings_withdrawal_monthly_top_n`; (2) a capability whose examples repeat a
  domain word many times (e.g. "client" across six examples) out-scored a
  sibling capability's own **verbatim** example, because `ts_rank_cd`'s
  default normalization (`0`) counts raw term frequency with no length or
  document-frequency correction — e.g. `client_lifecycle_summary`'s own
  example "Show client lifecycle summary." lost to
  `client_summary_by_office`/`organization_office_client_summary`.
- **Now (engine/planner.rs `best_capability`):** `matched_terms DESC` stays
  the primary sort key (unchanged mechanism). Two changes to the tie-break: the
  indexed tsvector weights `knowledge_index.title` (Postgres label `'A'`, full
  weight) above the rest of `retrieval_text` — description + examples,
  label `'C'` — so a capability's own name/identity counts for more than
  incidental word repetition across its example prose; and `ts_rank_cd`
  normalization `2` (divide by document length) replaces the unnormalized
  default, penalizing capabilities whose score is inflated by having more
  (or longer) examples rather than a denser match. `to_tsquery('simple', …)`
  is unchanged — no stemming/stopword dictionary was introduced, keeping the
  bilingual (ID/EN) catalog behavior the retrieval design deliberately relies
  on (`lexical_terms` doc comment).
- **Verified:** all four FIN-136-named repros now select their authoritative
  capability (`savings_withdrawal_monthly_top_n`,
  `savings_deposit_monthly_breakdown`, `client_lifecycle_summary`,
  `client_activation_top_n_offices` for the Indonesian control) — proven at
  the HTTP surface by the new `retrieval-selection` Bruno stage. A corpus-wide
  sweep of all 187 `examples:` across every capability manifest plus the 41
  phrase/capability pairs already asserted by the `answers` Bruno stage
  (`fineract-assistant-api/answers/**/*-answer.yml`) confirms **zero**
  regressions: the sweep's mismatch count dropped from 5 (pre-fix) to 1
  (post-fix).
- **Known residual (not a FIN-136-named repro, left open):**
  `savings_withdrawal_monthly_breakdown`'s own example "Show savings
  withdrawals per month for this year." still narrowly loses to
  `savings_withdrawal_monthly_top_n` (score margin ~0.075 vs ~0.069 in the
  post-fix corpus sweep). Its sibling on the deposit side is fixed by the same
  change; the withdrawal side's `_top_n` example ("Top 3 deposits per month
  this year." pattern) happens to be a slightly denser near-duplicate phrase
  of the `_breakdown` example. This is the same class of near-duplicate
  example wording between sibling ranking/breakdown capabilities noted for
  the deposit pair — a catalog-wording ambiguity, not a routing regression;
  flagged for the owner rather than chased with a phrase-specific rule (no
  allowlist/denylist, per the ticket's constraint).
- L2 stays 🧪: the §3 gate (Indonesian question finds its capability;
  `retrieval_miss` distinguishable from `out_of_scope`) was already proven
  and is untouched by this fix — this was a ranking-precision bug inside an
  already-passing mechanism, not a missing capability of L2 itself.

Update after owner decisions of 2026-09-25 (FIN-54, FIN-143, FIN-144):

- **OVR-6.2 moves from L4 to L8.** The planner only builds single-node plans
  (`engine/planner.rs`, `depends_on: []`; `engine/repository.rs` "a single-node
  plan has no fan-in"), and multi-node plans with fan-in are L8 work (§3,
  FIN-99). Keeping OVR-6.2 in the L4 gate made L4 depend on L8, which needs
  L7, which needs L4 — a cycle. The owner resolved it by moving the scenario's
  ownership: `scripts/acceptance-check.sh` maps `OVR-6.2 → L8` per ID (the
  scenario text in `architecture/overview.md` is unchanged). FIN-54 is
  re-parented under L8 and proven together with FIN-99.
- **Docs-only corrections, owner-authorised:** `architecture/engine.md` retry
  eligibility now reads `attempt < NODE_ATTEMPT_CAP` (attempt numbering starts
  at 1; cap 3 = attempts 1, 2, 3 — FIN-143, matching `operations/runtime.md`
  §1 and the FIN-141 decision); a never-admitted `Pending`/`Runnable` attempt
  on a job that becomes `Expired`/`Cancelled` is closed `Skipped` in the same
  settlement transaction (`engine.md` node matrix, `database-design.md` T11 —
  FIN-144; the code change follows in that ticket).
- L4 stays 🔨: every L4-owned OVR scenario now carries a test (6/6), but the
  answers are not yet correct for every question as asked — FIN-135 (the
  planner silently ignores user-stated dates/limits/currency/office) and
  FIN-144 are open. FIN-140 (Indonesian secret-field paraphrases and
  deferred-domain routing) is closed — see the update below.

Update after FIN-138 (flaky `retrieval-healthy` stage — mechanism gate, no
scenario ID):

- **Found:** `retrieval-vector/response.yml`, `retrieval-healthy/response.yml`
  and `retrieval-unavailable/response.yml` (all three written together by
  FIN-134) polled `GET /chat/jobs/:id/response` with a hand-rolled,
  count-bounded loop (`polls < 20`, `bru.setNextRequest`) and no sleep between
  attempts. `bru run --delay` only paces distinct requests in sequence, not a
  request re-queuing itself via `setNextRequest` — so all 20 attempts burned
  in single-digit milliseconds of wall time, far under the real latency of the
  external Voyage AI embedding call `retrieval-vector`/`retrieval-healthy`
  wait on. `retrieval-unavailable` never showed the flake only because its arm
  is disabled (`EMBEDDING_API_KEY=`) and settles before the first poll. This
  is the same class of problem `lib/poll.js`'s `awaitJob` (introduced for
  FIN-141's crash-recovery stages) already solves with a time-bounded
  (90 s), per-iteration `setTimeout`-based sleep.
- **Fix:** switched all three `response.yml` files to `lib/poll.js`'s
  `awaitJob`. `awaitJob` needs `setTimeout`, which the default `quickjs` Bruno
  sandbox does not provide (confirmed by reproducing `'setTimeout' is not
  defined'` locally) — `scripts/integration-test.sh` already runs every other
  external-call-bound stage (`answers`, `redis-down`, `crash-*`) under
  `BRU_SANDBOX=developer` for the same reason, so the `retrieval-unavailable`
  and `retrieval-healthy` (bundles `retrieval-vector` + `retrieval-healthy`)
  invocations now do too.
- **Verified:** reproduced the flake on the first attempt against
  `origin/main` (`retrieval-vector/response` failed "expected 404 to equal
  200" after 20 polls that completed in ~60 ms while the job settled 4+ s
  later). After the fix, `retrieval-vector` passed **10/10** consecutive
  targeted runs plus a full `retrieval-unavailable` + `retrieval-vector` +
  `retrieval-healthy` bundle run, all through the shared-Postgres lock
  wrapper. Classification: test-script bug (Bruno poll script), not an engine
  bug — `planner::semantic_capability`'s retrieval_miss/out_of_scope mapping
  (FIN-42) is unchanged and untouched.

Update after FIN-60/62/64 (L5.1/L5.3/L5.5, Lane C — live document conformance):

- **L5 stays 🔨.** RESP-8.1..8.6/8.8/8.10 (the seven scenarios a live path
  actually emits today) are now proven to *conform*, not just to have a unit
  test, at the HTTP surface — the gap `build-order.md` flagged after `d3a1535`
  ("has a test" is coverage, not conformance). RESP-8.7 (chart downgrade) and
  RESP-8.9 (expired dataset table) still have no live emitter — that is
  FIN-61/63/65, still out of scope — so the ceiling `d3a1535`'s update already
  named is unchanged. §5.1's count stays **36/59**: no new scenario ID was
  added, these three tickets raised existing RESP scenarios from
  unit-tested to HTTP-proven.
- **FIN-60 (§1–§2 block shape/vocabulary).** A shared assertion helper
  (`fineract-assistant-api/lib/response_shape.js`, `assertBlockShape`) checks
  every block of a served document carries `block_id` + `type` ∈ the 9-type
  vocabulary + `schema_version`, that `derived_from` on data-presenting blocks
  resolves to `evidence_json.lineage`, and that no `provenance`/`metrics`
  block is ever emitted. Wired into the four document kinds a live job
  actually produces: `table` (`engine/answered-response.yml`,
  `savings_balance_summary`), `metric` (`retrieval-vector/response.yml`,
  `savings_deposit_total` — `grouping: none` → one row → one `metric` block
  per column), `limitation` (`engine/policy-surface-response.yml`,
  OVR-6.6 `BlockedByPolicy`), and `note` (`resolver/autobind-response.yml`,
  auto-bind disclosure). All four already conformed — no compose/validate bug
  found. "A type outside the vocabulary is rejected" stays proven once, at
  `validate.rs:727` (`resp_8_10_a_type_outside_the_vocabulary_is_rejected`);
  nothing duplicates it at HTTP. **Stale doc note for the owner (not
  edited, Rule 3):** `responses.md:5-11`'s status box still lists `metrics`
  and `provenance` as emitted, contradicting its own §2 and the `d3a1535`
  fix this update's live assertions now also prove at HTTP.
- **FIN-62 (RESP-8.8, PII off).** Live assertions on the existing row-cap
  chain (`savings_client_activity`/`savings.activity_by_client`, whose
  `client_display_name` output field is `sensitivity: pii`, run with the
  runtime default PII-off — no settings mutated, `runtime.md:118`): the job's
  `scope_json.pii.enabled` is `false` (new `engine/row-cap-job-state.yml`),
  the served `table` block omits the column and declares it in
  `withheld_columns`, a `limitation` block `pii_withheld` states the
  withholding, and the retained dataset's rows (new
  `engine/row-cap-dataset-rows.yml`) never stored the column either (I5,
  fail-closed at `worker.rs:446-455`). `never_return` columns
  (`knowledge/policies/pii.yaml`) are asserted absent on both the table and
  the dataset rows.
- **FIN-64 (RESP-8.10, unknown block type skipped).** `validate.rs:722-727`
  is the right server-side proof — referenced, not duplicated. The
  client-compatibility rule (clients must skip an unknown `type` without
  failing render; mandatory information — limitation, auto-bind, withheld
  PII — never lives only in a new type) was already written into
  `api-reference.md` §5 ("Bentuk blok": *"Blok yang tidak dikenal wajib
  diabaikan, bukan menggagalkan render…"*) — no doc edit needed. FIN-60's
  vocabulary assertion (types ⊆ 9, on every live document) and FIN-62's
  `limitation`-states-withholding assertion are the live half of this
  scenario.

Update after FIN-140 (OVR-6.6 — Indonesian secret-field terms + deferred
loan/tax/accounting route):

- **Closes the two gaps FIN-139 left open (§5, "Not covered" above).**
  Reproduced first against a running instance (pre-fix): "Tampilkan kata
  sandi semua pengguna aplikasi." and "Berapa pajak yang terkumpul bulan
  ini?" opened a clarification instead of being refused; worse, "Show loan
  transactions.", "Tampilkan transaksi pinjaman bulan ini." and "Show journal
  entries for this month." were **misrouted and `Answered`** through
  `savings_activity_list` / `savings_withdrawal_top_n` — a deferred domain's
  subject silently answered with unrelated savings data.
- **Indonesian secret-field terms** — `secret_never_expose` in
  `schema/fineract/columns/sensitivity.yaml` gained a `synonyms:` list
  (`kata sandi`, `sandi`, `hash kata sandi`), the same convention
  `domains/*.yaml` concepts already use for bilingual terms. `loader.rs`
  feeds `synonyms` into `Surfaces::build` alongside `examples`, so these
  paraphrases settle `Completed` + `BlockedByPolicy` + `Unknown`,
  `surface_not_approved` — identical shape to the English case
  (`engine/policy-surface-id-*`).
- **New guard: `catalog::surface::DeferredDomains`.** A request whose
  subject is a domain with `status: deferred` (loans, tax, accounting_gl)
  now settles **`Unsupported`** (not `BlockedByPolicy` — this is not a
  policy refusal, `surface.rs` module doc) before scope, retrieval, plan and
  any source query, with reason `domain_deferred` (new row,
  `docs/contracts/api-reference.md`). It runs in `run_job` right after the
  `surface_not_approved` check.
- **Vocabulary is each deferred domain's own `concepts[].synonyms`** (EN/ID
  mixed, same field `loan.yaml`/`tax.yaml`/`accounting.yaml`/`savings.yaml`
  already declared) — `Domain` gained a `concepts: Vec<DomainConcept>` field
  in `model.rs`; no new Rust vocabulary.
- **Narrow by construction, same discipline as FIN-139:** whole-token phrase
  match, never substring; a term is dropped from the deferred vocabulary if
  it also occurs in an approved capability's own prose OR in a **non-deferred
  domain's own concept synonyms** — `"credit"` is a `loan` synonym
  (deferred) *and* a `savings` `deposit` synonym (`approved_mvp`,
  `domains/savings.yaml`), so it must never silence a legitimate savings
  question. `savings.yaml` also gained the Indonesian `"kredit"` synonym
  (same ambiguity, other language) so the guard treats both consistently.
  Proven by `ambiguous_terms_shared_with_an_approved_domain_never_trigger_the_deferred_guard`
  and the Bruno control chain `policy-deferred-savings-credit-control-*`
  ("Show savings credit transactions this month." still `Answered`).
- **Fixed in passing, same file:** `catalog/surface.rs`'s `singular()` only
  stripped a trailing `-s`, so "entries" normalized to "entrie" and never
  matched the concept synonym "entry" ("journal entry"). Extended to strip
  `-ies → -y` (a second common English plural pattern, not a phrase-specific
  rule) — this also benefits the existing `Surfaces` guard, not just the new
  one.
- Extended the existing catalog-wide unit test
  (`bruno_request_texts_are_refused_only_in_the_deferred_chain`, mirrors
  FIN-139's `..._surface_chain` test): every `request_text` in
  `fineract-assistant-api/` is checked against `DeferredDomains`; only
  `policy-deferred-*` chains (excluding the `-control-` variant, which
  proves the opposite) may be captured.
- L4 stays 🔨: OVR-6.2, 6.4 still have no test (unchanged by this ticket).
  `group_center.yaml`'s conditional-not-enabled reason is explicitly out of
  scope, left for its own ticket.

Update after FIN-142 (catalog-wording residual left open by FIN-136) and
FIN-145 (retrieval-selection poll flake, same class as FIN-138):

- **FIN-142 found:** the FIN-136 "known residual" above —
  `savings_withdrawal_monthly_breakdown`'s own example "Show savings
  withdrawals per month for this year." still narrowly lost to
  `savings_withdrawal_monthly_top_n`, because that capability's third example
  ("Show the biggest savings withdrawal per month.") was a near-duplicate
  phrase of the breakdown example and, being the denser document, edged it out
  under `ts_rank_cd` normalization `2`. Testing this with a real corpus
  (below) also surfaced an unrelated asymmetry: `savings_withdrawal_monthly_top_n`
  had **no** Indonesian example, while its deposit sibling
  (`savings_deposit_monthly_top_n`) did — so a terse Indonesian ranking
  phrase for withdrawals ("Penarikan terbesar setiap bulan tahun ini.") lost
  to the deposit capability on shared generic vocabulary.
- **FIN-142 fix (`knowledge/capabilities/savings/withdrawal_monthly_top_n.yaml`):**
  reworded the near-duplicate example to state the ranking intent explicitly
  ("Rank the biggest savings withdrawal per month, highest first.") and added
  an Indonesian example mirroring the deposit sibling's
  ("Penarikan terbesar setiap bulan tahun ini."). No scorer change, no
  allowlist/denylist — wording only, per the ticket's constraint.
- **FIN-142 corpus tool (`cargo run -p app -- retrieval-sweep`,
  `crates/app/src/retrieval_sweep_command.rs`):** a real-retrieval-path corpus
  check (the FIN-136 sweep had been an ad-hoc script, never checked in). It
  drives `planner::lexical_candidate` (the lexical arm only — the vector arm
  needs a live `EMBEDDING_API_KEY` and the residual was purely a lexical-arm
  phenomenon) against every manifest's own `examples:`, every phrase/capability
  pair already asserted by `answers/**/*-job.yml` and
  `retrieval-selection/*-response.yml`, plus a hand-written held-out set
  (`crates/app/fixtures/fin142_held_out.json`, 12 EN/ID phrasings of monthly
  breakdown vs. monthly top-N withdrawal/deposit questions, written before the
  wording edit). Baseline: 234/235 corpus (the one known FIN-142 mismatch),
  8/12 held-out. After the fix: **238/238 corpus (zero mismatches)**, 11/12
  held-out — no regression anywhere else in the catalog. The one remaining
  held-out miss ("Peringkat penarikan tabungan terbesar tiap bulan." →
  `savings_withdrawal_total` instead of `savings_withdrawal_monthly_top_n`) is
  a pre-existing three-way lexical ambiguity between `_total`/`_top_n`/
  `_breakdown` for a terse phrase carrying no explicit "top N" cue — outside
  the breakdown/top-n pair FIN-142 targets, and not a regression (the phrase
  already lost, to a different wrong capability, before this change). Flagged
  for the owner rather than chased with scorer tuning.
- **FIN-142 proof at the HTTP surface:** two new `retrieval-selection` Bruno
  cases (EN + the manifest's own ID example), test names `FIN-142: …`.
- **FIN-145 found:** `retrieval-selection/*-response.yml` (four files from
  FIN-136, plus the two FIN-142 added above) used the same count-bounded,
  zero-sleep `bru.setNextRequest` poll FIN-138 fixed elsewhere — all 20 poll
  attempts complete in single-digit milliseconds since `bru run --delay` does
  not pace a request re-queuing itself.
- **FIN-145 fix:** switched all six `response.yml` files to `lib/poll.js`'s
  `awaitJob` (unchanged from FIN-138's pattern; every FIN-136 assertion text
  is untouched). `scripts/integration-test.sh` splits `retrieval-selection`
  out of the `ENGINE_FOLDERS` bundle into its own `RETRIEVAL_SELECTION_FOLDERS`
  stage, run under `BRU_SANDBOX=developer` (the default `quickjs` sandbox has
  no `setTimeout`), same as `retrieval-unavailable`/`retrieval-healthy`;
  `engine`/`clarification`/`resolver`/`sse` stay on the default sandbox since
  they don't `require` `lib/poll.js`.
- **Verified:** `retrieval-selection` passed **5/5** consecutive locked runs
  (19/19 tests each) after the fix, plus a full locked Bruno suite. `cargo
  test`, `cargo clippy -D warnings`, `docs-check.sh` and `acceptance-check.sh`
  all green.

Update after FIN-137 (savings_account_identity_lookup / savings_account_terms_lookup):

- **Found:** both capabilities declared `account_number` as a plain
  `type: string` parameter, but the query manifests they run
  (`knowledge/queries/savings/account_identity_lookup.yaml`,
  `account_terms_lookup.yaml`) marked it `source: transient_sensitive_input`.
  K1 (`planner.rs Missing::unanswerable`) forbids binding an identity slot from
  free text, and neither capability declared a `probe:` for it, so the slot
  could never be asked at all — every request landed on `Unsupported` /
  `identity_slot_without_resolver`. This mirrors
  `savings_charge_type_identity_resolve`'s pre-FIN-34 state: an approved probe
  shape existed nowhere for this parameter (`knowledge/parameter-bindings/
  bindings.yaml:41-43` already anticipated a resolved answer needing "a
  declared home", but nothing produced one).
- **Fix:** added `savings_account_identity_resolve` (`kind: resolver`,
  `continuation: true`, same pattern as `client_identity_resolve` /
  `savings_charge_type_identity_resolve`), a new `identity_candidates` shape
  on the `savings.accounts` dataset, and its wrapping query manifest
  `savings.account_identity_candidates`. Both capabilities now bind
  `savings_account_id` (integer) — resolved through the probe — instead of
  `account_number` (string); their SQL filters `sa.id = $2::bigint` instead of
  `sa.account_no = $2::text`. The probe never projects the raw account number
  (`sensitive_business_identifier`, `columns/sensitivity.yaml`) — only
  `masked_account_number`, `savings_account_id`, and non-identity business
  fields (office, product name).
- **Also wired:** `charge_name` on `savings_charge_count_by_type` and
  `savings_charges_by_type` now declares the same `probe:` as
  `savings_charge_type_identity_resolve` (`output_slot: charge_name`) — the
  probe's underlying query and both capabilities' queries filter the same
  column (`m_charge.name`/`ch.name`, case-insensitive), so the resolved value
  binds exactly what typed free text used to. This was checked, not assumed
  (Rule 4): the probe's `charge_type_candidates` shape already outputs
  `charge_name`, so the identity/binding columns line up without inventing a
  new shape.
- **Verified** (`knowledge/VERIFICATION.md`): the new probe returns 205 rows /
  205 distinct `savings_account_id` in the full authorized scope (no fanout);
  `right(account_no, 4)` is unique for the two adjacent accounts used as
  fixtures (`'0001'`/`'0002'`), so `q=0001`/`q=0002` on
  `/clarification/options` narrows to exactly one candidate without widening
  scope — proven by the `account-identity-lookup-options.yml` /
  `account-terms-lookup-options.yml` Bruno stages, and the full account/terms
  answer chains (`account-identity-lookup-answer.yml`,
  `account-terms-lookup-answer.yml`) now compare the resolved answer against
  direct SQL via `answers.check()` instead of asserting `Unsupported`.
  `charge_count_by_type`/`charges_by_type`'s existing direct-SQL truth files
  are unchanged (`charge_name: "Withdrawal fee"` — only the binding mechanism
  changed, not the expected numbers). `cargo run -p app -- catalog` → 0 error.
- **Retired:** `resolver/noresolver-{session,job,response}.yml` — its scenario
  ("Which office and product belong to savings account?" hits an identity slot
  with no resolver) used `savings_account_identity_lookup`'s own
  `account_number` as its example. That was the last `transient_sensitive_input`
  query parameter without a `probe:` in the whole approved catalog, so fixing
  it here left the Bruno scenario with no fixture to reproduce. The mechanism
  it proved (`Missing::unanswerable`, `Unplannable::reason`/`explain`) is
  unchanged pure logic — now proven by a new unit test
  (`planner::tests::identity_slot_without_resolver_is_unanswerable_not_asked_as_text`)
  instead. `crates/chat/src/catalog/validate.rs`'s `identity_slot_has_resolver`
  warning still fires at catalog-load time if a future query re-introduces the
  situation.
- **Not reproduced with fresh fixture data:** K5 `resolver_unique` auto-bind
  for a savings account identity slot — no office in this dataset has exactly
  one savings account (minimum is 6, office 8), which is what would trigger
  it. The mechanism itself is shared, generic code
  (`crates/chat/src/engine/worker.rs`) already proven for `client_id` by
  `resolver/autobind-*.yml`; this ticket did not duplicate that proof with
  savings-specific fixtures.
- L1 stays 🧪: this closes two of the nine originally-failing/incorrect
  identity-slot capabilities' catalog shape, not a new acceptance scenario for
  L1 itself. The options search (`q` narrowing) proven here is **not** CLR-7:
  CLR-7 requires that suggestions never auto-submit and that a no-match can be
  refined through the `refine_search` answer kind, which is still unbuilt
  (`contracts/clarifications.md:11`, `:108`) — CLR-7 stays untested (FIN-79,
  L7). The test names that first claimed it were corrected in a follow-up.

Update after FIN-147 (L4 bug — second answer in one session failed at T7):

- **Symptom:** `engine/policy-narrow-events` went red on `main` once FIN-140's
  control chain answered in the same `policySession` first. Any session's
  second answered job that promotes an `ActiveScope` (or a `ResolvedEntity`
  with the same key) failed: T7 rolled back → worker error → lease reaped →
  `completed_node_not_rerun` → `Failed`. Other chains used a fresh session per
  answer, so nothing caught it.
- **Root cause:** `engine::repository::promote` superseded the old valid row
  with `superseded_by_id = <new id>` **before** inserting that id;
  `session_memory_superseded_by_id_fkey` is not deferrable, so it failed
  immediately. Inserting first is rejected by the partial unique indexes
  (`session_memory_one_active_scope`, `session_memory_entity_uniq`).
- **Fix (no schema change, same T7 transaction):** supersede the old valid
  row(s) without the pointer (`RETURNING id`; `invalidation_complete` is
  satisfied by `invalidated_at` + `superseded_by_newer`) → `INSERT` the new
  fact → set `superseded_by_id` on exactly those rows. History is preserved
  and linked as `memory-context.md` §3 requires; the new fact's `session_seq`
  is still allocated under the `chat_sessions` row lock (I3), so no stale
  promotion can overtake a newer valid fact.
- **Regression proof:** Bruno chain `engine/supersede-*` (own session:
  `office_ids: []`, then `[1]`; the second job must be `Completed` +
  `Answered`). The pre-fix statement order was reproduced as an FK violation
  in a rolled-back SQL transaction against the shared DB.
- **Not built (L7, MEM-7.6 / FIN-86):** invalidating `PriorResult` rows as
  `scope_changed` when the scope changes — `promote` still skips
  `PriorResult` entirely. This ticket moves no scenario ID in §5.1.
Update after FIN-146 (flaky `.../response` reads that can race job settlement
— mechanism gate, no scenario ID):

- **Found:** unlike FIN-138's `retrieval-*` stages, most `GET
  /chat/jobs/:id/response` requests across the collection are preceded by a
  request that already single-checks `job.lifecycle === "Completed"` (or an
  `awaitJob`/`awaitResponse` poll) — safe transitively, since that check
  fails first if the job isn't terminal yet. A minority read the response
  immediately after job creation with **no wait of any kind**: the five
  `engine/indonesian-*-response.yml` + `engine/default-limit-response.yml`
  chains (no `*-job-state.yml` between question and response — the ticket's
  named repro, `engine/indonesian-client-balance-response.yml`, races the
  worker for real), `resolver/bound-response.yml` (reads right after the
  202-Queued `answer-option.yml`), `resolver/nocandidate-response.yml` /
  `noresolver-response.yml` (read right after `POST /chat/jobs`), and
  `dataset-capped/response.yml` (same). `retrieval-selection/*-response.yml`,
  the sixth file of this same no-wait shape, was independently fixed by
  FIN-145 (landed on `main` mid-ticket, above) — not touched again here. Full
  audit table of every `.../response` reader in the collection is in the
  ticket's worker report (`/tmp/jarvis-reports/FIN-146.md`).
- **Fix:** same mechanism as FIN-138/FIN-145 — switched eight no-wait files
  to `lib/poll.js`'s `awaitJob` (`resolver/noresolver-response.yml`'s own
  fix was dropped on rebase: FIN-137, landed mid-ticket, retired the whole
  `resolver/noresolver-{session,job,response}.yml` scenario — `account_number`
  now has a resolver, so its "identity slot without resolver" fixture no
  longer exists), and added `BRU_SANDBOX=developer` to the `ENGINE_FOLDERS`
  (`engine clarification resolver sse`, `retrieval-selection` already split
  out by FIN-145) and `dataset-capped` stage invocations in
  `scripts/integration-test.sh` (`awaitJob` needs `setTimeout`, absent from
  the default `quickjs` sandbox). Everything already covered by a preceding
  terminal-state check, or already polling, was left untouched — this
  ticket owns only the previously-unguarded reads, not a rewrite of the
  chain.
- **Out of scope, left as follow-ups (owned by other tickets or the
  coordinator):** `engine/row-cap-response.yml` (not preceded by a
  terminal-state check — same no-wait shape as this ticket's fixes, but
  `row-cap-*` is owned elsewhere) and the six `engine/policy-*-response.yml`
  files (each preceded by a single-check `*-state.yml`, safe transitively —
  lower priority). Not touched here per this ticket's ownership boundary.
- **New bug found while verifying the fix, left unfixed (out of scope —
  app code, not a Bruno poll script):** `dataset-capped/response.yml` still
  fails intermittently **after** the connection-race fix (job reliably
  settles and returns 200/`Complete`/`validation_status: passed`), but the
  `dataset_truncated` `limitation` block it/DS-8.1 requires is sometimes
  absent even though `LOCAL_DATASET_MAX_ROWS=1` and the served table always
  has more than one row (truncation should be deterministic). Reproduced in
  2 of 4 isolated post-fix runs — a genuine, separate race in
  `compose.rs`/`worker.rs::retain_dataset`, not caused by and not curable
  from the test side. Flagged for the coordinator to file as its own ticket
  (same pattern as FIN-147 for `engine/policy-narrow-events`, above — that
  bug was already found and fixed independently before this ticket's own
  full-suite run landed).
- **Verified:** reproducing the flake directly proved unnecessary — the
  ticket's own repro (a FIN-60 run) already showed it once on `origin/main`.
  Post-fix, `engine` + `resolver` passed **5/5 consecutive** targeted locked
  runs (109/109 requests, 161/161 tests each), plus a full locked run, all
  green. `dataset-capped`'s connection-race is fixed (0/5 runs since the fix
  failed with a 404/connect error), but the stage as a whole is not 5/5
  clean because of the unrelated content-composition bug above (2/4
  isolated runs failed *that* assertion, not the connection). `cargo test`,
  `cargo clippy -D warnings`, `docs-check.sh` and `acceptance-check.sh` all
  green (coverage unchanged at 36/59 — mechanism fix, no new scenario ID,
  same class as FIN-138/FIN-145).

Update after FIN-144 (OVR-6.4 never-admitted attempts on terminal jobs):

- **Bug:** every terminal settlement to `Expired`/`Cancelled` closed only
  `Running` attempts (`Abandoned`, FIN-141). An attempt still
  `Pending`/`Runnable` — e.g. the worker errored after T3 and before
  admission — stayed `Runnable` on a terminal job forever, which
  `architecture/engine.md:155` and `data/database-design.md:309` (T11) forbid.
- **Now (`engine/repository.rs` `skip_unadmitted_attempts`):** all three
  paths that make a job `Expired`/`Cancelled` — reaper expiry
  (`settle_expired`), reaper `Cancelling` → `Cancelled` without a live lease
  (`settle_abandoned_cancelling`) and the worker-driven cancel commit
  (`settle_cancelled`, T9, fenced by `lease_token`) — close every
  `Pending`/`Runnable` attempt as `Skipped` in the same transaction as the
  job update, emitting `node.status_changed` `{"status":"Skipped"}` and a
  `node.skipped` audit row right before `job.expired`/`job.cancelled`.
  Terminal attempt rows are never touched; a second reaper pass finds no
  selectable job, so it changes nothing. Retry semantics are unchanged.
- **Proven (Bruno, `worker-error` stage):** the Expired path by
  `worker-error/events.yml` (attempt 1 → `Skipped` directly before
  `job.expired`, replacing the FIN-141 assertion "no `node.status_changed`");
  the reaper Cancelled path by the new chain
  `worker-error/cancel*.yml` (seq 6–11)
  (cancel while attempt 1 is `Runnable` → `Skipped` directly before
  `job.cancelled`). Both fail on the pre-fix code. The worker-driven cancel
  path is only reachable through a claim/cancel race, so it is covered by the
  shared helper rather than by a deterministic Bruno request.
- L4 stays 🔨: FIN-135 is still open.

### 5.1 Scenario coverage

Every acceptance scenario now carries a stable ID, added in place without
touching a word of the scenario text. `./scripts/acceptance-check.sh` collects
them from `docs/`, collects the IDs named by tests (Bruno `.yml` and Rust), and
fails when a layer marked ✅ in the table above still has a scenario without a
test.

Latest run — **36 of 59 scenarios have a test**:

| Prefix | Document | Scenarios | With a test | Owning layer |
| --- | --- | --- | --- | --- |
| `API-` | [contracts/api.md](contracts/api.md) | 6 | 6 | L0 |
| `SSE-` | [contracts/sse.md](contracts/sse.md) | 8 | 8 | L0 |
| `DS-` | [data/dataset-lifecycle.md](data/dataset-lifecycle.md) | 6 | 6 | L3 |
| `OVR-` | [architecture/overview.md](architecture/overview.md) | 7 | 6 | L4 (OVR-6.2 → L8) |
| `RESP-` | [contracts/responses.md](contracts/responses.md) | 10 | 10 | L5, L6 |
| `CLR-` | [contracts/clarifications.md](contracts/clarifications.md) | 8 | 0 | L7 |
| `MEM-` | [architecture/memory-context.md](architecture/memory-context.md) | 7 | 0 | L7 |
| `AC-` | [data/analytical-contracts.md](data/analytical-contracts.md) | 7 | 0 | L8 |
| | **Total** | **59** | **36** | |

API and SSE tests are Bruno requests (PR #1). SSE-5..8 each carry a test but
their tickets (FIN-25..28) stay In Progress — e.g. SSE-5's second clarification
stage waits on CLR-3. DS-8.1..8.4 are proven by Bruno requests (`engine/`,
`dataset-capped/`); DS-8.5 is proven at the schema by `tests/schema_smoke.sql`
T13 plus unit tests; DS-8.6 is proven at the release predicate
(`ds_8_6_*`), the only path any eviction takes
(`crates/chat/src/engine/dataset/`). RESP is now complete at the unit level, but "has a test" is a coverage
figure, not a conformance one — `acceptance-check.sh` cannot tell whether the
test actually proves its scenario, and RESP-8.7/8.9 in particular prove
composition logic that no live path emits yet.

**The count is 59, not the 86 stated in [§1](#rule-1--done-means-acceptance-scenarios-not-green-tests).**
One ID was given per scenario as the document actually writes it — one bullet or
one numbered item. The 86 in §1 appears to count the clauses inside compound
bullets (`"Radio and searchable select yield identical single-choice semantics;
checkbox false is not missing."` is one bullet, two claims). Splitting those into
separate IDs would mean rewriting scenario text, which Rule 3 forbids. §1 has
been left untouched: correcting its table is the repository owner's decision.
The discrepancy is a difference in counting, not a missing scenario — no
scenario present in the eight documents is unmapped.

L1 and L2 own no acceptance scenario of their own: their gate is the four rules
in `knowledge/CARRY-OVER.md` and §6.5, not a numbered list.

---

## 6. Deviations already found

These must be fixed before anything is built on top of them. Found by auditing
the code against the docs on 2026-09-15.

### 6.1 T8 skip does not promote memory — contract violation

[memory-context.md](architecture/memory-context.md) §3: *"the only promotion point
is the T7 response commit, **including T8 skip**"*; §7-3 requires "atomic memory
promotion"; [migration/carry-over.md](migration/carry-over.md) K2: *"context is
saved to memory at that commit"*; K3 distinguishes skip (promotes) from cancel
(does not).

`crates/chat/src/clarification/repository.rs` instead states *"there is no memory
promotion here, and that is a decision"* — a unilateral decision against the
contract.

### 6.2 Block shape deviates from §1 and §2 — RESOLVED in `d3a1535`

Fixed: `block_id`/`schema_version`/`derived_from` present, vocabulary limited to
the 9 types, auto-bind in a `note` block, lineage in `evidence_json`. The table
below records what was wrong before the fix.

| Contract | Reality |
| --- | --- |
| `block_id` required | `id` is used instead |
| `schema_version` per block required | missing |
| `derived_from` required on data blocks | missing |
| Vocabulary of 9 types (§2) | emits `provenance` (95 blocks) and `metrics` (65 blocks); **neither is in the vocabulary**. `metric` should be singular |
| Auto-bind disclosed in a `note` block (§5) | disclosed in a `limitation` block; `note` is **never used** |
| Lineage in `evidence_json` (#10) | `evidence_json` is always `{}`; lineage was invented as a `provenance` block |

### 6.3 The validator enforces the implementation, not the contract — RESOLVED in `d3a1535`

Fixed: D1 is now computed per block through `derived_from` and aggregated; D2
inspects the `note` block; §1 and §2 are enforced (a type outside the vocabulary
is rejected). The list below records what was wrong before the fix.

- D1 was computed at document level rather than per block, because `derived_from`
  did not exist. This approximation was never declared.
- D2 scanned any block carrying `auto_bound_slots` instead of the `note` block, so
  it **passed against the wrong shape**.
- §1 and §2 were not enforced at all.

### 6.4 `Cancelled` / `Expired` are recorded as `OperationalFailure`

32 terminal jobs carry `outcome='OperationalFailure'` although they were
cancelled or expired. K3 states that cancel is an abort, not an operational
failure. A dashboard colouring by `outcome` will show 32 false errors.

### 6.5 `Unsupported` conflates four different causes

| `completeness_reason` | Count | What it means |
| --- | --- | --- |
| `no_capability_matched` | 37 | **mixed** — some genuinely out of scope, some are our retrieval failing |
| `identity_slot_without_resolver` | 11 | catalog incomplete |
| `planner_not_implemented` | 6 | feature missing |
| `parameter_binding_unsupported` | 2 | feature missing |

"Out of scope" and "we failed to find a capability we actually have" look
identical in the data and in the UI. They must be separated.

### 6.6 DS-8.2 / DS-8.4 could not be proven at the HTTP surface — RESOLVED

DS-8.2 (a `purged` dataset still reads as purged) and DS-8.4 (a re-read by
another user / a narrower scope is refused) are acceptance scenarios about the
**HTTP surface**, and per the repo's own thesis their unit tests are not proof.
Adding Bruno tests for them is blocked by three mechanisms that do not exist and
must **not** be invented (Rules 3 and 4):

1. ~~**No `dataset_id` is discoverable over HTTP.**~~ **Resolved (FIN-43).**
   `evidence_json.lineage[].dataset_id` now carries the handle that retained the
   node's result; `engine/answered-dataset.yml` and `answered-dataset-rows.yml`
   learn the id from a job response and read the handle and its rows through
   `GET /chat/datasets/{id}`.
2. ~~**DS-8.2 needs a `purged` handle.**~~ **Resolved (FIN-44, FIN-47).** The
   owner-approved seam `POST /_local/datasets/{id}/purge` is assembled only when
   `APP_ENV=local` (absent from the router elsewhere) and runs the reaper's own
   purge rule for one handle without waiting for the ≥24h TTL. Production TTL
   (`DATASET_TTL_SECS = 86400` floor) is unchanged. `engine/dataset-purged*.yml`
   prove DS-8.2.
3. ~~**DS-8.4 needs a second principal and a narrower scope.**~~ **Resolved
   (FIN-45, FIN-49).** The local-only `auditor` principal (PR #1) reaches the
   `NotOwner` (404) branch; the owner-approved `office_ids` query on
   `GET /chat/datasets/{id}[/rows]` narrows (never widens) the caller's
   authorization and reaches `ScopeNarrowed` (403). Both are proven by the
   `engine/dataset-other-user*.yml` and `engine/dataset-narrowed-scope*.yml`
   Bruno requests.

All three mechanisms now exist; DS-8.1 (FIN-46), DS-8.2, DS-8.3 and DS-8.4 are
proven at the HTTP surface, DS-8.5 (FIN-50) at the schema and DS-8.6 (FIN-51)
at the release predicate every eviction path uses. L3 is 🧪 (see §5).

---

## 7. Checks that must be green

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p app -- catalog
cargo run -p app -- retrieval-sweep   # if knowledge/ examples changed (FIN-142)
psql -v ON_ERROR_STOP=1 -d "$APP_DATABASE_URL" -f tests/schema_smoke.sql   # if migrations changed
./scripts/docs-check.sh
./scripts/acceptance-check.sh
PORT=3107 ./scripts/integration-test.sh
```

All of them green does **not** mean the code conforms to the docs. It is only the
minimum bar before the conformance question may be asked at all.

Environment notes: port 3007 and Redis 6380 belong to the old repository on this
development machine — run with `APP_PORT=3107`. `WORKER_ENABLED` defaults to
`true`, so a leftover instance on *any* port will claim intake-stage jobs and
break the "job stays `Queued`" assertions; stop it rather than switching ports.

---

## 8. Running locally

```bash
cd fineract-ai-backend
docker compose up -d
sqlx migrate run --database-url "$APP_DATABASE_URL"
APP_PORT=3107 cargo run -p app
```

Frontend: start from [contracts/api-reference.md](contracts/api-reference.md) —
the surface that genuinely exists, with payloads copied from the running
application. Remember that "exists" does not mean "conforms": read §6.2 before
locking any render shape to it.
