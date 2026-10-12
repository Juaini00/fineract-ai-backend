# Technology stack and responsibilities

Date: 2026-09-08. Status: technology direction accepted; exact version lock and compatibility validation pending. This document records selection, not installation. No application dependencies have been added.

## Accepted foundation

| Area | Technology | Responsibility / boundary |
| --- | --- | --- |
| Language | Rust, edition 2024 | Backend and one Engine; app/core/chat crate boundary |
| HTTP | Axum, Tower HTTP | API, middleware, SSE; handlers delegate to services |
| Async runtime | Tokio, tokio-util, futures | Tasks, bounded concurrency, timeout/cancellation; not durable workflow ownership |
| Data access | SQLx | Repositories, parameterized execution, transactions and application migrations |
| Durable application state | PostgreSQL | Sessions, jobs, ledger, checkpoints, response/audit/public events |
| Live coordination | Redis | Notifications/live SSE coordination; no authoritative job history |
| Typed serialization | Serde, serde_json | Versioned plan, result, response and interaction payloads |
| Schema generation | Schemars | Schema for structured model/application contracts; semantic validation remains in Engine |
| Request validation | validator | ValidatedJson-style boundary; no scattered route-specific validation flow |
| Monetary values | rust_decimal, PostgreSQL NUMERIC | Decimal values; explicit grain/currency/rounding still required |
| Time and identity values | chrono, uuid | Explicit dates/timestamps and stable record identifiers |
| Configuration/errors | config, thiserror, anyhow | Typed settings/domain errors and composition diagnostics; sanitized API errors |
| Operational instrumentation | tracing, tracing-subscriber | Structured logs and spans correlated with jobs/attempts |
| Telemetry integration | OpenTelemetry direction | Export trace/metric signals; exact Rust crates/exporters/backend remain to be selected |

Read-only source credentials/pools are separate from the writable application database. SQL access follows route → service → repository → database. Redis outages must not destroy durable progress.

## Engine and model integration

Rig is the accepted preferred LLM client. The LLM proposes the plan for every admissible analysis request; existing authorization/read-only/policy preflight remains Engine-owned. Rig does not own scheduling, memory policy, re-plan, authorization or durable execution. Every call is bounded, validated and audited outside commit transactions. The owner-approved [FIN-162 contract](engine.md#llm-planner-contract-fin-162) requires a compact typed graph with grounded slots, explicit abstention/no-fit, Engine verification and distinct failure codes under the existing lifecycle/outcome matrix; no silent capability-selection fallback is permitted.

Petgraph is available as a graph-validation/topological utility where needed; it is not the scheduler or ledger. Tokio task/concurrency primitives execute admitted work. PostgreSQL holds durable status. No separate agent/workflow framework is selected to introduce another loop.

Reqwest remains available only for a concrete Rig/provider feature gap, not a parallel orchestration/client stack. FIN-163 wires the selected OpenAI-compatible local generation endpoint through Rig and proves bounded calls, config, usage/audit and failure settlement. Exact compatible dependency versions and token-accounting implementation are validated/pinned there; they cannot weaken schema validation or shared job budgets.

### Development provider and structured output

The interim cost-conscious selection remains the local `LLM_*` configuration
(owner decision 2026-10-12): `LLM_PROVIDER=mlx-local`,
`LLM_MODEL=mlx-community/bitnet-b1.58-2B-4T`, and
`LLM_CHAT_COMPLETIONS_URL=http://127.0.0.1:8090/v1/chat/completions`.
These identify the current development target, not hard-coded production
defaults. Provider/model/endpoint changes come from explicit configuration and
are recorded per call; no automatic hosted-provider or deterministic-planner
fallback is selected. Timeout, input/output limits and retries use `LLM_*`
settings under the Engine's absolute shared budgets.

Native JSON/schema output support must be checked against the actual provider
and model. A real smoke call on 2026-10-12 reached the configured model, but
`response_format={"type":"json_object"}` returned Markdown-fenced JSON even for
a one-field object. Reachability is proven; constrained JSON/schema enforcement
and planner accuracy are not. The client must not strip fences or accept a
best-effort object as a verified plan. The Engine validates the closed versioned
output, permits bounded correction, and settles invalid output explicitly.
Keeping the small model is conditional on the named planner acceptance
scenarios, not on configuration or a successful generation request.

## Data and catalog

Use approved SQL-side joins/projection/aggregation first, followed by bounded Rust composition. Do not add a dataframe/distributed analytics engine by default. DataFusion remains an optional candidate only if required operations and capacity tests justify it.

YAML analytical/dataset contracts and approved SQL assets remain the authoring surfaces. The YAML parsing library and SQL AST parser are not yet locked; sqlparser is a candidate for AST parsing, not a security validator. Engine compilation, parameterization, office scope, PII and function guards remain required regardless of library.

PostgreSQL-backed chunked retained datasets were recommended as an initial option; physical storage, retention and capacity evidence are not yet final. This document must not imply unlimited result storage in PostgreSQL or a mandatory object-storage service.

The existing pgvector/Voyage retrieval assets may supply a bounded candidate shortlist to the LLM under [the planner input contract](engine.md#input-and-retrieval). Lexical/vector scores do not select capabilities, decide identity clarification or classify empty/failed retrieval as unsupported scope. The Engine records the candidate catalog version/hash and retrieval diagnostics. Retrieval is not a second execution route and never replaces a failed generation call. This contract does not retune embedding models, ranking or catalog wording.

## Dashboard integration

The existing dashboard uses React, TypeScript, Vite, TanStack Query and Zod. It adapts to backend JSON/SSE contracts. No new frontend stack is imposed by this backend project.

TanStack Table, chart libraries, json-render and AI SDK UI were discussed as optional frontend choices, not mandatory backend dependencies. Initial backend streaming uses the agreed job event protocol; compatibility with an AI SDK UI protocol is not assumed. Fetch-based SSE supports bearer headers and explicit replay/reconnect in the client.

## Version and operational readiness

Before implementation, record exact compatible versions, required features and toolchain in an approved version matrix, then pin resolved dependencies through Cargo.lock when scaffolding is authorized. Old Cargo.toml declarations are reference evidence, not proof of compatibility for the new backend.

The interim development generation provider/model is selected above; production deployment selection remains explicit rather than an automatic fallback. Token accounting, native structured-output compatibility and exact Rig/provider versions are implementation gates for FIN-163. The broader outstanding deployment/observability/storage selections remain outside FIN-162; publishing this planner contract does not claim those mechanisms are built.

## References

- [Rig structured output](https://docs.rs/rig/latest/rig/agent/enum.OutputMode.html)
- [Tokio task management](https://docs.rs/tokio/latest/tokio/task/struct.JoinSet.html)
- [OpenTelemetry signals](https://opentelemetry.io/docs/concepts/signals/)
- [sqlparser API](https://docs.rs/sqlparser/latest/sqlparser/)
- [Existing dashboard dependencies](https://github.com/Juaini00/fineract-ai-reports-UI/blob/main/package.json)
