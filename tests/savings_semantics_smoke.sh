#!/usr/bin/env bash
#
# FIN-127 nonzero semantics smoke for queries/savings/savings_*.sql.
#
# Deterministic and read-only: the production SQL text is executed UNCHANGED, but every
# Fineract table it names is shadowed by a CTE of the same name built from inline VALUES,
# so no Fineract table is read and no fixture row exists anywhere. The connection only
# hosts the statement. Cases:
#   * savings_account_lifecycle: office authorization governs (unauthorized office 9 filtered out);
#   * savings_account_lifecycle: dormant/block sub-status (synthetic 200/400 rows) with reason_for_block;
#   * savings_interest_position: total_interest_earned_derived != total_interest_posted_derived kept distinct;
#   * savings_account_holds: hold != completed withdrawal (type 20/21 + parent/release linkage, reversal excluded);
#   * savings_withheld_tax: withheld-tax nonzero semantics (synthetic withhold_tax account + tax group join);
#   * savings_withheld_tax: empty semantics when no account has withhold_tax = true;
#   * savings_linked_accounts: linkage-without-transfer (surfaces association row without transfer execution);
#   * savings_account_balance_detail: four balance measures stay distinct (balance, available, on-hold, overdraft);
#   * savings_transactions_by_type: per-type breakdown with reversal exclusion and currency per code.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
URL="${SMOKE_DATABASE_URL:-postgres://root:password@127.0.0.1:5432/fineract_default}"

SQL_LIFECYCLE="$ROOT/queries/savings/savings_account_lifecycle.sql"
SQL_INTEREST="$ROOT/queries/savings/savings_interest_position.sql"
SQL_HOLDS="$ROOT/queries/savings/savings_account_holds.sql"
SQL_TAX="$ROOT/queries/savings/savings_withheld_tax.sql"
SQL_LINKS="$ROOT/queries/savings/savings_linked_accounts.sql"
SQL_BALANCE="$ROOT/queries/savings/savings_account_balance_detail.sql"
SQL_TXN_TYPE="$ROOT/queries/savings/savings_transactions_by_type.sql"

SHADOW_CTES="WITH
m_office(id, name) AS (VALUES
  (1::bigint, 'Head Office'::varchar(100)),
  (2, 'Office 2'),
  (9, 'Office 9')),
m_client(id, office_id) AS (VALUES
  (1::bigint, 2::bigint),
  (2, 9)),
m_savings_product(id, name, currency_code) AS (VALUES
  (1::bigint, 'Regular Savings'::varchar(200), 'USD'::varchar(3))),
m_tax_group(id, name) AS (VALUES
  (1::bigint, 'Standard Withholding Tax'::varchar(50))),
m_savings_account(id, client_id, product_id, status_enum, sub_status_enum, reason_for_block, submittedon_date, approvedon_date, rejectedon_date, withdrawnon_date, activatedon_date, closedon_date, currency_code, nominal_annual_interest_rate, interest_compounding_period_enum, interest_posting_period_enum, interest_calculation_type_enum, interest_calculation_days_in_year_type_enum, total_interest_earned_derived, total_interest_posted_derived, interest_posted_till_date, last_interest_calculation_date, account_balance_derived, available_balance_derived, total_savings_amount_on_hold, on_hold_funds_derived, allow_overdraft, overdraft_limit, withhold_tax, tax_group_id, total_withhold_tax_derived) AS (VALUES
  (10::bigint, 1::bigint, 1::bigint, 300::smallint, 200::smallint, NULL::varchar, DATE '2026-01-10', DATE '2026-01-10', NULL::date, NULL::date, DATE '2026-01-10', NULL::date, 'USD'::varchar(3), 2.50::numeric(19,6), 1::smallint, 4::smallint, 1::smallint, 365::smallint, 50.00::numeric(19,6), 45.00::numeric(19,6), DATE '2026-03-31', DATE '2026-03-31', 1000.00::numeric(19,6), 900.00::numeric(19,6), 100.00::numeric(19,6), 100.00::numeric(19,6), true, 200.00::numeric(19,6), true, 1::bigint, 15.00::numeric(19,6)),
  (11, 1, 1, 300, 400, 'Suspicious Activity', DATE '2026-01-15', DATE '2026-01-15', NULL, NULL, DATE '2026-01-15', NULL, 'USD', 2.50, 1, 4, 1, 365, 0.00, 0.00, NULL, NULL, 500.00, 500.00, 0.00, 0.00, false, 0.00, false, NULL, 0.00),
  (20, 2, 1, 300, 0, NULL, DATE '2026-01-10', DATE '2026-01-10', NULL, NULL, DATE '2026-01-10', NULL, 'USD', 2.50, 1, 4, 1, 365, 10.00, 10.00, NULL, NULL, 300.00, 300.00, 0.00, 0.00, false, 0.00, false, NULL, 0.00)),
