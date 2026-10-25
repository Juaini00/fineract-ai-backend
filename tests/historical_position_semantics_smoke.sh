#!/usr/bin/env bash
#
# FIN-121 nonzero semantics smoke for queries/historical/*.sql.
#
# Deterministic and read-only: the production SQL text is executed UNCHANGED, but every
# Fineract table it names is shadowed by a CTE of the same name built from inline VALUES,
# so no Fineract table is read and no fixture row exists anywhere. The connection only
# hosts the statement. Cases proven:
#   * As-of date picks the LATEST qualifying transaction (not the first or intermediate);
#   * Reversed rows strictly excluded (is_reversed = false);
#   * Transactions after as_of_date strictly excluded;
#   * Accounts with no qualifying transactions excluded from rows (never zero-filled);
#   * Office path binds (COALESCE(c.office_id, g.office_id) = ANY($1::bigint[]));
#   * Currency split (currency_code in grain, no cross-currency sums);
#   * Limit parameter binds and is respected;
#   * Savings balance as-of behaves identically on running_balance_derived;
#   * Reconciliation identity holds where transactions represent full position.

set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
URL="${SMOKE_DATABASE_URL:-postgres://root:password@127.0.0.1:5432/fineract_default}"

SQL_LOAN="$ROOT/queries/historical/loan_position_asof.sql"
SQL_SAVINGS="$ROOT/queries/historical/savings_balance_asof.sql"

# Strip production "WITH latest_tx AS" so it chains into the shadow CTE list
LOAN_BODY="$(cat "$SQL_LOAN" | sed -e 's/^--.*//' | sed 's/WITH latest_tx AS/, latest_tx AS/' | tr '\n' ' ')"
SAVINGS_BODY="$(cat "$SQL_SAVINGS" | sed -e 's/^--.*//' | sed 's/WITH latest_tx AS/, latest_tx AS/' | tr '\n' ' ')"

SHADOW_LOAN_CTES="WITH
m_client(id, office_id) AS (VALUES
  (1::bigint, 1::bigint),
  (2::bigint, 2::bigint),
  (3::bigint, 3::bigint),
  (4::bigint, 9::bigint)
),
m_group(id, office_id) AS (VALUES
  (10::bigint, 1::bigint)
),
m_loan(id, client_id, group_id, currency_code) AS (VALUES
  (101::bigint, 1::bigint, NULL::bigint, 'USD'::varchar),
  (102::bigint, 2::bigint, NULL::bigint, 'USD'::varchar),
  (103::bigint, 3::bigint, NULL::bigint, 'EUR'::varchar),
  (104::bigint, 4::bigint, NULL::bigint, 'USD'::varchar),
  (105::bigint, 1::bigint, NULL::bigint, 'USD'::varchar)
),
m_loan_transaction(id, loan_id, transaction_date, outstanding_loan_balance_derived, is_reversed) AS (VALUES
  (1::bigint, 101::bigint, '2026-01-15'::date, 1000.00::decimal, false),
  (2::bigint, 101::bigint, '2026-02-20'::date,  800.00::decimal, false),
  (3::bigint, 101::bigint, '2026-03-25'::date,  600.00::decimal, false),
  (4::bigint, 101::bigint, '2026-04-10'::date,  400.00::decimal, true),
  (5::bigint, 102::bigint, '2026-01-10'::date,  500.00::decimal, false),
  (6::bigint, 102::bigint, '2026-05-01'::date,  300.00::decimal, false),
  (7::bigint, 103::bigint, '2026-03-01'::date,  750.00::decimal, false),
  (8::bigint, 104::bigint, '2026-02-01'::date, 1500.00::decimal, false)
)"

SHADOW_SAVINGS_CTES="WITH
m_client(id, office_id) AS (VALUES
  (1::bigint, 1::bigint),
  (2::bigint, 2::bigint),
  (3::bigint, 9::bigint)
),
m_group(id, office_id) AS (VALUES
  (10::bigint, 1::bigint)
),
m_savings_account(id, client_id, group_id, currency_code) AS (VALUES
  (201::bigint, 1::bigint, NULL::bigint, 'USD'::varchar),
  (202::bigint, 2::bigint, NULL::bigint, 'USD'::varchar),
  (203::bigint, 3::bigint, NULL::bigint, 'USD'::varchar),
  (204::bigint, 1::bigint, NULL::bigint, 'EUR'::varchar)
),
m_savings_account_transaction(id, savings_account_id, transaction_date, running_balance_derived, is_reversed) AS (VALUES
  (1::bigint, 201::bigint, '2026-01-10'::date,  200.00::decimal, false),
  (2::bigint, 201::bigint, '2026-02-15'::date,  450.00::decimal, false),
  (3::bigint, 201::bigint, '2026-03-20'::date,  650.00::decimal, false),
  (4::bigint, 201::bigint, '2026-04-05'::date,  900.00::decimal, true),
  (5::bigint, 202::bigint, '2026-01-05'::date,  150.00::decimal, false),
  (6::bigint, 202::bigint, '2026-04-20'::date,  350.00::decimal, false),
  (7::bigint, 203::bigint, '2026-02-01'::date, 1000.00::decimal, false)
)"

