#!/usr/bin/env bash
#
# FIN-111 nonzero semantics smoke for queries/share/*.sql.
#
# Deterministic and read-only: the production SQL text is executed UNCHANGED, but every
# Fineract table it names is shadowed by a CTE of the same name built from inline VALUES,
# so no Fineract table is read and no fixture row exists anywhere. The connection only
# hosts the statement. Cases:
#   * share_accounts: office authorization governs (unauthorized office 9 filtered out);
#   * share_accounts: pending != ownership (approved shares vs pending shares separated);
#   * share_accounts: savings_account_id surfaced as linkage only, no settlement claim;
#   * share_account_transactions: units vs money never conflated (total_shares vs amount);
#   * share_account_transactions: historical unit_price recorded at txn time, no current product price backfill;
#   * share_account_transactions: correction via is_active=false surfaced, not dropped or netted;
#   * share_account_charges: master vs actual charge separated, NULL derived amounts stay NULL;
#   * share_charge_payment_allocations: grain is allocation row, amount is pb.amount (anti-fanout);
#   * share_account_dividends: paid requires posted status AND savings_transaction_id (both visible);
#   * share_account_dividends: period dates via 1:1 PK join to payout event without fanout.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
URL="${SMOKE_DATABASE_URL:-postgres://root:password@127.0.0.1:5432/fineract_default}"
SQL_ACCOUNTS="$ROOT/queries/share/share_accounts.sql"
SQL_TXNS="$ROOT/queries/share/share_account_transactions.sql"
SQL_CHARGES="$ROOT/queries/share/share_account_charges.sql"
SQL_ALLOC="$ROOT/queries/share/share_charge_payment_allocations.sql"
SQL_DIVIDENDS="$ROOT/queries/share/share_account_dividends.sql"

SHADOW_CTES="WITH
m_office(id, name) AS (VALUES
  (1::bigint, 'Head Office'::varchar(100)),
  (2, 'Office 2'),
  (9, 'Office 9')),
m_client(id, office_id) AS (VALUES
  (1::bigint, 2::bigint),
  (2, 9)),
m_share_product(id, name, currency_code, total_shares, unit_price) AS (VALUES
  (1::bigint, 'Regular Member Shares'::varchar(200), 'USD'::varchar(3), 1000::bigint, 25.00::numeric(10,2))),
m_share_account(id, client_id, product_id, status_enum, total_approved_shares, total_pending_shares, submitted_date, approved_date, rejected_date, activated_date, closed_date, currency_code, currency_digits, currency_multiplesof, savings_account_id, minimum_active_period_frequency, minimum_active_period_frequency_enum, lockin_period_frequency, lockin_period_frequency_enum, allow_dividends_inactive_clients) AS (VALUES
  (10::bigint, 1::bigint, 1::bigint, 300::smallint, 100::bigint, 50::bigint, DATE '2026-01-10', DATE '2026-01-10', NULL::date, DATE '2026-01-10', NULL::date, 'USD'::varchar(3), 2::smallint, 100::smallint, 555::bigint, NULL::numeric(19,6), 5::smallint, NULL::numeric(19,6), 5::smallint, true),
  (20, 2, 1, 300, 200, 0, DATE '2026-01-10', DATE '2026-01-10', NULL, DATE '2026-01-10', NULL, 'USD', 2, 100, 556, NULL, 5, NULL, 5, false)),
m_share_account_transactions(id, account_id, transaction_date, total_shares, unit_price, amount, charge_amount, amount_paid, status_enum, type_enum, is_active) AS (VALUES
  (101::bigint, 10::bigint, DATE '2026-01-15', 100::bigint, 10.00::numeric(10,2), 1000.00::numeric(20,2), 5.00::numeric(20,2), 1005.00::numeric(20,2), 300::smallint, 500::smallint, true),
  (102, 10, DATE '2026-01-20', 10, 12.00, 120.00, 0.00, 120.00, 300, 600, false),
  (201, 20, DATE '2026-01-15', 50, 10.00, 500.00, 0.00, 500.00, 300, 500, true)),
m_charge(id, name, currency_code) AS (VALUES
  (1::bigint, 'Share Purchase Fee'::varchar(100), 'USD'::varchar(3))),