m_savings_account_transaction(id, savings_account_id, office_id, transaction_type_enum, transaction_date, amount, is_reversed, hold_type, hold_status, parent_hold_transaction_id, release_id_of_hold_amount, is_lien_transaction) AS (VALUES
  (101::bigint, 10::bigint, 2::bigint, 20::smallint, DATE '2026-02-15', 100.00::numeric(19,6), false, 2::bigint, 100::bigint, NULL::bigint, 102::bigint, false),
  (102, 10, 2, 21, DATE '2026-02-20', 100.00::numeric(19,6), false, NULL, NULL, 101, NULL, false),
  (103, 10, 2, 2, DATE '2026-02-10', 50.00::numeric(19,6), false, NULL, NULL, NULL, NULL, false),
  (104, 10, 2, 20, DATE '2026-02-12', 30.00::numeric(19,6), true, 1, 100, NULL, NULL, false),
  (105, 10, 2, 1, DATE '2026-02-05', 200.00::numeric(19,6), false, NULL, NULL, NULL, NULL, false),
  (106, 10, 2, 3, DATE '2026-02-28', 5.00::numeric(19,6), false, NULL, NULL, NULL, NULL, false),
  (107, 10, 2, 22, DATE '2026-02-25', 20.00::numeric(19,6), false, NULL, NULL, 101, NULL, false),
  (201, 20, 9, 20, DATE '2026-02-15', 50.00::numeric(19,6), false, 2, 100, NULL, NULL, false)),
m_portfolio_account_associations(id, savings_account_id, loan_account_id, linked_savings_account_id, linked_loan_account_id, association_type_enum, is_active) AS (VALUES
  (1::bigint, 10::bigint, NULL::bigint, 11::bigint, NULL::bigint, 1::smallint, true),
  (2, NULL, 50::bigint, 10::bigint, NULL, 1::smallint, true),
  (3, NULL, 60::bigint, 20::bigint, NULL, 1::smallint, true))"

fail=0

echo "=== Test 1: savings_account_lifecycle ==="
BODY_LIFECYCLE="$(awk '/^SELECT/ {p=1} p {print}' "$SQL_LIFECYCLE")"
ACTUAL_LIFECYCLE="$(psql -X -A -t -F '|' -q -v ON_ERROR_STOP=1 -d "$URL" <<EOF
PREPARE lifecycle_smoke(bigint[], date, date, int) AS
$SHADOW_CTES
$BODY_LIFECYCLE;
EXECUTE lifecycle_smoke('{2}'::bigint[], DATE '2026-01-01', DATE '2026-03-31', 200);
EOF
)"

echo "$ACTUAL_LIFECYCLE"
if grep -q "^10|1|2|1|Regular Savings|300|200||2026-01-10|2026-01-10|||2026-01-10||USD$" <<<"$ACTUAL_LIFECYCLE"; then
  echo "ok   account 10 has sub_status_enum 200 (DORMANT) mapped"
else
  echo "FAIL account 10 unexpected lifecycle output"
  fail=$((fail + 1))
fi

if grep -q "^11|1|2|1|Regular Savings|300|400|Suspicious Activity|2026-01-15|2026-01-15|||2026-01-15||USD$" <<<"$ACTUAL_LIFECYCLE"; then
  echo "ok   account 11 has sub_status_enum 400 (BLOCK) and reason_for_block surfaced"
else
  echo "FAIL account 11 unexpected block output"
  fail=$((fail + 1))
fi

if grep -q "20|" <<<"$ACTUAL_LIFECYCLE"; then
  echo "FAIL account 20 in unauthorized office 9 was not filtered"
  fail=$((fail + 1))
else
  echo "ok   unauthorized office 9 account 20 filtered"
fi

