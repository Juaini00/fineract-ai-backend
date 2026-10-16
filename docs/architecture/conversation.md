# Conversational understanding (FIN-164)

Status: owner-approved design, 2026-10-16. Implementation and real-model readiness are separate gates. Publication is not authorized by design approval. English is the only supported product language.

## Authority and boundaries

[PRD](../product/prd.md) owns product scope; [Engine](engine.md) owns lifecycle and verified execution; [responses](../contracts/responses.md) owns blocks and D1–D3; [clarifications](../contracts/clarifications.md) owns forms/K5; [memory](memory-context.md) owns durable context and promotion. This contract extends the analysis-only FIN-162 understanding boundary, not execution authority. One Engine remains responsible for all stages. No second agent loop, arbitrary SQL, Fineract writes or unrestricted general-purpose chat.

## Durable submission and completion

All turns use existing authenticated `POST /chat/jobs`, durable session/job/message ownership, idempotency, one active job per session, lease fencing and existing cancellation/expiry semantics. No new endpoint, lifecycle or job outcome is introduced.

A non-analytical response uses document kind `conversation`, existing block vocabulary and `Completed`/`Answered`/`Complete`. Complete means the conversational task was completed, not that any analytical population was inspected. Such a turn admits no analytical plan, source node or dataset. It promotes no analytical memory facts: greetings and thanks never supersede ActiveScope or become PriorResult. Response, assistant message, terminal event and audit commit atomically at T7. No synthetic source provenance is allowed.

Unsupported business scope retains Unsupported/limitation; verified policy denial retains BlockedByPolicy. Analytical Empty/NotFound/Partial/Unknown keep their existing meanings. Provider failure, invalid output, no-fit and abstention remain distinct operational reasons, never unsupported-scope diagnoses.

SSE carries only genuine existing phases and durable events. No invented data execution progress or unvalidated narrative text. Reconnect reads persisted state/events; it does not regenerate. Duplicate submission replays the durable acknowledgement. A crash before response commit may require another admitted bounded call; exactly-once external generation is not promised. Fenced workers cannot commit stale output. Recovery reuses durable verified understanding when present and cannot reset model/token/repair budgets.

## Bounded model interface

Use one closed versioned understanding-or-analysis proposal at the existing planning boundary, with branches for social interaction, capability help, analysis, goal clarification, result explanation, unsupported-scope candidate, abstention and no-fit. Reject unknown/duplicate fields, inconsistent branch payloads, invalid references, ungrounded values and budget overflow. Confidence is diagnostic only.

Analysis embeds the existing FIN-162 compact grounded proposal. Preserve the original request, including a mixed greeting, for slot quotes and verification. Do not strip the greeting by phrase matching or replace the request with a lossy model summary. Every explicit operation/date/currency/grouping/scope constraint survives or receives an explicit refusal/clarification; never silently answer a supported subset.

Social interaction selects a bounded speech act: greeting, thanks, acknowledgement or assistant identity. The Engine renders concise English text without invented facts, actions or human identity. Deterministic rendering is permitted; deterministic phrase-based intent routing is not. A renderer cannot stand in for a failed model-understanding call.

Help references active approved capability IDs and catalog version/hash, reviewed English descriptions/examples, and verified implementation limitations. The Engine checks every selected reference against current authorization and execution/output support. Related tracker status is not capability evidence. Do not automatically translate catalog prose, promise unavailable features or claim general reliability from a successful example.

Goal clarification identifies missing goal dimensions within an Engine-owned vocabulary. It cannot guess a report, identity or analytical binding. Result explanation selects only an authorized supplied durable response/version and evidence references. Unsupported is only a candidate classification until an Engine-verified product-boundary or catalog-coverage reason exists. A retrieval miss or no_fit cannot prove unsupported scope.

Every call reuses existing Rig transport, admission/audit, cancellation, deadlines and shared absolute model/token budgets, outside commit transactions. Existing bounded repair policy applies; no new retry loop or speculative timeout increase. Policy violations cannot be repaired by asking the model to grant permission.

## Ambiguity and same-job continuation

