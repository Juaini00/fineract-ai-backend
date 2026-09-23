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
direct SQL on Fineract and matches.
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

Additive LLM narration, multi-node plans with fan-in, analytical contracts
(Mode 2), final security model, observability, OpenAPI.
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
| L3 Datasets | 🔨 | L0 🔨 |
| L4 Correct answers | 🔨 | L1 🧪, L3 🔨 |
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

### 5.1 Scenario coverage

Every acceptance scenario now carries a stable ID, added in place without
touching a word of the scenario text. `./scripts/acceptance-check.sh` collects
them from `docs/`, collects the IDs named by tests (Bruno `.yml` and Rust), and
fails when a layer marked ✅ in the table above still has a scenario without a
test.

Latest run — **30 of 59 scenarios have a test**:

| Prefix | Document | Scenarios | With a test | Owning layer |
| --- | --- | --- | --- | --- |
| `API-` | [contracts/api.md](contracts/api.md) | 6 | 6 | L0 |
| `SSE-` | [contracts/sse.md](contracts/sse.md) | 8 | 8 | L0 |
| `DS-` | [data/dataset-lifecycle.md](data/dataset-lifecycle.md) | 6 | 6 | L3 |
| `OVR-` | [architecture/overview.md](architecture/overview.md) | 7 | 0 | L4 |
| `RESP-` | [contracts/responses.md](contracts/responses.md) | 10 | 10 | L5, L6 |
| `CLR-` | [contracts/clarifications.md](contracts/clarifications.md) | 8 | 0 | L7 |
| `MEM-` | [architecture/memory-context.md](architecture/memory-context.md) | 7 | 0 | L7 |
| `AC-` | [data/analytical-contracts.md](data/analytical-contracts.md) | 7 | 0 | L8 |
| | **Total** | **59** | **30** | |

API and SSE tests are Bruno requests (PR #1). SSE-5..8 each carry a test but
their tickets (FIN-25..28) stay In Progress — e.g. SSE-5's second clarification
stage waits on CLR-3. The DS tests are unit-level
(`crates/chat/src/engine/dataset/`); DS-8.2 and DS-8.4 still lack a full Bruno
proof (§6.6). RESP is now complete at the unit level, but "has a test" is a coverage
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

### 6.6 DS-8.2 / DS-8.4 cannot be proven at the HTTP surface yet — blocked

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
2. **DS-8.2 needs a `purged` handle.** Purge happens only via the reaper (T11) on
   a ≥24h TTL (`dataset::ttl_secs`, floor `DATASET_TTL_SECS = 86400`, not
   env-configurable); there is no manual purge/expire endpoint. A test run cannot
   produce a purged dataset.
3. ~~**DS-8.4 needs a second principal and a narrower scope.**~~ **Resolved
   (FIN-45, FIN-49).** The local-only `auditor` principal (PR #1) reaches the
   `NotOwner` (404) branch; the owner-approved `office_ids` query on
   `GET /chat/datasets/{id}[/rows]` narrows (never widens) the caller's
   authorization and reaches `ScopeNarrowed` (403). Both are proven by the
   `engine/dataset-other-user*.yml` and `engine/dataset-narrowed-scope*.yml`
   Bruno requests.

Remaining: a test-only seam for purge (#2, FIN-44). Until it lands, DS-8.2 is
proven at the unit level only and L3 stays 🔨.

---

## 7. Checks that must be green

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p app -- catalog
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