echo "=== Test 2: savings_interest_position ==="
BODY_INTEREST="$(awk '/^SELECT/ {p=1} p {print}' "$SQL_INTEREST")"
ACTUAL_INTEREST="$(psql -X -A -t -F '|' -q -v ON_ERROR_STOP=1 -d "$URL" <<EOF
PREPARE interest_smoke(bigint[], date, date, int) AS
$SHADOW_CTES
$BODY_INTEREST;
EXECUTE interest_smoke('{2}'::bigint[], DATE '2026-01-01', DATE '2026-03-31', 200);
EOF
)"

echo "$ACTUAL_INTEREST"
if grep -q "^10|1|2|1|Regular Savings|USD|2.500000|1|4|1|365|50.000000|45.000000|2026-03-31|2026-03-31|2026-01-10$" <<<"$ACTUAL_INTEREST"; then
  echo "ok   earned (50.00) != posted (45.00) kept strictly distinct"
else
  echo "FAIL account 10 unexpected interest output"
  fail=$((fail + 1))
fi

echo "=== Test 3: savings_account_holds ==="
BODY_HOLDS="$(awk '/^SELECT/ {p=1} p {print}' "$SQL_HOLDS")"
ACTUAL_HOLDS="$(psql -X -A -t -F '|' -q -v ON_ERROR_STOP=1 -d "$URL" <<EOF
PREPARE holds_smoke(bigint[], date, date, int) AS
$SHADOW_CTES
$BODY_HOLDS;
EXECUTE holds_smoke('{2}'::bigint[], DATE '2026-02-01', DATE '2026-02-28', 200);
EOF
)"

echo "$ACTUAL_HOLDS"
if grep -q "^101|10|1|2|2|20|2026-02-15|100.000000|USD|2|100||102|f$" <<<"$ACTUAL_HOLDS"; then
  echo "ok   hold txn 101 (type 20) with release linkage 102 surfaced"
else
  echo "FAIL hold transaction 101 missing or incorrect"
  fail=$((fail + 1))
fi

if grep -q "^102|10|1|2|2|21|2026-02-20|100.000000|USD|||101||f$" <<<"$ACTUAL_HOLDS"; then
  echo "ok   release txn 102 (type 21) with parent hold 101 surfaced"
else
  echo "FAIL release transaction 102 missing or incorrect"
  fail=$((fail + 1))
fi

if grep -q "103|" <<<"$ACTUAL_HOLDS"; then
  echo "FAIL completed withdrawal txn 103 incorrectly included in holds"
  fail=$((fail + 1))
else
  echo "ok   completed withdrawal txn 103 excluded from holds"
fi

if grep -q "104|" <<<"$ACTUAL_HOLDS"; then
  echo "FAIL reversed hold txn 104 was not excluded"
  fail=$((fail + 1))
else
  echo "ok   reversed hold txn 104 excluded"
fi

echo "=== Test 4: savings_withheld_tax ==="
BODY_TAX="$(awk '/^SELECT/ {p=1} p {print}' "$SQL_TAX")"
ACTUAL_TAX="$(psql -X -A -t -F '|' -q -v ON_ERROR_STOP=1 -d "$URL" <<EOF
PREPARE tax_smoke(bigint[], int) AS
$SHADOW_CTES
$BODY_TAX;
EXECUTE tax_smoke('{2}'::bigint[], 200);
EOF
)"

echo "$ACTUAL_TAX"
if grep -q "^10|1|2|1|t|1|Standard Withholding Tax|15.000000|USD$" <<<"$ACTUAL_TAX"; then
  echo "ok   withheld tax positive row surfaced with tax group name and derived amount"
else
  echo "FAIL withheld tax unexpected output"
  fail=$((fail + 1))
fi

if grep -q "11|" <<<"$ACTUAL_TAX"; then
  echo "FAIL account 11 without withhold_tax was not filtered"
  fail=$((fail + 1))
else
  echo "ok   account 11 with withhold_tax=false excluded"
fi

EMPTY_TAX="$(psql -X -A -t -F '|' -q -v ON_ERROR_STOP=1 -d "$URL" <<EOF
PREPARE empty_tax_smoke(bigint[], int) AS
$SHADOW_CTES
$BODY_TAX;
EXECUTE empty_tax_smoke('{99}'::bigint[], 200);
EOF
)"

if [ -z "$EMPTY_TAX" ]; then
  echo "ok   empty scope returns genuine Empty (zero rows)"
else
  echo "FAIL empty scope returned rows: $EMPTY_TAX"
  fail=$((fail + 1))
