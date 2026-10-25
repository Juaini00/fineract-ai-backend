# Build order and passing gates

**Status: working contract. Binding for humans and agents alike.**
Replaces `status.md`, deleted 2026-09-15.

> Written in English on purpose. Most other documents in `docs/` are in
> Indonesian; this one is read first by every agent, so it uses the language
> agents handle most reliably. When you edit the Indonesian documents, keep them
> Indonesian.

---

## Correction 2026-10-12 — the LLM is the planner (owner decision)

The docs assign planning to the LLM ([README](README.md) "LLMs plan within
contracts", [PRD §4](product/prd.md#4-one-engine) "The LLM selects within
structured contracts", [overview](architecture/overview.md) step 5 "LLM
planning"). The planner was nevertheless built as lexical + vector matching
(`engine/planner.rs`), because no document defined the planner's input, output
or the role of retrieval (Rule 4 gap, filled by invention). The ladder then
turned that stand-in into a gate (old L2 "retrieval finds the capability"), and
the work that followed — lexical scoring, per-phrase catalog wording, regex
parameter parsing — was manual mapping of Indonesian phrasings, which has no end
and produced false `Unsupported` answers (§6.5).

Binding from now on:

1. **The LLM is the brain, not a layer.** For every request it understands the
   question, selects an approved capability, binds slot values and proposes the
   plan. The Engine verifies every output (catalog version, authorization, scope,
   budget, static checks) before anything executes, owns execution, and never lets
   the model write SQL or decide authorization.
2. **Retrieval is a candidate shortlist for the model, not the decider.**
3. **No LLM = no planning.** An unavailable model ends the job with an explicit
   outcome. There is no silent fallback to string matching.
4. **Unchanged:** K5 (clarification is triggered by resolver cardinality after a
   bounded probe, never by the model's guess; [carry-over K5](migration/carry-over.md));
   date phrases are resolved by the declared calendar contract; capabilities stay
   approved-only; narration stays additive and fallback never calls the model
   (RESP-8.6).
5. **Resolved — owner decision, Linear FIN-162 (2026-10-12):** the
   [planner contract](architecture/engine.md#llm-planner-contract-fin-162)
   uses a compact typed graph with grounded slots; the Engine expands/verifies
   execution details. Planning failures retain `Failed`/`OperationalFailure`
   with distinct reason codes, never become unsupported scope from a retrieval
   miss, and never invoke a deterministic selection fallback. The current local
   development model/provider remains selected in
   [tech-stack](architecture/tech-stack.md#development-provider-and-structured-output).
   This is a contract decision, not proof that the LLM planner is built.

Consequences recorded elsewhere in this file: L2 is redefined as the LLM planner
(epic FIN-161); every earlier L2 proof (FIN-40/41/42, tuning FIN-136/142/148/151,
re-proof FIN-154) measured the stand-in and **does not count**; the L4 🧪 claim
is scoped to the deterministic planner and is re-run in FIN-155 behind FIN-161.
Old-premise tickets were cancelled in Linear on this date.

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

**L1C** (catalog coverage, added 2026-09-26) sits between L1 and L2: L1 asks
whether the catalog's entries are correct, L1C asks whether the catalog covers
the agreed scope. Every layer that consumes the catalog (L2, L4, L7) depends on
both. `acceptance-check.sh` reads only `L<digit>` rows, so the L1C row is
ignored by it; L1C owns no numbered scenario (its gate is in §3).

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

### L1C — Catalog coverage (Linear epic FIN-107)

**Goal**: the catalog covers the **full-release data scope**, not only the 48
(now 49) capabilities carried over from `ai_report`. L1 proves those entries are
*correct*; it never measured whether they are *enough*.
**Owning docs**: [product/2026-09-09-dataset-scope-decisions.md](product/2026-09-09-dataset-scope-decisions.md)
§1 + D01–D15 (§5 steps 2–4) · [product/prd.md](product/prd.md) §2 ·
[data/analytical-contracts.md](data/analytical-contracts.md) (Mode 1 / Mode 2)
**Prerequisites**: L1 (mechanically correct catalog)
**Done when**, for every domain in the scope document except the two deferred
to deployment onboarding (D10 custom datatables FIN-119, D15 surveys/credit
bureau FIN-125):

1. The formal inventory (`docs/data/dataset-inventory.md`, FIN-152) has a row
   for every §1 line and every D01–D15 decision: requirement → source →
   grain → field/measure → relations/cardinality → office-scope path →
   time/as-of/currency → evidence rule → acceptance → capability status.
2. `knowledge/data-scope/` and `knowledge/domains/` agree with the scope
   documents (no MVP vocabulary, no contradiction with D09/D12/§1), and a
   documented domain without a contract answers `Unsupported` instead of
   being misrouted to a savings capability (FIN-153).
3. Each domain's capabilities (Mode 1 now; Mode 2 waits on FIN-98 in L8) pass
   the four `knowledge/CARRY-OVER.md` rules, recorded two-numbers-side-by-side
   in `knowledge/VERIFICATION.md`.

**Status**: ⬜ — the formal inventory now exists
([data/dataset-inventory.md](data/dataset-inventory.md), FIN-152, merged
2026-09-27: 99 rows, every §1 domain line and D01–D15 covered, zero
omissions), but domain capabilities remain unchanged and incomplete: 4 of
~20 domains have any capability (client, savings, organization, group); the
catalog reads 13 Fineract tables out of ~220 (D15 audit; the inventory's own
local read found 300 public tables and flags the 220↔300 gap for the owner,
unresolved). Loan, products, FD/RD (except one maturity capability), share,
GL traceability, tax, teller, standing instructions, provisioning, source
audit have **none**; the scheduler now has two (FIN-124, D14 — see "Update after FIN-124"). FIN-153 (`knowledge/` alignment against
the inventory, point 2 of "Done when" above) is merged: `knowledge/domains/`
and `knowledge/data-scope/areas/` now agree with the scope documents (six
contradictions fixed, `inventory_refs` mechanically checked — see the
"Update after FIN-153" note below), and every gap domain answers `Unsupported`
before retrieval, never misrouted to an unrelated approved capability. Point
3 (each domain's Mode-1 capabilities passing the four `CARRY-OVER.md` rules)
is still open — that is FIN-109 onward — so L1C itself does not move above ⬜.
**Order inside L1C** is the `blocks` relations on FIN-107's children: FIN-152
inventory → FIN-153 knowledge alignment → FIN-109 products → loan / FD-RD /
share / savings → client, linking resources, GL, provisioning, audit →
cross-domain (completeness, history, branch attribution, currency) → FIN-155
answers re-proof, which also waits on the LLM planner (FIN-161).

### L2 — The LLM planner

**Goal**: the LLM understands the user's question, selects the approved
capability that exists, binds its slots and proposes the plan; the Engine
verifies it. Linear epic FIN-161.
**Owning docs**: [architecture/engine.md](architecture/engine.md) ·
[architecture/overview.md](architecture/overview.md) step 5 ·
[architecture/tech-stack.md](architecture/tech-stack.md) ·
[planner contract and acceptance scenarios](architecture/engine.md#llm-planner-contract-fin-162)
(owner-approved FIN-162).
**Prerequisites**: L1 for the existing capabilities. Breadth over the full
catalog arrives with L1C; the planner is proven on existing capabilities first
and re-proven over the full catalog in FIN-155.
**Done when**: `PLN-1`–`PLN-9` pass at the surfaces named in
[engine.md](architecture/engine.md#planner-acceptance-scenarios): a real LLM
selects/binds an existing capability through Engine verification; retrieval
does not decide; static checks prevent invalid execution; unavailable,
invalid-output, abstention and no-fit states differ from unsupported scope;
K5, audit, budgets and commit isolation hold. Multi-node scheduling remains
the L8 `OVR-6.2` gate.

**Status**: 🔨 — Rig generation is wired on the plan path with bounded,
audited calls and strict proposal rejection. FIN-163 now has a targeted real
planner round-trip and provider-outage proof. This proves one supported
request and the unavailable-provider path, not complete L2 conformance.

FIN-163 provider correction (working branch, not published or closed):
`qwen3.8-flash` returns its hidden reasoning in a separate `reasoning_content`
field beside `content` with `finish_reason=stop`; Rig maps it to a
`Reasoning` part, so the `complete && only_text` gate in
`crates/core/src/generation.rs` rejected every valid answer as
`provider_output_limits` (the earlier 12/12 "empty" and 4/4 FIN-159 failures).
Reasoning parts are now ignored; the answer must still be exactly one `Text`
part with `finish_reason=stop` and within the token limits. No validation,
schema, fence or fallback change. Proof (locked runner, port 3363, real
`qwen-cloud`/`qwen3.8-flash`, Indonesian request "Tampilkan transaksi pinjaman
12 bulan terakhir."): job `62aa9bcd-c64d-4679-8358-bf13c662331e`, one planner
call, `ok`, 288 output tokens, `loan_transactions` with the oracle's parameters,
100 rows, AED 71 / USD 29 = independent SQL row-for-row; 8/8 Bruno tests.
This proves one supported request, not complete L2 / PLN conformance.

Second exercised path (current proof, same worktree, real provider, locked runner
port 3363, Indonesian request "Tampilkan biaya dan denda pinjaman di kantor yang
saya akses."): job `c0f93842-c8fb-4d20-80d5-7e13aebfd18e`, one planner call
(`planner.call_finished` ok), `loan_charges` plan persisted, one source node,
settled `Answered`/`Complete`; 22 requests (4 logical + polls), 8/8 Bruno tests:
parameters and office scope = SQL oracle, 100 rows AED 46 / EUR 6 / USD 48
row-for-row with independent SQL, per-row source currency, charged reconciles.
Negative controls re-run after the reasoning-part change (the fixture now sends
`reasoning_content` beside the fenced content): `planner-invalid` job
`0e4378ea-2c58-4b07-903a-1c57593f6874` = `planner_output_invalid`, 2 calls, no
plan, no source query; `planner-unavailable` job
`c593fd94-6e99-4838-ac9e-13fc067ebd55` = `planner_unavailable`, 2 calls, no plan,
no source query (8/8 tests each). Still unexercised: the remaining PLN scenarios
(see the L2 inventory); this is not a full L2 claim.

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
**Prerequisites**: L1, L1C, L3
**Done when**: for every capability in use, the job's result is compared against
direct SQL on Fineract and matches, and every L4-owned OVR scenario (OVR-6.1,
OVR-6.3…6.7) carries a test naming its ID. OVR-6.2 (multi-node fan-in) is owned
by L8 — owner decision 2026-09-25, see §5.1.
**Status**: 🧪 — since 2026-09-25: the answer-correctness gate (FIN-52 `answers`
stage, every reachable capability vs direct SQL) and every L4-owned OVR scenario
(6.1, 6.3–6.7) pass at the HTTP surface on `main`; see the §5 update of that date.
Ceiling is 🧪 because L1 and L3 are 🧪, not ✅. **Scope of that claim**: "every
reachable capability" means the 49 carried-over capabilities in four domains;
it says nothing about questions the catalog cannot yet answer (L1C ⬜). The
gate is re-run over the full catalog in FIN-155.

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
**Prerequisites**: L1, L1C, L2, L3, L4, L5, L6
**Done when**: all 22 scenarios pass, including promotion on T8 skip (§7-3),
non-optional `handle_state` (§5, C13), budgeted context selection (§5), and
incremental summaries with a watermark (§4).
**Status**: ❌ / 🔨 — the memory write path is correct and proven, but T8 skip
deviates (§6.1), nothing reads memory back, the summary is never computed, and
`dataset_id` is always NULL because L3 does not exist.

### L8 — Everything above

LLM narration (additive, evidence-grounded; the planner itself is L2, not L8),
multi-node plans with fan-in (including scenario OVR-6.2, moved here from L4 —
§5.1), analytical contracts (Mode 2), final security model, observability,
OpenAPI.
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
| **E — Catalog coverage (L1C)** | FIN-107 children in `blocks` order, starting with FIN-152 (inventory) and FIN-153 (knowledge alignment) | anything that edits `knowledge/` or retrieval wording; one domain per agent after FIN-153 |

Lane D should be **done first and finished before the others start**: without the
scenario map, lanes A, B and C have no way to prove they are done.

L2 waits for L1 (breadth from L1C). L4 waits for L1, L1C, L2 and L3. L7 waits for all of them.

**Priority since 2026-09-26 (owner decision): lane E comes first.** A system
cannot return correct answers for the agreed scope while its catalog covers four
of ~20 domains, so completing the catalog precedes further testing of anything
that consumes it. Concretely:

- **Frozen until L1C closes:** new L4 answer-correctness claims and new L7/L8
  work (FIN-9, FIN-12 and FIN-106 are blocked by FIN-155). Retrieval tuning and
  per-phrase catalog-wording fixes are **not frozen but abolished** (2026-10-12
  correction): phrasing is the planner's job (FIN-161).
- **May continue** because it does not depend on catalog breadth: L0 SSE
  tickets already in progress, L3, L5, L6.
- No Bruno run is part of this re-ordering; the runner scripts are being
  updated separately.

---

## 5. Status at a glance

| Layer | Status | Prerequisites met? |
| --- | --- | --- |
| L0 Foundation | 🔨 | — |
| L1 Correct catalog | 🧪 | L0 🔨 |
| L1C Catalog coverage | ⬜ | L1 🧪 |
| L2 LLM planner | 🔨 | L1 🧪 (L1C ⬜ for breadth); real positive proof remains incomplete |
| L3 Datasets | 🧪 | L0 🔨 |
| L4 Correct answers | 🧪 | L1 🧪, L1C ⬜, L3 🧪 |
| L5 Response shape | 🧪 | L0 🔨 |
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

> **Superseded 2026-10-12** — this update measured the lexical/vector stand-in,
> not the LLM planner; see "Correction 2026-10-12". Kept as history only.

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
  - Some phrasings are misrouted by retrieval, e.g. "Top withdrawals per month…"
    goes to `client_activation_top_n_offices`.

Update after FIN-135 (planner honours stated period/limit/currency, discloses what it cannot bind):

- **Deterministic extraction, no LLM.** A new pure module,
  `crates/chat/src/engine/param_parse.rs`, extracts three parameter kinds
  straight from `request_text`, generic over parameter *name* (never over
  which capability was selected): an explicit period (month range, "last N
  months"/"N bulan terakhir", a bare year with a context word, and single-day
  "today"/"hari ini" style phrases) for `from_date`/`to_date`; a stated
  row-count ("top N" / "N terbesar") for `limit`; and an ISO currency code or
  common name ("rupiah", "dollar") for `currency_code`. An open-ended period
  (a month or year that includes today) is clamped to `business_today` — a
  future partial period has no data yet. `planner::bind_parameters` tries a
  parsed value before the manifest default; declared caps
  (`hard_cap`/`guards.max_limit`/`guards.max_date_range_days`) still bind, now
  clamping the *stated* value instead of the default.
- **Entities stay behind a resolver (K1).** The parser never attempts to bind
  a free-text name (e.g. an office name) to an identity-like slot — that
  would be exactly the free-text-binds-identity mistake K1 forbids. When the
  text names something capitalized after a generic preposition
  ("from"/"dari"/...) and the query has a matching optional string parameter
  with no default, the mention is disclosed as **not applied** instead of
  silently dropped (I5): a new `limitation` block, `params_not_applied`
  (`compose.rs`), distinct from the D2 auto-bind note because nothing was
  actually bound.
- **Disclosure reuses the D2 auto-bind path.** `compose::AutoBound` gained a
  `provenance` field (`resolver_unique` | `deterministic_parse`); a
  text-derived bind is disclosed in the same `slots_auto_bound` note block a
  resolver auto-bind uses, so the existing D2 validator check (disclosed set
  == `job_node_runs.input_binding_json.auto_bound_slots`) covers it for free.
  `worker.rs` merges `plan.deterministic_binds` into the auto-bound list
  before both the ledger write and the response composition — sourced from
  `plan`, not a new table, because it is exactly as reproducible from
  `request_text` + the catalog as any other binding.
- **Verified**: `fineract-assistant-api/params/` (FIN-135) proves period
  (EN month range, ID month range, bare-year), limit (ID superlative,
  EN "top N"), currency (EN, combined with limit) and the K1 not-applied case
  at the HTTP surface — lineage parameters plus the disclosure block, checked
  against a request built from the *current* date so the collection does not
  rot. Environment note: this shared dev Postgres had several other Orca
  worktrees running their own `WORKER_ENABLED=true` app instance at the same
  time, outside the `jarvis-bruno-locked.sh` lock's reach (the lock only
  serializes Bruno *runner* invocations, not a stray long-lived `app`
  process) — a job created here can be claimed and answered by another
  worktree's pre-FIN-135 binary. Two clean, fully-isolated runs each showed
  one scenario answered by stale (pre-fix) logic, in each case a *different*
  scenario; the pure-logic unit tests in `param_parse.rs` (deterministic, no
  network) and repeated single-scenario manual HTTP checks in this session
  never showed a failure. Re-run `params` alone when the shared instance is
  quiet before relying on a red run here as a real regression.
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

> **Superseded 2026-10-12** — lexical-scorer tuning (FIN-136, cancelled); history only.

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

> **Superseded 2026-10-12** — per-phrase catalog wording (FIN-142, cancelled); history only.

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
Update after FIN-150 (`engine/row-cap-response.yml` reads the response
without waiting for settlement — mechanism gate, no scenario ID):

- **Found:** confirms the follow-up FIN-146 flagged above — `row-cap-response.yml`
  was preceded only by the job POST, no state check or poll, same no-wait
  shape as FIN-146's eight fixes. The sibling readers `row-cap-job-state.yml`
  (seq 32.6) and `row-cap-dataset-rows.yml` (seq 32.7) both run strictly
  after `row-cap-response.yml` in the same folder, so once the response read
  itself blocks on settlement they observe an already-terminal job — no
  no-wait gap of their own, left untouched.
- **Fix:** same mechanism as FIN-138/FIN-145/FIN-146 — switched
  `row-cap-response.yml` to `lib/poll.js`'s `awaitJob`, wrapping the existing
  assertions unchanged. `engine` already runs under `BRU_SANDBOX=developer`
  (set by FIN-146), so no runner change was needed.
- **Verified:** `engine` stage passed **3/3 consecutive** locked runs, plus a
  full locked run, all green. `cargo test`, `cargo clippy -D warnings`,
  `docs-check.sh` and `acceptance-check.sh` all green (coverage unchanged —
  mechanism fix, no new scenario ID).

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

Update after FIN-149 (test-harness bug — `retrieval-healthy` gone
deterministically red after any `knowledge/` change):

- **Symptom:** every `knowledge/` edit changes the catalog's `content_hash`,
  and `chat::catalog::prepare` (app startup) records that as a brand-new
  `knowledge_catalog_versions` row with zero embedded `knowledge_index` rows —
  vectors are filled only by `app catalog --sync --embed`, which
  `scripts/integration-test.sh` never ran. `retrieval-vector`/`retrieval-healthy`
  (FIN-41/FIN-42) then settled `Unsupported`/`retrieval_miss` on the semantic
  arm every time, regardless of the API key being configured — the runner's own
  header claimed the precondition was "embedding lengkap", but only checked the
  key was non-empty.
- **Root cause, part 2:** `catalog::repository::upsert_version` unconditionally
  `DELETE`d and re-inserted a version's `knowledge_index` rows on every call —
  even when the version already existed with the same `content_hash` — so a
  backfill (`app catalog --sync --embed`) run a second time against an
  unchanged catalog would wipe the embeddings it had just computed. Proven with
  `embedded_at` timestamps: identical across two consecutive full locked runs
  after the fix (no rewrite), and a wipe reproduced before the fix.
- **Fix:** `upsert_version` now writes `knowledge_index` rows only for a
  version it just inserted; an existing version (same `content_hash`) only gets
  its `status`/`metadata_json`/`synced_at` refreshed, leaving computed
  embeddings untouched (`crates/chat/src/catalog/repository.rs`).
  `scripts/integration-test.sh`'s `retrieval-healthy` stage now runs
  `app catalog --sync --embed --no-probe` before starting the app, then queries
  `knowledge_index`/`knowledge_catalog_versions` directly for the active
  `content_hash` and fails loudly if any row is still `embedding IS NULL` —
  EMBEDDING_API_KEY empty still skips the stage as before.
- **Regression proof:** a throwaway one-line edit to
  `knowledge/capabilities/savings/deposit_total.yaml` (reverted before commit)
  changed the content hash; a full locked run then backfilled the new version
  (98 rows) and both FIN-41/FIN-42 assertions passed. A second consecutive
  locked run left `embedded_at` unchanged (0 rows re-embedded), proving the
  fix is idempotent and does not re-spend the embedding API budget on an
  unchanged catalog.
- **Not built:** nothing deferred; this ticket moves no scenario ID in §5.1
  (test-harness fix, not a new capability).

Update after FIN-148 (terse Indonesian ranking phrase left open by FIN-142):

> **Superseded 2026-10-12** — per-phrase catalog wording (FIN-148, cancelled); history only.

- **FIN-148 found:** the residual FIN-142 flagged for the owner —
  "Peringkat penarikan tabungan terbesar tiap bulan." selecting
  `savings_withdrawal_total` instead of `savings_withdrawal_monthly_top_n` —
  was a catalog-wording gap, not a general lexical weakness. Direct SQL
  against `knowledge_index` (same query `planner::best_capability` runs)
  showed the word "peringkat" ("ranking") and "tiap" ("each/every") appeared
  in **zero** Indonesian examples across any `savings_withdrawal_*` manifest;
  `_total`, `_top_n` and `_monthly_breakdown` all tied on `matched_terms=3`
  (`bulan`/`penarikan`/`terbesar` or `tabungan`/`penarikan`/`bulan`), and
  `_total` won the `ts_rank_cd` tie-break purely on document-length
  normalization — an accident of catalog wording, not a signal about intent.
- **FIN-148 fix (`knowledge/capabilities/savings/withdrawal_monthly_top_n.yaml`):**
  added exactly one new example, `"Peringkat bulanan."` — the word "peringkat"
  gives `_monthly_top_n` `matched_terms=4` against the target phrase (beating
  every sibling's 3), and "bulanan" ("monthly") avoids repeating a term
  (`penarikan`/`terbesar`/`bulan`/`tiap`) that any other capability was
  *already tied on*. That constraint was not academic: three earlier attempts
  at this same fix — reusing "penarikan"/"terbesar"/"bulan" in a full sentence,
  or adding "tabungan"/"tiap" together — each fixed the target phrase but
  flipped a **different**, pre-existing near-tie (`_top_n` vs `_monthly_top_n`
  on "Penarikan terbesar bulan ini.", already tied 4-4 with a ~15% score gap
  at baseline; `_monthly_top_n` vs `savings_deposit_monthly_top_n` on a
  deposit phrase, tied 5-5) purely by inflating `ts_rank_cd` term-frequency
  for a word two capabilities already shared. The one-word, non-overlapping
  example was the only tested wording that closed the target gap without
  touching any other tie. No scorer change, no allowlist/denylist.
- **FIN-148 corpus proof (`cargo run -p app -- retrieval-sweep`):** baseline
  238/238 corpus, 11/12 held-out (the known FIN-142 residual). After the fix:
  **239/239 corpus** (one net new manifest example self-selects), **18/18
  held-out** — the original FIN-142 phrase now passes plus 6 new phrasings
  added to `crates/app/fixtures/fin142_held_out.json` (ranking variants with
  "top 5" / reordered clauses / a `savings_withdrawal_total` sanity check /
  a same-phrase-different-year variant), all withdrawal-domain per this
  ticket's edit scope. Two candidate held-out cases that probed
  `savings_deposit_*` capabilities or an English "ranking" synonym were
  dropped after confirming (via the same direct-SQL check) they fail
  identically at the pre-FIN-148 baseline — pre-existing, out-of-scope gaps,
  not regressions from this change. Re-run twice more after the final wording
  landed: stable at 239/239 / 18/18 both times.
- **FIN-148 proof at the HTTP surface:** one new `retrieval-selection` Bruno
  chain (`withdrawal-ranking-phrase-*`), test name `FIN-148: …`, using the
  same `lib/poll.js` `awaitJob` pattern FIN-145 standardized.
- **Verified:** `retrieval-selection` passed **5 of 6** locked runs (22/22
  tests each), including the 3 most recent consecutive. The one miss (job
  `093c5e8b`, 16:21:07) was diagnosed by direct SQL — its response's
  `evidence_json.lineage[0]` carries `catalog_version_id=b7844483…` /
  `catalog_content_hash=28234b15…`, this worktree's **pristine pre-fix**
  catalog (confirmed: 0 rows for `savings_account_identity_resolve`, no
  "Peringkat bulanan." in `withdrawal_monthly_top_n`'s indexed text) — not
  the current, correctly-fixed catalog every other pass used. Coordinator
  traced this to a rogue worker (`unknown-host/89286`) that answered jobs
  **outside** the `jarvis-bruno-locked.sh` lock from 16:20:16–16:24:42 using
  a stale pre-fix build, also stealing 37 of a sibling ticket's (FIN-144)
  jobs in the same window — a shared-runner infra incident, not a defect in
  this fix.

Update after the 2026-09-25 wave (FIN-135, FIN-137, FIN-140, FIN-144, FIN-147 and
the harness fixes FIN-138/145/146/149/150):

- **L4 🔨 → 🧪.** Every L4 ticket is closed and the gate holds on `main`
  (`ea116d0`): a full locked run with the fixed wrapper exits 0 — every stage
  green, including `answers` (FIN-52: job answers vs direct SQL), `params`
  (FIN-135: stated period/limit/currency bound and disclosed), the OVR-6.3…6.7
  chains, `worker-error`/`crash-*` (OVR-6.4 with FIN-141/144), the supersede
  regression (FIN-147) and `retrieval-healthy` after the harness's own
  embedding backfill (FIN-149); OVR-6.7 zero isolation violations. OVR-6.2 is
  L8-owned (owner decision, see above). Ceiling is 🧪: L1 and L3 are 🧪; ✅ is
  the owner's call.
- **Shared-Postgres runner integrity.** Results between ~15:15 and ~15:45
  (UTC+8) on 2026-09-25 were unreliable: the workspace wrapper
  `scripts/jarvis-bruno-locked.sh` (outside this repo) deleted locks it no
  longer owned and reclaimed stale locks non-atomically, so runs overlapped and
  worker-enabled apps claimed each other's jobs. The wrapper now releases only
  its own lock, serialises reclaim, and keeps the lock while its runner lives.
  An app started outside the wrapper still steals jobs (seen 16:20–16:24): the
  rule stays "never start the app against the shared Postgres except through
  the wrapper". Diagnose a suspicious failure with `audit_events`
  (`action='job.claimed'`, `detail_json->>'worker'`) and the catalog hash in
  `job_plans.contract_versions_json`.

Update after FIN-65/61/63 (L5.6, Lane C — `chart_spec` and dataset-backed
`table` wired into the served path; RESP-8.7 / RESP-8.9 proven at HTTP):

- **L5 🔨 → 🧪.** The two scenarios the FIN-60/62/64 update left without a live
  emitter now have one and pass at HTTP, so all ten RESP scenarios are proven
  against served documents. 🧪, not ✅: the prerequisite L0 is still 🔨
  (Rule 2). L6 is not re-rated here — its gate (responses.md §3–§6) was not
  re-assessed by this ticket.
- **FIN-65 (emission).** Owner decision 2026-09-25: a chart is **declared per
  capability** in `knowledge/` — `chart: { kind: time_series }` — never
  triggered by user phrasing. The time axis is the query's **declared grain**
  (L1.5): the first grain column whose `output_fields` type is `date`
  (`QueryManifest::time_dimension`); other grain columns become `series`. The
  catalog validator rejects a chart on a query with no date grain
  (`chart_matches_grain`, unit test `chart_requires_a_declared_time_grain`).
  Declared on the four capabilities whose query returns a monthly series
  (`request_shape.output: time_series`): `client_activation_monthly_breakdown`,
  `organization_office_opening_monthly_breakdown`,
  `savings_deposit_monthly_breakdown`, `savings_withdrawal_monthly_breakdown`.
  The monthly **top-N** capabilities are not charted — their grain is
  `transaction_id`. A charted capability always serves its `table` (also on one
  row) plus either a `chart_spec` (`derived_from` = the node, `data_block_id` =
  the table) or a `note` `chart_downgraded` (responses.md §7). The validator
  re-checks the declared shape against the node rows in the ledger
  (`job_node_runs.output_json`) and drops a non-conforming `chart_spec` under
  rule `§7`. Large results: above `NODE_OUTPUT_INLINE_THRESHOLD` (200 rows or
  32 KB, runtime.md §4) the `table` block carries `dataset_id` + `pagination`
  and no `rows` (responses.md §2, #11 rules 2–3); smaller results stay inline.
  Proven live by `engine/resp-chart-response.yml` (chart over
  Nov 2025–Sep 2026 deposits) and `engine/resp-large-response.yml` /
  `resp-large-rows.yml` ("top 100 deposits per month": 450 rows → handle-backed
  table, first page of 200 readable through its `rows_path`).
- **FIN-61 (RESP-8.7 at HTTP).** `engine/resp-downgrade-response.yml`: the same
  charted capability over June–September 2026 has deposits only in September —
  one point on the time axis — and serves `table` + `note` `chart_downgraded`,
  no `chart_spec`, still `Answered`/`Complete`/`passed`.
- **FIN-63 (RESP-8.9 at HTTP).** The response read path
  (`GET /chat/jobs/{id}/response`) now checks the state of every handle a
  `table` block references; an `expired`/`purged` handle is served as
  `expired_dataset_table` (`detail_available: false`, `rows: null`,
  `handle_state`, `as_of` from the dataset snapshot, `row_count` kept). The
  stored document is unchanged. `engine/resp-expired-purge.yml` purges the large
  table's handle through the FIN-44 local seam, and
  `engine/resp-expired-response.yml` reads the same response again.
- **Deviation from the brief's ownership list:** `crates/chat/src/job/service.rs`
  and `crates/chat/src/engine/dataset/service.rs` were edited — the response read
  path lives there, and RESP-8.9 is a read-time disclosure. The ledger query in
  `engine/repository.rs` now also reads `output_json` for the §7 check.
- §5.1 stays **36/59**: no new scenario ID; RESP-8.7/8.9 moved from unit-tested
  to HTTP-proven.

Update 2026-09-26 — diagnosis: the catalog was never measured for coverage
(owner decision; no code change, no Bruno run):

- **What was found.** `knowledge/` still holds the carried-over catalog: 49
  capabilities in four domains (client 15, savings 22, organization 11, group 1
  resolver), reading 13 Fineract tables. `git log` since 2026-09-13 shows only
  fixes, no new domain. The scope document (§1 + D01–D15) requires ~20 domains.
  Local `fineract_default` already holds data for most of them (116 loans,
  37,077 journal entries, 61 standing instructions, 308,960 job runs); client
  charges and provisioning entries have 0 rows and share accounts only 2.
- **Why it went unnoticed**, root first:
  1. The formal inventory required by `dataset-scope-decisions.md` §5 step 2
     was never written, so there was no yardstick for "complete".
  2. L1 measured *correctness* of the carried-over entries and was then read
     as "the catalog is done"; L2–L4 were built and tuned on top of it.
  3. `knowledge/data-scope/` and `knowledge/domains/` (MVP scope from
     `ai_report`) were treated as authority, against "docs are authority,
     carried-over code is a suspect". FIN-140's deferred-domain guard is built
     on those carried-over statuses.
  4. `app catalog` validates the catalog against itself (YAML ↔ SQL ↔ schema),
     never against the scope documents, so contradictions with D09, D12 and
     §1 passed (listed in FIN-153).
  5. The coverage epic FIN-107 (2026-09-20, 23 children) was never added to
     this document, so no resume/work-ticket pass could pick it, and the L2/L4
     🧪 rows read as further along than they are.
  6. Retrieval tuning (FIN-136/142/148) was fitted to a 49-entry catalog and
     will shift when new domains compete for the same vocabulary.
  7. `dataset-scope-decisions.md:7` still said "implementation not allowed"
     after the owner's 2026-09-20 decisions were final.
- **Changes.** New gate L1C (§3), lane E with priority and freeze rules (§4),
  status row (above). Linear: FIN-152 (inventory), FIN-153 (knowledge
  alignment + `Unsupported` for documented domains without a contract),
  FIN-154 (retrieval re-proof), FIN-155 (answers re-proof); FIN-107's children
  carry `blocks` relations in wave order; FIN-9, FIN-12, FIN-106 are blocked by
  FIN-155; FIN-151 is `Bloked`.
- **Unproven risk, recorded not tested:** a question about share, FD/RD,
  teller, standing instructions or provisioning has no deferred-domain file,
  so it may be misrouted to a savings capability as loan questions were before
  FIN-140. To be proven in FIN-153 once the Bruno runner update lands.

Update after FIN-153 (`knowledge/` alignment against the inventory):

- **One domain/decision record per `dataset-inventory.md` subject, not just
  the six contradictions.** `knowledge/domains/` now has a file for every
  baseline domain and every D01–D15 decision that has its own natural-language
  subject: `organization`, `products` (new), `client`, `loan`, `savings`,
  `fixed_recurring_deposit` (new), `share` (new), `linking_resources` (new),
  `accounting` (D12), `tax` (SAV-9/PROD-10), `audit` (D09, new),
  `teller_cashier` (D06, new), `standing_instructions` (D05, new),
  `provisioning` (D07, new), `scheduler` (D14, new), and `group_center`
  (CLI-4 + D08). Each cites its inventory row IDs in `inventory_refs`
  (mechanically required — see below). D01 (period-close, recomputed-only),
  D02 (historical branch attribution), D03 (currency/consolidation), and D11
  (completeness analysis) deliberately have **no** domain file: they are
  cross-cutting rules that change how an existing capability must answer, not
  a subject a user asks about on its own — see `knowledge/CARRY-OVER.md` "FIN-153"
  section for the reasoning and the explicit citation each still carries.
- **Six contradictions the ticket named, fixed.** (1) `accounting` no longer
  lists `trial balance` in `supported_intents` — D12 excludes full financial
  statements permanently (stays in `unsupported_intents`, `BlockedByPolicy`);
  single-named-account GL/journal traceability is a `gap_intents` entry. (2)
  D09 (`m_portfolio_command_source`) moved out of
  `audit-users-operations.yaml` `excluded_tables` into a new area
  `source-audit.yaml` and the new `audit` domain — distinct from the
  still-excluded `m_appuser`/`m_role`/`m_permission` administration tables and
  from Jarvis's own execution audit. (3) `client.yaml` no longer hard-rejects
  "address-level reporting" via `unsupported_intents` — CLI-6 is
  selective/agreed, moved to `gap_intents`. (4) `loan`, `tax`, and
  `accounting` changed status `deferred` → `gap`: their inventory rows are
  agreed baseline scope with no approved capability, not onboarding-dependent
  like D10 — `deferred` stays reserved for that (`custom_datatables`, status
  renamed `deferred_onboarding` for clarity). (5) `client.yaml` states CLI-5
  (all account types) and declares `gap_intents` for the loan/share sides; the
  new `share` domain (SHARE-1..8) exists so a share question is never
  misrouted to savings the way loan questions once were (FIN-140's original
  bug); `fixed_recurring_deposit` deliberately does **not** claim the
  `maturity`/`fixed deposit`/`recurring deposit` vocabulary an existing
  approved capability (`savings/deposit_maturity_by_client`, FDRD-2) already
  owns — only the rows it does not touch (funding, preclosure, RD
  contribution/shortfall). (6) `group_center.yaml` `supported_intents` no
  longer claims savings-only reporting — the only approved capability is
  `group_identity_resolve`; `candidate` status now feeds the same guard as
  `gap`/`deferred`, and `gap_intents` cover account summaries across every
  product plus D08 meeting attendance.
- **Mechanism, not prose.** `Domain`/`DataScopeArea` gained `inventory_refs`
  (non-empty, cites `dataset-inventory.md` row IDs) and `Domain` gained
  `gap_intents` (feeds the deferred-domain guard at intent granularity — one
  concept inside an otherwise-`approved_mvp` domain, e.g. client address).
  `catalog::validate` rejects a missing `inventory_refs` and the six
  contradictions' exact shape if reintroduced
  (`gap_domain_not_mislabeled_deferred`,
  `gap_intents_exclude_out_of_scope_phrases`, `gap_item_not_hard_rejected`,
  `audit_source_command_in_scope`, `client_address_not_hard_excluded`,
  `client_all_account_types_declared`), each with a unit test. `cargo run -p
  app -- catalog` → **0 error** on the realigned catalog.
- **Three reasons, not one.** `completeness_reason` for an uncontracted domain
  used to be a single `domain_deferred` constant regardless of *why* — that
  conflated "agreed, just not built" with "waiting on deployment" and "area
  not verified". It now reads `Term::source`, set per domain at guard-build
  time (`catalog::surface::reason_for`): `domain_gap` (agreed baseline scope,
  no capability — the common case above), `domain_deferred` (onboarding-
  dependent, D10/D15a/b only), `domain_conditional_not_enabled` (`candidate`,
  e.g. `group_center` until its deployment verifies the underlying area). All
  three stay distinct from `out_of_scope`, `retrieval_miss` (planner-level,
  no candidate at all) and `surface_not_approved` (policy hard-reject).
  Documented in `docs/contracts/api-reference.md`.
- **Proven live.** Full locked run: 598 requests, 0 failed (unchanged
  baseline) plus 15 new job/state Bruno scenarios (audit D09, client address
  CLI-6, share SHARE-1..8/CLI-5, group/center beyond savings CLI-4, products
  PROD-1, FD/RD FDRD-8, linking resources LINK-4, teller/cashier D06,
  standing instructions D05, provisioning D07, scheduler D14, group meeting
  D08) each asserting `Completed` + `Unsupported` + the correct one of the
  three reasons **and** `!= out_of_scope`, `!= retrieval_miss`, `!=
  surface_not_approved`. Existing `policy-deferred-{loan,tax,accounting}-*`
  (now `domain_gap`, updated) and the FIN-140 control
  (`policy-deferred-savings-credit-control-*`, "credit" ambiguity, now also
  checked against `domain_gap`) still pass.
- **Not done here (out of this ticket's boundary).** No domain SQL, capability,
  or migration for LC.1–LC.23; L1C stays ⬜ until point 3 of its "Done when"
  (each domain's Mode-1 capabilities passing the four `CARRY-OVER.md` rules,
  recorded in `knowledge/VERIFICATION.md`) is also met — that is FIN-109
  onward.

Update after FIN-126 (D13 — existing Fineract reports guard bug):

- **The D13 policy area now guards report requests, not just the table name.**
  `knowledge/data-scope/areas/out-of-scope.yaml` already listed
  `stretchy_report_execution`, but the runtime matcher only read
  `excluded_tables`, which was empty there. Its new `stretchy_report` entry
  rejects explicit table/feature requests. `excluded_phrases` in that same
  rejected area covers natural-language actions naming built-in Fineract
  reports (EN/ID) without broadly blocking the ordinary word "report";
  `knowledge/policies/unsupported_requests.yaml` also declares the D13
  hard-reject category.
  `catalog::surface::Surfaces` reads these phrases only from a
  `rejected_group` area; the guard in `engine::worker::run_job` runs before
  scope derivation, retrieval, planning and source SQL. D13 is permanently
  excluded (`dataset-scope-decisions.md:166-172`,
  `dataset-inventory.md:269`); this must not be treated as FIN-153's
  `domain_gap` or FIN-117's agreed meeting-domain requests.
- **Live HTTP proof:** `policy-surface-fin126-stretchy-report-{job,state,response}.yml`
  (EN: "Run Fineract report X") and the `-id-` trio (ID: "Jalankan
  report Fineract X") assert `Completed` + `BlockedByPolicy` +
  `Unknown` + `surface_not_approved`, `plan_version = null`, and a
  sanitized limitation without source lineage or report definition.
  `policy-surface-fin126-{list,show}-*` also reject listing and surfacing
  named built-in reports without a plan. `fin126-stretchy-report-control-*`
  asserts ordinary savings analysis mentioning "report" remains `Answered`;
  no report execution endpoint, capability, SQL, or schema read is added.
  The `catalog::surface` unit tests cover explicit `stretchy_report`
  vocabulary, action phrasing, metadata mention and approved controls.
  An app-database ledger check of these exact request texts found zero
  `job_plans` and zero `job_node_runs` for each rejected job; the
  `Answered` savings control had both a plan and a node.
- **Coverage remains unchanged.** D13 has no numbered scenario in §5.1;
  L1C stays ⬜ pending each domain's Mode-1 verification.

Update after FIN-117 (L1C D08 group/center meeting schedule):

- The local deployment's conditional group/center gate is activated only
  against verified `dataset-inventory.md` CLI-4/D08 evidence: two groups at
  different levels, two matching meeting-calendar instances (entity enums
  2/4, calendar type 1), both with `FREQ=DAILY`. Other deployments must
  verify group use, enum/recurrence semantics and PII before activating the
  area, domain and capability; without that evidence all stay conditional/
  candidate and schedule requests remain `Completed+Unsupported` with
  `domain_conditional_not_enabled`.
- `group_center_meeting_schedule` is a Mode-1 curated query. It binds an
  inclusive seven-day maximum window and authorized office IDs inside
  Fineract SQL, expands verified daily recurrences to **scheduled** dates,
  and returns group/center IDs, assigned staff ID and directly linked
  member IDs. No name is fetched: `m_group.display_name` is PII
  (`knowledge/schema/fineract/group_center.yaml:20-21`). A missing staff
  assignment or center direct membership stays NULL, never inferred.
  `m_meeting` and `m_client_attendance` each have zero rows here, so actual
  occurrence and attendance remain `domain_gap`, not schedule evidence.
- Direct SQL for office 3, 2026-09-28…2026-10-04 gives 21 scheduled
  group/member rows, seven occurrence dates and three distinct direct
  member IDs (4/5/9); the capability query gives the same 21/7/3.
  Office 2's center yields seven schedule rows, staff ID 1 and no direct
  member. The FIN-117 engine request chain verifies retrieval, completed
  `Answered+Complete` with lineage and the office-scoped source rows; the
  existing FIN-153 attendance/account-gap chains continue to assert
  `Completed+Unsupported`. The live `answers/group/fin117-*` chain checks
  the same capability's single-day columns, rows and bound parameters
  against independent Fineract SQL (`tests/answers/group_center_meeting_schedule.sql`);
  the locked answers stage passed 220 requests/272 tests. Source evidence
  and all four `CARRY-OVER.md` checks are recorded in
  `knowledge/VERIFICATION.md` §10.
- The locked HTTP engine stage passed 165 requests/202 tests, including both
  office-scoped schedules. A temporary unverified-deployment catalog (area
  conditional, domain/capability candidate) passed 22 requests/29 tests and
  returned `Completed+Unsupported` /
  `domain_conditional_not_enabled` without a plan for the same schedule
  request. The temporary catalog was removed and normal configuration
  restored before the full suite.
- Final post-rebase full locked run (including merged FIN-126 D13 coverage)
  passed all stages: 652 HTTP requests and 768 Bruno tests, with catalog
  validation at 0 errors/2 pre-existing resolver warnings.
  The first full attempt had exposed an unrelated timing race in the
  FIN-153 D02 historical-office state request (`Running` at its fixed
  1.5-second read); it was not changed here, and the subsequent complete
  run passed. Track that race separately rather than weakening D08 proof.
- L1C remains ⬜: D08 is one domain's Mode-1 slice, not all full-release
  domains. §5.1 remains **36/59** because FIN-117 has a domain gate but no
  numbered acceptance scenario.

Update after FIN-156 (D02 historical-office Bruno polling race):

- The FIN-153 D02 job-state check previously read once after a fixed 1.5-second
  delay; a full FIN-117 run observed `Running` before the job settled, while
  subsequent runs passed without an application change. It now uses the existing
  bounded `lib/poll.js` terminal-job wait before asserting the unchanged
  `Completed` + `Unsupported` + `Unknown` + `domain_gap` result and null plan.
  A terminal failure still reaches the assertion and fails; a job that never
  settles fails at the polling deadline instead of passing silently.
- The locked `engine` stage passed 177 requests / 218 tests, including
  `OVR-6.6/FIN-153 (gap, D02 historical office): Completed + Unsupported +
  Unknown, tanpa plan`. The full locked run passed all 14 stages: 651 requests /
  768 tests. This changes test synchronization, not the catalog or the D02
  contract; L1C stays ⬜ and §5.1 coverage stays 36/59.

Update after FIN-109 (Products masters, PROD-1..11):

- Owner-approved contract (`analytical-contracts.md` §6, `dataset-inventory.md`
  XR-SCOPE): verified organization-wide product/config masters are read
  tenant-wide by an active admin, never inferring per-office availability;
  pledged collateral (PROD-5) stays office-scoped. PROD-6 maps product to bucket
  through `m_product_loan.delinquency_bucket_id`; PROD-9 has no product-level
  RD frequency source on this deployment and answers `Unsupported`.
- Fifteen Mode-1 capabilities under `products_master`, each compared row-for-row
  with independent SQL (`tests/answers/`), including unused and zero-row masters.
  Office-specific or non-admin master requests return `Unsupported`.
- Fixed on the way: FIN-157 (INT2 columns decoded as `null`), FIN-158
  (three Bruno controls assumed "this month" has activity).
- L1C stays open: PROD rows are `inherited`, remaining domains are not; §5.1
  coverage is unchanged (no numbered L1C scenario).

Update after FIN-108 slice 1 (Loan identity, lifecycle, effective terms — LOAN-1..3):

- Loan domain is now `approved_mvp` for three capabilities reading `m_loan` only:
  `loans_by_client` (every status, never active-only), `loan_status_summary`
  (per status and currency), `loan_terms_by_client` (account terms beside the
  product's current default, never substituted). Office scope binds the client
  office, else the group office, in SQL.
- LOAN-4..12 stay `gap` in area `loans`; their subjects answer `Unsupported`
  through inventory-backed `gap_intents`. The surface guard no longer turns a
  gap area's tables into policy rejections when its domain is partly approved.
- Evidence resolved: loan status labels (`r_enum_value`), recovery type 8 and
  transaction types 20–34 (Apache Fineract `LoanTransactionType`; local
  `r_enum_value` stops at 19).
- FIN-108 stays open for LOAN-4..12; L1C stays open; §5.1 unchanged.

Update after FIN-108 slice 2 (loan repayment schedule, transactions — LOAN-5, LOAN-6):

- Two more Mode-1 capabilities read `m_loan_repayment_schedule` and
  `m_loan_transaction`: `loan_schedule_by_client` (per-client plan, never
  presented as an event) and `loan_transactions` (date-ranged, office-scoped
  directly on `m_loan_transaction.office_id`, reversed excluded by default,
  allocation across principal/interest/fee/penalty/overpayment).
- Allocation reconciles to the transaction amount for repayment-type rows
  only; a disbursement's amount is not spread across those columns (verified:
  95 of 2,859 non-reversed transactions are DISBURSEMENT and legitimately
  don't reconcile — documented, not hidden).
- Partial-domain contract fix: `loan.yaml`'s `concepts:` block was removed.
  Once a domain mixes `approved_mvp` and `gap` subjects, that domain's own
  concepts flow into `DeferredDomains`'s `excluded` list and silently cancel
  its own `gap_intents` (e.g. "arrears" stopped guarding LOAN-9). Guarding now
  rests on inventory-backed `gap_intents` alone; two FIN-109 capability
  descriptions that happened to contain the bare word "arrears" were reworded
  for the same reason.
- "Show loan transactions."/"Tampilkan transaksi pinjaman bulan ini." moved
  from refused (`domain_gap`) to `Answered`; the two `policy-deferred-loan*`
  Bruno controls now prove a still-gap subject (loan charges, LOAN-7) instead.
- LOAN-4, 7..12 stay `gap`. FIN-108 stays open; L1C stays open; §5.1
  unchanged.

Update after FIN-108 slice 3 (loan charges, balances, arrears — LOAN-7, LOAN-8,
LOAN-9):

- Three more Mode-1 capabilities under new area `loans_charges_balances`:
  `loan_charges` (`m_loan_charge` joined to `m_charge`, charged vs
  paid/waived/written-off/outstanding kept distinct), `loan_balances` (reads
  `m_loan`'s own `*_derived` columns directly, never recomputed from schedule
  or transactions), `loan_arrears` (`m_loan_arrears_aging`, 1:1 while
  overdue, `overdue_since_date_derived` carried on every row as the XR-ASOF
  signal — arrears currency still depends on COB/D14, which stays `gap`, so
  no job-run join was added).
- Office scope for all three binds the owning loan's client-or-group office
  (`COALESCE(c.office_id, g.office_id)`), same path as LOAN-1..3; none of
  `m_loan_charge`/`m_loan_arrears_aging` carry a direct `office_id`.
- "Show loan charges."/"Tampilkan biaya pinjaman bulan ini." moved from
  refused (`domain_gap`) to `Answered`; the `policy-deferred-loan*` Bruno
  controls now prove a still-gap subject (loan disbursement, LOAN-4) instead.
- LOAN-4, 10..12 stay `gap` in area `loans`. FIN-108 stays open for LOAN-4,
  10..12 (slice 4); L1C stays open; §5.1 unchanged.

Update after FIN-160 (Settings admin surface — read-only inspection + catalog
reindex, additive ops tooling outside the L0–L8 ladder):

- New `/settings/*` HTTP surface (`crates/chat/src/admin/`): models,
  connections, catalog, access, runtime, observability, audit (admin-wide,
  sanitized metadata only — owner decision 2026-10-04, no controlled
  evidence/raw SQL/prompts/secrets). Every handler rechecks the live
  `users`/`auth_sessions` row, not the stateless JWT claim alone.
- `POST /settings/catalog/reindex` + status reads
  (`crates/chat/src/catalog/reindex/`): revalidates the full disk catalog
  (including `knowledge/datasets` definitions and resolver shapes, which
  contribute to `content_hash` but are **not** a new `knowledge_index`
  `source_type` — that enum stays `capability`/`query`/… unchanged),
  republishes `capability`/`query` rows for that catalog version under one
  cross-process `pg_advisory_lock`, and reports the running (process-pinned)
  catalog hash separately from the latest indexed hash with an explicit
  `restart_required_for_latest` flag — no silent worker/resolver hot-swap.
  `rebuild_embeddings=false` preserves existing vectors whose retrieval text
  is unchanged (cheap lexical-only path, what `app catalog --sync --embed`
  now runs through); `true` forces a full Voyage-compatible re-embed. A
  provider failure leaves previously published vectors/metadata untouched
  and reports a public `error_code`; `app catalog`'s CLI sync/embed path was
  migrated onto this same `Admission` so CLI and HTTP share one lock and one
  publication contract instead of two.
- No LLM integration was added: `/settings/models` reports the configured
  `LLM_PROVIDER`/`LLM_MODEL` as `implementation: "not_integrated"`,
  `operational: false` for generation/planning/narration/summarization —
  which was true then and is **not** the design: planning belongs to the LLM
  (Correction 2026-10-12; FIN-161).
- Not in scope: FIN-102 (traces/metrics exporters — `/settings/observability`
  reports them `not_implemented`, not simulated), FIN-101 (final identity/
  permission model), FIN-105 (tunable runtime budgets). This ticket exposes
  existing mechanisms truthfully; it does not close any of those three.
Update after FIN-108 slice 4 (loan disbursement, collateral/guarantor,
reschedule/terms-change/write-off/recovery, D15 balance tables — LOAN-4,
10..12):

- Eleven Mode-1 capabilities: `loan_disbursement`, `loan_disbursement_tranches`
  (LOAN-4, loan grain and tranche grain as separate result sets; tranches
  carry currency, a reversed flag and an id tie-break); `loan_collateral`,
  `loan_guarantor`, `loan_guarantor_funding` (LOAN-10, never joined;
  collateral value is a current-master-rate computation in the collateral
  master's own currency beside the loan currency; funding currency comes from
  the guarantor-held savings account via `m_portfolio_account_associations`
  type 2, NULL when absent); `loan_reschedule`, `loan_terms_change`,
  `loan_writeoff`, `loan_recovery` (LOAN-11; terms change includes variations
  no reschedule request maps, read through scalar subqueries so the mapping
  never fans out; write-off/recovery carry currency, recovery excludes
  reversals); `loan_capitalized_income_balance`, `loan_buy_down_fee_balance`
  (LOAN-12/D15, one capability per table named by
  dataset-scope-decisions.md:188, reversed/`is_deleted` records excluded,
  `is_closed` labelled, currency per row).
- Correction of the first slice-4 attempt (never shipped): it read LOAN-12
  from `m_loan` flags only (`loan_capitalized_balances`, removed), its
  collateral/tranche/funding outputs had no currency, `loan_terms_change`
  INNER JOINed the reschedule mapping (dropping unmapped variations), the
  tranche order had no tie-break, it rewrote contract columns of
  `dataset-inventory.md` (reverted — only Capability/Confidence/Status are
  tracking columns), and it called empty-table results "proven". Its
  re-pinned incidental status test (`LOAN-12` inherited) is deleted, not
  re-pinned.
- Four data areas replace the gap area `loans`: `loans_disbursement`,
  `loans_collateral_guarantor`, `loans_reschedule_recovery`,
  `loans_capitalized_balances`; `loans` left `uncontracted_areas` and
  `gap_areas`; `reporting-scope.yaml` also gains `loans_charges_balances`
  (missed in slice 3).
- Evidence split, stated as such: real production rows exist for
  disbursement (116 loans, 74 with net ≠ approved), collateral (12, USD master
  vs AED loans), guarantor (1), reschedule (6) and terms change (6, all
  mapped). Tranche, guarantor funding/transaction, write-off, recovery and
  both D15 tables have 0 rows in this deployment: their positive behaviour is
  proven only by a throwaway read-only fixture (separate scratch database,
  schema-only copy, unmodified production SQL via PREPARE/EXECUTE with bound
  parameters, 7/7 expected outputs, 6/6 mutants detected) — a test of the
  query, not production evidence. See `knowledge/VERIFICATION.md` §14.
- LOAN-4/10/11/12 moved `gap` → `inherited` in `dataset-inventory.md` §5 with
  that evidence split recorded in the Confidence column.
- OVR-6.6 full-chain control `policy-deferred-loan-*` → `policy-deferred-tax-full-chain-*`
  (still-gap `tax` domain; assertions unchanged).
- `scripts/integration-test.sh`: a sub-path argument (`answers/loans`,
  `engine/login.yml`) runs only that part in its top-level folder's stage;
  bare folder names unchanged. `scripts/answer-expectations.sh`:
  optional `ANSWERS_ORACLE_PREFIX` limits oracle generation (default: all).
- FIN-108 slice 4 ships with this commit (LOAN-1..12 all `inherited`; FIN-108
  closes). L1C stays open; FIN-154/155 re-proof still waits for the full
  catalog; §5.1 unchanged (no numbered acceptance scenario is tied to LOAN-*).

Update after FIN-162 (owner-approved planner contract, 2026-10-12):

- The output contract is a compact versioned typed graph. The Engine derives
  execution details, validates source quotes/spans and slot types, resolves
  calendar expressions and verifies catalog, graph, authorization, policy and
  budgets before T3. Approved resolver cardinality/completeness still owns K5.
- The current MLX/BitNet 2B development model is retained. A provider smoke
  round-tripped, but JSON-object mode returned fenced JSON; this is evidence
  of reachability only, not schema enforcement or a working planner.
- Planning failures use the existing lifecycle/outcome matrix with distinct
  `planner_unavailable`, `planner_output_invalid`, `planner_abstained` and
  `planner_no_fit` codes. No new top-level outcome or schema migration is
  authorized by this contract.
- Planner scenarios `PLN-1`–`PLN-9` are named in the owning Engine contract. None is proven by
  contract publication or the provider smoke. **L2 stays ⬜**; the existing
  lexical/vector stand-in remains nonconforming. FIN-163 owns client wiring.
- Only documentation/acceptance-inventory checks are appropriate for this
  contract change. No full Rust or Bruno suite was run; implementation must
  prove the named scenarios with targeted real-planner and failure-path runs.

FIN-163 working-branch evidence (not published or closed):

- `foundation::config` consumes generation settings; Rig 0.40.0 owns the
  OpenAI-compatible call. Text messages are serialized as strings because
  the live llama.cpp Qwen template reports `supports_typed_content=false`.
  Request text is separate from candidate/context data. No Reqwest client
  fallback or deterministic capability-selection fallback was added.
- The Engine parses the closed compact proposal, checks source quotes/UTF-8
  spans and approved single-capability admission, and settles the four
  planner failure codes explicitly. Multi-node scheduling remains L8.
- Migration `20261012000000_planner_attempts.sql` preserves per-snapshot
  repair counts across lease recovery. Absolute model/token counters stay
  on `chat_jobs`; unknown usage retains the admission reservation.
- Targeted pure logic: proposal parser (3), verifier (4), config boundaries
  (2) passed. The schema smoke passed after applying the migration.
  Targeted core/chat clippy passed before the final text-message/context
  serialization changes; no full Rust suite was run.
- Locked negative stages `planner-unavailable`, `planner-invalid`,
  `planner-repair`, `planner-no-fit`, `planner-budget` passed 40/40 tests
  in the recorded run, with no commit-isolation violation. These results
  predate the final message serialization changes; they are not a claim
  of a complete final regression run.
- The real `planner` chain remains failing. Latest inspected job
  `3d9d74f5-5821-4b19-b936-a7f6444362dc` exhausted four attempts, each
  audited with observed usage and `source_quote_not_found`; no verified
  plan was admitted. PLN-1 is not proven. Do not weaken the golden
  capability/grounding assertions or strip/fix model output to close this.
- The current live server advertises
  `qwen3-4b-instruct-2507-q4_k_m`. The owner supplies
  `LLM_PROVIDER=llamacpp-local`; prior diagnostic process overrides used
  the inaccurate label `llama-local`. Local `.env` is not overwritten.
  FIN-163 remains In Progress, without a commit/PR or closure.

FIN-163 continuation after standalone model evaluation (unverified):

- Removed checkout-local Rust build artifacts with `cargo clean` before
  continuing development; no source or global Cargo dependency cache was
  removed.
- The Engine now supplies a closed native JSON schema for the version-1
  proposal, including all four slot variants. Rig uses `json_schema` without a
  JSON-object downgrade. The strict parser and admission verifier remain the
  authority; grammar support is not semantic conformance.
- `planner-v2` adds complete grounded-slot examples and explicit rules for
  unspecified dates, accepted bindings, integer types and repeated quotes.
  This is a new integration prompt, not a measured improvement or a claim that
  either standalone experiment prompt was adopted unchanged.
- New `planner-manual-success` and `planner-manual-unavailable` Bruno chains
  are prepared with the `planner-manual` environment. The English default is
  not a PLN-1 proof; that scenario still requires its Indonesian request,
  concrete calendar bindings and matching direct-SQL evidence.
- Owner explicitly deferred testing during this continuation. No compile,
  Rust tests, backend startup or Bruno execution has verified these changes.
  FIN-163 remains In Progress, PLN-1 remains unproven, and the existing
  pre-change evidence must not be reported as validation of this revision.

FIN-163 targeted verification after owner authorization:

- One locked run built `app` with development debug information and
  incremental compilation disabled. Rebuilt `target/` occupies 1.8 GB,
  compared with 8.6 GB before cleanup. No full Rust/Bruno suite or standalone
  model benchmark was repeated.
- Added runner selection for the two new manual folders; the database guard
  and per-database lock remain mandatory. The success stage supplies
  `Total setoran tabungan bulan ini` and refreshes only the deposit-total SQL
  oracle. The unavailable stage uses a closed local port, a 500 ms timeout and
  zero retries.
- `planner-manual-success`: 12/12 checks passed. Job
  `f1720ba6-0b1c-406d-b57a-851cef3cc61b` settled
  `Completed`/`Empty`/`Complete`, admitted plan version 1, and selected
  `savings_deposit_total`. Date-source spans, concrete date bindings
  (2026-10-01 through 2026-10-13), office scope and result agreed with the
  independently computed SQL. The population was empty; this run is not
  evidence for nonzero monetary arithmetic.
- The successful job made two audited attempts: the first timed out after
  30,027 ms; the second completed in 26,522 ms. Its observed model was
  `qwen3.5-4b`, prompt `planner-v2`, with 3,033 input and 80 output tokens.
  Usage for the timed-out attempt remains unknown/reserved. A successful
  retry is not first-pass model success or a reason to waive timeout budgets.
- `planner-manual-unavailable`: 7/7 checks passed. Job
  `a6c213b8-c3fe-4409-b4bb-3cde7897a895` settled
  `Failed`/`OperationalFailure`/`Unknown` with `planner_unavailable`.
  A separate read-only inspection confirmed the matching durable
  `job.failed` event, zero plans, zero node runs and zero promoted memory.
- The run reported zero `commit_isolation_violation` markers. Snapshot
  polling waited for the one submitted job; it did not repeat the scenario
  or submit additional analytical requests.
- Evidence supports PLN-1 for this empty-result request, PLN-4, and the
  correlated-call/usage portion of PLN-9. Other planner scenarios and the
  full regression gate were not exercised. FIN-163 remains In Progress
  without a commit, PR, CI result or closure.

Update after FIN-159 (LOAN-6/LOAN-7 — loan transactions/charges carry source
loan currency; uncommitted worktree deliverable, 2026-10-15):

- Defect (code bug, omission): `loan.loan_transactions` and
  `loan.loan_charges` returned monetary columns without the owning loan's
  `currency_code`, against FIN-107 decision 1 and
  `docs/data/analytical-contracts.md` §5–6 (currency per measure, no silent
  mixing). `queries/loan/loan_transactions.sql` now joins `m_loan` on
  `t.loan_id` (primary key; 2,859 non-reversed rows with and without the join,
  no null `loan_id`) and projects `l.currency_code`; the direct
  `t.office_id` filter, reversal exclusion, date window, grain
  `transaction_id`, `ORDER BY transaction_date DESC, id DESC` and bound LIMIT
  are unchanged. `queries/loan/loan_charges.sql` already joined `m_loan` for
  its client-or-group `COALESCE` office scope and now projects
  `l.currency_code`; every charge-state amount, grain, order and LIMIT are
  unchanged. Both manifests list `currency_code` last in `output_fields` with
  the existing `currency_per_row` check; `loan_transactions.yaml` lists
  `m_loan` in `tables` (same shape as `loan.loan_recovery`). Answer columns
  come from the manifest (`engine/planner.rs` `Plan.output_fields` →
  `engine/compose.rs::table_block`); no Rust changed.
- Oracles `tests/answers/{loan_transactions,loan_charges}.sql` derive
  `currency_code` independently from `m_loan` (charges keep their own
  `scoped` CTE). `answers/loans/{loan_transactions,loan_charges}-answer.yml`
  add `FIN-159 LOAN-6/LOAN-7: currency_code per row = source loan currency`
  (non-empty, row count = oracle, per-row currency = oracle, at least two
  distinct currencies so a hardcoded single currency cannot pass).
- Harness: `scripts/integration-test.sh` expands the optional `extra` array as
  `${extra[@]+"${extra[@]}"}` (bash 3.2 `set -u` safe, never an empty
  positional path — the earlier `"${extra[@]:-}"` attempt passed `""` to
  `bru` and silently selected the whole collection), and an explicit
  `PLANNER_PROVIDER_URL`/`PLANNER_PROVIDER`/`PLANNER_MODEL` process override
  now applies to the catalog step and every stage, not only planner stages.
- Proof, locked runner on port 3359, one chain per capability
  (`fin108-login` → `<cap>-session` → `<cap>-job` → `<cap>-answer`,
  `ANSWERS_ORACLE_PREFIX=<cap>`; 4 logical requests, 0 skipped):
  - Real configured provider (`qwen-cloud`, `qwen3.8-flash`): job
    `aa96119e-966d-4e90-b252-00d9531a44b7` failed `planner_output_invalid`,
    4/4 model calls `provider_output_limits` with observed usage far below
    limits (306–1,098 output tokens), i.e. the `complete && only_text` gate in
    `crates/core/src/generation.rs`. This was the FIN-163 provider problem,
    not a FIN-159 result; the planner was not changed. (Historical: root cause
    and fix are recorded under "FIN-163 provider correction" below.)
  - All HTTP proof below therefore uses a throwaway deterministic
    OpenAI-compatible **test fixture** provider outside the repo (process-only
    `PLANNER_PROVIDER_URL`, model `test-fixture`) returning grounded proposals
    for the two exact test requests through the unchanged Generation/Rig →
    verifier → scope → SQL → compose path. It is **not** real-LLM or L2/PLN
    proof.
  - Failing before (fix temporarily reverted, new oracles/assertions kept):
    both chains 4 requests (3 passed, 1 failed), tests 5/8; failures were the
    column-set check (`currency_code` absent), the row comparison (row 0
    `null` vs `USD`) and the FIN-159 currency test.
  - Passing after: `loan_transactions` job
    `e91c6b86-9752-437d-8d17-ce23708e30c3` 4/4 requests, 8/8 tests, 100 rows
    (`from_date` 2025-10-15, `to_date` 2026-10-15, `limit` 100), HTTP
    currencies AED 71 / USD 29 = oracle AED 71 / USD 29; `loan_charges` job
    `fdeece29-729d-4410-930b-9b77504de805` 4/4 requests, 8/8 tests, 100 rows
    (`limit` 100), HTTP AED 46 / EUR 6 / USD 48 = oracle AED 46 / EUR 6 /
    USD 48. Answer polls (`GET …/response` 404 until settled) are extra
    request executions on top of the 4 logical requests.
- Source populations (read-only Fineract, 2026-10-15): non-reversed
  transactions AED 1,704 / USD 1,121 / EUR 32 / AUD 2 (2025-11-10..2026-05-22,
  all inside the default 12-month window); charges AED 155 / USD 48 / EUR 14
  (217; no AUD charge exists). AUD/EUR transactions fall outside the default
  first page; the production SQL for 2026-02-24..2026-04-28 returns AED 81 /
  AUD 2 / USD 17 (SQL only, not exercised over HTTP).
- Gates: `cargo test --workspace -q` 229 passed; catalog 0 error / 2
  pre-existing warnings, validated; `./scripts/docs-check.sh` OK (29 routes,
  0 undocumented; `api-reference.md` lists no per-capability columns, so it
  is unchanged); `./scripts/acceptance-check.sh` 41/68 (LOAN-6/7 are
  dataset-inventory IDs, not counted there). Clippy not re-run: no Rust
  changed. No layer status in §5 changes; FIN-108/L1C/L2 are not claimed
  complete. FIN-155 stays blocked by FIN-161/full catalog work, not only by
  this column.

Update after FIN-124 (D14 scheduler read surface + the first enforced freshness
hand-off; **published for review; D14 is NOT complete and the inventory row
stays `gap`; the ticket is not closed here**):

- **Owner decision (recorded separately).** Tenant-wide operational access for D14
  only — an authenticated tenant admin may read approved job/run fields through a
  dedicated `operational_tenant_wide` declaration, **not** `org_wide_reference`, no
  invented office filter, no raw errors, no write — is recorded in
  [product/2026-10-15-d14-operational-access-decision.md](product/2026-10-15-d14-operational-access-decision.md).
  No other contract document was edited to fit code.
- **Implemented scope (what is actually built).**
  1. `scheduler_job_definitions` — job grain: name, cron, active, run counts in an
     explicit window, `window_observation`, the job's latest run overall.
  2. `scheduler_job_run_history` — run grain for ONE job chosen through the approved
     `scheduler.jobs` resolver (K1, no free-text job name): every attempt, success
     and failed, oldest first (ties by run id), `is_job_latest_run` and the job's
     latest status (unfiltered) per row. The resolver mechanism needed one change:
     a resolver manifest with no `authorized_scope` parameter (tenant-level data)
     now binds no office list (`ResolverSlot.scoped`).
  3. `scheduler_job_failed_runs` — failed runs across jobs, newest first.
  4. Windows are `[from_date 00:00:00, to_date + 1 day 00:00:00)` on the stored
     **timezone-naive** clock; no conversion, no calendar words. `error_message` /
     `error_log` are never read; a read that also asks to re-run a job is refused
     whole and states nothing was triggered.
  5. Freshness hand-off for `loan_arrears` (LOAN-9) only. `Complete` requires the
     required batch (`Loan COB`) to be observed AND its latest run recorded as
     `success` AND no in-scope loan behind the COB date. Otherwise `Partial`
     (`batch_coverage_incomplete`, `batch_latest_run_failed`) or `Unknown`
     (`batch_run_not_observed`, `batch_latest_status_unrecognized`,
     `batch_job_not_registered`, `cob_date_unavailable`, `coverage_probe_failed`,
     `freshness_probe_failed`). An absent or unfamiliar status is never read as
     success. One function (`compose::lowered_claim`) feeds node ledger, dataset
     handle and response; the validator rejects a document claiming better.
- **Source investigation (replaces the earlier "owner blocker" list).** A missing
  mapping was investigated first, not escalated. Findings (Fineract source + every
  `queries/*/*.sql`; detail in `knowledge/VERIFICATION.md` §16):
  1. Savings interest posting — `interest_posted_till_date` is the transaction date
     of the latest non-reversed interest-posting transaction; the posting batch
     re-selects every active account with a NULL or `<= business date - 1` value on
     each run. It is a recorded fact, **not** a coverage marker, so "posted through
     the business date" is **not derivable from account data**. No implemented query
     reads a posting-dependent column, so no capability gets a freshness claim, and
     recorded balances (`savings_balance_summary`) are not lowered. A question about
     posting freshness is answered with run observations only (every scheduler
     response states a run proves no business result) and stays a declared gap intent.
  2. Provisioning — D07 tables have 0 rows and no capability exists.
  3. The only batch-written account facts any implemented query reads are
     `m_loan_arrears_aging` / `last_closed_business_date` (`loan_arrears`); RD overdue
     values are written by transactions, not by a scheduler job. So `loan_arrears`
     is the sole hand-off required today. `Unknown`-vs-`Partial` needed no new owner
     policy: database-design I4, responses §3, dataset-inventory XR-ASOF and
     dataset-lifecycle already define it (known incomplete → `Partial`, unknown →
     `Unknown` with a deterministic reason).
- **Stored-clock windows (owner decision 2026-10-15).** Every `operational_tenant_wide`
  query with a `from_date` is gated in the planner by
  `param_parse::stored_clock_window_issue` on **positive evidence**, not a word list:
  the request must state the stored clock AND an explicit month range or ISO date
  range (the same parsers the grounded date binding uses), with no relative period
  beside it; otherwise it ends `stored_clock_window_required` (no plan, no source
  query). An unlisted calendar word or timezone name without an explicit window is
  therefore refused too. The earlier month-range proof did not state the stored
  clock and was re-run with it.
- **Freshness probe budget (review finding).** The two `loan_arrears` freshness probes
  are source queries and are now charged to the job's absolute `query_count` in T4
  (`NodeOutcome.extra_queries`, `Claim.queries_issued`: 1 batch probe, +1 coverage
  probe only when a COB date was read), recorded in the node `provenance_json`
  (`freshness.queries_issued`, `freshness.probe_queries`). Proof: arrears job
  `eb1cd242-3656-40b8-8ef6-35f827302bd6` has `query_count = 3` (primary + batch +
  coverage), `model_call_count = 1`, probes
  `scheduler.loan_cob_batch_evidence`, `loan.loan_cob_coverage`. Resolver option
  probes are not counted by the existing code (unchanged, `query_count` is only
  incremented at T4/reaper); the code has no `BUDGET_MAX_QUERIES` enforcement at all
  (documented in runtime.md, no config/check exists), so there is no cap to hook —
  a no-budget `Unknown` fallback is not applicable. Fencing is unchanged (probes run
  before T4, which stays lease-fenced). The unscoped (no `authorized_scope`) resolver
  is now guarded by catalog rule `resolver_unscoped_requires_operational`: it is
  accepted only for a Fineract query declaring `operational_tenant_wide`, whose tables
  the existing rule restricts to `operational_tenant_tables`.
- **Original question D over HTTP (Indonesian, one real-model request).** "Does the
  latest successful Post Interest For Savings run prove August interest and every
  account current?" → job `963d15e5-3b03-4684-bcc8-54584b5537b6`: the real model
  returned a valid `no_fit`; the response is `limitation` / `Unknown` /
  `planner_no_fit`, no rows, no lineage, and the `no_fit` text now states explicitly that
  a request combining readable run observations with a coverage claim no approved
  capability can establish is not answered from the supported part. No run list was
  presented as an answer. Run-history/definition SQL proof is unchanged.
- **English-only FIN-124 test inputs (owner instruction).** Scheduler HTTP prompts,
  test names/descriptions and the new D14 pure-regression inputs now use English.
  A targeted real-model run through the locked runner passed both English cases:
  "last night on the stored clock" (`58f1c405-39a7-4010-860a-469c58b01930`) and
  the August/all-account coverage question (`3d25f972-267e-4f7e-8299-67d5da1dcac8`).
  Both settled `Failed` / `OperationalFailure` / `Unknown` with `planner_no_fit`
  and zero source queries: safe refusal, not a completed coverage analysis.
  The run used 108 HTTP requests (6 logical, 102 response polls), 4 model calls
  (1 clock, 3 coverage), and passed 2/2 tests. The three changed pure regressions
  passed individually. Evidence: `/tmp/fin124-english-refusals.out`.
- **Scope of D14 as recorded (no owner question needed for this).** Implemented
  reads exist, so D14 is recorded with its actual limits: run observations on the
  stored clock; one enforced freshness hand-off (`loan_arrears`); savings
  interest-posting and provisioning coverage have no account-level source (section
  above). Genuine remaining limits: undeclared storage timezone (only stored-clock
  windows, owner-approved), no interest-job failure and no cross-midnight fixture.
  L2 and FIN-163 are not claimed complete.
- **Remaining (D14 row stays `gap` as a status choice).** The `job_run_history` storage
  timezone is undeclared (tenant `Asia/Beirut` vs a 5 h 30 min cron offset). Whether D14 may move to `inherited` for the
  implemented read surface plus the single `loan_arrears` hand-off, and whether to
  fold the operational-access decision into `analytical-contracts.md` §6, are owner
  decisions.
- **Planner diagnosis, 2026-10-15 (recovery + publication run).** Two latest
  real-model failures were diagnosed from the stored audit rows plus a throwaway
  proposal log (removed; no telemetry kept). (a) Failed runs: attempt 1 quoted
  "between April 2026 and May 2026", which is not in the request
  (`source_quote_not_found`, repaired); attempt 2 quoted "April 2026" and
  "May 2026" for the two bounds — grounded in the request, but the verifier only
  parsed whole month *ranges*, so a single-month slot phrase was rejected as
  `static_verification_rejected`. Fix (the demonstrated cause only): a date slot
  whose quote is exactly one month and one year now binds that month
  (`param_parse::parse_month_phrase`, slot verification only; `parse_period` and
  the request-level "explicit period" check are unchanged, no verifier or
  assertion relaxed). (b) Run history: the same request produced, across runs, a
  valid high-confidence `no_fit`, repeated 30 s provider timeouts
  (`planner_unavailable`; calls take 21–30 s of mostly reasoning tokens against
  `LLM_TIMEOUT_MS=30000`), one `static_verification_rejected` whose proposal text
  was not captured, and — in a bounded diagnostic — a correct `planned` proposal
  (correct capability, "April 2026 to May 2026" for both bounds, identity probe).
  The `no_fit` and the uncaptured rejection are **not** explained by any
  demonstrated code or catalog defect and no catalog/selection/retry change was
  made for them: they remain provider-reliability limits (the timeout is an
  operator setting and was not changed). Scoped final run
  (`answers/scheduler`, 256 requests, 32/32 tests, `/tmp/fin124-pub-scheduler2.out`)
  passed with the real model: run history `7dab742e-…`, definitions
  `1d11426b-…`, failed runs `6cd0e8f8-…` (each Answered/Complete, row-for-row
  equal to independent SQL), raw-error and re-run refusals `BlockedByPolicy`,
  office/clock/coverage refusals `Unknown`. The first scoped run of the same code
  failed at the run-history form (`f474d4ea-…`: one timeout, then
  `static_verification_rejected`) and is retained as failed evidence; the passing
  run is a second run, not a claim that the planner is reliable. Failed-runs now
  has a tracked chain (`answers/scheduler/failed-runs-*.yml`).
- **Proof (locked runner, port 3324, DB `jarvis_wt_fin_124_scheduler`, real
  `qwen-cloud`/`qwen3.8-flash`).** Chains 1, 2, 4 re-run at the final knowledge hash
  `3671aaa3…` with explicit stored-clock requests; chain 3 (`loan_arrears`, hash
  `5ffecef8…`) is unchanged and was not re-run. Four chains vs independent SQL
  (params, scope, window, status, order, limit):
  1. run history of job 12 "Apply penalty to overdue loans", April–May 2026 on the stored clock, via
     the clarification flow (job chosen from the resolver, 44 candidates): job
     `e33921c9-771a-4a06-bbdd-15938eaaaceb`, 33 rows = oracle 33 — 9 failed
     (2026-04-01..04-09) then 24 success (04-10..05-03), chronological, latest
     overall `success` flagged on the last row. No interest-posting job has ever
     failed in this fixture, so this is the mechanism proof, not an interest claim;
  2. job definitions, "December 2025 to December 2025 on the stored clock": `c5aaa101-e173-42cb-9a48-e767c4dc5ef7` (a first attempt, `d2da2a83-…`, was `planner_output_invalid` — model output, nothing executed),
     44 = 44, job 36 = 44,640 (both half-open edges), job 42 zero runs
     `no_observed_run`;
  3. `loan_arrears`: `8d6465fb-1c2a-4dec-96ee-15181cd96087`, 59 = 59,
     `Partial`/`batch_coverage_incomplete` on ledger, dataset and response; 87 of 87
     loans behind COB 2026-09-07 while the latest Loan COB run (2026-05-03) is a
     `success`;
  4. refusals: raw error/stack `a89300e2-…` (`surface_not_approved`), read + re-run
     `1cf5bab7-…` (`write_not_supported`, nothing triggered), office-specific
     `b8e94681-…` (real model: `planner_no_fit`; the
     `operational_office_unavailable` guard is a unit test, not HTTP), and calendar word
     "last night (semalam)" `7ab9569e-…` (real model planned; planner guard
     `stored_clock_window_required`, no plan, no source query).
  Counts (final runs): run 1 = 44 requests executed (34 `404` response polls, 10 others
  = login, session, job POST, state reads, clarification form/options/answer, 2 answer
  reads; run history 14/14 tests, then stopped on the definitions `planner_output_invalid`
  bail); run 2 = 62 requests (45 `404` polls, 17 logical incl. login/sessions), 15/15
  tests; real-model calls (distinct `model_call_id`) 6: run history 2, definitions 2
  (one invalid output, one answered), office 1, calendar word 1; the write and raw-error
  refusals are deterministic (0). The earlier `scheduler_job_failed_runs` HTTP proof
  (job `bae2e8d6-…`, 90 = 90, hash `2d9ee04f…`) was NOT re-run at the final hash.
- **Gates.** `cargo clippy --workspace -- -D warnings` clean; `cargo test
  --workspace` 200 + 41 passed; catalog 0 error / 2 pre-existing warnings, validated;
  `docs-check.sh` and `acceptance-check.sh` OK; Graphify updated (2,708 nodes).
  The stale `engine/policy-deferred-scheduler-*` Bruno pair (old `domain_gap`) was
  removed because the scheduler domain is approved for what is implemented;
  `loan_arrears-answer.yml` expects `Partial` on this fixture. L2 is not claimed.

**Readiness review and checkout recovery, 2026-10-15 (uncommitted; publication held):**

- The FIN-124 directory, Git worktree registration and local branch were absent at
  session start. The isolated database and historical reports survived. With owner
  approval, 73 files were reconstructed from successful local transcript operations,
  including the decision record, and the two obsolete scheduler `domain_gap` tests
  were removed again. The same worktree path and Linear branch were restored at
  `b798b84`; no implementation redesign, commit, push, PR, merge or ticket closure.
  Recovery snapshot and per-file hashes: `/tmp/fin124-recovery-20261015-source/`.
- Recovered YAML/SQL catalog bytes reproduce the retained content hash exactly:
  `3671aaa3ce24626322fa73772a28b0a522cefcb608d06bb714ab90076a2f4cfd`.
  This hash does not cover Rust/tests/docs. Compilation and runtime checks below
  separately exercise the reconstructed code. The deleted worktree's original
  `.env` and unrecorded untracked files cannot be claimed recovered byte-for-byte:
  bootstrap copied current main configuration and retargeted it through
  `jarvis-db.sh` to the surviving `jarvis_wt_fin_124_scheduler`, without dropping it.
- New verification: `cargo test --workspace` 241 passed; clippy with `-D warnings`
  passed; catalog 0 errors / 2 existing warnings. All new smoke inputs were English,
  used the real `qwen-cloud` / `qwen3.8-flash`, and ran through the locked runner on
  port 3324. No full collection, transaction/charge or outage-control run.

| Recovery run | Executed HTTP requests | 404 polls | Other requests | Observed result |
| --- | --- | --- | --- | --- |
| scheduler | 63 | 60 | 3 | Run-history clarification never opened because the model returned `no_fit`; script failed, 22 remaining requests skipped |
| scheduler-targeted | 52 | 45 | 7 | Definitions 9/9 tests passed; failed-runs smoke script failed on absent lineage after `planner_output_invalid`; 13 remaining requests skipped |
| freshness and refusals | 124 | 103 | 21 | 17/17 tests passed, including independent SQL row comparisons and I1 log check |

- Definitions job `24aad743-72ae-4453-80e2-0f6e9de09fae`: 44 jobs = independent
  SQL 44, job 36 has 44,640 December runs, boundary assertions passed, job 42
  remains `no_observed_run`. Arrears job `fbbb59be-1b53-4078-8001-fcb2c70b2fd9`:
  59 rows = independent SQL 59; 87/87 loans behind COB 2026-09-07 despite latest
  Loan COB `success`; node, dataset and response remain `Partial` /
  `batch_coverage_incomplete`, validator passed, `query_count=3`.
- **Current readiness is not green.** Run-history job
  `1490a21d-a3a6-469f-b7d3-1ea61bdb4660` ended `planner_no_fit`, 0 source queries.
  Its request bytes match the old successful `e33921c9…` request, and the correct
  capability is first in the reproduced shortlist. Failed-runs job
  `8d13b3a6-befb-418a-984a-430fec2214d0` had one unavailable-provider attempt,
  then a planned proposal rejected by the static verifier (`planner_output_invalid`),
  0 source queries. Neither is a new SQL result proof. Assertions, verifier and
  planner were not weakened or tuned per phrase to make these runs green.
- Raw-error and mixed read/re-run controls passed with `BlockedByPolicy` and no
  source/model calls. Office-specific, English last-night and English August /
  all-account coverage questions safely refused with `Unknown` / `planner_no_fit`,
  0 source queries; these are refusal proofs, not completed coverage analyses.
  Across all nine new jobs, job counters record ten model calls.
- Logs: `/tmp/fin124-recovery-{scheduler,scheduler-targeted,freshness}.out`;
  detailed current evidence is appended to `/tmp/jarvis-reports/FIN-124.md`.
  Throwaway failed-runs requests were removed from the collection and preserved
  under the recovery snapshot's `smoke/` directory. Existing historical evidence
  is retained, not promoted into a claim that the latest scheduler run passed.
  D14 remains `gap`; storage timezone, interest/provisioning failure fixtures,
  cross-midnight behavior and full query-budget enforcement remain unproven.
  FIN-163/FIN-161 being `Done` in Linear is not full L2 conformance; no tracker
  state was changed. After FIN-124 is ready, FIN-115 is the first remaining
  Wave-1 child in FIN-107's ordering with FIN-152/153 satisfied; it is not started.

**Update after FIN-110 (fixed and recurring deposits: identity, terms, funding, contributions, and shortfall):**

- Delivered 5 curated Mode-1 capabilities for domain `fixed_recurring_deposit` (FDRD-1..10): `fd_rd_accounts`, `fd_rd_term_preclosure`, `fd_rd_funding`, `rd_contributions`, `rd_shortfall`.
- Live `fineract_default` database facts: 36 FD/RD accounts (`deposit_type_enum` 200 = 14 FD, 300 = 22 RD; savings 100 = 171). 36 terms rows (1:1 with FD/RD accounts). 22 recurring detail rows (1:1 with RD accounts). All have `is_calendar_inherited = false`. Initial funding deposits occur at `activatedon_date`.
- Underlying FDRD-9 schedule table source confirmed in Fineract: `m_mandatory_savings_schedule` (727 rows, all `deposit_type_enum = 300`, generated by `RecurringDepositAccount.java:1162`). Retained as explicit gap intent (`gap_intents`) for standalone schedule queries.
- HTTP proof (`answers/fd_rd`): 55 requests / 71 tests passed against independent SQL oracles for all authorized offices `{1,2,3,4,5,6,8,9}` and narrowed scope `[2, 3]`, plus empty population test in office 2 (`outcome = "Empty"`).
- Deterministic synthetic proof: `tests/fdrd_semantics_smoke.sh` shadows all tables with inline `VALUES` CTEs asserting 5 semantic checks (type filtering, projection and preclosure terms, funding reading, anti-fanout contributions, batch shortfall facts with NULL preservation).
- Gates: catalog validated with 0 errors; 210 `chat` unit tests passed; clippy clean; `docs-check.sh` and `acceptance-check.sh` passed.

**Update after FIN-111 (share accounts: ownership, purchase/redemption, charges, allocations, and dividends):**

- Delivered 5 curated Mode-1 capabilities for domain `share` (SHARE-1..8): `share_accounts`, `share_account_transactions`, `share_account_charges`, `share_charge_payment_allocations`, `share_account_dividends`.
- Live `fineract_default` database facts: 2 share accounts (`m_share_account`), 3 share transactions (`m_share_account_transactions`), 2 share charges (`m_share_account_charge`), 2 share charge payment allocations (`m_share_account_charge_paid_by`), 0 share dividends (`m_share_account_dividend_details`).
- Underlying settlement proof (SHARE-7): not recorded in Fineract data model; retained as explicit gap intent (`gap_intents`) for domain `share`.
- HTTP proof (`answers/share`): 33 requests / tests against independent SQL oracles for all authorized offices `{1,2,3,4,5,6,8,9}` and narrowed scope `[2, 3]`, plus empty population test in office 2 (`outcome = "Empty"`).
- Deterministic synthetic proof: `tests/share_semantics_smoke.sh` shadows all tables with inline `VALUES` CTEs asserting 5 semantic checks (account scope & status, transaction date & type, charges & NULL preservation, allocations anti-fanout & evidence IDs, dividends lifecycle & Empty outcome).
- Gates: catalog validated with 0 errors; 211 `chat` unit tests passed; clippy clean; `docs-check.sh` and `acceptance-check.sh` passed.

**Update after FIN-128 (LC.21 client/ownership full §20 expansion: groups, rosters, addresses):**

- Delivered 3 curated Mode-1 capabilities for domain `client` (closing CLI gap rows): `client_group_membership` (CLI-4), `client_account_roster` (CLI-5), `client_address` (CLI-6).
- Live `fineract_default` database facts: 3 client group memberships (`m_group_client`), 6 client accounts across loan, savings, and share types (`m_loan`, `m_savings_account`, `m_share_account`), 0 client addresses (`m_client_address`).
- Domain and inventory alignment: moved CLI-4, CLI-5, and CLI-6 to `supported_intents` in `knowledge/domains/client.yaml`; added `m_address` to `client-foundation.yaml`; added `CLI-4-accounts` in `docs/data/dataset-inventory.md`, `group_center.yaml`, and `group-center-foundation.yaml`; updated dataset inventory rows CLI-4, CLI-5, CLI-6 to `inherited`.
- Guard protection: resolved lexical stripping of uncontracted `group_center` candidate guard by defining multi-word synonyms under concept `group_membership` in `client.yaml`.
- HTTP proof (`answers/client_ownership`): 25 requests / 27 tests passed against independent SQL oracles for all authorized offices `{1,2,3,4,5,6,8,9}` and narrowed scope `[2, 3]`, plus empty population tests (`outcome = "Empty"`).
- Deterministic synthetic proof: `tests/client_semantics_smoke.sh` shadows all tables with inline `VALUES` CTEs asserting 5 semantic checks (N:M group membership, center linkage NULL-safety, roster multi-type & no default-active filter, address details, and empty office scope semantics).
- Gates: catalog validated with 0 errors, 2 pre-existing warnings; 253 workspace unit tests passed; clippy clean; `docs-check.sh` and `acceptance-check.sh` passed.

**Update after FIN-127 (savings: full §22 scope expansion, closing SAV-1..11):**

- Delivered 7 curated Mode-1 capabilities for domain `savings` (SAV-1..11): `savings_account_lifecycle`, `savings_interest_position`, `savings_account_holds`, `savings_withheld_tax`, `savings_linked_accounts`, `savings_account_balance_detail`, `savings_transactions_by_type`.
- Live `fineract_default` database facts: 187 active/submitted savings accounts, 15,607 savings transactions, 23 active hold/release rows, 90 portfolio account associations (`m_portfolio_account_associations`), 0 tax-withheld savings accounts (tested as genuine Empty outcome with Complete completeness).
- Multi-currency safe: transaction breakdown groups strictly by `[currency_code, transaction_type_enum]` preventing multi-currency rollups without exchange rate conversion (#14).
- HTTP proof (`answers/savings`): 42 new requests / tests against independent SQL oracles for all authorized offices `{1,2,3,4,5,6,8,9}` and narrowed scope `[2, 3]`, plus empty population tests (`outcome = "Empty"`).
- Deterministic synthetic proof: `tests/savings_semantics_smoke.sh` shadows all tables with inline `VALUES` CTEs asserting 7 semantic checks (lifecycle scope & sub-status mapping, interest earned vs posted, holds and releases, withheld tax Empty outcome, linked accounts associations, balance detail components, and transactions by type grouping).
- Gates: catalog validated with 0 errors; `chat` unit tests passed; clippy clean; `docs-check.sh` and `acceptance-check.sh` passed.

**Update after FIN-130 (resource penghubung: full §25 scope expansion, closing LINK-3..8):**

- Delivered 6 curated Mode-1 capabilities for domain `linking_resources` (`knowledge/domains/linking_resources.yaml`): `account_association_links` (LINK-3), `account_transfer_transactions` (LINK-4), `loan_payment_allocation` (LINK-5), `charge_settlement_evidence` (LINK-6), `transaction_reversals` (LINK-7), and `payment_type_resolution` (LINK-8).
- Domain and inventory alignment: transitioned domain `linking_resources` from `status: gap` to `status: approved_mvp`; moved LINK-3..8 to `inherited` in `docs/data/dataset-inventory.md`; standing instructions (D05 / FIN-114) remain excluded/deferred.
- Live `fineract_default` database facts: 90 portfolio account associations (`m_portfolio_account_associations`, all type 1), 498 transfer details (`m_account_transfer_details`), 728 transfer transactions (`m_account_transfer_transaction`), 165 savings charge paid-by rows (`m_savings_account_charge_paid_by`; the loan side has no paid-by table — `m_loan_charge.amount_paid_derived` is a derived total, LOAN-7), 489 payment details (`m_payment_detail`), 88 reversed loan transactions and 6,788 reversed/reversal savings transactions (share reversals absent from the Fineract data model; GL reversal linkage delivered via `acc_gl_journal_entry.reversed`/`reversal_id` without claiming D12).
- Boundaries and constraints: `$1::bigint[]` authorized office scope strictly bound on all queries; hard limit cap 200 enforced with population disclosure; PII (account numbers, routing/check numbers, external IDs, client names) strictly excluded; accounting boundary (D12) shielded from general ledger reversals.
- HTTP proof (`answers/linking_resources`): 182 requests / 96 tests, 100% PASS (locked, re-run by the coordinator) against independent SQL oracles for all authorized offices `{1,2,3,4,5,6,8,9}` and narrowed scope `[2, 3]`, plus genuine Empty population tests in office 9 (`outcome = "Empty"`) and zero commit-isolation violations (OVR-6.7).
- Deterministic synthetic proof: `tests/linking_resources_semantics_smoke.sh` shadows all tables with inline `VALUES` CTEs asserting 10 semantic checks.
- Gates: catalog validated with 0 errors; workspace unit tests passed; clippy clean; `docs-check.sh` and `acceptance-check.sh` passed.

**Update after FIN-112 (GL traceability: D12 journal & saldo satu GL account):**

- Delivered 3 curated Mode-1 capabilities for domain `accounting` (`knowledge/domains/accounting.yaml`): `gl_journal_for_account`, `gl_journal_for_transaction`, and `gl_account_balance`.
- Domain and inventory alignment: transitioned domain `accounting` from `status: gap` to `status: approved_mvp`; moved D12 (`acc_gl_journal_entry`, `acc_gl_account`) to `inherited (FIN-112)` in `docs/data/dataset-inventory.md` with 3 capabilities registered; data area `accounting-gl` updated to `status: included_mvp_domain`.
- Boundaries and constraints: Trial balance, balance sheet, and profit-and-loss reports are explicitly and permanently out of scope per D12 boundary (retained under `unsupported_intents` only). All queries bind `$1::bigint[]` authorized office scope. Hard limit cap 200 enforced with population disclosure. Account numbers and external IDs excluded.
- Anti-double-count reversal rule (§31): `gl_account_balance` debit, credit, and balance sums strictly exclude both reversed entries (`reversed = true`) and paired reversal entries (`EXISTS (SELECT 1 FROM acc_gl_journal_entry rev WHERE rev.reversal_id = gl.id)`), while surfacing their counts explicitly. Sums are strictly grouped by `(office_id, currency_code)` with no cross-currency consolidation.
- Engine enhancement: Added `Bound::NullDate` support in `chat::engine::planner`, `executor`, and `compose` so optional date parameters without defaults bind `Option::<NaiveDate>::None` cleanly without failing.
- HTTP proof (`answers/accounting`): 35 requests / 48 tests, 100% PASS against independent SQL oracles for all authorized offices `{1,2,3,4,5,6,8,9}` and narrowed scope `[2, 3]`, plus empty population tests in office 9 (`outcome = "Empty"`) and zero commit-isolation violations (OVR-6.7).
- Deterministic synthetic proof: `tests/accounting_semantics_smoke.sh` shadows all tables with inline `VALUES` CTEs asserting 9 semantic criteria.
- Gates: catalog validated with 0 errors; workspace unit tests passed; clippy clean; `docs-check.sh` and `acceptance-check.sh` passed.
- Coordinator boundary re-proof (2026-10-24, follow-up commit): the first delivery left subject filters optional in all
  three SQLs, so an unnamed request returned the whole-ledger recap (23 office × currency balance rows) that D12
  excludes; fixed with SQL named-subject guards (zero rows without a subject), `named_subject_required` /
  `transaction_subject_required` manifest checks, plural supported-intents removed from `gl_account_balance`, and smoke
  grown 9 → 12 assertions (ACC-10..12). Re-proof: locked Bruno 51 requests / 48 tests PASS with 0 commit-isolation
  violations; catalog 0 errors; populations re-verified by direct SQL (`acc_gl_journal_entry` 37,077,
  `acc_gl_account` 362, `acc_product_mapping` 259, `acc_gl_closure` 1, `acc_accounting_rule` 0); reversal pointer
  direction pinned from Fineract source (`revertJournalEntry` sets the ORIGINAL's `reversal_id` to the new reversal
  entry, `JournalEntryWritePlatformServiceJpaRepositoryImpl:428-431`).

**Update after FIN-114 (Standing instructions: D05 config, status, schedule, execution attempts, failures):**

- Delivered 4 curated Mode-1 capabilities for domain `standing_instructions` (`knowledge/domains/standing_instructions.yaml`): `standing_instruction_config`, `standing_instruction_schedule`, `standing_instruction_executions`, and `standing_instruction_failures`.
- Domain and inventory alignment: transitioned domain `standing_instructions` from `status: gap` to `status: approved_mvp`; moved D05 (`m_account_transfer_standing_instructions`, `m_account_transfer_standing_instructions_history`) to `inherited (FIN-114)` in `docs/data/dataset-inventory.md` with 4 capabilities registered; data area `standing-instructions` created with `status: included_mvp_domain`.
- Boundaries and constraints: Write operations (create/update/delete standing instructions), manual trigger/run execution, standing instruction template discovery, and full transaction mapping across all legs are permanently excluded per D05 boundary (retained under `unsupported_intents`). All queries bind `$1::bigint[]` authorized office scope across `((atd.from_office_id IS NOT NULL AND atd.from_office_id = ANY($1)) OR (atd.to_office_id IS NOT NULL AND atd.to_office_id = ANY($1)))`. Hard limit cap 200 enforced with population disclosure. Account numbers and external IDs excluded.
- Semantics: DELETED instructions (`status = 3`) are strictly excluded from all 4 capabilities. Recurrence schedule surfaced verbatim with no next-run computation and no date arithmetic. Failure claims strictly trace to `history.status = 'failed'`; absence of execution or transfer is an actual Empty outcome, never presumed failure.
- Synthetic proof: `tests/standing_instructions_semantics_smoke.sh` shadows all tables with inline `VALUES` CTEs asserting 10 semantic criteria (exclusion of DELETED status, NULL to_office_id visibility, recurrence_frequency NULL preservation, failure status filtering, anti-fanout, office authorization boundary, and currency resolution from savings/loan legs without cross-currency conversion).
- HTTP proof (`answers/standing_instructions`): 47 requests / 61 tests, 100% PASS against independent SQL oracles for all authorized offices `{1,2,3,4,5,6,8,9}` and narrowed scope `[2, 3]`, plus empty population tests in office 9 (`outcome = "Empty"`) and zero commit-isolation violations (OVR-6.7).
- Gates: catalog validated with 0 errors; workspace unit tests passed; clippy clean; `docs-check.sh` and `acceptance-check.sh` passed.

**Update after FIN-116 (Actual provisioning results: D07 recorded runs and entry details):**

- Delivered 2 curated Mode-1 capabilities for domain `provisioning` (`knowledge/domains/provisioning.yaml`): `provisioning_run_history` (grain: `[history_id]`) and `provisioning_entries` (grain: `[entry_id]`).
- Domain and inventory alignment: transitioned domain `provisioning` from `status: gap` to `status: approved_mvp`; moved D07 (`m_provisioning_history`, `m_loanproduct_provisioning_entry`) to `inherited (FIN-116)` in `docs/data/dataset-inventory.md` (61→62 inherited, 31→30 gap); data area `provisioning` created with `status: included_mvp_domain` in `knowledge/data-scope/areas/provisioning.yaml`.
- Boundaries and constraints: Simulation and projection of provisioning, multiplying current criteria % by historical loan outstanding (D07's own rule), loan-level contributor synthesis (no `loan_id` column on entry), and full-GL approval are permanently excluded (retained under `unsupported_intents`). `m_provisioning_history` declared as org-wide reference (`org_wide_reference: true`). `provisioning_entries` binds authorized office scope via `e.office_id = ANY($1::bigint[])`. Schema misspelling `reseve_amount` aliased to `reserve_amount` and never leaked into the public API.
- Semantics: Both tables are genuinely empty (0 rows) on this deployment fixture. The HTTP response is an actual `Empty` outcome (0 rows) with `completeness: Complete`. Nonzero semantics are proven synthetically via VALUES-only production-SQL smoke.
- Synthetic proof: `tests/provisioning_semantics_smoke.sh` shadows underlying tables with inline `VALUES` CTEs asserting 8 semantic criteria (run history date window filtering, journal_entry_created flag preservation, entry join to history and criteria IDs, office authorization scoping, entry date window filtering, currency separation without cross-currency conversion, reserve_amount alias correction, and boundary safeguards).
- HTTP proof (`answers/provisioning`): 22 requests / 21 tests, 100% PASS (coordinator re-run) against independent SQL oracles for all authorized offices `{1,2,3,4,5,6,8,9}` and narrowed scope `[2, 3]` (port 3316), and zero commit-isolation violations (OVR-6.7).
- Gates: catalog validated with 0 errors; workspace unit tests passed; clippy clean; `docs-check.sh` and `acceptance-check.sh` passed (41/68 scenario-reference inventory unchanged).

**Update after FIN-118 (Fineract user actions / source audit: D09 actor, action, resource, time):**

- Delivered 2 curated Mode-1 capabilities for domain `source_audit` (`knowledge/domains/audit.yaml`): `command_source_actions` (grain: `[id]`) and `command_source_trace` (grain: `[id]`).
- Domain and inventory alignment: transitioned domain `source_audit` (`knowledge/domains/audit.yaml`) from `status: gap` to `status: approved_mvp`; moved D09 (`m_portfolio_command_source`) to `inherited (FIN-118)` in `docs/data/dataset-inventory.md` (62→63 inherited, 30→29 gap); data area `source-audit` updated to `status: included_mvp_domain` with `included_tables: [m_portfolio_command_source]` in `knowledge/data-scope/areas/source-audit.yaml`.
- Boundaries and constraints: Actor attribution is strictly derived from `maker_id` (`m_appuser`), never from officer assignment tables (`m_staff`). Full command JSON payloads (`command_as_json`), raw execution results (`result`), client IP addresses (`client_ip`), API URLs (`api_get_url`), and external identifiers (`*_external_id`) are never selected or returned. Both queries bind `$1::bigint[]` authorized office scope via `cs.office_id = ANY($1::bigint[])`. Commands with `office_id` NULL (tenant-wide actions, 2,579 rows) are excluded by convention under office scope. Free-text reason justification, maker-checker rejection/cancellation tracking, historical staff assignment, and action rollbacks are permanently excluded per D09 boundary (retained under `unsupported_intents`).
- Semantics: In live Fineract data (`4,456` total rows), `made_on_date` is 100% NULL; timestamps are stored in `made_on_date_utc` (`2025-11-10` to `2026-09-08`). Inventory's mention of `created_date` was documented schema drift. `command_source_trace` enforces a named-subject guard requiring at least one business resource (`client_id`, `loan_id`, `savings_account_id`, `group_id`, or `resource_id`) and returns 0 rows (`Empty` outcome) if no subject is provided.
- Synthetic proof: `tests/source_audit_semantics_smoke.sh` shadows underlying tables with inline `VALUES` CTEs asserting 10 semantic criteria (office scoping, maker/checker username join to `m_appuser`, date window filtering, named-subject guard returning 0 rows when all subjects are NULL, single-subject trace filtering, entity/action/status filtering, and exclusion of forbidden payload columns).
- HTTP proof (`answers/source_audit`): 21 requests / 30 tests, 100% PASS (coordinator re-run) against independent SQL oracles for all authorized offices `{1,2,3,4,5,6,8,9}` and narrowed scope `[2, 3]` (port 3318), verifying population cap (1877 total, 200 returned), savings account 37 trace (16 rows in office 4, 0 rows in narrowed offices 2 and 3), and zero commit-isolation violations (OVR-6.7).
- Gates: catalog validated with 0 errors; workspace unit tests passed; clippy clean; `docs-check.sh` and `acceptance-check.sh` passed.


### 5.1 Scenario coverage

Every acceptance scenario now carries a stable ID, added in place without
touching a word of the scenario text. `./scripts/acceptance-check.sh` collects
them from `docs/`, collects the IDs named by tests (Bruno `.yml` and Rust), and
fails when a layer marked ✅ in the table above still has a scenario without a
test.

Latest inventory run (FIN-163 working branch) — **41 of 68 scenarios have a test**, not necessarily a passing or complete conformance proof:

| Prefix | Document | Scenarios | With a test | Owning layer |
| --- | --- | --- | --- | --- |
| `API-` | [contracts/api.md](contracts/api.md) | 6 | 6 | L0 |
| `SSE-` | [contracts/sse.md](contracts/sse.md) | 8 | 8 | L0 |
| `PLN-` | [architecture/engine.md](architecture/engine.md#planner-acceptance-scenarios) | 9 | 5 | L2 |
| `DS-` | [data/dataset-lifecycle.md](data/dataset-lifecycle.md) | 6 | 6 | L3 |
| `OVR-` | [architecture/overview.md](architecture/overview.md) | 7 | 6 | L4 (OVR-6.2 → L8) |
| `RESP-` | [contracts/responses.md](contracts/responses.md) | 10 | 10 | L5, L6 |
| `CLR-` | [contracts/clarifications.md](contracts/clarifications.md) | 8 | 0 | L7 |
| `MEM-` | [architecture/memory-context.md](architecture/memory-context.md) | 7 | 0 | L7 |
| `AC-` | [data/analytical-contracts.md](data/analytical-contracts.md) | 7 | 0 | L8 |
| | **Total** | **68** | **41** | |

API and SSE tests are Bruno requests (PR #1). SSE-5..8 each carry a test but
their tickets (FIN-25..28) stay In Progress — e.g. SSE-5's second clarification
stage waits on CLR-3. DS-8.1..8.4 are proven by Bruno requests (`engine/`,
`dataset-capped/`); DS-8.5 is proven at the schema by `tests/schema_smoke.sql`
T13 plus unit tests; DS-8.6 is proven at the release predicate
(`ds_8_6_*`), the only path any eviction takes
(`crates/chat/src/engine/dataset/`). RESP is now complete at the unit level, but "has a test" is a coverage
figure, not a conformance one — `acceptance-check.sh` cannot tell whether the
test actually proves its scenario, and RESP-8.7/8.9 in particular prove
composition logic that no live path emitted until FIN-65/61/63 — both are now
proven at HTTP as well.

**The original corpus had 59 IDs, not the 86 stated in [§1](#rule-1--done-means-acceptance-scenarios-not-green-tests).** FIN-162 adds nine planner IDs, making the current total **68**.
For that original corpus, one ID was given per scenario as the document actually writes it — one bullet or
one numbered item. The 86 in §1 appears to count the clauses inside compound
bullets (`"Radio and searchable select yield identical single-choice semantics;
checkbox false is not missing."` is one bullet, two claims). Splitting those into
separate IDs would mean rewriting scenario text, which Rule 3 forbids. §1 has
been left untouched: correcting its table is the repository owner's decision.
The discrepancy is a difference in counting, not a missing scenario — no
scenario present in the eight documents is unmapped.

L1 owns no numbered acceptance scenario: its gate is the four rules in
`knowledge/CARRY-OVER.md`. L2 now owns `PLN-1`–`PLN-9` in the Engine planner
contract (FIN-162), replacing the old unnumbered retrieval gate. L1C's gate
remains the formal inventory plus per-domain verification in §3.

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

> Correction 2026-10-12: `no_capability_matched` is a symptom of the lexical
> stand-in, not a product category. Under the LLM planner the cases become: model
> abstained, model output failed Engine verification, model unavailable, or
> genuinely out of scope — each with its own outcome (contract open, FIN-162).

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

Embeddings (FIN-149): any `knowledge/` edit creates a new catalog version with
zero vectors until `app catalog --sync --embed` runs — `retrieval-vector`/
`retrieval-healthy` fail deterministically until it does. Locally this is a
manual step (`cargo run -p app -- catalog --sync --embed`);
`scripts/integration-test.sh` runs it automatically for its own
`retrieval-healthy` stage. It is safe to run repeatedly: `upsert_version` only
writes `knowledge_index` rows for a version it just created, so re-running the
backfill against an unchanged catalog does not re-spend the embedding API
budget.


Update for FIN-115 publication (L1C D06 teller/cashier + D15e inter-office cash;
**ticket remains open; D06/D15e inventory rows stay `gap`**):

- Delivered read contracts: `teller_cashier_assignments`, `cashier_cash_movements`,
  `cashier_recorded_cash_position`, and `inter_office_cash_transfers`. The live schema
  disproves the inventory's old "no cash-movement table" observation:
  `m_cashier_transactions` contains 49 allocation/settlement rows. Source correction
  and historical attempts are retained in `knowledge/VERIFICATION.md` §17; the
  owning inventory contract is not silently rewritten.
- Position is cumulative through `to_date`, with the position before `from_date`.
  Formula: allocation + cash in - cash out - settlement, per cashier and currency.
  Derived savings/loan/client cash requires explicit cash payment evidence and
  creator-staff attribution within assignment dates. Matching assignments are
  classified before aggregation: ambiguous events enter no amount, are disclosed,
  and withhold affected positions as NULL rather than duplicating money.
  Physical cash counts, reconciliation and discrepancies remain unsupported.
- Final changed-path proof used real `qwen-cloud/qwen3.8-flash` through the locked
  runner: 58 requests, 28 positive acceptance assertions plus one diagnostic
  observation (the runner displays 29/29). Position matched independent SQL for
  16 groups and 4 narrowed groups (`office_ids` [2,3]); assignments matched 5 and
  2 rows. Cashier 3 / USD is 0, cashier 2 / USD is 20800, and cashier 4 / USD is
  35970 and AED is 25000 through 2026-04-30. Earlier unchanged journal movement
  proof matched 31/2 rows; inter-office is proven only as Empty = SQL 0.
- Six VALUES-only production-SQL regression checks prove ambiguous/adjacent
  assignments, unauthorized-cashier exclusion, zero positions, NULL currency and
  unique groups. They read no Fineract table. Workspace unit tests, clippy,
  catalog validation, docs-check and acceptance-check passed. No numbered L1C
  scenario, full collection, full D06 or full L2 conformance is claimed.
- Completion revision (FIN-115): the original assignments wording failed before a model call because the six candidate
  contracts were serialized verbosely (~16.6 KB). Candidates are now encoded losslessly (column rows, no repeated null members);
  the shortlist, bound, limits, retries, timeout, prompt and verifier are unchanged and a genuinely oversized input is still
  refused. Real-model locked runs pass the original request and the position capability with rows equal to independent SQL
  (jobs 5fda7d49…, 04ca8a19…, 6c2eabff…, ae0b5f05…; 45 requests / 28 tests). A VALUES-only smoke (`tests/teller_cash_semantics_smoke.sh`) proves loan/client/journal 103-104/part-day/overlap/
  inter-office read semantics on unchanged production SQL; local real data for those paths is empty (inter-office is an actual
  Empty answer) and no nonzero real-data or full-domain claim is made. By owner decision the inventory rows D06 and D15e are now
  `inherited` for these source-supported Mode-1 capabilities only (physical cash count, reconciliation and discrepancy stay gap
  intents); this is not an L2 promotion. Earlier FIN-115 failures in this file stay as history. Details: `knowledge/VERIFICATION.md` §17.6.

### FIN-165 catalog reference repair

The D06 inventory promotion exposed five invalid gap references at catalog
startup (job-level blocker for every locked run). `D06-physical` now records
the still-unimplemented physical count, reconciliation and discrepancy scope as
its own gap row; the five domain gap intents cite it. D06 stays `inherited`
for recorded cash only; the validator is unchanged. Catalog validation: zero
errors (two pre-existing resolver warnings). HTTP proof: "Show physical cash
count." remains `Completed` + `Unsupported` with `plan_version = null`
(job `2b9213bc-0cc9-4391-8a00-8a4e611629d6`, 4 requests / 4 tests).

### FIN-166 deposit request-shape correction

The readiness probe for "Hi, show savings deposits for April 2026 by currency."
failed with a valid model `no_fit` (job `77cbc77d-4452-4ff8-9548-bbbbe01c90fa`,
one call, 25077 ms, retained) because `savings_deposit_total` declared
`grouping: none, output: scalar` while its approved query manifest declares
grain `[currency_code]` with currency output. Only that request_shape was
corrected to match the existing SQL — no prompt, retrieval, timeout, retry or
schema change. After the correction the same request planned on attempt 1
(job `12cc9d8c-7055-4a1a-a297-1eb91ede8425`, call 14939 ms, usage 2752 in /
712 out) and its three rows (AED 154 / 176248, EUR 8 / 4553, USD 60 / 15740,
1–30 April) matched an independently authored DBX SQL exactly. This does not
prove general planner reliability and does not promote L2.

### FIN-164 conversational assistant (this change)

Real-model readiness gate first: the original greeting baseline was reproduced
(job `bf4dde1a-…` → `planner_no_fit`, zero calls) and the plain deposit
positive passed (12 tests) before dependent work, with every failure retained.
The Engine now proposes one closed understanding envelope (intent + optional
FIN-162 analysis payload) through the same Rig client, admission/audit,
budgets and repair bounds; the original request is never rewritten. Analysis
branches are verified and executed exactly as before; social/help/goal-question/
honest-limitation turns settle a durable `conversation` response document with
zero analytical nodes and zero promoted session facts; `scope_help` maps to the
existing Unsupported limitation, and the write/PII/scope guards still run
before any model call (permanent `engine/assistant` Bruno scenarios: greeting,
help, thanks, goal, context, outside, write, mixed-rows-vs-SQL). The required
40-prompt / 120-observation real-model reliability sweep has no verified result
in the Linear records reviewed on 2026-10-19; FIN-164 remains incomplete.
No numbered L2/PLN or L7 scenario is
promoted; FIN-75 / FIN-88 / FIN-100 remain open and their behaviors are not
claimed — the assistant states their unavailability instead.

Readiness continuation, 2026-10-19 (base `607d020`, no application changes):

- One locked run on port 3364, checkout database `jarvis`, ran
  `engine/assistant/auth` and `engine/assistant/mixed`. Build and catalog preflight
  passed (91 capabilities, 94 manifests, zero errors / two existing warnings).
  The run executed five HTTP requests including two terminal-state polls:
  three of four tests passed; the response-read request was skipped after bail.
- The exact mixed April request produced job
  `47888ac8-b38c-44b5-9829-b60ac6836af6`, terminal `Failed` /
  `OperationalFailure` / `planner_output_invalid`, no plan. Durable audit records
  the effective application provider/model as `gemini/gemini-3.5-flash-lite`;
  no provider, model, endpoint, key, timeout, retry or verifier was changed.
  This is not the coding-agent model.
- Attempt 1 (`01a151da-bf32-71f5-a18f-83aa9e69f51b`) failed `invalid_schema`
  after 1,807 ms (2,754 input / 242 output tokens). Attempt 2
  (`01a151da-c64e-7611-b3b6-365187ae6ba0`) failed
  `static_verification_rejected` after 1,419 ms (2,770 input / 162 output).
  Provider reachability is proven; valid analytical planning is not.
- Independent read-only SQL reproduced April totals/counts:
  AED 176248 / 154, EUR 4553 / 8, USD 15740 / 60.
  No new HTTP analytical result exists to compare with those rows.
- Readiness remains blocked under `architecture/conversation.md` §Readiness
  and acceptance. No sweep, full suite, retry run or dependent feature build
  was performed. Existing failures and historical successful proofs remain valid
  only for their recorded revisions/configurations. Runtime log:
  `/tmp/jarvis-fin164-resume-20261019.log`; durable job/call IDs above survive
  loss of the local log.

### FIN-164 analytical slot protocol clarification, 2026-10-21 (unpublished)

- **Observed failure, not a parser bypass.** The initial eight-scenario retest
  passed seven scenarios; mixed job `f8a08c1e-25c0-4c96-bf69-d76318c078c9`
  exhausted four calls with `invalid_schema`, no plan and zero source queries.
  A diagnostic-only run (`e1378375-2963-4b90-9f8d-72cefae67bb8`) retained
  the actual rejected proposal: a date slot used `kind: date` with `value`;
  subsequent output used `date_phrase` but still carried the forbidden `value`.
  The unchanged closed parser correctly rejects both. Temporary completion
  capture was removed before the positive proofs.
- **Change.** `engine/llm.rs` now states all four existing slot encodings and
  their exact fields in the system instruction, including date phrases carrying
  only `kind` and `source`, and probe arguments carrying only `value` and
  `source`. Parameter types are not slot-kind names. The audited prompt version
  is `understanding-v3`; the schema remains version 1. There is no input
  normalization, alias, phrase/capability routing, verifier relaxation, new
  retry loop, provider/model switch or contract change.
- **Real-provider readiness now passes on this revision/configuration.**
  Locked runs on port 3364, database `jarvis`, used the same effective
  `gemini/gemini-3.5-flash-lite` and catalog `a969bb327b0ee38c6c1f09421a566868e0cd4934989692f66776b86645e92a82`.
  The exact mixed April request is job
  `1d7b203f-2c10-4c7a-acb5-75af995e6437`; the plain request
  "Show savings deposits for April 2026 by currency." is job
  `4148efbe-dd13-4a2f-adfe-66fb356bed27`. Both settle
  `Completed`/`Answered`/`Complete`, plan version 1, one successful first-attempt
  model call and one source query. Model durations are 2,811/2,674 ms; observed
  token totals are 3,120/3,118. Both served tables equal independently authored
  read-only SQL row-for-row for 1–30 April: AED amount 176248/count 154,
  EUR 4553/8, USD 15740/60. The existing mixed Bruno assertions failed before
  and pass after the instruction correction; parsing/grounding assertions were
  not changed.
- **Scoped regressions.** All eight exact canonical assistant requests pass
  once, plus the plain analytical request: 59 executed HTTP requests including
  polls, 51/51 tests. Canonical help uses "What can you help me with?".
  Six non-analytical model paths each use one successful first attempt and zero
  source queries; the write refusal uses no model call or source query.
  Existing `planner-invalid`, `planner-unavailable`, `planner-no-fit` and
  `planner-repair` controls pass, 34 requests / 32 tests; fixtures are negative
  proof only, never positive real-model evidence. Throwaway plain/help request
  files were removed; the permanent assistant chains remain unchanged.
- **Deterministic gates.** Workspace tests pass (208 chat + 41 foundation),
  clippy passes for all workspace targets with warnings denied, docs-check
  reports zero broken links and all 29 routes documented, and acceptance-check
  reports the unchanged 41/68 scenario-reference inventory. No new permanent
  prompt-wording or schema-copy test was added; the existing mixed HTTP chain
  supplies the failing-before/passing-after regression.
- **Limits.** This is readiness plus a single observation per canonical request,
  not the frozen 40-prompt/120-observation reliability gate, full collection or
  full FIN-164/L2 conformance. No layer is promoted and the ticket is not
  closed. No commit, push, PR or merge was performed. Local evidence:
  `/tmp/jarvis-fin164-schema-fix-20261021-130732/`; earlier failed evidence
  remains retained separately.

### FIN-164 targeted publication check, 2026-10-22

- The owner requested continuation and publication/merge without whole-application
  retesting. Existing 2026-10-21 canonical, failure-path and deterministic evidence
  above is retained, not rerun or represented as new evidence.
- One current locked run (`engine/assistant/auth`, `engine/assistant/mixed`,
  port 3364, checkout database `jarvis`) passed: six executed HTTP requests,
  five tests, zero I1 log violations. Job
  `362269f8-fd0a-43c7-8e4f-7cab8f4782dd` settled
  `Completed` / `Answered` / `Complete`, plan version 1, one source query.
  Model call `01a16152-5130-7363-9f1a-cd88b3b292ff` succeeded on attempt 1:
  1,845 ms, 2,907 input / 213 output tokens, effective
  `gemini/gemini-3.5-flash-lite`, prompt `understanding-v3`, schema 1,
  unchanged catalog hash `a969bb327b0ee38c6c1f09421a566868e0cd4934989692f66776b86645e92a82`.
- The served April rows match newly executed independent read-only SQL:
  AED amount 176248 / count 154, EUR 4553 / 8, USD 15740 / 60.
  This proves the current mixed-request path through Engine verification and
  execution, not merely a direct provider response.
- No further source fix, configuration switch, full collection or reliability
  sweep was needed for this check. The 40-prompt / 120-observation gate remains
  unproven; publication does not close FIN-164 or promote L2/L7.
  Runtime log: `/tmp/jarvis-fin164-live-20261022.log`.

### FIN-113 direct client charges (D04: assessments and payment allocations)

- **Delivered Mode-1 curated query capabilities**: `client_charge_assessments` and `client_charge_payment_allocations` under domain `client` and data area `client_charges` (`knowledge/data-scope/areas/client-charges.yaml`).
- **Semantic invariants enforced**:
  - Master vs Assessment: Reads operational client charge assessments (`m_client_charge`) and allocations (`m_client_charge_paid_by`), not charge masters (`m_charge`).
  - Office authorization: Client office authorization (`$1::bigint[]` filtering `c.office_id`) is strictly enforced; transaction office (`ccp.office_id`) is preserved as audit evidence, never used for authorization.
  - Derived amounts & non-zero semantics: Nullable derived amounts (`amount_outstanding_derived`, etc.) are left NULL when uncomputed, never coerced to 0.00.
  - Allocation grain: Allocations are at `allocation_id` grain (`ccp.id`), avoiding fan-out.
  - Currency separation: Multi-currency records are reported per row with explicit `currency_code`, never cross-currency aggregated.
  - Reversals: Reversals/waives are surfaced explicitly (`is_reversed`, `is_waived`), not filtered out silently.
  - PII exclusion: Client names, notes, and external identifiers excluded; only IDs and financial data returned.
- **Verification evidence**:
  - Direct SQL oracles in `tests/answers/client_charge_*.sql` computed on the local database (empty population = empty response verified row-for-row).
  - Values-only CTE test suite `tests/client_charge_semantics_smoke.sh` exercises non-zero semantics on identical SQL logic, verifying lack of fan-out, reversal surfacing, per-currency separation, and NULL preservation.
  - Locked Bruno integration runs (`answers/client_charges`) passed 21 requests, 28 tests (zero commit isolation violations).
  - Gap intent updated: `client.yaml` moved `client charge` from `gap_intents` to `supported_intents` as two capabilities; `policy-deferred-client-charge-job.yml` updated to assert remaining gap `client loan account roster` (CLI-5).
  - `dataset-inventory.md` row D04 updated to `inherited` (empty fixture wording mirroring D15e).
- L1C remains open; §5.1 scenario coverage remains 41/68 (no numbered scenario for D04).


