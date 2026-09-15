# Job progress and SSE contract

Status: agreed protocol behavior, recorded 2026-09-08. SSE is a product
experience surface, not the internal audit stream.

> **Status implementasi (2026-09-15).** `GET /chat/jobs/{id}/events` berjalan:
> replay dari cursor, cursor tidak sah gagal eksplisit, duplikat aman,
> disconnect bukan cancel, terminal menutup stream, fallback polling dibuktikan
> dengan notifikasi dimatikan. **Belum**: retensi replay/purge riwayat, auth
> expiry di tengah stream, dan verifikasi melalui proxy deployment nyata.
> Bentuk frame dan kosakata event yang benar-benar dipancarkan ada di
> [api-reference.md §7](api-reference.md#7-sse--get-chatjobsjob_idevents);
> status lengkap di [status.md](../status.md).

## Transport and durability

Endpoint: `GET /chat/jobs/{job_id}/events`, UTF-8 `text/event-stream`. The dashboard uses fetch-based SSE to send the bearer header; the client explicitly manages framing, cursor persistence, reconnect, backoff and authentication refresh. Native EventSource automatic reconnection is not assumed for a fetch implementation.

Public events are persisted in PostgreSQL with a strictly increasing sequence per job. Event/state changes must have a consistent commit boundary. Redis notifies subscribers of available work but is not the durable event source. A missed notification is recoverable by reading PostgreSQL; bounded fallback polling must be specified and capacity-tested.

Deliver duplicates safely; do not promise exactly-once delivery. Ordering describes committed events, not an invented execution order between parallel nodes. The frontend applies each sequence at most once.

## Public phases

| Phase | Example English label |
| --- | --- |
| `queued` | Your request is queued. |
| `understanding` | Understanding your request. |
| `mapping_knowledge` | Finding relevant reporting capabilities. |
| `planning` | Preparing the analysis. |
| `validating` | Checking the analysis scope and requirements. |
| `resolving_entities` | Finding the matching accounts. |
| `querying` | Retrieving the required data. |
| `analyzing` | Comparing and aggregating the results. |
| `composing_response` | Preparing your report. |
| `validating_response` | Checking the report against its supporting data. |
| `waiting_for_user` | Your selection is needed to continue. |

Emit a phase only when the corresponding work actually occurs. Fast-paths skip unnecessary phases; a re-plan may revisit phases. Phase is an experience projection, not another job state machine. Display parallel node statuses alongside the phase. Do not expose SQL, raw prompts, stack traces or internal model reasoning.

Show current activity, elapsed time and counts of completed nodes for the current plan version where useful. Do not convert node counts into a claimed percentage of execution time. Re-plans may change the node count. Heartbeats establish connection liveness only, not worker progress; worker leases/deadlines independently detect stalled execution.

## Event vocabulary

| Event | Trigger |
| --- | --- |
| `job.accepted` | Durable job creation |
| `job.phase_changed` | Actual public phase transition |
| `node.status_changed` | A node changes execution status |
| `clarification.required` | A durable active form is ready to present |
| `clarification.accepted` | A valid answer commits |
| `job.resumed` | Worker starts the resumed execution |
| `job.notice` | Relevant retry, delay or coverage limitation |
| `job.completed` | Validated response is durable |
| `job.failed` | Operational failure is durable |
| `job.cancelled` | Cancellation has settled durably |
| `job.expired` | Expiry has settled durably |

Durable envelopes carry `schema_version`, `job_id`, `sequence`, `occurred_at`, applicable `plan_version`, and event-specific fields. Node events identify their node. Clarification events identify the active form and revision, with a bounded form payload or retrieval reference. Completion identifies a response version and completeness, with bounded content or an authorized retrieval reference. The exact inline-size threshold remains open.

```text
id: 42
event: job.phase_changed
data: {"schema_version":1,"job_id":"job_01","sequence":42,"plan_version":2,"phase":"mapping_knowledge","message":"Finding relevant reporting capabilities.","occurred_at":"2026-09-08T12:00:00Z"}

```

IDs in examples are illustrative. Event cursors are scoped to the authorized job and validated by the server, never treated as authorization credentials.

## Reconnect and snapshot recovery

1. Fetch the consistent job snapshot/cursor described in [API](api.md).
2. Subscribe using that cursor as `Last-Event-ID`; replay committed events strictly after it.
3. Follow live events without a replay-to-live race; notifications cannot replace reading the durable sequence.
4. Reconnect with the last applied cursor and ignore duplicates.
5. If the cursor is expired or invalid, require a fresh snapshot explicitly; do not silently omit missing history. Exact wire error/control encoding remains open.
6. After a terminal event or terminal snapshot, the client stops reconnecting. A closed connection alone is not terminal proof.

On `WaitingForUser`, the frontend may close the stream after receiving the form. After submitting an answer it resubscribes from the saved cursor or a new snapshot, so progress emitted before reconnection is retained. Refresh retrieves the same pending form.

## Runtime behavior

- Connection loss never cancels a job; use the cancel endpoint.
- Heartbeat comments keep transport alive and are not persisted as audit or public progress events.
- Bound outgoing buffers; disconnect slow clients and allow replay rather than consuming unbounded memory.
- Disable/bypass proxy buffering that delays events and verify flush behavior through the real deployment proxy.
- Reauthorize subscriptions. Token expiry/revocation must stop protected delivery according to the final identity policy; never solve reconnect by putting bearer tokens in URLs.
- Initial release streams genuine progress and the validated final response, not unchecked narrative tokens.

## Acceptance scenarios

- Fast completion before subscription still yields the final response.
- Snapshot/subscribe and replay/live boundaries do not lose events.
- Network loss, duplicate delivery and reconnect preserve correct UI state.
- Expired cursor causes explicit snapshot recovery.
- Waiting/submit/reconnect and multi-stage clarification continue one job.
- Parallel nodes and re-plan display correct plan version and node counts.
- Heartbeats cannot mask a dead worker; Redis outage can recover from durable events.
- Slow clients, auth expiry and proxy buffering are tested without unbounded buffers or data leaks.

## References

- [WHATWG SSE standard](https://html.spec.whatwg.org/dev/server-sent-events.html): event framing and Last-Event-ID.
- [MDN SSE usage](https://developer.mozilla.org/en-US/docs/Web/API/Server-sent_events/Using_server-sent_events): transport behavior and deployment considerations.