fi

echo "=== Test 5: savings_linked_accounts ==="
BODY_LINKS="$(awk '/^SELECT/ {p=1} p {print}' "$SQL_LINKS")"
ACTUAL_LINKS="$(psql -X -A -t -F '|' -q -v ON_ERROR_STOP=1 -d "$URL" <<EOF
PREPARE links_smoke(bigint[], int) AS
$SHADOW_CTES
$BODY_LINKS;
EXECUTE links_smoke('{2}'::bigint[], 200);
EOF
)"

echo "$ACTUAL_LINKS"
if grep -q "^1|10||11||1|t|1|2$" <<<"$ACTUAL_LINKS"; then
  echo "ok   association 1 surfaced (linkage without transfer evidence)"
else
  echo "FAIL association 1 missing or incorrect"
  fail=$((fail + 1))
fi

if grep -q "^2||50|10||1|t|1|2$" <<<"$ACTUAL_LINKS"; then
  echo "ok   association 2 (loan 50 to savings 10) surfaced"
else
  echo "FAIL association 2 missing or incorrect"
  fail=$((fail + 1))
fi

if grep -q "3|" <<<"$ACTUAL_LINKS"; then
  echo "FAIL association 3 in office 9 was not filtered"
  fail=$((fail + 1))
else
  echo "ok   association 3 in unauthorized office 9 filtered"
fi

echo "=== Test 6: savings_account_balance_detail ==="
BODY_BALANCE="$(awk '/^SELECT/ {p=1} p {print}' "$SQL_BALANCE")"
ACTUAL_BALANCE="$(psql -X -A -t -F '|' -q -v ON_ERROR_STOP=1 -d "$URL" <<EOF
PREPARE balance_smoke(bigint[], date, date, int) AS
$SHADOW_CTES
$BODY_BALANCE;
EXECUTE balance_smoke('{2}'::bigint[], DATE '2026-01-01', DATE '2026-03-31', 200);
EOF
)"

echo "$ACTUAL_BALANCE"
if grep -q "^10|1|2|1|Regular Savings|USD|1000.000000|900.000000|100.000000|100.000000|t|200.000000|2026-01-10$" <<<"$ACTUAL_BALANCE"; then
  echo "ok   four balance measures distinct: balance=1000, available=900, on_hold=100, funds_on_hold=100, overdraft_limit=200"
else
  echo "FAIL account 10 four balance measures unexpected output"
  fail=$((fail + 1))
fi

echo "=== Test 7: savings_transactions_by_type ==="
BODY_TXN_TYPE="$(awk '/^SELECT/ {p=1} p {print}' "$SQL_TXN_TYPE")"
ACTUAL_TXN_TYPE="$(psql -X -A -t -F '|' -q -v ON_ERROR_STOP=1 -d "$URL" <<EOF
PREPARE txn_type_smoke(bigint[], date, date, int) AS
$SHADOW_CTES
$BODY_TXN_TYPE;
EXECUTE txn_type_smoke('{2}'::bigint[], DATE '2026-02-01', DATE '2026-02-28', 200);
EOF
)"

echo "$ACTUAL_TXN_TYPE"
if grep -q "^USD|1|1|200.000000$" <<<"$ACTUAL_TXN_TYPE" && \
   grep -q "^USD|2|1|50.000000$" <<<"$ACTUAL_TXN_TYPE" && \
   grep -q "^USD|3|1|5.000000$" <<<"$ACTUAL_TXN_TYPE" && \
   grep -q "^USD|20|1|100.000000$" <<<"$ACTUAL_TXN_TYPE" && \
   grep -q "^USD|21|1|100.000000$" <<<"$ACTUAL_TXN_TYPE" && \
   grep -q "^USD|22|1|20.000000$" <<<"$ACTUAL_TXN_TYPE"; then
  echo "ok   per-type breakdown: types 1, 2, 3, 20, 21, 22 correctly counted and summed"
  echo "ok   reversed hold txn 104 (type 20, amount 30) was excluded from type 20 total"
else
  echo "FAIL transactions by type unexpected output"
  fail=$((fail + 1))
fi

if [ $fail -eq 0 ]; then
  echo "=== ALL 7 SAVINGS SEMANTICS SMOKE TESTS PASSED ==="
  exit 0
else
  echo "=== $fail SMOKE TESTS FAILED ==="
  exit 1
fi
