# Dataset inventory — full-release catalog coverage (L1C)

Ticket: FIN-152 (LC.0a), first child of FIN-107 in `blocks` order. Status: formal
inventory drafted 2026-09-27; **not yet cross-checked against `knowledge/`**
(that alignment is FIN-153). This document is a comparison surface, not a
capability catalog — it does not add, remove, or edit any `knowledge/*` or
`queries/*` asset.

**Language deviation.** The ticket brief that produced this document asked for
Indonesian. The workspace root
[`CLAUDE.md`](../../../CLAUDE.md#layout) fixes the language for **new**
documents and new sections as English (owner decision, 2026-09-27), overriding
any older, more specific instruction. This document is new, so it is written in
English; existing Indonesian documents it cites keep their own language.

## 0. Purpose, sources and what this is not

**Purpose.** [`build-order.md`](../build-order.md) §3 (L1C) requires, before any
further catalog or retrieval work: a row for every line in
[`dataset-scope-decisions.md`](../product/2026-09-09-dataset-scope-decisions.md)
§1 (the eight baseline domains) and every decision D01–D15, each carrying
requirement → source → grain → field/measure → relations/cardinality →
office-scope path → time/as-of/currency → evidence rule → acceptance →
capability status. This document is that inventory.

**Sources read for this inventory** (path\:line cited inline per row where a
specific claim depends on it):

- [`product/2026-09-09-dataset-scope-decisions.md`](../product/2026-09-09-dataset-scope-decisions.md)
  §1 (`:19-38`), §2 (`:40-58`), §3 D01–D15 (`:64-194`), §5 (`:208-216`) — the
  requirement authority.
- [`product/prd.md`](../product/prd.md) §2, §6–§7 — scope and intent-preservation
  rules that bound what a row may claim.
- [`data/analytical-contracts.md`](analytical-contracts.md) §1–§2, §5–§7 — Mode
  1/Mode 2, grain/measure vocabulary, office-scope enforcement, currency rule.
- [`data/database-design.md`](database-design.md) §1 (I1–I8), §2 (C1–C20, D1–D6)
  — invariants a future capability/contract on these rows must respect.
- `knowledge/data-scope/areas/*.yaml`, `knowledge/domains/*.yaml`,
  `knowledge/capabilities/{client,organization,savings,group}/*.yaml` — the
  **current, MVP-era** state being compared against, read but not edited (its
  realignment is FIN-153).
- `fineract_default` (local read-only Postgres, this session, 2026-09-27) —
  existence and row counts for every table cited as "known" below. Query used:
  `SELECT count(*) FROM <table>` per table, run individually; results in §4.

**What this is not.** No SQL, capability, migration, or fixture was written or
run against Fineract to produce this document (read-only `SELECT count(*)` and
`information_schema` lookups only, per the ticket's constraint and D13). No row
here is a capability approval — approval still requires the four
`knowledge/CARRY-OVER.md` rules (build-order.md §3 point 3). Coverage here means
"the requirement has a row with a stated status," not "the system can answer
it."

**Status vocabulary used in every table:**

| Status | Meaning |
| --- | --- |
| `inherited` | Requirement is met by an **existing, approved** Mode-1 capability today. |
| `gap` | Requirement is agreed scope but lacks an approved capability; source mapping may be known, candidate, or unresolved. |
| `deferred-onboarding` | Requirement's existence/shape depends on a specific deployment (D10 custom datatables, D15 surveys/PPI/credit-bureau) — cannot be closed generically; see §7. |
| `excluded` | Decision explicitly forbids this execution surface (D13); it is not a missing capability. |
| `cross-reference` | Requirement is inventoried under another row; this row does not create a second capability obligation. |
| `summary` | Decision heading groups sub-rows; its status is determined by those sub-rows. |

**Mapping confidence** (own column per row, per the brief):

| Confidence | Meaning |
| --- | --- |
| `known` | Table existence and row count verified against `fineract_default` this session (§4 has the query result). |
| `candidate` | Table name is the standard Fineract convention for this concept but was **not** queried this session (rare — used only where verifying every column would not change the row's status). |
| `unresolved` | Business semantics (which enum value means what, which column is authoritative) is not settled even though the table exists — flagged explicitly, never guessed. |

---

## 1. Cross-domain rules (stated once, referenced by ID from every row)

These are FIN-107's 2026-09-20 decisions
([dataset-scope-decisions.md:16](../product/2026-09-09-dataset-scope-decisions.md))
plus their owning contract sections. Every row below that needs one of these
cites it by tag (`[XR-n]`) instead of repeating the rule.

| Tag | Rule | Owning section |
| --- | --- | --- |
| `XR-CUR` | Totals are per `currency_code`; no automatic conversion. Cross-currency consolidation only via an `exchange_rates` exact-match row, `exchange_rate_id` recorded (C19). No exchange-rate table exists in `fineract_default` today (verified, §4) — D03 stays fully open until one is chosen. | analytical-contracts.md §5, §6 (C19) |
| `XR-GRAIN` | Every measure declares its grain (client / account / transaction). Aggregating child rows before a `1:N` join is mandatory; a join that would duplicate a parent-side measure without stating the resulting grain is rejected (§2.2). | analytical-contracts.md §2.2, §5 |
| `XR-PAGE` | Result pagination is keyset (`sort_key_json`), not offset. | database-design.md §3 (L3, `dataset_lifecycle.md`) |
| `XR-PII` | Field sensitivity classes (`public_business` / `sensitive_business_identifier` / `pii` / `security_sensitive` / `secret_never_expose` / `free_text_sensitive`) gate output; `pii` needs `can_view_pii` **and** capability approval; `secret_never_expose` never appears anywhere. Column-to-class mapping is a separate, unfinished audit (#15). | analytical-contracts.md §2.1 |
| `XR-SCOPE` | `office_ids` comes from `authorized_scope`, never widened by the user. Every office-bound Fineract fact query declares an `office_scope_path` and `require_office_filter = true`, enforced inside SQL with a bound parameter, never a Rust-side filter. Verified organization-wide reference/configuration masters may instead declare `org_wide_reference` and be read tenant-wide by an authenticated admin, without an office predicate on the master row; this is a narrow exception, not a way to expose office-bound facts. A join to account/client/transaction facts still filters their authorized offices in SQL, and a global master alone cannot prove office-specific product usage or availability. | analytical-contracts.md §6 |
| `XR-ASOF` | `as_of` / freshness is declared per contract; if a required batch (COB, interest posting, provisioning) has not run, the affected analysis is marked `Partial`, not silently wrong (ties to D01/D07/D11/D14). | database-design.md I4; dataset-scope-decisions.md D14 |
| `XR-EVID` | Config/relationship existing is not proof money moved; schedules are not events; a numeral in narration must trace to an evidenced block or a declared `derivation` (D3, responses.md §4). | dataset-scope-decisions.md §1 rules; database-design.md §2.2 D3 |
| `XR-MODE` | Mode 1 (curated capability) is the only executable path now; Mode 2 (analytical contract compiled to SQL) is L8, gated on FIN-98. A row with no Mode-1 capability and no plan to author one this phase is still `gap`, not `Unsupported`-by-design. | analytical-contracts.md §1; build-order.md L8 |

---

## 2. Domain A — Organization
(dataset-scope-decisions.md:23)

| ID | Requirement | Source table(s) & grain | Field / measure | Relationship (cardinality) | Office-scope path | Time / as-of / currency | Evidence rule | Acceptance | Capability (Mode-1) | Confidence | Status |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| ORG-1 | Manage Offices | `m_office` (grain: office) | `id, parent_id, hierarchy, name, opening_date` | self-referencing `1:N` (`parent_id`) forms the office tree | `id` itself (root of `XR-SCOPE`) | none (reference data) | n/a | office list/tree resolvable and scope-filterable | `organization/office_list_basic`, `office_hierarchy_tree`, `office_identity_resolve`, `office_name_lookup` | known (8 rows, §4) | inherited |
| ORG-2 | Manage Holidays | `m_holiday` (grain: holiday) | name, from/thru date, office applicability | `N:1` to office (or org-wide) | via office link on the holiday row | date range | n/a | holiday list scoped and dated | none | known (1 row, §4) | gap |
| ORG-3 | Manage Employees (staff) | `m_staff` (grain: staff) | name, office_id, is_loan_officer, is_active | `N:1` `m_office` | `office_id` | n/a | staff list scoped by office | none directly; staff appears as `officer` join in savings/organization capabilities, no standalone staff-listing capability | known (25 rows, §4) | gap |
| ORG-4 | Currency Configuration | `m_organisation_currency` (enabled currencies), `m_currency` (ISO reference, 164 rows — **not** all enabled) | `code, decimal_places, in_multiples_of` | `m_organisation_currency` is the enabled subset of `m_currency` | n/a | n/a | `XR-CUR` — enabling a currency is not an exchange rate | which currencies are configured/enabled, at what precision | none | known (`m_organisation_currency`=6, `m_currency`=164, §4) | gap |
| ORG-5 | Working Days | `m_working_days` (grain: singleton config row) | `recurrence` (RRULE-style), `repayment_reschedule_type` | n/a (org-wide config) | n/a | n/a | n/a | working-day rule readable | none | known (1 row, §4) | gap |
| ORG-6 | Payment Type | `m_payment_type` (grain: payment type) | `name, is_cash_payment, position` | referenced by `m_payment_detail` used across loan/savings transactions | n/a | n/a | n/a | payment type reference list | none | known (6 rows, §4) | gap |
| ORG-7 | Loan Provisioning Criteria | `m_provisioning_criteria` (criteria header), `m_provisioning_criteria_definition` (per-bucket %) | `criteria_name`; definition: category × min/max days × percentage | `1:N` criteria → definitions; `N:1` from `m_loanproduct_provisioning_mapping` (products) | n/a (criteria is org-wide) | n/a | criteria is config, not a result — see D07/PROD-6 for the actual provisioning **result** | criteria list and its per-bucket definitions | none | known (`m_provisioning_criteria`=0, `m_provisioning_criteria_definition`=0 rows — configured but currently empty on this fixture, §4) | gap |
| ORG-8 | Business date (supporting reference) | Not a Fineract table — business date is a runtime/config value (Fineract `business-date` global config or `m_global_configuration` row); needs verification which mechanism this deployment uses | n/a | n/a | n/a | drives `XR-ASOF` for every "as of today" answer | as-of anchor for freshness statements | none | unresolved — global-config table not queried this session | gap |
| ORG-9 | Restricted fund (supporting reference) | `m_fund` (Fineract fund/restricted-fund table — not queried this session) | fund name, external_id | `N:1` from `m_loan.fund_id` | n/a | n/a | n/a | fund reference resolvable when a loan cites one | none | unresolved (existence not verified this session) | gap |
| ORG-10 | GL code/name for product/provisioning mapping (supporting reference) | `acc_product_mapping` (259 rows, §4), joined to `acc_gl_account` (362 rows, §4) | `product_id, product_type, financial_account_type` → `gl_code, name` | `N:1` product → GL account per mapping type | GL account is org-wide, not office-scoped | n/a | supports GL traceability (D12), not a report on its own | product-to-GL mapping resolvable by name/code | none | known (§4) | gap |
| ORG-11 | Enum / reference values (supporting reference) | `m_code` + `m_code_value` (Fineract's generic lookup tables — not queried this session) | code name → value id/label | referenced from many domains (`gender_cv_id`, `closure_reason_cv_id`, etc.) | n/a | n/a | enum resolution must be declared before a capability uses it (mirrors L1 rule 4 / `XR-EVID`) | code/value lookup resolvable | none | candidate (standard Fineract tables, not queried this session) | gap |

**Note on org-wide vs office-scoped rows** (`XR-SCOPE`): ORG-2, ORG-4, ORG-5,
ORG-6, ORG-7, ORG-10, ORG-11 and the master/config portions of PROD-1..11
have no meaningful per-office split. Read-only global-reference access requires
the declared and validated §6 exception; office scope still applies when these
rows join an office-scoped fact table. Pledged collateral under PROD-5 remains
office-scoped, not a global master.

---

## 3. Domain B — Products
(dataset-scope-decisions.md:24)

| ID | Requirement | Source table(s) & grain | Field / measure | Relationship (cardinality) | Office-scope path | Time / as-of / currency | Evidence rule | Acceptance | Capability | Confidence | Status |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| PROD-1 | Loan Products (master, incl. unused) | `m_product_loan` (grain: product) | name, currency_code, interest terms, min/max principal | `1:N` to `m_loan` (usage) — master listed independent of usage per §1 rule | none (product is org-wide unless product-office restriction table is checked) | n/a | product master ≠ account usage; unused products must still list | none | known (12 rows, §4) | inherited |
| PROD-2 | Savings Products | `m_savings_product` (grain: product) | name, currency_code, nominal_annual_interest_rate | `1:N` to `m_savings_account` | none | n/a | same as PROD-1 | `savings/products_by_client` (usage-side only — no standalone product-master capability) | known (23 rows, §4) | inherited |
| PROD-3 | Share Products | `m_share_product` (grain: product) | name, currency_code, total_shares | `1:N` to `m_share_account` | none | n/a | same as PROD-1 | none | known (1 row, §4) | inherited |
| PROD-4 | Charges (master) | `m_charge` (grain: charge definition) | name, currency_code, charge_time_enum, charge_calculation_enum, is_penalty | referenced by `m_savings_account_charge`, `m_loan_charge`, `m_client_charge`, `m_share_account_charge` | none | n/a | master vs actual charging is D04's separation | `savings/charge_type_identity_resolve`, `charge_count_by_type`, `charges_by_type` (savings-scoped only) | known (59 rows, §4) | inherited |
| PROD-5 | Collateral Management | `m_collateral_management` (type master), `m_client_collateral_management` (8 rows), `m_loan_collateral_management` (12 rows) | collateral type, quantity, base value/pct → collateral value | `1:N` client → pledged collateral; `N:M` loan ↔ collateral via `m_loan_collateral_management` | via client/loan office path | n/a | pledged ≠ realized; no valuation simulation | collateral list per client/loan | none | known (§4) | inherited |
| PROD-6 | Delinquency Buckets | `m_delinquency_bucket` (4 rows, grain: bucket), `m_delinquency_range` (6 rows), `m_delinquency_bucket_mappings` (10 rows, bucket↔range), `m_product_loan.delinquency_bucket_id` (product↔bucket FK) | bucket name; range classification/min/max days overdue; product ID attached to bucket | `1:N` bucket → range mappings; `1:N` bucket → loan products through `m_product_loan.delinquency_bucket_id` (not through `m_delinquency_bucket_mappings`) | n/a (config) | n/a | bucket assignment is config, not an actual arrears reading — see LOAN-9 | bucket/range list and product-to-bucket mapping, without implying a loan is currently delinquent | none | known (schema columns and FK path verified locally; counts §4) | inherited |
| PROD-7 | Products Mix | `m_product_mix` (grain: allowed/restricted product-to-product pairing) | restricted product ids | `N:M` self-join on `m_product_loan` | n/a | n/a | n/a | pairing rule list | none | known (0 rows — configured empty on this fixture, §4) | inherited |
| PROD-8 | Fixed Deposit Products | `m_savings_product.deposit_type_enum = 200` (FD) joined to `m_deposit_product_term_and_preclosure` (11 term rows across FD/RD) | min/max deposit term, pre-closure penalty terms | `1:1` FD product row ↔ term/preclosure row | none | n/a | product master, see FDRD-1..7 for account-level; projected maturity is not payout | standalone FD product and its term/preclosure configuration | none | verified locally: 8 FD products and 8 matching term rows; enum 200 also documented in `knowledge/datasets/savings/deposits.yaml` | inherited |
| PROD-9 | Recurring Deposit Products | `m_savings_product.deposit_type_enum = 300` (RD) joined to `m_deposit_product_term_and_preclosure` and `m_deposit_product_recurring_detail` (3 rows each for RD) | `deposit_amount` on product term; `is_mandatory`, `allow_withdrawal`, `adjust_advance_towards_future_payments` on recurring detail | `1:1` RD product ↔ term/preclosure row and recurring-detail row | none | n/a | product configuration is distinct from an account's contribution schedule; this deployment has **no product-level recurring-frequency column** in these tables, so do not invent or infer one from `lockin_period_frequency` | RD master amount and mandatory rule resolvable; product-level recurring frequency stays Unsupported without an approved source (account-level schedule belongs to FIN-110) | none | verified locally: 3 RD products with both detail rows; enum 300 documented in `knowledge/datasets/savings/deposits.yaml`; frequency source absent | inherited |
| PROD-10 | Tax Configurations | `m_tax_component` (51 rows), `m_tax_group` (16 rows), mapping table `m_tax_group_mappings` (not queried this session) | tax rate, component name → group | `N:M` component ↔ group via mapping | n/a | tax rate has an effective-date history (`m_tax_component_history`, not queried) | tax config vs actual tax withheld is a separate question (see SAV-9) | tax config list | none | known for component/group (§4); mapping table candidate | inherited |
| PROD-11 | Floating Rates | `m_floating_rates` (0 rows), `m_floating_rates_periods` (0 rows, §4) | rate name, base lending rate, differential, effective period | `1:N` floating rate → periods | n/a | period `from_date` | floating-rate product linkage not verified against `m_product_loan.is_floating_interest_rate` this session | floating rate schedule list | none | known — both tables exist and are **empty** on this fixture; feature likely unused by this tenant | inherited |

---

## 4. Domain C — Client / ownership
(dataset-scope-decisions.md:25)

| ID | Requirement | Source table(s) & grain | Field / measure | Relationship (cardinality) | Office-scope path | Time / as-of / currency | Evidence rule | Acceptance | Capability | Confidence | Status |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| CLI-1 | Identity / business profile | `m_client` (grain: client) | `display_name` (pii), `mobile_no` (pii), `date_of_birth` (pii), `client_type_cv_id`, `client_classification_cv_id`, `legal_form_enum` (person/entity), `m_client_non_person` for entity attributes (4 rows) | `1:1` client ↔ non-person extension where `legal_form_enum` = entity | `office_id` direct | n/a | `XR-PII` — pii fields gated | identity resolvable with PII gate honored | `client/identity_resolve`, `name_lookup` | known (43 rows, §4) | inherited |
| CLI-2 | Lifecycle | `m_client.status_enum, sub_status, activation_date, closedon_date, rejectedon_date, withdrawn_on_date, reactivated_on_date` | status/sub-status enum values (**unresolved** enum→label mapping not re-verified this session beyond what `lifecycle_summary` already declares) | n/a | `office_id` | full lifecycle date set | schedule/config ≠ event still applies: a status column IS the recorded event here | lifecycle summary by office scope | `client/lifecycle_summary`, `activation_monthly_breakdown`, `activation_top_n_offices` | known (§4) | inherited |
| CLI-3 | Office / staff assignment | `m_client.office_id, staff_id, transfer_to_office_id` | current office/staff; `transfer_to_office_id` signals pending transfer | `N:1` to `m_office`, `m_staff` | `office_id` | current, not historical (see D02) | current-office labeling required per D02 default branch | assignment resolvable | `client/summary_by_office`, `relationship_by_id`, `relationship_lookup` | known (§4) | inherited |
| CLI-4 | Group / center (if used) | `m_group` joined via `m_group_client` (3 rows) | group id/name, `level_id` (1=center per `m_group_level`, §4 evidence) | `N:M` client ↔ group via `m_group_client`; `m_group.parent_id` links group → center | `m_group.office_id` | n/a | this deployment **does** use group/center (2 groups, one at each level — verified, §4) | client's group/center resolvable | `group/group_identity_resolve` (group only, not client→group linkage) | known (§4) | gap |
| CLI-5 | All account types (loan, savings, share) owned by the client | `m_loan.client_id`, `m_savings_account.client_id`, `m_share_account.client_id` | account id/type/status per product | `1:N` client → each account type | via each account's own office path (§6, §8) | n/a | resource penghubung rule: "seluruh pinjaman Budi" resolved without default-active assumption (§1 line 34) | full account roster across the three types, unfiltered by default status | `client/clients_with_account_counts` (savings only) | known (§4) | gap (savings-only inherited; loan and share account rosters have no capability) |
| CLI-6 | Contact / address (selective) | `m_client_address` (1 row) | address fields (**pii/free_text_sensitive** — class not finalized, §2.1) | `1:N` client → address rows | via client | n/a | "selective" per §1 — not a default-on field set | address resolvable only where explicitly approved | none | known (1 row, §4) | gap |
| CLI-7 | Identity documents / files (not default) | `m_client_identifier` (0 rows) | identifier type + value (pii/sensitive) | `1:N` client → identifiers | via client | n/a | explicitly excluded from default output per §1 and `client-foundation.yaml` `excluded_tables` | excluded by default; row exists only to record the exclusion | none (by design) | known (0 rows, §4) | gap (explicitly excluded, not to be closed by a capability without a new decision) |
| CLI-8 | Notes (not primary source of truth) | Fineract's generic notes table (`m_note` — not queried this session) | free text | `1:N` from client/loan/savings/group to notes | via parent entity | n/a | `free_text_sensitive`; per §1 notes must never be treated as the authoritative source for a fact | excluded from analytical answers; may only ever be surfaced as a labeled note, never as evidence for a numeral (`XR-EVID`) | none (by design) | unresolved (table not queried this session) | gap |

---

## 5. Domain D — Loan
(dataset-scope-decisions.md:26; D15 adds two balance sub-areas here)

| ID | Requirement | Source table(s) & grain | Field / measure | Relationship (cardinality) | Office-scope path | Time / as-of / currency | Evidence rule | Acceptance | Capability | Confidence | Status |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| LOAN-1 | Identity / relations | `m_loan` (grain: loan account) | `account_no` (sensitive id), `client_id`, `group_id`, `product_id`, `loan_officer_id`, `fund_id` | `N:1` to client **or** group, product, staff, fund | via `m_loan.client_id → m_client.office_id`, else `m_loan.group_id → m_group.office_id` (no direct `office_id` on `m_loan`; locally all 116 loans have a client, 0 group-only, and every `m_loan_transaction.office_id` equals this path — verified FIN-108) | n/a | office path must be declared explicit per `analytical-contracts.md` §6.4 | loan identity resolvable, scope enforced through client/group | none | known (116 rows, §4) | gap |
| LOAN-2 | Lifecycle | `m_loan.loan_status_id`, `submittedon_date, approvedon_date, disbursedon_date, closedon_date, rejectedon_date, writtenoffon_date, overpaidon_date, charged_off_on_date` | status id + label from Fineract's own `r_enum_value` (`enum_name='loan_status_id'`: 100 submitted, 200 approved, 300 active, 400 withdrawn by client, 500 rejected, 600 closed, 601 written-off, 602 rescheduled, 700 overpaid; verified FIN-108, local values 100/200/300/600) | n/a | via client/group | full lifecycle date set | status is the recorded event | lifecycle resolvable | none | known (status labels verified FIN-108) | gap |
| LOAN-3 | Effective terms vs product now | `m_loan.nominal_interest_rate_per_period, interest_period_frequency_enum, term_frequency, number_of_repayments` vs `m_product_loan` current config | account-level effective terms vs product's current default | `N:1` loan → product | via loan | terms are as-approved on the loan, not "whatever the product says today" — an explicit distinction the requirement calls out | must not silently substitute current product config for the account's effective terms | none | known (§4) | gap |
| LOAN-4 | Planned/actual disbursement, staged | `m_loan.expected_disbursedon_date, disbursedon_date, net_disbursal_amount`; `m_loan_disbursement_detail` (0 rows, multi-tranche) | planned vs actual disbursement date/amount; per-tranche rows when staged | `1:N` loan → disbursement details (tranche) | via loan | disbursement date is the event date | planned ≠ actual must stay distinguishable | none | known (§4; 0 tranche rows on this fixture — single-disbursement loans only, so multi-tranche is unexercised) | gap |
| LOAN-5 | Schedule and paid/outstanding | `m_loan_repayment_schedule` (grain: installment) | `duedate, principal_amount, principal_completed_derived, interest_amount, interest_completed_derived` etc. | `1:N` loan → schedule rows | via loan | schedule is not an event (§1 rule) — must be labeled as planned, cross-checked against LOAN-6 for actuals | schedule vs actual repayment distinguishable | none | known (1,661 rows, §4) | gap |
| LOAN-6 | Transactions / allocations / reversals | `m_loan_transaction` (grain: transaction) | `transaction_type_enum, amount, principal_portion_derived, interest_portion_derived, fee_charges_portion_derived, penalty_charges_portion_derived, is_reversed, reversed_on_date` | `N:1` loan; `office_id` direct on this table (unlike `m_loan`) | `m_loan_transaction.office_id` direct | `transaction_date` | reversed transactions must not double-count (`is_reversed`) — mirrors savings `XR-EVID`/D3 pattern | allocations sum to transaction amount; reversals excluded by default | none | known (2,947 rows, §4) | gap |
| LOAN-7 | Charges/penalties (paid/waived/written-off/outstanding) | `m_loan_charge` (grain: account-charge) | `amount, amount_paid_derived, amount_waived_derived, amount_writtenoff_derived, amount_outstanding_derived, is_penalty` | `N:1` loan; `N:1` `m_charge` (master, PROD-4) | via loan | n/a | must separate charged from actually-collected/waived/written-off, mirroring D04's separation | charge lifecycle resolvable per state | none | known (217 rows, §4) | gap |
| LOAN-8 | Balances | `m_loan.*_derived` columns (`principal_outstanding_derived, interest_outstanding_derived, total_outstanding_derived`, etc.) | outstanding principal/interest/fees/penalties, total | n/a (denormalized on the loan row) | via loan | derived balances are Fineract-computed, not Jarvis-recomputed (mirrors GL rule in D12 — read recorded value, no re-derivation) | balance query reads `_derived` columns, never recomputes from schedule+transactions independently | none | known (§4) | gap |
| LOAN-9 | Arrears / delinquency with as-of/freshness | `m_loan_arrears_aging` (grain: loan, 1 row per loan currently in arrears) | `principal_overdue_derived, total_overdue_derived, overdue_since_date_derived` | `1:1` loan (only present while overdue) | via loan | `XR-ASOF` — arrears is a point-in-time snapshot table, freshness depends on COB having run (D14) | overdue amount tied to an as-of date, not asserted as always-current | none | known (59 rows, §4) | gap |
| LOAN-10 | Collateral / guarantor | `m_loan_collateral_management` (12 rows, PROD-5), `m_guarantor` (1 row), `m_guarantor_funding_details`, `m_guarantor_transaction` | guarantor identity/type; pledged collateral value | `1:N` loan → guarantor; `N:M` loan ↔ collateral | via loan | n/a | collateral/guarantor listed distinctly, not conflated | resolvable per loan | none | known (§4) | gap |
| LOAN-11 | Reschedule / terms-change / write-off / recovery | `m_loan.rescheduledon_date`; write-off via `writtenoffon_date` + `writeoff_reason_cv_id`; recovery via `m_loan_transaction` filtered by `transaction_type_enum` (recovery-repayment = `8` per both `r_enum_value` and Apache Fineract `LoanTransactionType`; 0 such rows locally. Note: local `r_enum_value` stops at 19 while rows carry 20/23/25/26/27/32 — labels for those come from `LoanTransactionType` source, verified FIN-108), `m_loan_recovery_payment` **does not exist** in this schema (verified missing, §4) | reschedule/write-off dates and reason; recovery amount via transaction filter | n/a | via loan | event dates | must be evidence-backed, not inferred from balance deltas | reschedule/write-off/recovery resolvable, recovery via transaction-type filter (no dedicated table) | none | known (recovery type value verified FIN-108) | gap |
| LOAN-12 (D15) | Capitalized-income / buy-down-fee balances | `m_loan.capitalized_income_derived, capitalized_income_adjustment_derived, buy_down_fee_calculation_type` etc. (columns present, §4 shows the columns exist on `m_loan`), `m_loan_capitalized_income_balance` (0 rows), `m_loan_buy_down_fee_balance` (0 rows) | capitalized income / buy-down fee balance amounts | `1:N` loan → balance history rows | via loan | n/a | D15 disposition: "masuk baseline Loan sebagai sub-area balance, butuh detail kontrak, bukan scope baru" — in scope, contract detail still open | resolvable once contract detail is written | none | known — tables exist, currently **empty** (feature unused on this fixture, §4) | gap |

---

## 6. Domain E — Savings
(dataset-scope-decisions.md:27)

| ID | Requirement | Source table(s) & grain | Field / measure | Relationship (cardinality) | Office-scope path | Time / as-of / currency | Evidence rule | Acceptance | Capability | Confidence | Status |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| SAV-1 | Identity / relations | `m_savings_account` (grain: account) | `account_no` (sensitive), `client_id`, `group_id`, `product_id`, `field_officer_id`, `deposit_type_enum` | `N:1` client **or** group, product, officer | via `client_id`/`group_id` → office | n/a | n/a | identity resolvable | `savings/account_identity_lookup`, `account_identity_resolve` | known (207 rows, §4) | inherited |
| SAV-2 | Lifecycle / dormant / block | `status_enum, sub_status_enum` (dormant/block is a `sub_status_enum` value — **unresolved** exact enum value not re-verified this session), `activatedon_date, closedon_date, reason_for_block` | status + block reason | n/a | via account | n/a | n/a | lifecycle/dormant/block state resolvable | `organization/office_dormant` (office-level dormant summary only) | known columns (§4); enum value unresolved | gap (partially inherited at office-aggregate level only) |
| SAV-3 | Effective terms | `nominal_annual_interest_rate, interest_compounding_period_enum, interest_posting_period_enum, min_required_opening_balance, allow_overdraft, overdraft_limit` | account-level effective terms | `N:1` to product for comparison | via account | n/a | same "effective vs product now" caveat as LOAN-3 | resolvable | `savings/account_terms_lookup` | known (§4) | inherited |
| SAV-4 | Balance vs available/holds/overdraft | `account_balance_derived, available_balance_derived, total_savings_amount_on_hold, overdraft_limit, on_hold_funds_derived` | four distinct balance measures, not interchangeable | n/a | via account | `XR-CUR` per account currency | balance ≠ available ≠ on-hold must stay distinct fields, never merged | balance query returns the specific measure asked for | `savings/balance_summary` (does not appear to separately expose hold/overdraft — **verify field selection matches all four before declaring this fully closed**) | known (§4) | inherited (core balance only; hold/overdraft split not confirmed exposed) |
| SAV-5 | Deposit/withdrawal/transfer/adjustment/reversal | `m_savings_account_transaction.transaction_type_enum, is_reversed, amount, transaction_date` | per-type transaction amounts | `N:1` account; `office_id` direct on this table | `m_savings_account_transaction.office_id` direct | `transaction_date`, reversed excluded by default (`savings-transactions.yaml` rule, verified above) | reversed txn excluded unless explicitly analyzing reversals | `savings/deposit_total`, `withdrawal_total`, `deposit_monthly_breakdown`, `withdrawal_monthly_breakdown`, `deposit_top_n`, `withdrawal_top_n`, `activity_list`, `client_activity` | known (15,607 rows, §4) | inherited (deposit/withdrawal only; transfer/adjustment enum values not separately confirmed) |
| SAV-6 | Interest recorded vs posted | `total_interest_earned_derived` vs `total_interest_posted_derived`, `interest_posted_till_date` | two distinct measures — earned-to-date is not the same as posted | n/a | via account | posting is periodic (`interest_posting_period_enum`) | must not conflate earned with posted | resolvable, kept distinct | none | known (§4) | gap |
| SAV-7 | Charges | `m_savings_account_charge` (182 rows) + `m_savings_account_charge_paid_by` (165 rows, links charge to the settling transaction) | amount/paid/waived/written-off/outstanding, `is_penalty` | `1:N` account → charges; `1:N` charge → paid-by (settlement evidence) | via account | n/a | settlement evidence via `paid_by`, not inferred from balance drop alone | `savings/account_charges_recent`, `charge_count_by_type`, `charges_by_type`, `pending_charges_clients`, `strictly_overdue_charges_clients` | known (§4) | inherited |
| SAV-8 | Holds | `m_savings_account_transaction` where `hold_type`/`hold_status` populated (lien/hold transactions), `is_lien_transaction`, `release_id_of_hold_amount` | hold amount, hold status, release linkage | self-referencing via `parent_hold_transaction_id` | via account | hold placed/released dates | a hold amount is not a completed withdrawal | hold list distinct from completed transactions | none | known columns exist on `m_savings_account_transaction` (§4); no rows sampled for actual hold usage this session | gap |
| SAV-9 | Applied tax | `withhold_tax, tax_group_id` on the account; tax component detail via PROD-10's tax tables | tax withheld amount vs tax config | `N:1` account → tax group | via account | n/a | tax config (PROD-10) ≠ tax actually withheld — same separation pattern as D04 | resolvable, distinct from tax master | none | known columns exist (§4) | gap |
| SAV-10 | Linked accounts | `m_portfolio_account_associations` (90 rows, `savings_account_id ↔ linked_savings_account_id`/`linked_loan_account_id`) | `association_type_enum, is_active` | `1:1` or `1:N` depending on association type — **cardinality not re-verified per type this session** | via either linked account | n/a | linkage existing is not proof of a transfer (§1 rule 35 / `XR-EVID`) — see LINK-3 | linkage list resolvable, distinct from actual transfer evidence | none | known (§4) | gap |
| SAV-11 | History available | Implicit: `m_savings_account_transaction` full history is retained (no truncation observed at 15,607 rows for 207 accounts) | n/a | n/a | via account | full history assumed available unless a retention/archival policy is found | n/a | history queries not artificially windowed | `savings/activity_list` | known (§4) | inherited |

---

## 7. Domain F — Fixed Deposit / Recurring Deposit
(dataset-scope-decisions.md:28, §2 "FD/RD")

FD and RD share the `m_savings_account` row (`deposit_type_enum` distinguishes
them) plus type-specific extension tables. The exact `deposit_type_enum` value
for FD vs RD was **not** re-verified against `m_code_value` this session — flagged
as `unresolved` on every row below rather than assumed from naming convention.

| ID | Requirement | Source table(s) & grain | Field / measure | Relationship (cardinality) | Office-scope path | Time / as-of / currency | Evidence rule | Acceptance | Capability | Confidence | Status |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| FDRD-1 | Identity | `m_savings_account` filtered to FD/RD `deposit_type_enum` (grain: deposit account) | account_no, client_id, product_id | `N:1` client, product | via client | n/a | enum-value filter unresolved | identity resolvable once enum confirmed | none | unresolved (enum value) | gap |
| FDRD-2 | Terms | `m_deposit_account_term_and_preclosure` (36 rows) | `min_deposit_term, deposit_period, deposit_amount, maturity_amount, maturity_date` | `1:1` account ↔ term row | via account | maturity_date is a projection, not payout proof (§2 FD/RD rule) | `maturity_amount` labeled as projected, never asserted as paid out | `savings/deposit_maturity_by_client` (labeled maturity data — verify it does not overstate as paid) | known (§4) | gap |
| FDRD-3 | Funding | `m_savings_account_transaction` type = deposit, at/near `submittedon_date`/`activatedon_date` for the account | initial funding amount/date | via account | via account | n/a | n/a | funding transaction identifiable | none | known table (§4); "funding transaction" filter unresolved | gap |
| FDRD-4 | Interest | `nominal_annual_interest_rate`, `total_interest_posted_derived` on `m_savings_account` (shared with SAV-6) | same fields as SAV-6, FD/RD does not have a separate interest table | n/a | via account | FD not assumed to have periodic installment schedule (§2 rule) | must not impose a savings-style periodic schedule on FD | none | known (§4) | gap |
| FDRD-5 | Charges / tax | `m_savings_account_charge`, `withhold_tax`/`tax_group_id` — same tables as SAV-7/SAV-9 | same as SAV-7/SAV-9 | via account | via account | n/a | same separation rules as SAV-7/SAV-9 | none | known (§4) | gap |
| FDRD-6 | Maturity / preclosure | `m_deposit_account_term_and_preclosure.pre_closure_penal_applicable, pre_closure_penal_interest, on_account_closure_enum` | preclosure penalty terms; on-closure action (transfer/withdraw/reinvest) | `1:1` account | via account | closure instruction ≠ evidence of actual payout/transfer (§2 rule — "jangan mengarang rantai reinvestment tanpa FK/evidence") | on-closure instruction distinguished from evidenced payout transaction | none | known (§4) | gap |
| FDRD-7 | Transactions and relations | `m_savings_account_transaction` (shared, SAV-5) | same as SAV-5 | via account | via account | n/a | same as SAV-5 | none | known (§4) | gap |
| FDRD-8 (RD) | Contributions | `m_savings_account_transaction` filtered to RD deposit type + `m_deposit_account_recurring_detail.mandatory_recommended_deposit_amount` | expected vs actual periodic contribution | `1:1` account ↔ recurring-detail row | via account | contribution due date from schedule (recurring detail), actual from transaction | RD shortfall ≠ loan arrears (§2 rule — separate concept) | contribution vs mandatory amount comparable | none | known (`m_deposit_account_recurring_detail`=22 rows, §4) | gap |
| FDRD-9 (RD) | Schedule | `m_deposit_account_recurring_detail` (schedule is implicit — no separate per-installment RD schedule table found; Fineract computes RD due dates from `is_calendar_inherited` + frequency, not a stored per-row schedule like loans) | recurring frequency, calendar linkage | `1:1` account | via account | n/a | schedule is not an event, same as LOAN-5 | schedule derivable, not stored per-installment | none | **unresolved** — no per-installment RD schedule table found; needs confirmation this is computed, not missing | gap |
| FDRD-10 (RD) | Shortfall | `m_deposit_account_recurring_detail.total_overdue_amount, no_of_overdue_installments` | shortfall amount/count | `1:1` account | via account | as-of freshness applies (depends on whether overdue calc batch has run — ties `XR-ASOF`) | RD shortfall explicitly distinct from loan arrears, per §2 | none | known (§4) | gap |

---

## 8. Domain G — Share accounts
(dataset-scope-decisions.md:29, §2 "Share")

| ID | Requirement | Source table(s) & grain | Field / measure | Relationship (cardinality) | Office-scope path | Time / as-of / currency | Evidence rule | Acceptance | Capability | Confidence | Status |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| SHARE-1 | Identity / lifecycle | `m_share_account` (grain: account, 2 rows) | `account_no, client_id, product_id, status_enum`, lifecycle dates | `N:1` client, product | via `client_id → m_client.office_id` | n/a | n/a | identity/lifecycle resolvable | none | known (§4) | gap |
| SHARE-2 | Effective terms | `minimum_active_period_frequency, lockin_period_frequency, allow_dividends_inactive_clients` | terms at account level | via account | via account | n/a | n/a | resolvable | none | known (§4) | gap |
| SHARE-3 | Ownership by status | `total_approved_shares` vs `total_pending_shares` | ownership measure varies by status — pending ≠ owned (§2 rule) | n/a | via account | n/a | pending purchase not counted as ownership | ownership count uses approved shares, pending reported separately | none | known (§4) | gap |
| SHARE-4 | Purchase / redemption | `m_share_account_transactions` (grain: transaction, 3 rows) | transaction type (purchase/redeem), quantity, price at transaction time (distinct from current product price per §2 rule) | `N:1` account | via account | transaction_date | historical purchase price ≠ current product price — must not backfill with today's price | resolvable, price fields distinct | none | known (§4) | gap |
| SHARE-5 | Charges | `m_share_account_charge` (2 rows) | amount, charge_id (→ PROD-4 master) | `1:N` account → charges | via account | n/a | same D04-style separation | none | known (§4) | gap |
| SHARE-6 | Dividends | `m_share_account_dividend_details` (0 rows), `m_share_product_dividend_pay_out` (0 rows) | dividend amount, status | `N:1` account/product | via account | dividend requires **posted** status AND a linked savings transaction reference (§2 rule) — neither condition verifiable while both tables are empty | dividend claim must cite both status=posted and the linked savings transaction | none | known — both tables exist and are **empty** on this fixture (§4); feature unused by this tenant, needs fixture before this row can move past `gap` | gap |
| SHARE-7 | Settlement (proven) | `m_share_account.savings_account_id` (linked savings account for settlement) | linkage only — does not by itself prove a purchase/redemption settled (§2 rule: "linked savings tidak membuktikan settlement tiap purchase/redemption") | `N:1` share account → savings account | via linked savings account's own office path | n/a | settlement must be evidenced per-transaction, not inferred from the existence of a link | settlement claims require per-transaction evidence, not the link alone | none | known (§4) | gap |
| SHARE-8 | History / corrections | `m_share_account_transactions` (SHARE-4) filtered/annotated for corrections | corrections follow `is_active` evidence, not an assumed reversal field (§2 rule: "jangan mengasumsikan field reversal") | via account | via account | n/a | no reversal field assumed to exist without verification | correction distinguishable from a fresh transaction only via `is_active`, not a guessed reversal column | none | known base table (§4); no reversal-style column found on `m_share_account_transactions` in the schema dump (§4) — confirms the §2 warning is warranted | gap |

---

## 9. Domain H — Linking / cross-cutting resources
(dataset-scope-decisions.md:30)

| ID | Requirement | Source table(s) & grain | Field / measure | Relationship (cardinality) | Office-scope path | Time / as-of / currency | Evidence rule | Acceptance | Capability | Confidence | Status |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| LINK-1 | Account ownership | covered by CLI-5 (loan/savings/share → client/group) | — | — | — | — | — | — | — | — | cross-reference (CLI-5) |
| LINK-2 | Account-product | covered per-domain (PROD-1/2/3 `1:N` to their accounts) | — | — | — | — | — | — | — | cross-reference (PROD-1..3) |
| LINK-3 | Linked accounts | `m_portfolio_account_associations` (90 rows, grain: association) | `association_type_enum, is_active`, both sides (`loan_account_id`/`savings_account_id`/`linked_*`) | `1:N` or self-`N:M` depending on type (SAV-10 duplicate — cross-referenced, not re-counted) | via either side's office path | n/a | link existing ≠ transfer proof (§1 rule 35) | linkage resolvable, kept distinct from LINK-4 transfer evidence | none | known (§4) | gap |
| LINK-4 | Actual transfer | `m_account_transfer_details` (498 rows, header) + `m_account_transfer_transaction` (728 rows, the evidenced movement) | from/to office, client, account; transfer amount/date on the transaction row | `1:1` details ↔ N transactions over time (recurring transfers reuse the same detail row) | `from_office_id`/`to_office_id` on the detail row | `transaction_date` on the transaction row | this is the evidence row LINK-3's link alone cannot provide | actual money movement only asserted from the transaction row, never from the link header alone | none | known (§4) | gap |
| LINK-5 | Payment allocation | `m_loan_transaction.principal_portion_derived/interest_portion_derived/fee_charges_portion_derived/penalty_charges_portion_derived` (loan side, LOAN-6); savings has no equivalent multi-component allocation (a savings transaction is single-purpose) | allocation of one payment across components | n/a (denormalized on the transaction row) | via transaction | n/a | components must sum to the transaction amount (anti-double-count, §1 rule 36) | allocation resolvable and internally consistent | none | known (§4) | gap |
| LINK-6 | Charge settlement | `m_savings_account_charge_paid_by` (165 rows, `savings_transaction_id` links a charge to its paying transaction); loan side has no equivalent explicit "paid-by" table — `m_loan_charge`'s `amount_paid_derived` is a derived total, not a per-transaction settlement link | which transaction paid which charge (savings); derived total only (loan) | `1:N` savings charge → paying transaction(s) | via account | n/a | settlement evidence, not inferred from balance drop | savings settlement traceable to a transaction; loan settlement stated as derived total only, gap flagged | none | known (§4) | gap |
| LINK-7 | Reversal / correction | per-domain reversal fields: `m_savings_account_transaction.is_reversed/is_reversal`, `m_loan_transaction.is_reversed`, `acc_gl_journal_entry.reversed/reversal_id` — share has **no** reversal field (see SHARE-8) | reversal flag + reference to the original | `1:1` transaction ↔ its reversal, where the field exists | via transaction | reversal date where recorded | anti-double-count when a reversed transaction is excluded by default | reversal handling consistent per domain, share's absence stated explicitly rather than assumed | none | known (§4) | gap |
| LINK-8 | Payment type | `m_payment_type` (ORG-6) joined via `m_payment_detail` (referenced by both `m_savings_account_transaction.payment_detail_id` and `m_loan_transaction.payment_detail_id`) | payment type name/is_cash | `N:1` transaction → payment_detail → payment_type | via transaction | n/a | n/a | payment type resolvable per transaction where a `payment_detail_id` is set | none | known (§4); `m_payment_detail` itself not queried this session (candidate) | gap |

---

## 10. Cross-domain decisions D01–D15

Every row cites the exact decision paragraph in
[`dataset-scope-decisions.md`](../product/2026-09-09-dataset-scope-decisions.md)
and states the disposition already fixed by the 2026-09-20 owner decision
(`:16`) where one exists, instead of re-litigating it.

| ID | Requirement (disposition) | Source table(s) | Relationship / grain | Office-scope path | Time / as-of / currency | Evidence rule | Acceptance | Capability | Confidence | Status |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| D01 | Period-close reporting: **recomputed-only** (`dataset-scope-decisions.md:16` fixes this — the "reproduce exact as-published" branch is **not** implemented; only current recomputation from live data is in scope) | no dedicated position-snapshot table found this session; recomputation reads the same domain tables above (loan/savings balances) as-of a filter date | n/a | inherited from the underlying domain query | recomputed as-of the requested date, not a stored historical snapshot | must not claim to reproduce a "published" report that was never snapshotted | as-of recomputation only; "show me what was published" answered as `Unsupported` (owner-fixed scope) | none | n/a (scope decision, not a table) | gap |
| D02 | Historical branch attribution: **historical office only where history exists; otherwise current-office, explicitly labeled** (`:16`, `:72-78`) | no client/account office-history table found this session (`m_client` and `m_savings_account`/`m_loan` store only the **current** `office_id`; no `m_client_office_history`-style table exists in the schema dump gathered above) | n/a | current `office_id` is the only path available on the tables inventoried above | as-of the report date for the historical branch — but with no history table, every domain row above is **current-office only** | a current-office answer must be labeled as such, never presented as historical | branch attribution defaults to current-office with an explicit label per row above (CLI-3, LOAN-1, SAV-1, SHARE-1); a true historical-office query is `gap` pending a history table | none | known — no office-history table found this session (verified negative) | gap |
| D03 | Currency and consolidation (`:16`, `:80-86`) | no `exchange_rates`-style table exists in `fineract_default` (verified this session, §4 — searched via `information_schema` for `%exchange%`, only `m_organisation_currency` matched) | n/a | n/a | `XR-CUR` — per-currency totals only until a rate source is chosen | consolidated totals must record `exchange_rate_id`; none exists yet, so consolidation stays fully unsupported | per-currency totals inherited wherever a domain row above states `XR-CUR`; consolidation is `gap` with no candidate table | none | known (negative result) | gap |
| D04 | Direct client charges (`:88-94`) | `m_client_charge` (0 rows), `m_client_charge_paid_by` (0 rows) — both exist, both **empty** on this fixture (verified §4) | `1:N` client → charge; `1:N` charge → paid-by | via client | n/a | master (PROD-4) vs actual charge vs payment vs waiver vs outstanding must stay separated, mirroring LOAN-7/SAV-7 | resolvable once populated; currently sparse data, not a schema gap — record fixture need as a future owner decision, do not seed data (ticket constraint) | none | known (§4) | gap |
| D05 | Standing instructions (`:96-102`) | `m_account_transfer_standing_instructions` (61 rows) + `m_account_transfer_standing_instructions_history` (1,219 rows, execution attempts) | `1:N` instruction → history/attempt rows; instruction → `m_account_transfer_details` (LINK-4) | via either linked account's office path | `valid_from/valid_till`, `last_run_date` | absence of a transfer does not prove failure — instruction may be inactive or not yet due (`:102`) | failure claims must trace to a history row with a failure status, never inferred from "no transfer found" | none | known (§4) | gap |
| D06 | Teller/cashier cash operations (`:104-110`) | `m_cashiers` (6 rows, assignment), `m_tellers` (6 rows); no `m_cash_journal`/cash-movement table was found this session (`m_cash_journal` queried, **missing** — confirmed via `information_schema`, §4) | `N:1` cashier → teller; `N:1` cashier → staff | via teller/office | shift `start_date/end_date`, `start_time/end_time` | recorded cash ≠ physical cash; reconciliation only answerable if a reconciliation record exists — none found | cashier/teller assignment resolvable; cash position/reconciliation is `gap` with **no candidate table found**, not merely unverified | none | known (assignment tables); cash-movement table confirmed absent | gap |
| D07 | Actual provisioning results (`:112-118`) | `m_provisioning_history` (0 rows), `m_loanproduct_provisioning_entry` (0 rows) — both exist, both empty (§4); criteria config is ORG-7 | `N:1` result → criteria/version used | via loan (through the entry detail, once populated) | provisioning is periodic — `XR-ASOF` applies; batch may not have run (D14) | must not multiply current provisioning % config by historical outstanding as a substitute for a real result (`:118`) | resolvable once populated; sparse on this fixture, not a schema gap | none | known (§4) | gap |
| D08 | Group/center activity, meetings, attendance (`:120-126`) | Fineract's calendar tables (`m_calendar`, `m_calendar_instance`, `m_meeting`/`m_meeting_client` — **not queried this session**, existence unresolved) | `N:1` calendar → group/center; `1:N` meeting → attendance if the table exists | via `m_group.office_id` | scheduled date vs actual meeting date, if attendance table exists | a calendar entry existing does not prove a meeting happened or was attended (`:126`) | schedule resolvable via `m_group`; realization/attendance is `unresolved` pending table verification | none | unresolved — calendar/meeting tables not queried this session | gap |
| D09 | Fineract user actions / source audit (`:128-134`) | `m_portfolio_command_source` (4,456 rows) | `N:1` command → `m_appuser` (actor), business entity via `resource_get_id`/`resource_id` (columns not individually enumerated this session) | via the target entity's own office path | `created_date` on the command row | the managing officer is not automatically the actor — actor comes from the command row, not from an assignment table (`:134`) | actor/action/reason resolvable per business entity where a command row exists; this is Fineract's own operational audit, distinct from Jarvis's own `audit_events` (database-design.md I8) | none | known (§4) | gap |
| D10 | Custom datatables (`:136-144`) — **deferred to deployment onboarding** | `x_registered_table` (16 rows on this local instance, §4) | registry → dynamically-named physical table per row | per-registration, not generic | per-registration | per-registration | per-registration; nothing may be assumed about an unregistered/unmapped table | **Confirms** the decision's own caveat: the ai_report reference example (`extra_client_details`, `extra_family_details`, `extra_loan_details`, `:142`) is **absent** on this local instance (verified missing, §4); the 16 tables actually registered here (`EmploymentDetails`, `AddressDetails`, `KYCFields`, `SourceOfFunds`, …) are a **different set**. This is exactly why D10 is per-deployment, not a static schema. | none | known (registry checked; physical table names not individually resolved this session) | deferred-onboarding |
| D11 | Completeness analysis (`:146-154`) | cross-cutting requirement over every domain table above, not a table of its own | n/a | n/a | n/a | empty/unclassified must be grouped and counted explicitly, never dropped or coerced to zero (`I4`) | applies to every domain row above wherever a grouping field can be null/unclassified (e.g. ORG classification codes, CLI client_type) | none | n/a (cross-cutting rule, not a table) | gap |
| D12 | Accounting/GL traceability (`:156-164`) | `acc_gl_journal_entry` (37,077 rows), `acc_gl_account` (362 rows), `acc_gl_closure` (1 row, closure boundary) | `N:1` journal entry → GL account; entry references `loan_transaction_id`/`savings_transaction_id`/`client_transaction_id`/`share_transaction_id` back to the source transaction | `acc_gl_journal_entry.office_id` direct (GL account itself is org-wide, per decision text) | `entry_date`; closure interacts with D01 | reversal (`reversed`/`reversal_id`) must be honored and manual vs system entries labeled (`type_enum`/`manual_entry`); no re-derivation of accounting — read recorded values and aggregate deterministically only | single named GL account balance/journal traceability in scope; full trial balance/P&L explicitly out of scope (`:162`) | none | known (§4) | gap |
| D13 | Existing Fineract reports — **out of execution scope entirely** (`:166-172`) | `stretchy_report` (Fineract's own report definitions — not queried, and per D13 must never be executed) | n/a | n/a | n/a | executing an admin-authored report bypasses office-scope/PII/SELECT-only/allowlist guards (`analytical-contracts.md` §4.4) | Jarvis never surfaces or runs `stretchy_report`; a valuable report is ported to a validated analytical-contract instead, never run raw | none | n/a (explicitly excluded) | excluded |
| D14 | Scheduler/batch job runs (`:174-180`) | `job` (44 rows), `job_run_history` (308,960 rows) | `1:N` job → run history | n/a (operational, not office-scoped) | `start_time/end_time`, `status`, `trigger_type` | `XR-ASOF` — this is the freshness signal every as-of claim above (D01, D07, D09, D11, LOAN-9, FDRD-10) should be able to cite | job/run status resolvable; not real-time monitoring, no trigger/re-run capability (write forbidden) | none | known (§4) | gap |
| D15 | Gap-review closure — dispositions (`:182-194`) | see sub-rows below | — | — | — | — | closure of the systematic 220-table review; **local count differs from the ticket figure — see §4 note** | — | — | summary (D15a..D15e) |
| D15a | Surveys / PPI poverty scoring — **deferred to deployment onboarding** | `m_surveys` (0 rows), `m_survey_responses` (0 rows), `m_survey_scorecards` (0 rows), `ppi_scores` (**20 rows**), `ppi_likelihoods` (0 rows) | `1:N` survey → responses/scorecards; ppi tables reference client/survey | via client | n/a | **anomaly found and recorded, not resolved:** `ppi_scores` has 20 rows while `m_surveys` has 0 — PPI scoring data exists without a parent survey definition row on this fixture. Do not assume the survey/PPI pipeline is unused; flag for owner review before building a capability. | condition-dependent per D10 pattern, per decision text | none | known (§4) | deferred-onboarding |
| D15b | Credit bureau results | `m_creditreport` (0 rows, "MASUK" per decision) vs `m_creditbureau` (1 row, integration config — explicitly excluded by the decision, `:187`) | `N:1` report → client/loan | via client/loan | n/a | only the **result** table is in scope; the **config** table is excluded — the two must not be conflated | resolvable once `m_creditreport` is populated; currently empty on this fixture | none | known (§4) | deferred-onboarding |
| D15c | Loan capitalized-income/buy-down-fee balances | duplicate of LOAN-12 (cross-referenced, not re-counted) | — | — | — | — | — | — | — | cross-reference (LOAN-12) |
| D15d | `m_family_members` — selective, not default | `m_family_members` (1 row) | `1:N` client → family member | via client | n/a | same "selective/not-default" pattern as CLI-6/CLI-8 | excluded by default; row exists to record the exclusion | none | known (§4) | gap |
| D15e | `m_office_transaction` (inter-office cash) — under D06/accounting, explicitly labeled | `m_office_transaction` (0 rows) | `N:1` from/to office | both `from_office_id`/`to_office_id` | n/a | must be labeled as inter-office cash movement, not conflated with client-facing transactions | resolvable once populated; empty on this fixture | none | known (§4) | gap |

**D15 table-count note.** The decision text (`:184`) cites "220 tabel
`createTable`" from the prior systematic audit. `fineract_default` on this
worktree's local instance reports **300** tables in `information_schema.tables`
(query in §4). This is recorded as an open discrepancy, not resolved here: the
two counts may reflect different Fineract versions, a different
migration/plugin set, or the audit's 220 excluding views/dynamic
(`x_registered_table`-backed) tables. Do not treat either number as
authoritative without re-running the audit against this exact schema —
flagged for the owner, not silently reconciled.

**Excluded, non-analytic (`:192`, confirmed, no row needed above).** Documents/
images, notes (see CLI-8), campaign SMS/email, notifications, XBRL/MIX export,
`m_adhoc`, webhooks/templates, external services/business events, interop,
self-service/pockets/device registration, entity-to-entity mapping,
auth/roles/2FA/OAuth, field-config/cache, `stretchy_report` (D13), full
financial statements (the excluded part of D12), post-dated checks. None of
these were queried this session; their exclusion is a scope decision already
fixed in the source document, not something this inventory re-verifies.

---

## 11. Local `fineract_default` evidence (read-only, this session, 2026-09-27)

All counts below are `SELECT count(*) FROM <table>` against
`postgres://…@127.0.0.1:5432/fineract_default` (the `.env`-configured
`FINERACT_DATABASE_URL` of this worktree), run once this session. No table was
written to. "Fixture/demo count" language is avoided per the ticket's warning
(`:11` of the scope document: "Data contoh/demo tidak membuktikan penggunaan
pada tenant aktual") — these are simply this local instance's current values,
not evidence of production tenant behavior.

| Table | Rows | Table | Rows |
| --- | --- | --- | --- |
| `m_office` | 8 | `m_holiday` | 1 |
| `m_staff` | 25 | `m_organisation_currency` | 6 |
| `m_working_days` | 1 | `m_payment_type` | 6 |
| `m_provisioning_criteria` | 0 | `m_provisioning_criteria_definition` | 0 |
| `m_product_loan` | 12 | `m_savings_product` | 23 |
| `m_share_product` | 1 | `m_charge` | 59 |
| `m_client_collateral_management` | 8 | `m_loan_collateral_management` | 12 |
| `m_deposit_account_recurring_detail` | 22 | `m_deposit_account_term_and_preclosure` | 36 |
| `m_deposit_product_term_and_preclosure` | 11 | `m_deposit_product_recurring_detail` | 3 |
| `m_delinquency_bucket` | 4 | `m_delinquency_range` | 6 |
| `m_delinquency_bucket_mappings` | 10 | `m_product_mix` | 0 |
| `m_tax_component` | 51 | `m_tax_group` | 16 |
| `m_floating_rates` | 0 | `m_floating_rates_periods` | 0 |
| `m_client` | 43 | `m_client_non_person` | 4 |
| `m_group` | 2 | `m_group_client` | 3 |
| `m_group_level` | 2 | `m_client_identifier` | 0 |
| `m_client_address` | 1 | `m_client_charge` | 0 |
| `m_client_charge_paid_by` | 0 | `m_family_members` | 1 |
| `m_loan` | 116 | `m_loan_transaction` | 2,947 |
| `m_loan_repayment_schedule` | 1,661 | `m_loan_charge` | 217 |
| `m_loan_arrears_aging` | 59 | `m_loan_collateral` | 0 |
| `m_guarantor` | 1 | `m_loan_disbursement_detail` | 0 |
| `m_loan_capitalized_income_balance` | 0 | `m_loan_buy_down_fee_balance` | 0 |
| `m_savings_account` | 207 | `m_savings_account_transaction` | 15,607 |
| `m_savings_account_charge` | 182 | `m_savings_account_charge_paid_by` | 165 |
| `m_savings_officer_assignment_history` | 48 | `m_share_account` | 2 |
| `m_share_account_transactions` | 3 | `m_share_account_charge` | 2 |
| `m_share_account_dividend_details` | 0 | `m_share_product_dividend_pay_out` | 0 |
| `m_account_transfer_details` | 498 | `m_account_transfer_transaction` | 728 |
| `m_portfolio_account_associations` | 90 | `m_account_transfer_standing_instructions` | 61 |
| `m_account_transfer_standing_instructions_history` | 1,219 | `acc_gl_account` | 362 |
| `acc_gl_journal_entry` | 37,077 | `acc_gl_closure` | 1 |
| `acc_product_mapping` | 259 | `acc_accounting_rule` | 0 |
| `m_appuser` | 29 | `m_role` | 9 |
| `m_permission` | 1,150 | `m_portfolio_command_source` | 4,456 |
| `request_audit_table` | 0 | `x_registered_table` | 16 |
| `m_surveys` | 0 | `m_survey_responses` | 0 |
| `m_survey_scorecards` | 0 | `ppi_scores` | 20 |
| `ppi_likelihoods` | 0 | `m_creditreport` | 0 |
| `m_creditbureau` | 1 | `job` | 44 |
| `job_run_history` | 308,960 | `m_cashiers` | 6 |
| `m_tellers` | 6 | `m_office_transaction` | 0 |
| `m_currency` | 164 | | |

**Confirmed missing/absent (not simply zero-row) on this local instance:**
`m_loan_provisioning_criteria`, `m_product_collateral`, `m_center` (see §4
group-level note above — center is `m_group.level_id = 1`, not a separate
table), `m_loan_guarantor` (the real table is `m_guarantor`), `m_loan_recovery_payment`
(no dedicated table; recovery is a `m_loan_transaction` type), `m_share_account_dividend`
(the real tables are `m_share_account_dividend_details` /
`m_share_product_dividend_pay_out`), `m_cash_journal`, `extra_client_details`,
`extra_family_details`, `extra_loan_details` (the ai_report reference example
names — absent here, see D10). `fineract_default` has **300** public tables
total (see D15 table-count note, §10).

---

## 12. Coverage matrix — §1 domains + D01–D15 against inventory IDs

| §1 heading / decision | Inventory IDs | Row count | Statuses present |
| --- | --- | --- | --- |
| Organization (`:23`) | ORG-1..ORG-11 | 11 | 1 inherited, 10 gap |
| Products (`:24`) | PROD-1..PROD-11 | 11 | 11 inherited (FIN-109) |
| Client/kepemilikan (`:25`) | CLI-1..CLI-8 | 8 | 3 inherited, 5 gap |
| Loan (`:26`) | LOAN-1..LOAN-12 | 12 | 0 inherited, 12 gap |
| Savings (`:27`) | SAV-1..SAV-11 | 11 | 6 inherited, 5 gap |
| FD/RD (`:28`) | FDRD-1..FDRD-10 | 10 | 0 inherited, 10 gap |
| Share accounts (`:29`) | SHARE-1..SHARE-8 | 8 | 0 inherited, 8 gap |
| Resource penghubung (`:30`) | LINK-1..LINK-8 | 8 (2 cross-referenced to CLI-5/PROD-1..3, not double-counted) | 0 inherited, 6 gap, 2 cross-ref |
| D01 | D01 | 1 | gap |
| D02 | D02 | 1 | gap |
| D03 | D03 | 1 | gap |
| D04 | D04 | 1 | gap |
| D05 | D05 | 1 | gap |
| D06 | D06 | 1 | gap |
| D07 | D07 | 1 | gap |
| D08 | D08 | 1 | gap |
| D09 | D09 | 1 | gap |
| D10 | D10 | 1 | **deferred-onboarding** |
| D11 | D11 | 1 | gap |
| D12 | D12 | 1 | gap |
| D13 | D13 | 1 | excluded by design |
| D14 | D14 | 1 | gap |
| D15 | D15, D15a..D15e | 6 | 1 summary, 2 **deferred-onboarding** (D15a, D15b), 1 cross-reference (D15c→LOAN-12), 2 gap |

**Totals:** 79 baseline-domain rows + 20 decision rows (D01–D14 = 14;
D15 heading + five sub-rows = 6) = **99 inventory rows**, covering all 8
§1 headings and all 15 decisions (D01–D15) with **zero omissions**.
Twenty-one rows are `inherited` (an approved Mode-1 capability exists today;
PROD-1..11 added by FIN-109); three
(D10, D15a, D15b) are `deferred-onboarding`. D13 is `excluded`; D15 is a
`summary`; LINK-1, LINK-2 and D15c are `cross-reference` rows. The remaining
70 rows are `gap`: agreed scope lacking a capability, including unresolved
source mappings. Coverage counts requirements, not executable capabilities.

---

## 13. Open items carried forward (not closed by this document)

These are the concrete `unresolved` markers scattered through §2–§10, collected
so FIN-153 does not have to re-scan every table:

1. Enum value mappings not re-verified this session: `m_client.status_enum`/`sub_status`,
   `m_savings_account.sub_status_enum` (dormant/block),
   `m_savings_account.deposit_type_enum` (FD vs RD). (`m_loan.loan_status_id`
   and the loan-transaction recovery type were resolved by FIN-108.)
2. Tables referenced by name but not queried this session: `m_fund`,
   `m_global_configuration` (business date), `m_code`/`m_code_value` (generic
   enums), `m_tax_group_mappings`, `m_payment_detail`, `m_note`, calendar/meeting
   tables for D08, `stretchy_report` metadata (D13, read-only existence only —
   never execution).
3. Cardinality of `m_portfolio_account_associations.association_type_enum` per
   type value (SAV-10/LINK-3) — not individually verified.
4. No exchange-rate table exists anywhere in `fineract_default` — D03 has no
   candidate source at all, not merely an unverified one.
5. No RD per-installment schedule table found (FDRD-9) — needs confirmation
   this is computed at read-time by Fineract rather than genuinely missing.
6. `ppi_scores` has rows while `m_surveys` has none (D15a) — an anomaly to
   raise with the owner, not silently resolved.
7. The D15 table-count discrepancy (220 vs 300, §10/§11) — needs the owner to
   say which count, and which Fineract build, is authoritative.
8. Sparse/empty tables recorded as fixture needs, per the ticket's instruction
   to never seed data: D04 (`m_client_charge`), D06 (no cash-movement table
   found at all — a different problem than sparse data), D07 (provisioning
   results), SHARE-6 (dividends), PROD-7 (product mix), PROD-11 (floating
   rates), D15e (`m_office_transaction`). These are **owner decisions**, not
   engineering follow-ups: whether/how to obtain representative fixture data
   for a deployment that exercises these paths.

## 14. Gates run for this ticket

Per the ticket's explicit instruction, **no Bruno run** was performed — FIN-152
is documentation-only (no route/service/repository/capability/SQL-execution
change), and the ticket brief supersedes the generic worker contract's
integration-test requirement for this specific ticket. Gates actually run:

- `./scripts/docs-check.sh` — link check across `docs/`.
- `./scripts/acceptance-check.sh` — scenario-ID mapping (L1C owns no numbered
  scenario per `build-order.md:118`, so this is a no-op confirmation that
  nothing here was mistakenly given a scenario ID it doesn't own).
- Read-only `psql` queries against `fineract_default` (§11) — no writes, no
  migrations, no fixtures.

No Rust code changed, so `cargo check`/`clippy`/`cargo test` were not run (no
`.rs` file in this diff).
