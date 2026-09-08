# Engine lifecycle

Status: agreed lifecycle direction and clarification/transport integration, recorded 2026-09-08. The full transition matrix, recovery mechanics and database constraints remain design work; this is not implementation-ready.

## One owner

One Engine owns context assembly, planning, verification, node scheduling, deterministic composition, response assembly and persistence. Job API handlers accept/read commands and delegate. Executors and LLM clients never introduce separate orchestration loops.

```text
authenticate/validate → persist accepted job → acquire work
→ assemble context → plan/verify → execute ready nodes
→ compose deterministic evidence → assemble/validate response
→ persist response + memory promotion + completion event
```

Missing information suspends the same job through [clarification](../contracts/clarifications.md). Live facts may trigger a bounded, versioned re-plan. All paths use the same budgets and policy guards. Public progress is a projection through [SSE](../contracts/sse.md), not an alternate execution flow.

## State dimensions

| Dimension | Proposed agreed vocabulary |
| --- | --- |
| Job lifecycle | `Queued`, `Running`, `WaitingForUser`, `Cancelling`, `Completed`, `Failed`, `Cancelled`, `Expired` |
| Outcome | `Answered`, `Empty`, `NotFound`, `Unsupported`, `BlockedByPolicy`, `Invalid`, `OperationalFailure` |
| Analysis completeness | `Complete`, `Partial`, `Unknown` |

WaitingForUser is suspended, not terminal. Completed means a validated response is durable; it says nothing about delivery to the browser or analytical completeness. The complete allowed status/outcome combinations and cancellation/expiry race precedence remain open.

Response assembly is an Engine stage after data-plan execution, not a mandatory Respond graph node. A simple request can therefore have one data node while still receiving validation, audit and response assembly. This supersedes the old PRD's conflicting node-count terminology. Clarification forms are interaction state managed around resolver/data nodes, not a second engine.

## Agreed interaction transitions

| Trigger | State/action | Durable/public effect |
| --- | --- | --- |
| New request accepted | Queued | Persist job and job.accepted before HTTP 202 |
| Worker claims work | Running | Record execution ownership; emit actual phase/node progress |
| Missing user input | Stop new admissions, drain bounded running work, then WaitingForUser | Retain completed outputs and persist active form before clarification.required |
| Invalid/stale answer | Remain waiting | Field/conflict error; no resume |
| Valid answer commits | Queued for continuation | Persist answer/bindings/audit and clarification.accepted before HTTP 202 |
| Worker takes continuation | Running | job.resumed; run resolver or remaining nodes |
| New live ambiguity | Another form in same job | Retain prior accepted answers and explain new question |
| Response commits | Completed | Persist response, memory promotion and terminal event atomically where applicable |
| Client disconnects | No lifecycle transition | Work continues; client can replay or fetch state |

Scheduling follows fan-in: all required dependencies must be complete before a node runs. Node completion and output completeness are checked separately. For a plan requiring some failed inputs, fail-policy decides which independent results can still be presented with explicit gaps.

Initial session concurrency direction: at most one nonterminal job per session, including WaitingForUser. A competing new request returns an authorized conflict; the user can wait, cancel or use another session. Within-job and cross-session concurrency remain budgeted. Database enforcement is pending design.

## Verification and recovery boundaries

Static plan/contract/authorization checks precede source-data execution; DB-assisted validation is a separate bounded step before executing the query. Application persistence is not prohibited by source read-only policy.

Re-plans create a version and consume the same job budget. Reuse an output only if bindings, scope, contract version and freshness requirements remain valid. Persisted completed outputs are not rerun. An external read/model call that finishes before its completion commit can have an uncertain outcome after a crash; bounded retry policy must explicitly handle it rather than promise exactly-once external execution.

Worker lease/fencing, cancellation settlement, terminal races, clarification expiry and numeric budgets must be resolved with database/runtime design. No source transaction stays open during human waiting. Required audit failure blocks the protected transition; telemetry-export failure has a separate policy.

## Required follow-up design

- Full state/outcome matrix and transactional preconditions, including failure, cancel and expiry races.
- Lease claim, renewal, fencing and uncertain-attempt recovery.
- Exact reuse rules and retry/re-plan budgets.
- Atomic response/memory promotion and session concurrency constraints.
- Node-kind/status schema and version compatibility.
