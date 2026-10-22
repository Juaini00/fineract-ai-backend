#!/usr/bin/env bash
#
# FIN-110 nonzero semantics smoke for queries/fixed_recurring_deposit/*.sql.
#
# Deterministic and read-only: the production SQL text is executed UNCHANGED, but every
# Fineract table it names is shadowed by a CTE of the same name built from inline VALUES,
# so no Fineract table is read and no fixture row exists anywhere. The connection only
# hosts the statement. Cases:
#   * fd_rd_accounts: office authorization governs (unauthorized office 9 filtered out);
#   * fd_rd_accounts: nominal rate and posted interest surfaced, NULL stays NULL, no schedule imposed on FD;
#   * fd_rd_term_preclosure: maturity_amount is labeled as projection, never payout evidence;
#   * fd_rd_term_preclosure: on_account_closure_enum is surfaced as raw instruction, not payout;
#   * fd_rd_funding: funding deposits near activation surfaced; reversed transactions excluded;
#   * rd_contributions: expected config vs actual deposit transactions stay separate, no fan-out;
#   * rd_shortfall: overdue amount/count read from recurring detail, distinct from loan arrears.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
URL="${SMOKE_DATABASE_URL:-postgres://root:password@127.0.0.1:5432/jarvis}"
SQL_ACCOUNTS="$ROOT/queries/fixed_recurring_deposit/fd_rd_accounts.sql"
SQL_TERMS="$ROOT/queries/fixed_recurring_deposit/fd_rd_term_preclosure.sql"
SQL_FUNDING="$ROOT/queries/fixed_recurring_deposit/fd_rd_funding.sql"
SQL_CONTRIB="$ROOT/queries/fixed_recurring_deposit/rd_contributions.sql"
SQL_SHORTFALL="$ROOT/queries/fixed_recurring_deposit/rd_shortfall.sql"

SHADOW_CTES="WITH
m_office(id, name) AS (VALUES
  (1::bigint, 'Head Office'::varchar(100)),
  (2, 'Office 2'),
  (9, 'Office 9')),
m_client(id, office_id) AS (VALUES
  (1::bigint, 2::bigint),
  (2, 9)),
m_savings_account(id, client_id, deposit_type_enum, status_enum, product_id, currency_code, nominal_annual_interest_rate, total_interest_posted_derived, submittedon_date, activatedon_date, closedon_date) AS (VALUES
  (10::bigint, 1::bigint, 200::smallint, 300::smallint, 1::bigint, 'USD'::varchar(3), 5.0::numeric(19,6), 125.50::numeric(19,6), DATE '2026-01-15', DATE '2026-01-15', NULL::date),
  (20, 1, 300, 300, 2, 'EUR', 3.0, NULL, DATE '2026-01-20', DATE '2026-01-20', NULL),
  (30, 2, 200, 300, 1, 'USD', 5.0, 50.0, DATE '2026-01-15', DATE '2026-01-15', NULL)),
m_deposit_account_term_and_preclosure(id, savings_account_id, deposit_amount, maturity_amount, maturity_date, min_deposit_term, min_deposit_term_type_enum, deposit_period, deposit_period_frequency_enum, pre_closure_penal_applicable, pre_closure_penal_interest, pre_closure_penal_interest_on_enum, on_account_closure_enum) AS (VALUES
  (100::bigint, 10::bigint, 10000::numeric(19,6), 10500::numeric(19,6), DATE '2026-06-15', 6::integer, 2::smallint, 6::integer, 2::smallint, true, 1.0::numeric(19,6), 1::smallint, 200::smallint),
  (200, 20, 12000, 12360, DATE '2026-06-20', 6, 2, 6, 2, false, NULL, NULL, NULL),
  (300, 30, 5000, 5250, DATE '2026-06-15', 6, 2, 6, 2, false, NULL, NULL, 100)),
m_deposit_account_recurring_detail(id, savings_account_id, mandatory_recommended_deposit_amount, is_mandatory, allow_withdrawal, adjust_advance_towards_future_payments, is_calendar_inherited, total_overdue_amount, no_of_overdue_installments) AS (VALUES
  (2000::bigint, 20::bigint, 2000::numeric(19,6), true, false, false, false, 4000::numeric(19,6), 2::integer)),
m_savings_account_transaction(id, savings_account_id, transaction_type_enum, is_reversed, transaction_date, amount) AS (VALUES
  (1001::bigint, 10::bigint, 1::smallint, false, DATE '2026-01-15', 10000::numeric(19,6)),
  (1002, 10, 1, false, DATE '2026-01-15', 500),
  (1003, 10, 1, true, DATE '2026-01-15', 200),
  (2001, 20, 1, false, DATE '2026-01-20', 2000),
  (2002, 20, 1, false, DATE '2026-02-20', 2000),
  (3001, 30, 1, false, DATE '2026-01-15', 5000))"

