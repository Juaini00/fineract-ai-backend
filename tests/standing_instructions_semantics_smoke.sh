#!/usr/bin/env bash
#
# FIN-114 nonzero semantics smoke for queries/standing_instructions/*.sql.
#
# Deterministic and read-only: the production SQL text is executed UNCHANGED, but every
# Fineract table it names is shadowed by a CTE of the same name built from inline VALUES,
# so no Fineract table is read and no fixture row exists anywhere. The connection only
# hosts the statement. Cases proven (semantics absent from live data):
#   * DELETED status 3 instructions never leak into config or schedule;
#   * DELETED status 3 instruction attempts never leak into executions or failures;
#   * NULL to_office_id instruction is visible via from_office_id;
#   * recurrence_frequency NULL stays NULL (never coerced to 0);
#   * failures query strictly returns failed attempts, never success attempts;
#   * attempt-grain listing never fans instructions (1:1 per history attempt);
#   * office isolation: office 9 instructions and attempts excluded under {2,3} scope;
#   * currency resolved accurately from source leg (savings vs loan).
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
URL="${SMOKE_DATABASE_URL:-postgres://root:password@127.0.0.1:5432/fineract_default}"

SQL_CONFIG="$ROOT/queries/standing_instructions/standing_instruction_config.sql"
SQL_SCHEDULE="$ROOT/queries/standing_instructions/standing_instruction_schedule.sql"
SQL_EXECUTIONS="$ROOT/queries/standing_instructions/standing_instruction_executions.sql"
SQL_FAILURES="$ROOT/queries/standing_instructions/standing_instruction_failures.sql"

SHADOW_CTES="WITH
m_savings_account(id, currency_code) AS (VALUES
  (10::bigint, 'USD'::varchar(3))),
m_loan(id, currency_code) AS (VALUES
  (20::bigint, 'EUR'::varchar(3))),
m_account_transfer_details(id, from_office_id, to_office_id, from_client_id, to_client_id, from_savings_account_id, to_savings_account_id, from_loan_account_id, to_loan_account_id, transfer_type) AS (VALUES
  (1::bigint, 2::bigint, 2::bigint, 101::bigint, 102::bigint, 10::bigint, NULL::bigint, NULL::bigint, NULL::bigint, 1::smallint),
  (2, 2, 3, 103, 104, NULL, NULL, 20::bigint, NULL, 2),
  (3, 3, 3, 105, 106, 10, NULL, NULL, NULL, 1),
  (4, 2, NULL::bigint, 107, NULL::bigint, 10, NULL, NULL, NULL, 1),
  (5, 9, 9, 108, 109, 10, NULL, NULL, NULL, 1)),
m_account_transfer_standing_instructions(id, name, account_transfer_details_id, priority, status, instruction_type, amount, valid_from, valid_till, recurrence_type, recurrence_frequency, recurrence_interval, recurrence_on_day, recurrence_on_month, last_run_date, value_date_offset_days) AS (VALUES
  (1::bigint, 'SI Active Savings'::varchar(250), 1::bigint, 3::smallint, 1::smallint, 1::smallint, 100.00::numeric(19,6), DATE '2026-01-01', NULL::date, 1::smallint, 0::smallint, 1::smallint, 1::smallint, NULL::smallint, DATE '2026-04-01', 0),
  (2, 'SI Active Loan', 2, 2, 2, 2, NULL, DATE '2026-01-01', NULL, 2, NULL::smallint, NULL::smallint, NULL::smallint, NULL::smallint, DATE '2026-04-01', 0),
  (3, 'SI Deleted Status 3', 3, 3, 3, 1, 50.00, DATE '2026-01-01', NULL, 1, 0, 1, 1, NULL, DATE '2026-04-01', 0),
  (4, 'SI Null To Office', 4, 3, 1, 1, 75.00, DATE '2026-01-01', NULL, 1, 0, 1, 1, NULL, DATE '2026-04-01', 0),
  (5, 'SI Office 9 Only', 5, 3, 1, 1, 200.00, DATE '2026-01-01', NULL, 1, 0, 1, 1, NULL, DATE '2026-04-01', 0)),
