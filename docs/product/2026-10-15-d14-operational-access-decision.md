# D14 — operational tenant-wide access (owner decision record)

Date: 2026-10-15. Recorded by the FIN-124 worker from the owner decision relayed by
the Orca coordinator in the current conversation. This file records **the decision
only**. It is deliberately separate from the FIN-124 implementation changes and
changes no other contract document (`docs/data/analytical-contracts.md` §6,
`docs/data/dataset-inventory.md` XR-SCOPE and the scope decisions are untouched);
folding it into them is the owner's decision, in the owner's own commit.

## Decision

Specifically for **D14** (scheduler/batch job runs, `dataset-scope-decisions.md:174-180`):

1. An authenticated administrator bound to the source connection's tenant may read the
   approved **job definition** and **job run** fields tenant-wide, without an office
   predicate.
2. The read is declared by a **dedicated operational declaration**
   (`operational_tenant_wide`), **not** `org_wide_reference`. It is a separate,
   narrower class: it does not widen the product-master exception and it is not
   evidence about any office-bound fact.
3. **No office filter is invented.** No approved relationship ties a job or a run to an
   office, so an office-specific scheduler question is refused, never answered with a
   guessed filter.
4. **No raw error text.** `job_run_history.error_message`, `job_run_history.error_log`
   and `job.initializing_errorlog` are never read or returned.
5. **No write.** Nothing triggers, re-runs, enables, disables or edits a job; a request
   that mixes a read with a re-run is refused as a whole and states that nothing was
   triggered or changed.

## What the decision does not do

- It does not extend tenant, PII, write or SQL-surface permissions
  (`analytical-contracts.md` §6 already says the org-wide exception does not either).
- It does not make a scheduler row evidence of business coverage. A successful run
  never proves business-period or account coverage (XR-ASOF); coverage is proven from
  account-level facts or the analysis is `Partial`/`Unknown` with a reason.
- It does not interpret calendar words ("semalam", "last night"). Windows are explicit
  half-open ranges over the stored timezone-naive scheduler clock.

## Subsequent owner decision: stored-clock windows

The owner selected **explicit stored-clock windows only** while the deployment's
scheduler storage timezone remains unverified. A window must explicitly refer to
the stored timezone-naive timestamps. Do not convert it to a business timezone
or infer "semalam"/"last night"; refuse or clarify such requests instead.
This does not establish the storage timezone or prove a cross-midnight fixture.

The owner did **not** select a new coverage policy: they requested checking the
existing documents first. `database-design.md` I4 already distinguishes unknown
completeness from known incompleteness; `responses.md` §3 requires the worst
contributor's completeness and a reason. A missing source-to-coverage mapping
does not authorize a `Complete` freshness claim.


## How the implementation expresses it

| Decision | Mechanism |
| --- | --- |
| Dedicated class | query guard `operational_tenant_wide: true`, mutually exclusive with `org_wide_reference` and with an office filter (catalog validator `fineract_scope_class_declared`, `operational_not_office_fact`) |
| Approved tables | `knowledge/policies/office_scope.yaml` `operational_tenant_tables` (`job`, `job_run_history`, `m_business_date`); SQL tables must equal the manifest tables (`approved_operational_tenant_only`) |
| Admin of the tenant | the same gate the org-wide masters use (`global_reference_allowed`: active admin, `fineract_tenant` matches, `source = admin_projection`) |
| No invented office filter | planner refuses an office-specific request (`operational_office_unavailable`); scheduler SQL has no office parameter |
| No raw errors | manifests may not output `error_message`/`error_log` (`no_raw_error_output`); `error message`, `error log`, `stack trace` are `secret_never_expose` surface terms |
| No write | domain `unsupported_intents` plus the deterministic write guard, which now also refuses a read that asks for a job re-run |
