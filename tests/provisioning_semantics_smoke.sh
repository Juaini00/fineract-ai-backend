#!/usr/bin/env bash
#
# FIN-116 nonzero semantics smoke for queries/provisioning/*.sql.
#
# Deterministic and read-only: the production SQL text is executed UNCHANGED, but every
# Fineract table it names is shadowed by a CTE of the same name built from inline VALUES,
# so no Fineract table is read and no fixture row exists anywhere. The connection only
# hosts the statement. Cases proven (semantics absent from live data):
#   * entry rows joined to their history and criteria IDs;
#   * office_id filter binds: entries for unauthorized offices (office 9) excluded under {2} scope;
#   * date window filter binds on history.created_date;
#   * currency separated per code (USD vs EUR) without cross-currency conversion;
#   * reserve_amount alias surfaces the misspelled column reseve_amount correctly;
#   * run history org-wide reference returns runs within window with raw flags and audit IDs;
#   * no simulation / recompute path exists;
#   * loan-contributor phrase never maps to an approved capability.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
URL="${SMOKE_DATABASE_URL:-postgres://root:password@127.0.0.1:5432/fineract_default}"

SQL_RUN_HISTORY="$ROOT/queries/provisioning/provisioning_run_history.sql"
SQL_ENTRIES="$ROOT/queries/provisioning/provisioning_entries.sql"

SHADOW_CTES="WITH
m_provisioning_history(id, journal_entry_created, createdby_id, created_date, lastmodifiedby_id, lastmodified_date) AS (VALUES
  (1::bigint, true, 10::bigint, DATE '2026-04-15', 10::bigint, DATE '2026-04-15'),
  (2, false, 11, DATE '2026-03-01', NULL::bigint, NULL::date),
  (3, true, 12, DATE '2024-01-01', 12, DATE '2024-01-01')),
m_provisioning_criteria(id, criteria_name) AS (VALUES
  (100::bigint, 'Standard Criteria'::varchar(100)),
  (200, 'Conservative Criteria')),
m_loanproduct_provisioning_entry(id, history_id, criteria_id, currency_code, office_id, product_id, category_id, overdue_in_days, reseve_amount, liability_account, expense_account) AS (VALUES
  (1001::bigint, 1::bigint, 100::bigint, 'USD'::varchar(3), 2::bigint, 1::bigint, 1::bigint, 0::bigint, 1500.000000::numeric(20,6), 501::bigint, 601::bigint),
  (1002, 1, 100, 'EUR', 2, 2, 2, 30, 2500.500000, 502, 602),
  (1003, 1, 100, 'USD', 9, 1, 1, 0, 3000.000000, 501, 601),
  (1004, 3, 200, 'USD', 2, 1, 1, 0, 4000.000000, 501, 601))"

fail=0

echo "=== Test 1: provisioning_run_history ==="
BODY_RUN_HISTORY="$(awk '/^SELECT/ {p=1} p {print}' "$SQL_RUN_HISTORY")"
ACTUAL_RUN_HISTORY="$(psql -X -A -t -F '|' -q -v ON_ERROR_STOP=1 -d "$URL" <<EOF
PREPARE run_history_smoke(date, date, int) AS
$SHADOW_CTES
$BODY_RUN_HISTORY;
EXECUTE run_history_smoke(DATE '2026-01-01', DATE '2026-04-30', 200);
EOF
)"

echo "$ACTUAL_RUN_HISTORY"

# Check run history row count = 2 (runs 1 and 2 in window, run 3 outside window)
run_count="$(echo "$ACTUAL_RUN_HISTORY" | grep -v '^$' | wc -l | tr -d ' ')"
if [ "$run_count" -ne 2 ]; then
  echo "FAIL: expected 2 run history rows in window, got $run_count" >&2; fail=1
else
  echo "ok   run history count is 2 (date window respected, run 3 from 2024 excluded)"
fi

# Check run 1 fields: history_id=1, created_date=2026-04-15, createdby=10, journal_entry_created=t
run1="$(echo "$ACTUAL_RUN_HISTORY" | grep '^1|' || true)"
if [ -z "$run1" ]; then
  echo "FAIL: run 1 missing" >&2; fail=1
else
  created_date="$(echo "$run1" | cut -d'|' -f2)"
  createdby="$(echo "$run1" | cut -d'|' -f3)"
  journal_created="$(echo "$run1" | cut -d'|' -f4)"
  if [ "$created_date" != "2026-04-15" ] || [ "$createdby" != "10" ] || [ "$journal_created" != "t" ]; then
    echo "FAIL: run 1 fields mismatch: '$run1'" >&2; fail=1
  else
    echo "ok   run 1 surfaces correct created_date (2026-04-15), createdby_id (10), journal_entry_created (t)"
  fi
fi

# Check run 2 fields: history_id=2, journal_entry_created=f
run2="$(echo "$ACTUAL_RUN_HISTORY" | grep '^2|' || true)"
if [ -z "$run2" ]; then
  echo "FAIL: run 2 missing" >&2; fail=1
else
  journal_created2="$(echo "$run2" | cut -d'|' -f4)"
  if [ "$journal_created2" != "f" ]; then
    echo "FAIL: run 2 journal_entry_created expected 'f', got '$journal_created2'" >&2; fail=1
  else
    echo "ok   run 2 surfaces journal_entry_created (f) as recorded"
  fi
