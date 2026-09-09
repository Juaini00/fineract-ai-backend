# Jarvis — product requirements

Date: 2026-09-08. Status: agreed product direction; technical design in progress. This document consolidates the earlier Jarvis rewrite concept and subsequent clarification, response, context and transport decisions for the new backend repository. It is not authorization to implement the application.

## 1. Product and project boundary

Jarvis is a data-analysis assistant for Fineract administrators, integrated into the existing reporting dashboard. Users express a reporting goal in natural language and receive structured results with grounded explanations and traceable evidence.

Implementation will take place in `Juaini00/fineract-ai-backend`, not by continuing the old ai_report application. The existing dashboard will adapt to the agreed backend contracts. Backend-generated data and presentation contracts are in scope; frontend implementation is not.

The rewrite addresses fragmented runtimes and decisions made before relevant data is available. One Engine owns execution throughout. Proven assets may be imported later according to a reviewed carry-over inventory; neither all old code nor all old migrations are automatically authoritative for the new design.

## 2. Scope

This package defines the full agreed application scope through production release and maintenance readiness, not an MVP-only design. Large Fineract datasets are a baseline workload. Implementation milestones may be incremental; they do not remove required release behavior or postpone its architecture. Explicitly deferred product scope, such as global memory, remains separate from unfinished required design.

In scope:

- Read and analyze approved Fineract data from a read-only source/replica.
- Multi-step analysis across related entities, including charge, product and account scenarios once their contracts are approved.
- Follow-up questions using resolved identities, active scope and prior-result references.
- Structured responses with metrics, tables, chart specifications, comparisons, findings, limitations and supported follow-up suggestions.
- Durable jobs, staged clarification, bounded context/results, auditable execution and meaningful progress streaming.

Out of scope:

- Creating/changing banking records, transactions, configuration or schema in Fineract.
- Simulating banking operations such as interest or repayment processing.
- Arbitrary model-authored SQL over unrestricted schema, or arbitrary executable UI/code.
- Answers requiring data surfaces that have not been approved.
- Global cross-session memory in the initial release.

Read-only applies to Fineract access. The application database necessarily supports writes for sessions, jobs, results, events and audit, and controlled application migrations.

## 3. User outcomes and success criteria

The [data scope decisions dated 2026-09-09](2026-09-09-dataset-scope-decisions.md) record the accepted domain baseline and subsequent requirements: reproducible period-close reports distinct from corrected historical positions, historical office attribution, per-currency totals and approved-rate consolidation, client-level charge mapping, standing instructions, teller/cashier operations, recorded provisioning results, group/center activities, Fineract user-action history, deployment-specific custom datatables and data-completeness analysis. These are product requirements subject to verified source evidence and approved execution contracts; the formal dataset inventory remains unfinished. Snapshot/storage, exchange-rate source, historical authorization and per-resource semantics are not finalized by scope acceptance.

1. A complete-parameter simple request runs through the same Engine with minimal planning overhead.
2. Independent data steps run concurrently within shared budgets; dependent steps wait for all required inputs.
3. Known ambiguities are batched. Newly discovered data-dependent ambiguity can produce another bounded clarification stage on the same job without repeating resolved questions.
4. Large results yield complete deterministic analysis or explicit limitations. A subset is never presented as the full requested population.
5. Follow-ups reuse valid facts/results and disclose fresh retrieval when old data is unavailable or unsuitable.
6. Users can see genuine progress, answer structured clarification, refresh/reconnect and retrieve a durable result.
7. An investigator can trace a response or failure through plan, node attempts, data provenance, composition and response validation using the job identity.
8. No source query executes without approved contracts, authorization, scope and resource guards.

Acceptance must include ordinary requests and failure/recovery cases. A job marked complete does not by itself prove answer correctness or data completeness.

## 4. One Engine

The canonical lifecycle is owned by [engine.md](../architecture/engine.md): durable acceptance → context → plan/verify → ready-node execution → deterministic composition → response assembly/validation → durable completion.

Executors perform approved probe/resolve, curated query, analytical query and composition operations. They do not own separate agent loops. The LLM selects within structured contracts; the Engine controls execution, policy and budgets.

Response assembly is an Engine stage following the data graph, so a fast-path can contain one data node. WaitingForUser is suspension, not a terminal job outcome. Lifecycle, analytical outcome and completeness are distinct dimensions.

Static validation precedes source execution. DB-assisted query validation is a separate bounded step; this does not prohibit persisting application state or validating a query against the DB. Changed plans are versioned and verified again. Exact recovery/state matrices remain blocking design work.

