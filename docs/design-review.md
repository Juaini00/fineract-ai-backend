# Design completeness review

Status: initial gap review, 2026-09-08. No application implementation, final database schema, or final transport protocol is implied by this document.

## Recorded decisions update — 2026-09-08

The [consolidated PRD](product/prd.md) and [tech stack](architecture/tech-stack.md) are now recorded for the new repository. The original PRD coverage/ambiguity tables below remain historical baseline findings; the consolidated documents supersede the old maximum-one-clarification, mandatory Respond-node, global no-writes and in-place rewrite wording. Unresolved security, schema, capacity and operational decisions remain explicit.

The coverage table below describes the original PRD baseline. Subsequent accepted interaction decisions are now recorded in [Engine](architecture/engine.md), [clarifications](contracts/clarifications.md), [API](contracts/api.md), and [SSE](contracts/sse.md).

- D01 resolved in direction: batch known ambiguities; allow bounded genuinely new data-dependent stages on the same job. No absolute one-round guarantee.
- D03 resolved in direction: response assembly is an Engine stage after the data graph; fast-path may have one data node.
- D04 resolved: WaitingForUser is suspended, not terminal.
- D02/D05 clarified in direction: distinguish static checks, DB-assisted validation and source execution; application persistence is allowed. Exact transactional sequencing remains open.
- D06/D07: new repository documentation is authoritative for the new project; old code and migrations have not been imported or modified by this update.
- Clarification form types, suggestions, staged submission, HTTP acceptance/idempotency and genuine public SSE phases are recorded. Numerical limits, complete schemas and option-resolver endpoint remain open.
- Snapshot/cursor replay, duplicates, Redis notification-only behavior, explicit cancellation and fetch-based authenticated SSE are recorded. Exact stale-cursor wire response and token-revocation handling remain open.

Next review focus: database ERD, transaction boundaries, worker lease/fencing and audit persistence. The documentation package is still not implementation-ready.

## Existing concept coverage

| Area | Existing PRD coverage | Required design work |
| --- | --- | --- |
| Audit | §8 provenance, §10 every job/query, §11 carry-over | Event types/fields, access, redaction, retention, durable write boundaries and audit-write failure policy |
| Logging/tracing/metrics | Telemetry foundation referenced in §11 | Correlation IDs, spans, severity, metrics/alerts, exporters and sampling; distinguish these from audit |
| SSE | §8 node transitions and final answer; §6 PostgreSQL/Redis roles | Event IDs/schema/version, replay cursor, ordering, duplicate handling, authentication, expired cursors, disconnects and slow clients |
| Engine | Graph, fan-in, pause/resume and budgets | Complete state transitions, leases/fencing, cancellation, retry/re-plan accounting and crash boundaries |
| Database | Table names and durable-state principle | ERD, keys/constraints, indexes, schema versions, transaction boundaries and cleanup |
| Memory/context | Structured memory, summary and token budget | Incremental summary watermark, bounded selection, concurrent session requests, deletion/retention and failure behavior |
| Responses | Structured output and evidence | Composable block schemas, claim/evidence validation, suggestions, compatibility and fallback |
| Analytical layer | Curated queries and structured compiler | First complete contract, parser/validator selection, measure semantics and catalog versioning |
| Dataset lifecycle | Handles, completeness and bounded processing | Storage choice, snapshot/freshness behavior, authorization, pagination and expiry |
| Operations | Broad timeout/retry and recovery principles | Deployment topology, capacity targets, provider outages, quotas, backups and restore tests |

## Cross-layer requirements to resolve

- Audit answers who accessed which approved data under which scope and with what outcome. Operational logs explain failures. Traces follow work across components. Metrics measure aggregate health/cost. These are separate contracts, even if they share correlation IDs.
- Define a shared correlation model covering request, user/tenant where applicable, session, job, node, query attempt, and model call. Avoid user/job IDs as unbounded metric labels.
- Audit and logs must not default to raw result rows, credentials, prompts, or PII. Record contract/version, redacted scope metadata, outcome, completeness and timing as appropriate; retention and privileged diagnostic access need explicit rules.
- Decide how committed node state, durable events, and audit records are coordinated. Analyze crashes before commit, after commit but before publication, and during reconnect. Redis publication cannot be the only evidence of job progress.
- Define SSE replay and duplicate semantics; do not promise exactly-once transport. Specify disconnect versus cancellation, terminal events, expired replay cursors, bounded buffering and recovery through a current-state endpoint.
- Resolve bearer authentication for streaming with the dashboard; do not place durable bearer tokens in URLs. Document token expiry/revocation during a stream.
- Specify at-least-once recovery boundaries for external reads/model calls. A node can execute externally and crash before its completion checkpoint; `ExecuteOnce` cannot erase that ambiguity. Persisted completed outputs must not be rerun, and uncertain attempts require an explicit retry policy.
- Specify concurrent job handling within one session and how memory promotion avoids overwriting newer state.
- Distinguish source-data freshness from application transaction consistency; parallel queries and resumed queries do not inherently share a source snapshot.
- Require analytical findings to distinguish calculated facts from hypotheses. Suggestions must reference supported operations and receive fresh authorization when selected.
- Define application-level input/output/storage limits in addition to LLM context limits. Long session history must be paginated and selectively retrieved, not loaded in full per request.

## Existing PRD ambiguities to settle before porting as authoritative design

| ID | Ambiguity | Decision needed |
| --- | --- | --- |
| D01 | Maximum one clarification versus sequential runtime clarifications | Exact user-visible guarantee and handling of newly discovered ambiguity |
| D02 | Verify before any DB touch versus PREPARE against DB | Static validation, DB-assisted validation and data-query boundaries |
| D03 | One-node fast-path versus terminal Respond node | One canonical node accounting/response lifecycle |
| D04 | WaitingForUser called a terminal state while the same job resumes | Suspended versus terminal job taxonomy |
| D05 | SELECT-only/no writes phrased globally | Explicitly separate read-only Fineract access from application persistence and migrations |
| D06 | Old-runtime deletion/in-place migration wording | New repository lifecycle; leave old application untouched |
| D07 | All migrations carried automatically | Later asset inventory and migration adaptation based on the new schema; no automatic application of legacy migrations |
| D08 | Session described as context-window | Separate durable history/memory from the bounded per-call working set |
| D09 | Generated SQL described as eliminating injection entirely | Precise compiler guarantees plus validation, parameterization and defense-in-depth; no absolute security claim |
| D10 | PII gate versus admin projection and optional API-key scope | Explicit binding policy for the new bearer identity; voluntary scope is not a hard authorization boundary |

## Information still required

- Initial deployment environment, CPU/RAM/storage constraints, replica availability, and expected concurrent jobs.
- Initial supported analytical questions and realistic dataset sizes.
- Model/provider choice and data-handling constraints.
- Existing-dashboard identity/token integration and initial tenant model.
- Required retention periods, result freshness expectations and operational recovery targets.

These inputs inform budgets and schema decisions. They do not block documenting the already agreed product and ownership boundaries.

## Review sequence

1. Resolve canonical lifecycle and the ambiguities above.
2. Define API/response/SSE and database transaction boundaries together.
3. Complete analytical contracts, dataset lifecycle, memory and context policies.
4. Finalize stack/provider versions and runtime capacity against those requirements.
5. Walk the same representative requests through every document, including failure and recovery cases.
6. Review the complete package with the user before copying implementation assets or starting application work.