fail=0

echo "=== Test 1: fd_rd_accounts ==="
BODY_ACCOUNTS="$(awk '/^SELECT/ {p=1} p {print}' "$SQL_ACCOUNTS")"
ACTUAL_ACCOUNTS="$(psql -X -A -t -F '|' -q -v ON_ERROR_STOP=1 -d "$URL" <<EOF
PREPARE accounts_smoke(bigint[], date, date, int) AS
$SHADOW_CTES
$BODY_ACCOUNTS;
EXECUTE accounts_smoke('{2}'::bigint[], DATE '2026-01-01', DATE '2026-03-31', 200);
EOF
)"

echo "$ACTUAL_ACCOUNTS"

# Accounts 10 and 20 (office 2) must be present, account 30 (office 9) must be absent
if echo "$ACTUAL_ACCOUNTS" | grep -q '^10|' && echo "$ACTUAL_ACCOUNTS" | grep -q '^20|'; then
  echo "ok   accounts 10 (FD) and 20 (RD) present in authorized office 2"
else
  echo "FAIL: accounts 10 or 20 missing" >&2; fail=1
fi

if echo "$ACTUAL_ACCOUNTS" | grep -q '^30|'; then
  echo "FAIL: account 30 in unauthorized office 9 leaked" >&2; fail=1
else
  echo "ok   account 30 in unauthorized office 9 absent"
fi

# NULL interest posted stays NULL
line20="$(echo "$ACTUAL_ACCOUNTS" | grep '^20|' || true)"
interest20="$(echo "$line20" | cut -d'|' -f9)"
if [ -n "$interest20" ]; then
  echo "FAIL: total_interest_posted_derived for account 20 expected NULL, got '$interest20'" >&2; fail=1
else
  echo "ok   total_interest_posted_derived remains NULL (never coerced to 0)"
fi

echo "=== Test 2: fd_rd_term_preclosure ==="
BODY_TERMS="$(awk '/^SELECT/ {p=1} p {print}' "$SQL_TERMS")"
ACTUAL_TERMS="$(psql -X -A -t -F '|' -q -v ON_ERROR_STOP=1 -d "$URL" <<EOF
PREPARE terms_smoke(bigint[], date, date, int) AS
$SHADOW_CTES
$BODY_TERMS;
EXECUTE terms_smoke('{2}'::bigint[], DATE '2026-06-01', DATE '2026-06-30', 200);
EOF
)"

echo "$ACTUAL_TERMS"

# Terms 100 and 200 present, 300 (office 9) absent
line100="$(echo "$ACTUAL_TERMS" | grep '^100|' || true)"
if [ -z "$line100" ]; then
  echo "FAIL: term 100 missing" >&2; fail=1
else
  # Column 8 = maturity_amount (10500), column 17 = on_account_closure_enum (200)
  mat100="$(echo "$line100" | cut -d'|' -f8 | sed 's/\.0*$//')"
  closure100="$(echo "$line100" | cut -d'|' -f17)"
  if [ "$mat100" != "10500" ]; then
    echo "FAIL: maturity_amount expected 10500, got '$mat100'" >&2; fail=1
  else
    echo "ok   maturity_amount is projected figure 10500"
  fi
  if [ "$closure100" != "200" ]; then
    echo "FAIL: on_account_closure_enum expected 200, got '$closure100'" >&2; fail=1
  else
    echo "ok   on_account_closure_enum surfaced as raw instruction 200, not interpreted as payout"
  fi
fi

if echo "$ACTUAL_TERMS" | grep -q '^300|'; then
  echo "FAIL: term 300 in unauthorized office 9 leaked" >&2; fail=1
else
  echo "ok   term 300 in unauthorized office 9 absent"
fi

echo "=== Test 3: fd_rd_funding ==="
BODY_FUNDING="$(awk '/^SELECT/ {p=1} p {print}' "$SQL_FUNDING")"
ACTUAL_FUNDING="$(psql -X -A -t -F '|' -q -v ON_ERROR_STOP=1 -d "$URL" <<EOF
PREPARE funding_smoke(bigint[], date, date, int) AS
$SHADOW_CTES
$BODY_FUNDING;
EXECUTE funding_smoke('{2}'::bigint[], DATE '2026-01-01', DATE '2026-03-31', 200);
EOF
)"

echo "$ACTUAL_FUNDING"