## 5. Data access modes

Prefer an approved curated capability with a pre-authored query when it satisfies the request. Otherwise use an approved analytical contract if one covers the required analysis. If neither exists, return Unsupported without falling back to unrestricted schema.

Analytical contracts declare entities, typed fields, sensitivity, selectable/filterable/groupable/aggregatable usage, relationships/cardinality, measure grain, office-scope paths, allowed functions and resource caps. The model produces a structured analytical specification; the Engine compiles it into parameterized SQL.

The execution guard checks approved surfaces/fields, SELECT-only single-statement structure, function restrictions, scope predicates and PII policy. Source access uses read-only transactions and statement timeouts. The model cannot widen authorization. AST parsing and database preparation do not replace semantic policy checks, and the compiler is not an absolute security guarantee.

Exact analytical schema, parser/compiler dependencies, first approved contracts and validation rules must be completed before implementation.

## 6. Composition and large results

Prefer SQL-side projection, joins and aggregation at the grain required by the answer. Where no approved single query fits, use bounded deterministic composition over dataset handles. Relationships must not multiply measures incorrectly; multiple charges per account must not duplicate account balances or account counts.

Node results carry schema/grain, scope, provenance, available row count, completeness and reason, plus a retained dataset handle when applicable. Total matching row count can be unknown. Preview metadata is separate from analytical completeness.

Complete totals may coexist with a truncated preview. Missing/truncated inputs that affect a derived output prevent a complete claim; independent unaffected outputs can remain complete. Top-N is computed over the requested population before display truncation. Output LIMIT must not silently truncate input before aggregation and does not replace statement timeout.

Limit rows, bytes, memory, time, query/retry count and concurrency per node and per job. Check actual runtime consumption as well as planned bounds. Large results do not get copied wholesale into checkpoint JSON, model messages or SSE payloads. ID selections are also bounded or referenced through authorized handles.

Before every LLM call, budget all instructions, schemas, conversation, bindings and evidence together, reserving output/reasoning capacity and a safety margin. On overflow, remove optional context/preview, use approved semantics-preserving aggregation or targeted retrieval within budget, then return qualified results or an operational limitation. Never silently narrow dates, offices or population. If narration cannot fit or validate, preserve structured output without it.

Physical dataset storage, consistency/snapshot policy, expiry, numeric budgets and retention remain technical design decisions. Pagination must preserve result identity and authorization.

## 7. Clarification and suggestions

### Explicit query and presentation intent

Administrators may request supported output formats, selected fields and their order, typed filters, sorting, grouping, measures and limits. Preserve these as explicit request requirements through planning, SQL execution, composition and rendering; do not silently replace them with model preferences or conversation defaults.

Resolve requested names to approved contract field IDs. Validate visibility, allowed operators, types, relationships and authorization before execution. Authorization constrains all requests. Unsupported or forbidden requirements must be disclosed without leaking restricted schema; do not silently omit a field/filter and claim the full request was satisfied. Ambiguity uses the clarification contract. The precedence for structured inputs conflicting with natural language still requires a detailed interaction decision.

Separate source-row filtering, aggregate-result filtering, field projection and presentation. Filters over the requested population must execute in approved source SQL or an equivalent complete, authorized deterministic stage, never merely on the visible page. Filtering on an unselected field may be valid when the contract permits it. User-requested output order and supported format are retained; technical fields used for joins/order need not be exposed in the response.

The detailed analytical/API/response contracts must specify boolean filter groups, date boundaries and business timezone, currency/decimal/null semantics, sorting/ties, server pagination, chart compatibility and how unmet requirements are represented. These are release design requirements, not finalized schemas in this paragraph.

### Multi-resource loan activity

Loan activity is a representative full-release design scenario requiring an explicit inventory of approved event/resource types, fields, relationships and accounting/time semantics. Do not equate a loan record or one transaction table with all loan activity. Retrieve only the resources/fields needed for the request, while providing complete coverage of its defined scope through bounded processing and stable pagination. A timeline needs a normalized event contract, source references, deterministic ordering and completeness per source; the exact inventory and schema remain required design work.

Resource exhaustion may produce an explicit limitation, but this fallback is not acceptance evidence that normal target-scale Fineract workloads are supported. Release acceptance must establish realistic data sizes, selectivity, concurrency and successful large-result cases, including multi-resource loan activity and explicit field/filter/format requests.