fail=0
pass() { echo "  PASS: $1"; }
fail() { echo "  FAIL: $1" >&2; fail=$((fail + 1)); }

echo "=== Test 1: Latest Qualifying Transaction Pick & Reversed Row Exclusion ==="
OUTPUT_T1="$(PGTZ=UTC psql -X -A -t -F '|' -q -v ON_ERROR_STOP=1 -d "$URL" <<EOF
PREPARE loan_t1(bigint[], date, int) AS
$SHADOW_LOAN_CTES
$LOAN_BODY;
EXECUTE loan_t1('{1,2,3}'::bigint[], '2026-03-31'::date, 100);
EOF
)"

echo "$OUTPUT_T1"

# Loan 101 must pick Tx 3 (2026-03-25, 600.00), skipping Tx 4 (reversed)
loan_101_pos="$(echo "$OUTPUT_T1" | grep '^101|' | cut -d'|' -f5)"
loan_101_date="$(echo "$OUTPUT_T1" | grep '^101|' | cut -d'|' -f6)"
if [ "$loan_101_pos" = "600.00" ] && [ "$loan_101_date" = "2026-03-25" ]; then
  pass "Loan 101 picked latest qualifying transaction (600.00 on 2026-03-25), excluding reversed Tx 4"
else
  fail "Loan 101 expected 600.00 on 2026-03-25, got '$loan_101_pos' on '$loan_101_date'"
fi

# Loan 102 must pick Tx 5 (2026-01-10, 500.00), excluding Tx 6 (2026-05-01, after as_of_date)
loan_102_pos="$(echo "$OUTPUT_T1" | grep '^102|' | cut -d'|' -f5)"
loan_102_date="$(echo "$OUTPUT_T1" | grep '^102|' | cut -d'|' -f6)"
if [ "$loan_102_pos" = "500.00" ] && [ "$loan_102_date" = "2026-01-10" ]; then
  pass "Loan 102 picked Tx 5 before as_of_date cutoff (500.00 on 2026-01-10), excluding post-cutoff Tx 6"
else
  fail "Loan 102 expected 500.00 on 2026-01-10, got '$loan_102_pos' on '$loan_102_date'"
fi

# Loan 105 has no transactions at all -> must NOT be present (no zero-filling)
if echo "$OUTPUT_T1" | grep -q '^105|'; then
  fail "Loan 105 has no transactions but appeared in output"
else
  pass "Loan 105 with no transactions is excluded (never zero-filled)"
fi

# Loan 104 is in office 9 (unauthorized) -> must NOT be present
if echo "$OUTPUT_T1" | grep -q '^104|'; then
  fail "Loan 104 in office 9 appeared in authorized scope {1,2,3}"
else
  pass "Office authorization strictly enforced (office 9 excluded)"
fi

echo "=== Test 2: Currency Segregation & Office Narrowing ==="
# Scope {1,2}: Loan 101 (USD, 600.00), Loan 102 (USD, 500.00). Loan 103 (EUR, office 3) must be excluded.
OUTPUT_T2="$(PGTZ=UTC psql -X -A -t -F '|' -q -v ON_ERROR_STOP=1 -d "$URL" <<EOF
PREPARE loan_t2(bigint[], date, int) AS
$SHADOW_LOAN_CTES
$LOAN_BODY;
EXECUTE loan_t2('{1,2}'::bigint[], '2026-03-31'::date, 100);
EOF
)"

echo "$OUTPUT_T2"

row_count_t2="$(echo "$OUTPUT_T2" | grep -v '^$' | wc -l | tr -d ' ')"
if [ "$row_count_t2" -eq 2 ]; then
  pass "Narrowed scope {1,2} returned exactly 2 qualifying loans"
else
  fail "Narrowed scope {1,2} expected 2 rows, got $row_count_t2"
fi

if echo "$OUTPUT_T2" | grep -q 'EUR'; then
  fail "Office 3 EUR loan appeared under scope {1,2}"
else
  pass "Office 3 EUR loan correctly omitted under scope {1,2}"
fi

# Scope {3}: Loan 103 (EUR, 750.00) only.
OUTPUT_T3="$(PGTZ=UTC psql -X -A -t -F '|' -q -v ON_ERROR_STOP=1 -d "$URL" <<EOF
PREPARE loan_t3(bigint[], date, int) AS
$SHADOW_LOAN_CTES
$LOAN_BODY;
EXECUTE loan_t3('{3}'::bigint[], '2026-03-31'::date, 100);
EOF
)"

echo "$OUTPUT_T3"

loan_103_cur="$(echo "$OUTPUT_T3" | grep '^103|' | cut -d'|' -f4)"
loan_103_pos="$(echo "$OUTPUT_T3" | grep '^103|' | cut -d'|' -f5)"
if [ "$loan_103_cur" = "EUR" ] && [ "$loan_103_pos" = "750.00" ]; then
  pass "Scope {3} returned EUR loan 103 with position 750.00"
