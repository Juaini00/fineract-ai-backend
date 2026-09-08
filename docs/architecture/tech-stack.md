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

Rig is the accepted preferred LLM client. It does not own Jarvis scheduling, memory policy, re-plan, authorization or durable execution. Each model call is bounded, validated and audited by the Engine. Native structured-output support is provider/version-dependent; runtime schema and semantic validation remain mandatory.

Petgraph is available as a graph-validation/topological utility where needed; it is not the scheduler or ledger. Tokio task/concurrency primitives execute admitted work. PostgreSQL holds durable status. No separate agent/workflow framework is selected to introduce another loop.

Reqwest is available for HTTP integration or a narrow provider feature gap. A parallel direct-provider implementation is not required unless the chosen Rig/provider combination cannot satisfy a concrete contract. Provider/model, tokenizer/counting strategy, structured-output compatibility and exact versions remain blocking selections.

## Data and catalog

Use approved SQL-side joins/projection/aggregation first, followed by bounded Rust composition. Do not add a dataframe/distributed analytics engine by default. DataFusion remains an optional candidate only if required operations and capacity tests justify it.

YAML analytical/dataset contracts and approved SQL assets remain the authoring surfaces. The YAML parsing library and SQL AST parser are not yet locked; sqlparser is a candidate for AST parsing, not a security validator. Engine compilation, parameterization, office scope, PII and function guards remain required regardless of library.

PostgreSQL-backed chunked retained datasets were recommended as an initial option; physical storage, retention and capacity evidence are not yet final. This document must not imply unlimited result storage in PostgreSQL or a mandatory object-storage service.

The existing pgvector/Voyage retrieval assets are carry-over candidates. Final retrieval behavior, embedding provider/model and fallback/error policy need a contract review; they are not a second execution route.

## Dashboard integration

The existing dashboard uses React, TypeScript, Vite, TanStack Query and Zod. It adapts to backend JSON/SSE contracts. No new frontend stack is imposed by this backend project.

TanStack Table, chart libraries, json-render and AI SDK UI were discussed as optional frontend choices, not mandatory backend dependencies. Initial backend streaming uses the agreed job event protocol; compatibility with an AI SDK UI protocol is not assumed. Fetch-based SSE supports bearer headers and explicit replay/reconnect in the client.

## Version and operational readiness

Before implementation, record exact compatible versions, required features and toolchain in an approved version matrix, then pin resolved dependencies through Cargo.lock when scaffolding is authorized. Old Cargo.toml declarations are reference evidence, not proof of compatibility for the new backend.

Outstanding selections: model/provider, YAML/SQL parser, schema/API publication tooling, observability exporter/storage, physical dataset storage, deployment resources, concrete budgets/retention and acceptance tooling. Existing Bruno scenarios are carry-over candidates; no test files are imported yet.

## References

- [Rig structured output](https://docs.rs/rig/latest/rig/agent/enum.OutputMode.html)
- [Tokio task management](https://docs.rs/tokio/latest/tokio/task/struct.JoinSet.html)
- [OpenTelemetry signals](https://opentelemetry.io/docs/concepts/signals/)
- [sqlparser API](https://docs.rs/sqlparser/latest/sqlparser/)
- [Existing dashboard dependencies](https://github.com/Juaini00/fineract-ai-reports-UI/blob/main/package.json)
