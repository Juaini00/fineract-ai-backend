# Jarvis backend design documentation

Status: design in progress. Application implementation has not been authorized until the design package is complete and reviewed.

## Project boundary

- Target repository: https://github.com/Juaini00/fineract-ai-backend
- Existing dashboard: https://github.com/Juaini00/fineract-ai-reports-UI
- This is a new backend repository, not an extension of the existing ai_report application.
- The frontend will adapt to the agreed backend contracts. Frontend implementation and library selection are outside this backend project; response and transport contracts are in scope.
- [Jarvis PRD](product/prd.md) is the consolidated product baseline for this repository, incorporating the earlier `2026-09-08-jarvis-rewrite-prd.md` concept and subsequent agreed decisions. It is not a completed implementation specification.
- Dataset assets and migrations will be imported and reviewed later. No source code, datasets, or migrations have been imported at this stage.

## Agreed direction

Jarvis is a read-only Fineract data-analysis assistant for administrators. Read-only applies to the Fineract source; the application database necessarily persists jobs, sessions, results, and audit records.

One Engine owns planning, scheduling, policy enforcement, budgets, recovery, and response assembly. Libraries and node executors do not introduce independent orchestration loops. SQL and approved deterministic operations produce numerical evidence; LLMs plan within contracts and explain evidence.

Large results use dataset handles and bounded processing. Analysis completeness is separate from preview truncation and node completion. The Engine never silently narrows a requested population to fit resource limits.

Responses are versioned structured documents composed from approved blocks: narrative, metrics, tables, charts, findings, comparisons, limitations, suggestions, and clarification. Suggestions offer supported follow-up analysis and do not execute themselves.

Durable job/session state belongs in PostgreSQL. Redis is live coordination only. Session history is separate from the bounded LLM working context. Global memory remains deferred. Long sessions require bounded retrieval, incremental summaries, pagination, and explicit storage/retention policies.

The accepted backend foundation is Rust, Axum/Tower, Tokio, SQLx/PostgreSQL, Redis, Serde/Schemars, Validator, rust_decimal, and Tracing. Rig is the preferred LLM client direction; Engine ownership remains in Jarvis. SQL-first aggregation and bounded Rust composition are preferred. Exact versions, provider compatibility, SQL/YAML validation dependencies, observability exporters, and dataset storage remain subject to technical review. DataFusion, alternative direct provider clients, and UI libraries discussed as candidates are not mandatory dependencies.

## Documentation ownership map

Start with the [PRD](product/prd.md), [accepted tech stack](architecture/tech-stack.md), then the lifecycle and interaction contracts below. Outstanding design work remains tracked in [design review](design-review.md).

The ownership map includes both recorded contracts and planned documents; it does not imply implementation readiness. Each rule has one authoritative home; other documents link to it instead of defining competing flows.

Recorded on 2026-09-08: [Engine lifecycle](architecture/engine.md), [clarification contract](contracts/clarifications.md), [job API](contracts/api.md), and [SSE experience/protocol](contracts/sse.md). These own the agreed behavior for the new repository and supersede conflicting old-PRD wording for those topics. Full schemas, database transactions and operational limits remain open as identified in each document.

| Planned document | Owns |
| --- | --- |
| `product/prd.md` | Product scope, supported analysis, user journeys, success criteria |
| `architecture/overview.md` | Component boundaries and the single execution flow |
| `architecture/engine.md` | State machines, scheduling, clarification, re-plan, cancellation and recovery |
| `architecture/tech-stack.md` | Selected libraries, versions, responsibilities, limitations and rationale |
| `data/database-design.md` | ERD, ownership, constraints, indexes, transactions and migration strategy |
| `data/analytical-contracts.md` | Semantic layer, compiler/validation, measure grain and catalog lifecycle |
| `data/dataset-lifecycle.md` | Handles, storage, consistency, pagination, completeness and expiry |
| `architecture/memory-context.md` | Job/session memory, compaction, concurrency, token budgets and deferred global memory |
| `contracts/api.md` | HTTP endpoints, authentication, idempotency, pagination and error envelope |
| `contracts/clarifications.md` | Typed forms, dependent stages, suggestions, validation and answer bindings |
| `contracts/responses.md` | Block schemas, evidence, findings, suggestions, compatibility and fallbacks |
| `contracts/sse.md` | Event schemas, ordering, replay, authentication, reconnect and backpressure |
| `security/access-data-policy.md` | Bearer identity, tenant/office scope, PII, secrets and authorization boundaries |
| `operations/observability.md` | Audit, logs, traces, metrics, redaction, access and retention |
| `operations/runtime.md` | Deployment, worker topology, capacity, dependencies, backup and recovery |
| `verification/acceptance.md` | Cross-layer scenarios, failure matrix and readiness evidence |
| `migration/carry-over.md` | Source revision, assets to import, edits, compatibility and verification |
| `decisions/` | Accepted architectural decisions and explicitly superseded alternatives |

## Readiness gate

Implementation may begin only after the user reviews a coherent design package with:

1. One canonical end-to-end flow and explicit state transitions, including all failure/resume paths.
2. Database constraints and transaction boundaries aligned with jobs, memory, audit and SSE.
3. Versioned request, response, event, plan, and analytical contracts with worked examples.
4. Stack selections justified against those contracts, with provider/dependency compatibility recorded.
5. Concrete initial capacity, budgets, retention, deployment and operational failure policies.
6. A traceable acceptance scenario for each required behavior.
7. No unresolved blocking decisions. Deferred scope has explicit boundaries and does not hide a required dependency.

Track current gaps in [design-review.md](design-review.md). Completing an outline or copying the old PRD does not satisfy this gate.
