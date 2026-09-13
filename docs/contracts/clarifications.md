# Clarification contract

Status: agreed interaction design, recorded 2026-09-08. Wire examples are illustrative; exhaustive JSON Schema, payload limits and resolver API are still required before implementation. Lifecycle is owned by [Engine](../architecture/engine.md), transport by [API](api.md) and [SSE](sse.md).

## Ownership and purpose

A clarification is a versioned form belonging to an existing job. It binds missing inputs; it never creates a replacement job. The Engine owns questions, resolvers, validation and continuation. The frontend renders semantic field types and may adapt presentation hints without changing answer semantics. Public product text is English.

Batch all currently known ambiguities into one form. Ask a subsequent stage only when new information is required to discover its valid choices. This replaces the old absolute maximum-one-question guarantee. Never re-ask a resolved slot without recording why its answer is no longer valid.

## Form model

Every form carries `schema_version`, `clarification_id`, `job_id`, `revision`, associated plan version, purpose, fields, suggestions, expiry and continuation bindings. A field has a stable ID, semantic type, label, required flag, validation constraints and optional display hint/dependencies.

| Semantic type | Possible UI | Answer |
| --- | --- | --- |
| `single_choice` | Radio, select, searchable select | One server-issued option ID |
| `multiple_choice` | Checkboxes, multi-select | Server-issued option IDs |
| `text` | Input, textarea | Bounded string |
| `number` | Numeric input | Typed numeric value under field constraints |
| `date` | Date picker | Valid calendar date under the declared format |
| `date_range` | Range picker | Start/end under explicit boundary semantics |
| `boolean` | Yes/no, checkbox | Boolean; false is distinct from unanswered |

Choice IDs resolve server-side to authorized typed bindings; labels are not entity identifiers. Recheck ownership, option membership, dependencies, scope and current authorization at submission. Never trust hidden fields or a client-supplied entity ID as authorization.

Large option sets use a bounded, authorized, paginated resolver tied to the form and field. Its endpoint and cursor schema remain open. Do not send thousands of options to the model/client. If selecting all matches is supported, represent a server-validated scoped selection with defined result identity; it does not mean all options on the visible page. The exact all-matches payload is open.

## Dependencies and stages

- Known dependencies are displayed within one form (for example, account type controls applicable fields). No new LLM turn is required merely to show/hide known fields. Server validation uses the same dependencies.
- Data-dependent stages submit through the normal responses endpoint, resume an approved resolver, and can produce a new form with a new clarification ID. Earlier accepted answers remain durable and are reused.
- Updating an unsubmitted parent field invalidates dependent draft answers. Changing an already accepted parent requires explicit revalidation/re-plan; a stale form cannot silently edit accepted history.
- Show a descriptive stage label, not a fabricated total stage count.
- A no-match path allows a refined search. A substantive change in the requested analysis is handled through intent validation/re-plan, not forced into a field value.

Example: resolve three matching clients → user selects one → resolver finds multiple accounts → user chooses an account/subset → same job continues.

## Suggestions

Allow examples, explained non-identity defaults, and supported alternatives such as refining a search. Suggestions are not submitted answers until the user explicitly submits them. Do not preselect an identity based on model confidence. Display only authorized distinguishing attributes.

Parameter presets such as a reporting period disclose concrete dates. Suggestions must not silently change office scope or the analytical population. A suggested follow-up analysis is distinct from a clarification answer and requires a new authorized request when selected.

## Submission and validation

Submit through `POST /chat/jobs/{job_id}/responses` with `Idempotency-Key`:

```json
{
  "clarification_id": "clr_01",
  "revision": 1,
  "answers": {
    "client": {"type": "single_choice", "option_id": "opt_02"}
  }
}
```

The server validates ownership, lifecycle, clarification revision, required fields, types, choice membership and dependencies. Ordinary validation errors return per-field errors without another LLM call. Rejected answers do not mutate the accepted form or resume execution.

Acceptance durably coordinates the answer, resolved slots, clarification state, runnable job state, audit and public events. Return HTTP 202 only after that commit. Scheduling occurs asynchronously. Accepted facts record `user_confirmed` provenance. Actual `job.resumed` is emitted when a worker resumes, not when HTTP merely acknowledges the answer.