[clarifications.md](../contracts/clarifications.md) owns typed fields, conditional forms, resolver-dependent stages, suggestions and validation. Supported semantic types are single choice, multiple choice, text, number, date, date range and boolean. Radio/select/checkbox presentation does not change server semantics.

Suggestions can explain defaults, provide examples or offer supported alternatives; they do not auto-submit or guess identity. Large option sets are authorized and paginated. Subsequent forms retain accepted facts and completed outputs. Submit through the same job's responses endpoint; invalid input does not resume the job or require a new LLM call.

Clarification/resolver rounds and waiting time are bounded. Exact limits remain open; the old absolute one-round guarantee is superseded.

## 8. Memory and context

PostgreSQL is the durable source of truth; Redis is live coordination only. Separate job execution state, durable session history, structured session facts and the bounded per-call LLM working set.

Retain resolved entities, active filters and prior results with provenance/completeness and dataset references. Select only relevant memory and recent turns for each call. Summarization is incremental and versioned; it does not become the authority for permissions or numerical results. Large histories are paginated rather than loaded in full per request.

Expired/unavailable handles are explicit. A fresh source query is a new retrieval, not an invisible continuation of the old result. Recheck authorization on follow-up. Memory promotion, concurrent session behavior, summary failure recovery, deletion and retention require detailed contracts. Global memory remains deferred.

## 9. Responses and experience

Return a versioned response document composed of approved blocks. Data is authoritative; LLM narrative is additive and evidence-grounded. Findings distinguish facts from hypotheses. Numerical values, supporting references, completeness and relevant limitations must survive formatting.

The frontend renders tables and charts from structured data/specifications. Suggestions refer to supported follow-up analyses and execute only after a new user action with authorization. Unvalidated narrative is not streamed in the initial release; genuine progress is streamed while the final document is prepared.

[api.md](../contracts/api.md) owns request/response submission and durable acknowledgement. [sse.md](../contracts/sse.md) owns progress phases, parallel-node updates, replay, reconnect and delivery limitations. HTTP JSON uses the success/data/error envelope; public errors never expose raw SQL, prompt or stack details.

## 10. Audit, security and failure handling

Audit must support diagnosis of both stopped jobs and incorrect successful responses. Trace input references, scope, plan/contract versions, node attempts, output provenance/completeness, deterministic operations, model/prompt versions, evidence supplied, validation and final response identity. Store structured decisions and controlled evidence; do not depend on internal model reasoning or expose sensitive payloads in general logs.

Audit, operational logs, traces and metrics have distinct purposes. Required audit persistence gates protected progress; public durable events are coordinated with state changes. Correlation identifiers connect the records. Detailed audit schema, retention, access and failure policy remain design work.

Bearer identity establishes ownership and binding access. Request filters may narrow but cannot widen that authority. Optional API-key scope is not a hard boundary for an otherwise fully authorized admin. Final dashboard identity integration, tenant model and binding PII permissions remain unresolved security decisions.

Persist completed outputs for recovery. External calls that finish before a checkpoint commit can have uncertain outcomes; recovery must not promise exactly-once external execution. Partial results follow explicit dependency/fail policies. Model narration failure preserves valid structured output; exhausted budgets must not cause unbounded retries or unsupported data fallback.

## 11. Technology and carry-over

[tech-stack.md](../architecture/tech-stack.md) records accepted technologies and candidate/open selections. Keep the three-crate boundary: app (composition), core (foundation), chat (reporting/Engine). Route → service → repository → database; SQLx calls remain in repositories.

Later carry-over inventory will identify approved dataset/SQL/domain assets, relevant migrations, and foundation/security code to reuse or adapt, with source revisions and verification. Do not import obsolete runtimes by default or apply legacy migrations without reviewing the new schema. The old application remains untouched by this new repository's lifecycle.

## 12. Implementation readiness

The [documentation index](../README.md) and [design review](../design-review.md) track readiness. Before application work, complete database/transaction design, full lifecycle/recovery matrices, analytical and response schemas, memory/dataset lifecycle, identity policy, operational budgets and acceptance scenarios.

Verification must cover simple and multi-step analysis, staged clarification, idempotency, join-grain correctness, source truncation versus preview limits, combined context overflow, parallel resource pressure, response validation fallback, worker crash boundaries, audit failures, SSE replay and authorization. Numerical correctness needs full-scope expected results, not merely schema validity or coverage percentage. Run real approved queries in end-to-end acceptance once implementation is authorized.
