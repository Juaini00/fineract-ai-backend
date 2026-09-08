# Job HTTP contract

Status: agreed endpoint responsibilities, recorded 2026-09-08. Full OpenAPI/JSON Schema, exact authentication integration, pagination limits and error-code registry are pending. This document does not claim a complete API specification.

## Endpoints

| Method/path | Responsibility |
| --- | --- |
| `POST /chat/jobs` | Accept a new question in a session; return 202 after durable creation |
| `GET /chat/jobs/{job_id}` | Read a consistent state snapshot, active clarification or final response reference, and event cursor |
| `GET /chat/jobs/{job_id}/events` | Authorized SSE replay and live events |
| `POST /chat/jobs/{job_id}/responses` | Accept a versioned clarification answer on the same job; return 202 after commit |
| `POST /chat/jobs/{job_id}/cancel` | Request cancellation explicitly; disconnecting is not cancellation |

Clarification option search, session/message pagination, response retrieval and dataset pagination need endpoint schemas in subsequent design work. This inventory is deliberately limited to the agreed job interaction.

HTTP JSON uses `{success, data, error}` with inactive branches null. Stream framing is separately defined in [SSE](sse.md); it is not one long JSON envelope. Public errors are sanitized and contain machine-readable codes. Response/form payloads are versioned.

## Acceptance and idempotency

Job creation and clarification submission require `Idempotency-Key`. Scope keys to the authenticated principal and logical operation (including target job where applicable). Compare a canonical payload fingerprint; never expose raw sensitive inputs through the fingerprint or logs.

- Same key/payload returns the stored original acknowledgement without new execution.
- Same key/different payload returns 409.
- Concurrent identical submissions converge on one accepted operation through database enforcement; details belong in database design.
- Retry handling still checks current authentication/ownership before disclosing a stored acknowledgement.
- Key retention and retry horizon must be specified before implementation; clients cannot assume deduplication forever.

HTTP 202 means the operation is durably accepted, not that execution or analysis succeeded. Acknowledgements include job ID and sufficient resource references to fetch state/subscribe. Field-level answer errors return 422 without state mutation or another model call. A stale clarification revision or conflicting lifecycle returns 409 with an authorized current-state reference. Authentication and non-disclosing ownership errors will be finalized with the security contract.

Cancellation marks an active job for cancellation; workers stop admitting work and settle active attempts according to the Engine contract. Terminal jobs are not reopened. Exact repeat-cancel HTTP semantics remain part of the error/state matrix review.

## Snapshot and event cursor

GET state must return one consistent snapshot and an opaque cursor identifying the durable event boundary represented by it. The active form and/or final response reference must correspond to that snapshot. A new subscriber replays events after that cursor, eliminating the gap between fetching state and subscribing.

Late subscribers can retrieve a completed result even if the job finished before SSE connected. Clients must not infer failure or success solely from an open/closed connection. See [SSE](sse.md) for stale cursor and duplicate handling.

## Security and audit boundaries

Validate bearer identity and ownership on every operation, including replay, clarification options and result access. Durable tokens never appear in URLs. Authorization refresh and expiry behavior remain dependent on the dashboard identity decision.

Accepted state transitions, required audit records and corresponding public events commit together where they share the application database. External model/source calls are outside that transaction. Do not acknowledge success before persistence. Required audit persistence is a prerequisite to advancing protected work; general telemetry export failures have a separate operational policy.

## Acceptance scenarios

- Job/answer acknowledged only after durable acceptance.
- Retry after a lost HTTP acknowledgement does not create/resume twice.
- Snapshot followed by subscription has no missing transition.
- Refresh restores the same active clarification or persisted result.
- Invalid fields and stale forms do not mutate job state.
- Unauthorized users cannot inspect jobs, choices, cursors or stored acknowledgements.