m_account_transfer_standing_instructions_history(id, standing_instruction_id, status, execution_time, amount, error_log) AS (VALUES
  (101::bigint, 1::bigint, 'failed'::varchar(20), TIMESTAMP '2026-04-10 10:00:00', 100.00::numeric(19,6), 'Insufficient account balance.'::varchar(500)),
  (102, 1, 'success', TIMESTAMP '2026-04-15 10:00:00', 100.00, ''),
  (103, 3, 'failed', TIMESTAMP '2026-04-12 10:00:00', 50.00, 'Attempt on deleted instruction'),
  (104, 4, 'failed', TIMESTAMP '2026-04-18 10:00:00', 75.00, 'Attempt on null to_office instruction'),
  (105, 5, 'failed', TIMESTAMP '2026-04-20 10:00:00', 200.00, 'Office 9 attempt'))"

fail=0
pass() { echo "  PASS: $1"; }
fail() { echo "  FAIL: $1" >&2; fail=$((fail + 1)); }

run_config() {
  local scope_array="$1"
  local limit="$2"
  local body
  body="$(cat "$SQL_CONFIG")"
  local full_sql="${SHADOW_CTES}, prod_query AS (${body}) SELECT * FROM prod_query;"
  psql -X -A -t -q -v ON_ERROR_STOP=1 -d "$URL" \
    -c "PREPARE stmt (bigint[], integer) AS ${full_sql}; EXECUTE stmt ('${scope_array}', ${limit}); DEALLOCATE stmt;"
}

run_schedule() {
  local scope_array="$1"
  local limit="$2"
  local body
  body="$(cat "$SQL_SCHEDULE")"
  local full_sql="${SHADOW_CTES}, prod_query AS (${body}) SELECT * FROM prod_query;"
  psql -X -A -t -q -v ON_ERROR_STOP=1 -d "$URL" \
    -c "PREPARE stmt (bigint[], integer) AS ${full_sql}; EXECUTE stmt ('${scope_array}', ${limit}); DEALLOCATE stmt;"
}

run_executions() {
  local scope_array="$1"
  local from_date="$2"
  local to_date="$3"
  local limit="$4"
  local body
  body="$(cat "$SQL_EXECUTIONS")"
  local full_sql="${SHADOW_CTES}, prod_query AS (${body}) SELECT * FROM prod_query;"
  psql -X -A -t -q -v ON_ERROR_STOP=1 -d "$URL" \
    -c "PREPARE stmt (bigint[], date, date, integer) AS ${full_sql}; EXECUTE stmt ('${scope_array}', ${from_date}, ${to_date}, ${limit}); DEALLOCATE stmt;"
}

run_failures() {
  local scope_array="$1"
  local from_date="$2"
  local to_date="$3"
  local limit="$4"
  local body
  body="$(cat "$SQL_FAILURES")"
  local full_sql="${SHADOW_CTES}, prod_query AS (${body}) SELECT * FROM prod_query;"
  psql -X -A -t -q -v ON_ERROR_STOP=1 -d "$URL" \
    -c "PREPARE stmt (bigint[], date, date, integer) AS ${full_sql}; EXECUTE stmt ('${scope_array}', ${from_date}, ${to_date}, ${limit}); DEALLOCATE stmt;"
}

echo "Running standing instructions semantics smoke assertions..."

# 1. DELETED status 3 instruction (id 3) never leaks into standing_instruction_config
out_cfg="$(run_config "{2,3}" 100)"
if echo "$out_cfg" | grep -q "^3|"; then
  fail "SI-1 standing_instruction_config: DELETED status 3 instruction leaked into output: $out_cfg"
else
  pass "SI-1 standing_instruction_config: DELETED status 3 instruction strictly excluded"
fi