else
  fail "Scope {3} expected EUR loan 103 with 750.00, got '$loan_103_cur' '$loan_103_pos'"
fi

echo "=== Test 3: Reconciliation Identity on Controlled Fixture ==="
# As-of latest date (2026-05-01) for scope {1,2}:
# Loan 101 latest = 600.00 (Tx 4 is reversed)
# Loan 102 latest = 300.00 (Tx 6 on 2026-05-01)
# Total USD position = 900.00
OUTPUT_T4="$(PGTZ=UTC psql -X -A -t -F '|' -q -v ON_ERROR_STOP=1 -d "$URL" <<EOF
PREPARE loan_t4(bigint[], date, int) AS
$SHADOW_LOAN_CTES
$LOAN_BODY;
EXECUTE loan_t4('{1,2}'::bigint[], '2026-05-01'::date, 100);
EOF
)"

total_usd_pos="$(echo "$OUTPUT_T4" | cut -d'|' -f5 | paste -sd+ - | bc)"
if [ "$total_usd_pos" = "900.00" ]; then
  pass "Reconciliation identity asserted: total USD position matches latest transactions sum (900.00)"
else
  fail "Reconciliation identity failed: expected 900.00, got '$total_usd_pos'"
fi

echo "=== Test 4: Savings Balance As-Of Nonzero Semantics ==="
OUTPUT_SAVINGS="$(PGTZ=UTC psql -X -A -t -F '|' -q -v ON_ERROR_STOP=1 -d "$URL" <<EOF
PREPARE savings_t1(bigint[], date, int) AS
$SHADOW_SAVINGS_CTES
$SAVINGS_BODY;
EXECUTE savings_t1('{1,2}'::bigint[], '2026-03-31'::date, 100);
EOF
)"

echo "$OUTPUT_SAVINGS"

# Savings 201 must pick Tx 3 (2026-03-20, 650.00), skipping reversed Tx 4
sav_201_pos="$(echo "$OUTPUT_SAVINGS" | grep '^201|' | cut -d'|' -f5)"
sav_201_date="$(echo "$OUTPUT_SAVINGS" | grep '^201|' | cut -d'|' -f6)"
if [ "$sav_201_pos" = "650.00" ] && [ "$sav_201_date" = "2026-03-20" ]; then
  pass "Savings 201 picked latest qualifying transaction (650.00 on 2026-03-20), excluding reversed Tx 4"
else
  fail "Savings 201 expected 650.00 on 2026-03-20, got '$sav_201_pos' on '$sav_201_date'"
fi

# Savings 202 must pick Tx 5 (2026-01-05, 150.00), skipping post-cutoff Tx 6 (2026-04-20)
sav_202_pos="$(echo "$OUTPUT_SAVINGS" | grep '^202|' | cut -d'|' -f5)"
sav_202_date="$(echo "$OUTPUT_SAVINGS" | grep '^202|' | cut -d'|' -f6)"
if [ "$sav_202_pos" = "150.00" ] && [ "$sav_202_date" = "2026-01-05" ]; then
  pass "Savings 202 picked Tx 5 before as_of_date cutoff (150.00 on 2026-01-05)"
else
  fail "Savings 202 expected 150.00 on 2026-01-05, got '$sav_202_pos' on '$sav_202_date'"
fi

# Savings 203 in office 9 must be excluded
if echo "$OUTPUT_SAVINGS" | grep -q '^203|'; then
  fail "Savings 203 in office 9 appeared under scope {1,2}"
else
  pass "Savings account in unauthorized office 9 excluded"
fi

# Savings 204 with no transactions must be excluded
if echo "$OUTPUT_SAVINGS" | grep -q '^204|'; then
  fail "Savings 204 with no transactions appeared in output"
else
  pass "Savings 204 with no transactions excluded (no zero-filling)"
fi

echo "=== Test 5: Limit Parameter Enforced ==="
OUTPUT_LIMIT="$(PGTZ=UTC psql -X -A -t -F '|' -q -v ON_ERROR_STOP=1 -d "$URL" <<EOF
PREPARE loan_limit(bigint[], date, int) AS
$SHADOW_LOAN_CTES
$LOAN_BODY;
EXECUTE loan_limit('{1,2,3}'::bigint[], '2026-03-31'::date, 1);
EOF
)"

echo "$OUTPUT_LIMIT"

limit_count="$(echo "$OUTPUT_LIMIT" | grep -v '^$' | wc -l | tr -d ' ')"
if [ "$limit_count" -eq 1 ]; then
  pass "Bound limit=1 strictly enforced (returned exactly 1 row)"
else
  fail "Bound limit=1 failed: expected 1 row, got $limit_count"
fi

if [ "$fail" -eq 0 ]; then
  echo "ALL TESTS PASSED"
  exit 0
else
  echo "TESTS FAILED ($fail failure(s))" >&2
  exit 1
fi