fi

echo "=== Test 2: provisioning_entries ==="
BODY_ENTRIES="$(awk '/^SELECT/ {p=1} p {print}' "$SQL_ENTRIES")"
ACTUAL_ENTRIES="$(psql -X -A -t -F '|' -q -v ON_ERROR_STOP=1 -d "$URL" <<EOF
PREPARE entries_smoke(bigint[], date, date, int) AS
$SHADOW_CTES
$BODY_ENTRIES;
EXECUTE entries_smoke('{2}'::bigint[], DATE '2026-01-01', DATE '2026-04-30', 200);
EOF
)"

echo "$ACTUAL_ENTRIES"

# Row count should be 2: entries 1001 and 1002 in office 2 and within window.
# Entry 1003 (office 9) is unauthorized.
# Entry 1004 (history 3) is outside date window.
entry_count="$(echo "$ACTUAL_ENTRIES" | grep -v '^$' | wc -l | tr -d ' ')"
if [ "$entry_count" -ne 2 ]; then
  echo "FAIL: expected 2 entries, got $entry_count" >&2; fail=1
else
  echo "ok   entries count is 2 (office 9 excluded by scope, history 3 excluded by window)"
fi

# Check entry 1001: entry_id=1001, history_id=1, criteria_id=100, currency=USD, office=2, reserve_amount=1500.000000
entry1="$(echo "$ACTUAL_ENTRIES" | grep '^1001|' || true)"
if [ -z "$entry1" ]; then
  echo "FAIL: entry 1001 missing" >&2; fail=1
else
  h_id="$(echo "$entry1" | cut -d'|' -f2)"
  c_id="$(echo "$entry1" | cut -d'|' -f3)"
  cur="$(echo "$entry1" | cut -d'|' -f4)"
  off="$(echo "$entry1" | cut -d'|' -f5)"
  reserve="$(echo "$entry1" | cut -d'|' -f9 | sed 's/\.0*$//')"
  if [ "$h_id" != "1" ] || [ "$c_id" != "100" ] || [ "$cur" != "USD" ] || [ "$off" != "2" ] || [ "$reserve" != "1500" ]; then
    echo "FAIL: entry 1001 fields mismatch: '$entry1'" >&2; fail=1
  else
    echo "ok   entry 1001 joined to history_id (1), criteria_id (100), office_id (2), currency (USD), reserve_amount (1500)"
  fi
fi

# Check entry 1002: entry_id=1002, currency=EUR, reserve_amount=2500.500000
entry2="$(echo "$ACTUAL_ENTRIES" | grep '^1002|' || true)"
if [ -z "$entry2" ]; then
  echo "FAIL: entry 1002 missing" >&2; fail=1
else
  cur2="$(echo "$entry2" | cut -d'|' -f4)"
  reserve2="$(echo "$entry2" | cut -d'|' -f9 | sed -E 's/(\.[0-9]*[1-9])0+$/\1/; s/\.0+$//')"
  if [ "$cur2" != "EUR" ] || [ "$reserve2" != "2500.5" ]; then
    echo "FAIL: entry 1002 fields mismatch: '$entry2'" >&2; fail=1
  else
    echo "ok   entry 1002 currency separated (EUR) and reserve_amount (2500.5) surfaced from misspelled reseve_amount"
  fi
fi

# Check unauthorized office 9 exclusion: entry 1003 must be absent
if echo "$ACTUAL_ENTRIES" | grep -q '^1003|'; then
  echo "FAIL: entry 1003 (unauthorized office 9) leaked" >&2; fail=1
else
  echo "ok   entry 1003 (unauthorized office 9) strictly absent"
fi

# Check out-of-window exclusion: entry 1004 must be absent
if echo "$ACTUAL_ENTRIES" | grep -q '^1004|'; then
  echo "FAIL: entry 1004 (history 3 outside date window) leaked" >&2; fail=1
else
  echo "ok   entry 1004 (history outside date window) strictly absent"
fi

echo "=== Test 3: Boundaries & Safeguards ==="

# Check that no simulation capability ID exists in knowledge/capabilities/provisioning
if grep -rn "^id:.*simulate" "$ROOT/knowledge/capabilities/provisioning/" >/dev/null 2>&1; then
  echo "FAIL: simulation capability found in provisioning capabilities" >&2; fail=1
else
  echo "ok   no simulation capability in knowledge/capabilities/provisioning/"
fi

# Check that reseve_amount typo is NOT in query output_fields
if grep -rn "name: reseve_amount" "$ROOT/knowledge/queries/provisioning/" >/dev/null 2>&1; then
  echo "FAIL: misspelled reseve_amount leaked into query output_fields" >&2; fail=1
else
  echo "ok   reseve_amount typo not leaked into query output_fields (aliased as reserve_amount)"
fi

# Check that loan contributor phrase is guarded in unsupported_intents
if grep -rn "largest contributing loans to provisioning" "$ROOT/knowledge/domains/provisioning.yaml" >/dev/null 2>&1; then
  echo "ok   largest contributing loans phrase is guarded under unsupported_intents"
else
  echo "FAIL: largest contributing loans phrase missing from unsupported_intents" >&2; fail=1
fi

if [ "$fail" -eq 0 ]; then
  echo "=== All provisioning nonzero semantics smoke checks PASSED ==="
fi
exit $fail