Same key and same payload replays the stored acknowledgement without resuming twice. Same key with another payload or a stale revision produces a conflict. Unavailable options require a refreshed form with an explanation. See [API](api.md) for errors.

## Pause, expiry and audit

When clarification becomes necessary, stop admitting new nodes and drain currently running work within its existing deadline before suspending. Completed durable outputs are retained; no source transaction stays open while waiting for a person.

Clarification/resolver rounds and waiting duration are bounded. Numeric limits are pending runtime design. Exhaustion reports the unresolved limitation; it never guesses identities or starts an infinite question loop. Expired jobs cannot be silently resumed.

Audit records form/plan revision, unresolved slot, resolver and option-set references, validation outcome, accepted answer provenance and why another stage was required. Sensitive choices belong in controlled evidence storage, not general logs or public events.

## Acceptance scenarios

- Radio and searchable select yield identical single-choice semantics; checkbox false is not missing.
- Conditional required fields validate correctly; changing a parent invalidates dependent choices.
- Two data-dependent stages continue one job and retain completed outputs.
- Double submit/network retry is idempotent; stale revisions do not resume.
- Foreign, expired or out-of-scope choices are rejected.
- Long option lists are paginated; all-matches never means visible-page-only.
- Suggestions do not auto-submit and no-match can refine search.
- Budget exhaustion and expiry terminate waiting predictably without fabricated bindings.

## Manual input dan skip (disepakati 2026-09-12)

Ditambahkan setelah tinjauan kelengkapan schema. Skema JSON penuh dan batas numerik tetap terbuka.

### Jenis jawaban (`answer_kind`)

Mengetik manual didukung. Setiap jawaban membawa jenis eksplisit:

| `answer_kind` | Arti |
| --- | --- |
| `option_id` | Memilih opsi terbitan server. **Satu-satunya jenis yang mengikat identitas.** |
| `typed_value` | Nilai yang diketik untuk field non-identitas (text/number/date/date_range/boolean) sesuai constraint field. |
| `refine_search` | Teks bebas sebagai **istilah pencarian**; memicu resolver ulang dan menghasilkan opsi baru. |
| `change_intent` | Pengguna mengubah permintaan analisisnya; ditangani melalui validasi intent/re-plan, bukan dipaksa menjadi nilai field. |

**Aturan keamanan:** teks bebas pada slot identitas **tidak pernah** menjadi binding. Ia selalu diperlakukan sebagai `refine_search`. Hanya option ID terbitan server yang mengikat entitas, konsisten dengan larangan mempercayai entity ID dari klien sebagai otorisasi.

Penyimpanan jawaban memisahkan `answer_kind`, teks mentah, dan binding hasil resolusi.

### Aksi `skip`

Pengguna dapat menghentikan percakapan saat sebuah form aktif. Skip **bukan abort**: ia adalah instruksi eksplisit yang menandai job selesai.

- Valid **hanya** ketika job berada pada `WaitingForUser`.
- Dikirim melalui `POST /chat/jobs/{job_id}/responses` dengan `action: "skip"` beserta `Idempotency-Key`; tidak ada endpoint baru.
- Engine berhenti menerima node baru, menyusun response, lalu **commit response + memory promotion + terminal event secara atomik** — jalur commit yang sama dengan jawaban normal.
- Hasil: `lifecycle = Completed`, `outcome = SkippedByUser`, `completeness = Partial` atau `Unknown`.
- Response tetap dikembalikan kepada pengguna (menyatakan bahwa ia memilih tidak melanjutkan) dan **menyertakan hasil parsial yang sudah valid** dengan gap yang ditandai eksplisit; output node yang telah selesai tidak dibuang.
- Setelah skip job bersifat terminal dan tidak dapat dilanjutkan. Pertanyaan terkait berikutnya adalah job baru yang membaca session memory.
- Form dicatat dengan terminal state `skipped` beserta aktor dan waktu.

Skip berbeda dari `cancel`: cancel berlaku pada job yang sedang berjalan, berakhir `Cancelled`, tanpa kewajiban response document dan tanpa memory promotion.