m_share_account_charge(id, account_id, charge_id, charge_time_enum, charge_calculation_enum, charge_payment_mode_enum, amount, amount_paid_derived, amount_waived_derived, amount_writtenoff_derived, amount_outstanding_derived, is_paid_derived, waived, is_active) AS (VALUES
  (1001::bigint, 10::bigint, 1::bigint, 14::smallint, 1::smallint, 0::smallint, 5.00::numeric(19,6), 5.00::numeric(19,6), NULL::numeric(19,6), NULL::numeric(19,6), 0.00::numeric(19,6), true, false, true),
  (2001, 20, 1, 14, 1, 0, 10.00, 10.00, NULL, NULL, 0.00, true, false, true)),
m_share_account_charge_paid_by(id, charge_transaction_id, share_transaction_id, amount) AS (VALUES
  (1::bigint, 1001::bigint, 101::bigint, 5.00::numeric(20,2)),
  (2, 2001, 201, 10.00)),
m_share_product_dividend_pay_out(id, product_id, dividend_period_start_date, dividend_period_end_date) AS (VALUES
  (5001::bigint, 1::bigint, DATE '2025-01-01', DATE '2025-12-31')),
m_share_account_dividend_details(id, dividend_pay_out_id, account_id, amount, status, savings_transaction_id) AS (VALUES
  (901::bigint, 5001::bigint, 10::bigint, 150.00::numeric(19,6), 300::smallint, 8888::bigint),
  (902, 5001, 10, 75.00::numeric(19,6), 100, NULL::bigint),
  (903, 5001, 20, 200.00, 300, 9999))"

fail=0

echo "=== Test 1: share_accounts ==="
BODY_ACCOUNTS="$(awk '/^SELECT/ {p=1} p {print}' "$SQL_ACCOUNTS")"
ACTUAL_ACCOUNTS="$(psql -X -A -t -F '|' -q -v ON_ERROR_STOP=1 -d "$URL" <<EOF
PREPARE accounts_smoke(bigint[], date, date, int) AS
$SHADOW_CTES
$BODY_ACCOUNTS;
EXECUTE accounts_smoke('{2}'::bigint[], DATE '2026-01-01', DATE '2026-03-31', 200);
EOF
)"

echo "$ACTUAL_ACCOUNTS"
if grep -q "^10|1|2|1|Regular Member Shares|USD|300|2026-01-10|2026-01-10||2026-01-10||USD|2|100|100|50||5||5|t|555$" <<<"$ACTUAL_ACCOUNTS"; then
  echo "ok   account 10 in authorized office 2 present with approved (100) vs pending (50) separated"
else
  echo "FAIL account 10 unexpected output"
  fail=$((fail + 1))
fi
if grep -q "20|" <<<"$ACTUAL_ACCOUNTS"; then
  echo "FAIL account 20 in unauthorized office 9 was not filtered"
  fail=$((fail + 1))
else
  echo "ok   account 20 in unauthorized office 9 absent"
fi

echo "=== Test 2: share_account_transactions ==="
BODY_TXNS="$(awk '/^SELECT/ {p=1} p {print}' "$SQL_TXNS")"
ACTUAL_TXNS="$(psql -X -A -t -F '|' -q -v ON_ERROR_STOP=1 -d "$URL" <<EOF
PREPARE txns_smoke(bigint[], date, date, int) AS
$SHADOW_CTES
$BODY_TXNS;
EXECUTE txns_smoke('{2}'::bigint[], DATE '2026-01-01', DATE '2026-03-31', 200);
EOF
)"

echo "$ACTUAL_TXNS"
if grep -q "^101|10|1|2|2026-01-15|100|10.00|1000.00|5.00|1005.00|300|500|t$" <<<"$ACTUAL_TXNS"; then
  echo "ok   units (100) vs money (1000.00) distinct; recorded unit_price (10.00) preserved without product price (25.00) backfill"
else
  echo "FAIL transaction 101 unexpected output"
  fail=$((fail + 1))
fi
if grep -q "^102|10|1|2|2026-01-20|10|12.00|120.00|0.00|120.00|300|600|f$" <<<"$ACTUAL_TXNS"; then
  echo "ok   correction via is_active=false surfaced, not dropped or netted, no reversal field invented"
else
  echo "FAIL transaction 102 unexpected output"
  fail=$((fail + 1))
fi
if grep -q "201|" <<<"$ACTUAL_TXNS"; then
  echo "FAIL transaction 201 in unauthorized office 9 was not filtered"
  fail=$((fail + 1))
