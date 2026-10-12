# WrenAI reference review — proposals (2026-10-12)

Status: **proposals only, not agreed contract.** Recorded at the owner's request
while Jarvis is still in development. Nothing here changes an existing contract,
acceptance scenario, or the build ladder; each proposal needs an owner decision
(and, where it touches a contract, its own commit to that contract) before work
starts. No ticket exists yet.

## Source reviewed

- [Canner/WrenAI](https://github.com/Canner/WrenAI) tag `0.29.2`,
  `wren-ai-service/src/web/v1/services/ask.py` and
  `wren-ai-service/src/pipelines/generation/intent_classification.py` — the last
  release that still ships its own question pipeline.
- WrenAI `main` (2026-10), `core/wren/src/wren/skills_content/usage/SKILL.md`
  and `docs/core/concepts/memory_system.md` — the current design, which delegates
  question handling to the host agent (Claude Code, Cursor, MCP clients).
- Licence: `core/`, `sdk/`, `skills/`, `examples/` Apache-2.0; `docs/` CC BY 4.0.
  Ideas may be reused; copying code would still need attribution per Apache-2.0.

## How WrenAI handles a question (0.29.2)

1. `historical_question` — reuse SQL from a near-identical, previously answered
   question.
2. `sql_pairs_retrieval` + `instructions_retrieval` — fetch similar example
   NL→SQL pairs and team rules.
3. `intent_classification` — one LLM call returns `TEXT_TO_SQL`, `GENERAL`
   (incomplete question), `MISLEADING_QUERY` (off-topic) or `USER_GUIDE`, plus a
   `rephrased_question` that rewrites a follow-up into a standalone question
   using prior turns.
4. `db_schema_retrieval` — no relevant table fails with `NO_RELEVANT_DATA`.
5. The model writes SQL; it is corrected up to three times and executed.

The decision to ask is made by the model **from text alone, before any data is
looked at**. There is no value-level disambiguation, no single-candidate
auto-bind, no choice form and no multi-stage clarification; `GENERAL` and
`MISLEADING_QUERY` simply end with an assistance message.

## Comparison with Jarvis

Jarvis's agreed design is ahead of WrenAI on clarification and follow-up and
stays authoritative:

| Need | WrenAI | Jarvis (owning doc) |
| --- | --- | --- |
| Ask only when needed | LLM guesses from text | Resolver cardinality after a bounded probe: 0 → refine search, 1 → auto-bind with disclosure, >1 → form ([carry-over K5](../migration/carry-over.md)) |
| Choose among candidates | — | Server-issued `single_choice` options, paginated resolver ([clarifications](../contracts/clarifications.md)) |
| Staged clarification | — | Data-dependent stages on the same job, `CLR-3` |
| Follow-up on a prior answer | LLM `rephrased_question` | Structured `session_memory` facts (`ResolvedEntity`, `ActiveScope`, `PriorResult`) ([memory-context](../architecture/memory-context.md)) |
| SQL authorship | Model writes governed SQL | Approved capabilities only (`AGENTS.md`); model never writes SQL |

## Proposals

Each proposal must keep the Jarvis invariants: approved-capability execution
only, no model-authored SQL, authorization never read from memory or cache, and
clarification still driven by resolver cardinality.

### P1 — Verified-question reuse (from `historical_question` / memory "store")

When a new request is near-identical to a previously **validated** request
(same capability, same resolved slots after deterministic parsing), reuse the
earlier routing decision (capability choice and slot mapping) instead of
re-deriving it. Only the routing is reused; the capability is executed again
against live data, authorization and office scope are rechecked, and any slot
whose resolver is not `Complete` still follows K5.

Open questions for the owner: scope of reuse (per session, per user, or
global — global memory is currently deferred); what counts as "validated"
(response `passed` only, or also explicit user confirmation as in WrenAI's
store-after-confirm rule); invalidation when the catalog version changes.

### P2 — Explicit non-data intent classes (from `intent_classification`)

Compare WrenAI's four classes with Jarvis's policy guards
(`write_not_supported`, `surface_not_approved`, `domain_gap`, …). Candidates for
an explicit, typed outcome rather than a failed routing: casual conversation
("hi"), questions about Jarvis itself ("what can you do?"), and questions about
the catalog ("what data do you have?"). The outcome would be a response from
approved blocks, without data execution. Classification may stay deterministic;
a model may only propose a class, never authorize one.

### P3 — Confirmation signal for memory promotion (from memory "store" rules)

WrenAI stores a question/answer pair when the user confirms it or implicitly
accepts it by continuing with a follow-up, and never when the user says it is
wrong. Jarvis already promotes facts at response commit (`MEM-7.1`). Proposal:
record whether a promoted fact was later implicitly accepted, explicitly
confirmed, or disputed, as provenance only, so P1 can rely on it and an
investigation (PRD §10) can see it. It never changes a fact's authority or
authorization.

### P4 — Semantic-layer authoring practices (from MDL / `enrich-context`)

Review how WrenAI structures metric definitions, enums, units, approved joins and
worked examples as reviewable files, and compare with `knowledge/`. Goal: gaps
in `knowledge/` that affect retrieval quality (for example synonyms, worked
example questions per capability). Documentation/catalog work only.

### P5 — Guardrails for the Mode 2 last-resort path (from `wren-core` / `policy.py`)

Jarvis's fallback when no curated capability exists is **Mode 2**
([analytical-contracts §1](../data/analytical-contracts.md)): the Engine compiles
parameterized SQL at runtime from verified declarative contracts (entity, field,
usage, relationship, measure grain, `office_scope_path`, cap). It is the last
choice after Mode 1, and only when neither exists is the answer `Unsupported`.
The model still never writes SQL. Mode 2 is the least mature part of the
design: it sits in L8 (FIN-98), the compiler is open (D5), and the parser is
only a candidate (`sqlparser`, explicitly not a security validator).

WrenAI solves the same problem — SQL assembled against a semantic layer, not raw
tables — with layered guards worth studying for the Mode 2 compiler and
validator:

| WrenAI guard (source on `main`) | What it does | Jarvis counterpart to compare |
| --- | --- | --- |
| Read-only AST check (`core/wren/src/wren/policy.py`, sqlglot) | Always on: only SELECT-family statements reach a connector | PRD §5 execution guard: SELECT-only, single statement |
| Strict mode: manifest-only tables (`_check_tables`) | Any table or table-valued function in FROM/JOIN that is not an MDL model is rejected (fail-closed) | Approved surfaces only; `excluded_tables` |
| Data-reader block in every AST position (`_check_data_readers`) | Blocks file/URL/remote readers (`read_csv`, `dblink`, …) in projections and subqueries too, not only FROM | Function restrictions — the position-coverage lesson applies |
| Denied-function list (`_check_functions`) | Operator-maintained backstop | Function restrictions |
| Row-level access control (`core/wren-core/.../access_control/mod.rs`) | Per-model row filter injected into the logical plan from session properties; property values must be literals; rule syntax validated against the model | D5 office-scope enforcement in SQL (`office_ids` from `authorized_scope`, `require_office_filter`) |
| Column-level access control | Columns hidden per session | Sensitivity classes (`secret_never_expose`, `exclude`) |
| `dry-plan` | Expands the query against the semantic layer for inspection before execution | Static validation before DB-assisted validation (PRD §4) |

Lessons to evaluate:

1. **Enforce scope in the plan, not in text.** WrenAI injects row filters into
   the DataFusion logical plan, so every model reference — including ones inside
   subqueries — carries the filter. Compare with how D5 will guarantee
   `office_ids` on every fact path in compiled SQL.
2. **Fail-closed where possible, and say where it is not.** WrenAI documents
   that its non-source-position reader check is a blocklist, not an allowlist.
   Jarvis's validator should state the same boundary explicitly per check.
3. **Session properties as literals only.** Scope values enter as typed
   literals, never as expressions — the same rule Jarvis applies to bindings.
4. **Candidate dependency.** `wren-core` is Rust on Apache DataFusion,
   Apache-2.0. Whether to depend on it, port ideas, or keep a separate compiler
   is an owner decision, recorded in `architecture/tech-stack.md` if adopted.

Constraint: none of this introduces model-authored SQL. A fallback that lets the
model write SQL would contradict PRD §2, analytical-contracts §1 and
`AGENTS.md`, and is a contract change for the owner, not part of this proposal.

## Not proposed

- Adopting WrenAI or Vanna as Jarvis's question pipeline — both rely on
  model-written SQL. (Using `wren-core` only as a compiler library is P5, item 4.)
- LLM rewriting of follow-up questions as the source of truth — superseded by
  structured `session_memory`.