# 2. DELETED status 3 instruction never leaks into standing_instruction_schedule
out_sch="$(run_schedule "{2,3}" 100)"
if echo "$out_sch" | grep -q "^3|"; then
  fail "SI-2 standing_instruction_schedule: DELETED status 3 instruction leaked into schedule: $out_sch"
else
  pass "SI-2 standing_instruction_schedule: DELETED status 3 instruction strictly excluded"
fi

# 3. DELETED status 3 instruction history attempts (id 103) never leak into executions or failures
out_exec="$(run_executions "{2,3}" "'2026-04-01'" "'2026-04-30'" 100)"
if echo "$out_exec" | grep -q "^103|"; then
  fail "SI-3 standing_instruction_executions: attempt 103 for DELETED instruction leaked: $out_exec"
else
  pass "SI-3 standing_instruction_executions: attempts for DELETED instructions strictly excluded"
fi

out_fail="$(run_failures "{2,3}" "'2026-04-01'" "'2026-04-30'" 100)"
if echo "$out_fail" | grep -q "^103|"; then
  fail "SI-4 standing_instruction_failures: failed attempt 103 for DELETED instruction leaked: $out_fail"
else
  pass "SI-4 standing_instruction_failures: attempts for DELETED instructions strictly excluded from failures"
fi

# 4. NULL to_office_id instruction (id 4) is still visible via from_office_id
if echo "$out_cfg" | grep -q "^4|SI Null To Office|1|1|3|75.*|USD|2||1|"; then
  pass "SI-5 standing_instruction_config: NULL to_office_id instruction visible via from_office_id"
else
  fail "SI-5 standing_instruction_config: NULL to_office_id instruction missing or malformed: $out_cfg"
fi

# 5. recurrence_frequency NULL stays NULL (never coerced to 0)
if echo "$out_sch" | grep -q "^2|SI Active Loan|2|3|2|||||0|"; then
  pass "SI-6 standing_instruction_schedule: recurrence_frequency NULL preserved as NULL (not coerced to 0)"
else
  fail "SI-6 standing_instruction_schedule: recurrence_frequency NULL coerced or missing: $out_sch"
fi

# 6. Failures query never returns a success row even when window contains both
if echo "$out_fail" | grep -q "^101|1|SI Active Savings|failed|" && ! echo "$out_fail" | grep -q "^102|"; then
  pass "SI-7 standing_instruction_failures: returned only failed attempt (101), strictly excluded success attempt (102)"
else
  fail "SI-7 standing_instruction_failures: returned success attempt or missing failed attempt: $out_fail"
fi

# 7. Attempt-grain listing never fans instructions (1:1 per history attempt)
count_attempts="$(echo "$out_exec" | grep -c "^101|" || true)"
if [ "$count_attempts" -eq 1 ]; then
  pass "SI-8 standing_instruction_executions: anti-fanout holds (attempt 101 appears exactly once)"
else
  fail "SI-8 standing_instruction_executions: fanout detected (attempt 101 count: $count_attempts)"
fi

# 8. Office isolation: office 9 instructions and attempts excluded under {2, 3} scope
if ! echo "$out_cfg" | grep -q "^5|" && ! echo "$out_exec" | grep -q "^105|"; then
  pass "SI-9 office authorization: office 9 instruction (5) and attempt (105) strictly excluded under {2,3} scope"
else
  fail "SI-9 office authorization: office 9 row leaked: cfg=$out_cfg, exec=$out_exec"
fi

# 9. Currency resolution: SI 1 resolves 'USD' from savings, SI 2 resolves 'EUR' from loan
if echo "$out_cfg" | grep -q "^1|.*|USD|" && echo "$out_cfg" | grep -q "^2|.*|EUR|"; then
  pass "SI-10 currency resolution: resolved accurately from savings (USD) and loan (EUR) legs without conversion"
else
  fail "SI-10 currency resolution: currency mismatch: $out_cfg"
fi

echo "=== Smoke result: $fail failure(s) ==="
exit $fail