else
  echo "ok   transaction 201 in unauthorized office 9 absent"
fi

echo "=== Test 3: share_account_charges ==="
BODY_CHARGES="$(awk '/^SELECT/ {p=1} p {print}' "$SQL_CHARGES")"
ACTUAL_CHARGES="$(psql -X -A -t -F '|' -q -v ON_ERROR_STOP=1 -d "$URL" <<EOF
PREPARE charges_smoke(bigint[], int) AS
$SHADOW_CTES
$BODY_CHARGES;
EXECUTE charges_smoke('{2}'::bigint[], 200);
EOF
)"

echo "$ACTUAL_CHARGES"
if grep -q "^1001|10|1|2|1|Share Purchase Fee|USD|14|1|0|5.000000|5.000000|||0.000000|t|f|t$" <<<"$ACTUAL_CHARGES"; then
  echo "ok   charge 1001 present with master name/currency; NULL derived amounts stay NULL"
else
  echo "FAIL charge 1001 unexpected output"
  fail=$((fail + 1))
fi
if grep -q "2001|" <<<"$ACTUAL_CHARGES"; then
  echo "FAIL charge 2001 in unauthorized office 9 was not filtered"
  fail=$((fail + 1))
else
  echo "ok   charge 2001 in unauthorized office 9 absent"
fi

echo "=== Test 4: share_charge_payment_allocations ==="
BODY_ALLOC="$(awk '/^SELECT/ {p=1} p {print}' "$SQL_ALLOC")"
ACTUAL_ALLOC="$(psql -X -A -t -F '|' -q -v ON_ERROR_STOP=1 -d "$URL" <<EOF
PREPARE alloc_smoke(bigint[], int) AS
$SHADOW_CTES
$BODY_ALLOC;
EXECUTE alloc_smoke('{2}'::bigint[], 200);
EOF
)"

echo "$ACTUAL_ALLOC"
if grep -q "^1|1001|101|10|1|2|USD|5.00|14|2026-01-15|500|t$" <<<"$ACTUAL_ALLOC"; then
  echo "ok   allocation grain is 1 row; amount is pb.amount (5.00); share and charge txn IDs surfaced"
else
  echo "FAIL allocation 1 unexpected output"
  fail=$((fail + 1))
fi
if grep -q "^2|" <<<"$ACTUAL_ALLOC"; then
  echo "FAIL allocation 2 in unauthorized office 9 was not filtered"
  fail=$((fail + 1))
else
  echo "ok   allocation 2 in unauthorized office 9 absent"
fi

echo "=== Test 5: share_account_dividends ==="
BODY_DIVIDENDS="$(awk '/^SELECT/ {p=1} p {print}' "$SQL_DIVIDENDS")"
ACTUAL_DIVIDENDS="$(psql -X -A -t -F '|' -q -v ON_ERROR_STOP=1 -d "$URL" <<EOF
PREPARE dividends_smoke(bigint[], int) AS
$SHADOW_CTES
$BODY_DIVIDENDS;
EXECUTE dividends_smoke('{2}'::bigint[], 200);
EOF
)"

echo "$ACTUAL_DIVIDENDS"
if grep -q "^901|5001|10|1|2|150.000000|300|8888|2025-01-01|2025-12-31$" <<<"$ACTUAL_DIVIDENDS"; then
  echo "ok   dividend 901 surfaced with posted status (300) AND savings_transaction_id (8888) with payout period dates"
else
  echo "FAIL dividend 901 unexpected output"
  fail=$((fail + 1))
fi
if grep -q "^902|5001|10|1|2|75.000000|100||2025-01-01|2025-12-31$" <<<"$ACTUAL_DIVIDENDS"; then
  echo "ok   dividend 902 surfaced with initiated status (100) and NULL savings_transaction_id (not paid, both visible)"
else
  echo "FAIL dividend 902 unexpected output"
  fail=$((fail + 1))
fi
if grep -q "903|" <<<"$ACTUAL_DIVIDENDS"; then
  echo "FAIL dividend 903 in unauthorized office 9 was not filtered"
  fail=$((fail + 1))
else
  echo "ok   dividend 903 in unauthorized office 9 absent"
fi

if [ "$fail" -eq 0 ]; then
  echo "=== All share nonzero semantics smoke checks PASSED ==="
  exit 0
else
  echo "=== $fail checks FAILED ==="
  exit 1
fi