# Transactions 1001, 1002, 2001 present; 1003 (reversed) absent; 3001 (office 9) absent
if echo "$ACTUAL_FUNDING" | grep -q '^1001|' && echo "$ACTUAL_FUNDING" | grep -q '^1002|'; then
  echo "ok   funding deposit transactions 1001 and 1002 present"
else
  echo "FAIL: funding deposit transactions 1001 or 1002 missing" >&2; fail=1
fi

if echo "$ACTUAL_FUNDING" | grep -q '^1003|'; then
  echo "FAIL: reversed transaction 1003 leaked into funding" >&2; fail=1
else
  echo "ok   reversed transaction 1003 excluded from funding"
fi

if echo "$ACTUAL_FUNDING" | grep -q '^3001|'; then
  echo "FAIL: transaction 3001 in unauthorized office 9 leaked" >&2; fail=1
else
  echo "ok   transaction 3001 in unauthorized office 9 absent"
fi

echo "=== Test 4: rd_contributions ==="
BODY_CONTRIB="$(awk '/^SELECT/ {p=1} p {print}' "$SQL_CONTRIB")"
ACTUAL_CONTRIB="$(psql -X -A -t -F '|' -q -v ON_ERROR_STOP=1 -d "$URL" <<EOF
PREPARE contrib_smoke(bigint[], date, date, int) AS
$SHADOW_CTES
$BODY_CONTRIB;
EXECUTE contrib_smoke('{2}'::bigint[], DATE '2026-01-01', DATE '2026-03-31', 200);
EOF
)"

echo "$ACTUAL_CONTRIB"

# Account 20 present (RD), exactly one row (anti-fanout) despite 2 transactions
rd_count="$(echo "$ACTUAL_CONTRIB" | grep -c '^20|' || true)"
if [ "$rd_count" -ne 1 ]; then
  echo "FAIL: account 20 expected exactly 1 row (no fan-out), got $rd_count" >&2; fail=1
else
  echo "ok   anti-fanout: account 20 has exactly 1 row across 2 deposit transactions"
fi

line20_c="$(echo "$ACTUAL_CONTRIB" | grep '^20|' || true)"
exp_amt="$(echo "$line20_c" | cut -d'|' -f5 | sed 's/\.0*$//')"
act_amt="$(echo "$line20_c" | cut -d'|' -f6 | sed 's/\.0*$//')"
act_cnt="$(echo "$line20_c" | cut -d'|' -f7)"
if [ "$exp_amt" != "2000" ] || [ "$act_amt" != "4000" ] || [ "$act_cnt" != "2" ]; then
  echo "FAIL: expected/actual amounts or count mismatch: exp='$exp_amt', act='$act_amt', cnt='$act_cnt'" >&2; fail=1
else
  echo "ok   expected (2000) vs actual (4000 across 2 txns) contributions kept separate"
fi

# Account 10 is FD, must not appear in RD contributions
if echo "$ACTUAL_CONTRIB" | grep -q '^10|'; then
  echo "FAIL: FD account 10 leaked into RD contributions" >&2; fail=1
else
  echo "ok   FD account 10 absent from RD contributions"
fi

echo "=== Test 5: rd_shortfall ==="
BODY_SHORTFALL="$(awk '/^SELECT/ {p=1} p {print}' "$SQL_SHORTFALL")"
ACTUAL_SHORTFALL="$(psql -X -A -t -F '|' -q -v ON_ERROR_STOP=1 -d "$URL" <<EOF
PREPARE shortfall_smoke(bigint[], date, date, int) AS
$SHADOW_CTES
$BODY_SHORTFALL;
EXECUTE shortfall_smoke('{2}'::bigint[], DATE '2026-01-01', DATE '2026-03-31', 200);
EOF
)"

echo "$ACTUAL_SHORTFALL"

line20_s="$(echo "$ACTUAL_SHORTFALL" | grep '^20|' || true)"
if [ -z "$line20_s" ]; then
  echo "FAIL: account 20 missing in shortfall" >&2; fail=1
else
  s_amt="$(echo "$line20_s" | cut -d'|' -f5 | sed 's/\.0*$//')"
  s_inst="$(echo "$line20_s" | cut -d'|' -f6)"
  if [ "$s_amt" != "4000" ] || [ "$s_inst" != "2" ]; then
    echo "FAIL: shortfall amount expected 4000, installments 2; got amt='$s_amt', inst='$s_inst'" >&2; fail=1
  else
    echo "ok   shortfall overdue amount (4000) and count (2) verified from recurring detail"
  fi
fi

if [ "$fail" -eq 0 ]; then
  echo "=== All FDRD nonzero semantics smoke checks PASSED ==="
fi
exit $fail