An unclear reporting goal opens a versioned intent-purpose form on the same job before an analytical plan is admitted. Its plan association is null, not a fabricated plan. Accepted goal inputs have explicit provenance and are reused in continued understanding; the original request remains preserved. All currently known missing dimensions are batched. Existing submission ownership, revision, idempotency and validation apply. Invalid/stale answers do not resume or cause a new LLM call.

Identity ambiguity still requires an approved authorized resolver and K5 cardinality. Goal clarification does not prove CLR-3. Subsequent data-dependent forms retaining prior answers and completed outputs require FIN-75. No infinite loop: existing clarification rounds/deadlines/cancellation/expiry apply. Newly accepted goal data and verified understanding must be durably correlated so recovery cannot silently change intent or discard bindings.

## Context and explanation dependencies

FIN-88 owns actual relevance/recency-bounded reading of history, valid session facts and prior-result references before each call. Combined instructions/schema/history/bindings/evidence/output reserve stay within budget. Never load full history or derive authorization from summary/facts. Ambiguous prior-result selection is clarified, not guessed. Expired/unavailable handles are disclosed; fresh retrieval is explicit and independently authorized.

FIN-100 owns additive narration after deterministic composition, using bounded authorized evidence. Proposed explanation claims carry evidence references and approved derivations, with facts distinguished from hypotheses. D1–D3 validation precedes inclusion. Numeric matching alone does not justify causal or other unsupported prose; bounded supported statement forms must prevent unsupported claims. Failure drops invalid narration, preserves valid structured data and discloses the limitation. Deterministic fallback never calls the model.

FIN-75, FIN-88 and FIN-100 retain their build-ladder prerequisites. FIN-164 completion includes dependent behavior; it cannot be claimed from a standalone greeting slice or a substitute implementation under a different name.

## English-only behavior

All new replies, forms, help, examples, suggestions, denials, operational explanations, test names and testing prompts are English. Non-English prose receives an English request to restate in English without analytical dispatch. Stored source names/labels are not translated. Existing read-only/policy checks still apply. No automatic language matching, translation or multilingual feature is added.

## Readiness and acceptance

Before constructing dependent flows, exercise the effective configured provider/model through locked Bruno HTTP with required login/session prerequisites and checkout database isolation. Observe the exact greeting baseline separately. Require two positive analytical requests (plain savings deposits and the exact mixed April request) with statically verified plans and durable valid responses equal row-for-row to independently authored SQL against fineract_default. Record first-pass and repaired success separately. If the gate fails, diagnose and block dependent construction; do not replace it with fixtures, configuration echo or JSON parsing.

Each observation retains revision, effective provider/model, prompt/schema/catalog versions and hashes, exact request, session/job/call identity, all attempts, latency, observed usage or explicit unknown/reservation, validation, durable response/outcome, correlated event/audit and source-query counts. Authorization reads and analytical queries are separate. Preserve all failures after later success.

Before reliability results, freeze and review 40 English prompts: five variants in each of greeting, help, thanks, mixed analysis, vague goal, prior-result explanation, unsupported action and forbidden write. Include the eight exact FIN-164 owner examples. Freeze wording, expected semantic checks, SQL oracles, configuration and ordering seed before execution. Run three fresh-session observations per prompt (120). Explanation cases establish the intended authorized prior result. No unrecorded tuning loop.

Passing threshold: at least 114/120 correct first attempts, at least 14/15 in every class and all eight canonical examples 3/3. Zero unauthorized access, source writes, unverified execution, scope widening, cross-user leakage or fabricated numerical results regardless of aggregate rate. Failures stay in the denominator; repaired success is separate. This establishes only the reviewed corpus, not population reliability. Measure latency p50/p95; existing deadlines apply without a newly invented latency SLO.

Targeted boundaries cover policy/mixed-intent precedence, unsupported versus no-fit, missing/expired/ambiguous prior result, PII, empty/partial results, stale forms, duplicate submission, reconnect, ownership and affected provider unavailable/invalid-output paths. Negative fixtures are not positive real-model evidence. Permanent tests require one correct outcome, never success-or-known-failure. Assign acceptance IDs with the concrete reviewed test manifest, not retroactively from observed outcomes.
