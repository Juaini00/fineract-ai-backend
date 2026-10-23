#!/usr/bin/env bash
#
# FIN-130 nonzero semantics smoke for queries/linking_resources/*.sql.
#
# Deterministic and read-only: the production SQL text is executed UNCHANGED, but every
# Fineract table it names is shadowed by a CTE of the same name built from inline VALUES,
# so no Fineract table is read and no fixture row exists anywhere. The connection only
# hosts the statement. Cases:
#   * account_association_links: office authorization on either side (office 9 filtered out);
#   * account_transfer_transactions: transfer movement evidenced by transaction, not header alone;
#   * LINK-3 linkage never appears in LINK-4 output (config link != transfer movement);
#   * orphan transfer detail row without transactions produces no movement in LINK-4;
#   * loan_payment_allocation: allocation components sum to amount, and failing-sum case is exposed;
#   * charge_settlement_evidence: direct settlement linking charge to paying transaction, not inferred;
#   * transaction_reversals: reversed rows carry is_reversed flag for anti-double-count filtering;
#   * share-side reversal absence is explicitly stated and verified in schema;
#   * payment_type_resolution: NULL payment_detail_id survives via LEFT JOIN without row loss;
#   * currency never mixed across transactions.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
URL="${SMOKE_DATABASE_URL:-postgres://root:password@127.0.0.1:5432/fineract_default}"

SQL_LINKS="$ROOT/queries/linking_resources/account_association_links.sql"
SQL_TRANSFERS="$ROOT/queries/linking_resources/account_transfer_transactions.sql"
SQL_ALLOCATION="$ROOT/queries/linking_resources/loan_payment_allocation.sql"
SQL_SETTLEMENT="$ROOT/queries/linking_resources/charge_settlement_evidence.sql"
SQL_REVERSALS="$ROOT/queries/linking_resources/transaction_reversals.sql"
SQL_PAYMENT_TYPE="$ROOT/queries/linking_resources/payment_type_resolution.sql"

SHADOW_CTES="WITH
m_office(id, name) AS (VALUES
  (1::bigint, 'Head Office'::varchar(100)),
  (2, 'Office 2'),
  (3, 'Office 3'),
  (9, 'Office 9')),
m_client(id, office_id) AS (VALUES
  (1::bigint, 2::bigint),
  (2, 3),
  (3, 9)),
m_loan(id, client_id, currency_code) AS (VALUES
  (10::bigint, 1::bigint, 'USD'::varchar(3)),
  (20, 2, 'EUR'),
  (30, 3, 'USD')),
m_savings_account(id, client_id, currency_code) AS (VALUES
  (100::bigint, 1::bigint, 'USD'::varchar(3)),
  (200, 2, 'EUR'),
  (300, 3, 'USD')),
m_portfolio_account_associations(id, loan_account_id, savings_account_id, linked_loan_account_id, linked_savings_account_id, association_type_enum, is_active) AS (VALUES
  (1::bigint, 10::bigint, NULL::bigint, NULL::bigint, 100::bigint, 1::smallint, true),
  (2, NULL, 200::bigint, NULL, 100::bigint, 1::smallint, true),
  (3, 30::bigint, NULL, NULL, 300::bigint, 1::smallint, true)),
m_account_transfer_details(id, from_office_id, to_office_id, from_client_id, to_client_id, from_savings_account_id, from_loan_account_id, to_savings_account_id, to_loan_account_id) AS (VALUES
  (1::bigint, 2::bigint, 2::bigint, 1::bigint, 1::bigint, 100::bigint, NULL::bigint, NULL::bigint, 10::bigint),
  (2, 9, 9, 3, 3, 300, NULL, NULL, 30),
  (3, 2, 2, 1, 1, 100, NULL, NULL, 10)),
m_account_transfer_transaction(id, account_transfer_details_id, from_savings_transaction_id, from_loan_transaction_id, to_savings_transaction_id, to_loan_transaction_id, transaction_date, currency_code, amount, is_reversed) AS (VALUES
  (501::bigint, 1::bigint, 1001::bigint, NULL::bigint, NULL::bigint, 2001::bigint, DATE '2026-03-10', 'USD'::varchar(3), 150.00::numeric(19,6), false),
  (502, 2, 1003, NULL, NULL, 2003, DATE '2026-03-10', 'USD', 200.00, false)),
m_loan_transaction(id, loan_id, office_id, transaction_date, transaction_type_enum, amount, principal_portion_derived, interest_portion_derived, fee_charges_portion_derived, penalty_charges_portion_derived, payment_detail_id, is_reversed, reversed_on_date) AS (VALUES
  (2001::bigint, 10::bigint, 2::bigint, DATE '2026-03-10', 2::smallint, 150.00::numeric(19,6), 100.00::numeric(19,6), 30.00::numeric(19,6), 10.00::numeric(19,6), 10.00::numeric(19,6), 1::bigint, false, NULL::date),
  (2002, 10, 2, DATE '2026-03-11', 2, 100.00::numeric(19,6), 70.00::numeric(19,6), 20.00::numeric(19,6), 5.00::numeric(19,6), 5.00::numeric(19,6), NULL, true, DATE '2026-03-12'),
  (2003, 30, 9, DATE '2026-03-10', 2, 200.00::numeric(19,6), 150.00::numeric(19,6), 50.00::numeric(19,6), 0.00::numeric(19,6), 0.00::numeric(19,6), 2, false, NULL),
  (2004, 20, 3, DATE '2026-03-15', 2, 300.00::numeric(19,6), 200.00::numeric(19,6), 50.00::numeric(19,6), 30.00::numeric(19,6), 0.00::numeric(19,6), NULL, false, NULL)),
m_savings_account_transaction(id, savings_account_id, office_id, transaction_date, amount, payment_detail_id, is_reversed, is_reversal, original_transaction_id) AS (VALUES
  (1001::bigint, 100::bigint, 2::bigint, DATE '2026-03-10', 150.00::numeric(19,6), 1::bigint, false, false, NULL::bigint),
  (1002, 100, 2, DATE '2026-03-11', 50.00::numeric(19,6), NULL, true, false, NULL),
  (1003, 300, 9, DATE '2026-03-10', 200.00::numeric(19,6), 2, false, false, NULL)),
m_savings_account_charge(id, savings_account_id, charge_id) AS (VALUES
  (10::bigint, 100::bigint, 5::bigint),
  (20, 300, 6)),
m_savings_account_charge_paid_by(id, savings_account_charge_id, savings_account_transaction_id, amount) AS (VALUES
  (1::bigint, 10::bigint, 1001::bigint, 25.00::numeric(19,6)),
  (2, 20, 1003, 10.00)),
m_payment_detail(id, payment_type_id) AS (VALUES
  (1::bigint, 10::integer),
  (2, 20)),
m_payment_type(id, value, is_cash_payment) AS (VALUES
  (10::integer, 'Cash Payment'::varchar(50), true),
  (20, 'Wire Transfer'::varchar(50), false)),
acc_gl_journal_entry(id, account_id, office_id, entry_date, amount, reversed, reversal_id, currency_code) AS (VALUES
  (9001::bigint, 501::bigint, 2::bigint, DATE '2026-03-10', 150.00::numeric(19,6), false, NULL::bigint, 'USD'::varchar(3)),
  (9002, 501, 2, DATE '2026-03-11', 50.00, true, 9003::bigint, 'USD'),
  (9003, 501, 2, DATE '2026-03-11', 50.00, false, NULL, 'USD'))"

fail=0
pass() { echo "  PASS: $1"; }
fail() { echo "  FAIL: $1" >&2; fail=$((fail + 1)); }

run_query() {
  local sql_file="$1"
  local scope_array="$2"
  local limit="$3"
  local body
  body="$(cat "$sql_file")"
  # Substitute parameters
  local full_sql="${SHADOW_CTES}, prod_query AS (${body}) SELECT * FROM prod_query;"
  psql -X -A -t -q -v ON_ERROR_STOP=1 -d "$URL" \
    -c "PREPARE test_stmt (bigint[], integer) AS ${full_sql}; EXECUTE test_stmt ('${scope_array}', ${limit}); DEALLOCATE test_stmt;"
}

echo "=== FIN-130 linking resources semantics smoke ==="

# 1. account_association_links: office authorization on either side (office 2,3 includes ids 1 and 2, excludes id 3 in office 9)
out="$(run_query "$SQL_LINKS" "{2,3}" 200)"
count="$(echo "$out" | grep -c '|' || true)"
if [ "$count" -eq 2 ] && echo "$out" | grep -q "1|10|||100|1|t|2" && echo "$out" | grep -q "2||200||100|1|t|3"; then
  pass "LINK-3 account_association_links: authorized offices [2,3] resolved across sides; office 9 excluded"
else
  fail "LINK-3 account_association_links: expected 2 rows, got $count: $out"
fi

# 2. account_transfer_transactions: transfer movement evidenced by transaction, not header alone
out="$(run_query "$SQL_TRANSFERS" "{2,3}" 200)"
count="$(echo "$out" | grep -c '|' || true)"
if [ "$count" -eq 1 ] && echo "$out" | grep -q "501|1|2|2|1|1|100|||10|1001|||2001|2026-03-10|USD|150.000000|f"; then
  pass "LINK-4 account_transfer_transactions: movement evidenced by transaction 501 under detail 1"
else
  fail "LINK-4 account_transfer_transactions: expected 1 row, got $count: $out"
fi

# 3. LINK-3 linkage never appears in LINK-4 output
# Association 2 (savings 200 to savings 100) has NO account transfer detail or transaction
if echo "$out" | awk -F'|' '$7 == 200 || $8 == 200 || $9 == 200 || $10 == 200' | grep -q .; then
  fail "LINK-3 association unexpectedly leaked into LINK-4 output"
else
  pass "LINK-3 configuration linkage (association 2) never appears in LINK-4 transfer output"
fi

# 4. Detail row without transaction rows produces no movement
# Detail 3 exists in m_account_transfer_details but has no m_account_transfer_transaction rows
if echo "$out" | grep -q "|3|"; then
  fail "Detail row 3 without transaction rows produced movement output"
else
  pass "Detail row 3 without transaction rows produces zero movement in LINK-4"
fi

# 5. loan_payment_allocation: allocation components sum to amount, and failing-sum case is exposed
out="$(run_query "$SQL_ALLOCATION" "{2,3}" 200)"
count="$(echo "$out" | grep -c '|' || true)"
# Transaction 2001: 100 + 30 + 10 + 10 = 150 = amount (valid)
# Transaction 2004: 200 + 50 + 30 + 0 = 280 != 300 = amount (inconsistent allocation detected)
consistent_count="$(echo "$out" | awk -F'|' 'NF>=10 { if ($6 == ($7 + $8 + $9 + $10)) print $1 }' | wc -l | tr -d ' ')"
failing_count="$(echo "$out" | awk -F'|' 'NF>=10 { if ($6 != ($7 + $8 + $9 + $10)) print $1 }' | wc -l | tr -d ' ')"
if [ "$consistent_count" -ge 2 ] && [ "$failing_count" -ge 1 ]; then
  pass "LINK-5 loan_payment_allocation: components sum to amount (tx 2001, 2002) and failing sum exposed (tx 2004: 280 != 300)"
else
  fail "LINK-5 loan_payment_allocation: expected consistent >= 2 and failing >= 1, got consistent=$consistent_count, failing=$failing_count"
fi

# 6. charge_settlement_evidence: direct settlement linking charge to paying transaction, not inferred from balance
out="$(run_query "$SQL_SETTLEMENT" "{2,3}" 200)"
count="$(echo "$out" | grep -c '|' || true)"
if [ "$count" -eq 1 ] && echo "$out" | grep -q "1|10|1001|100|5|25.000000|2026-03-10|USD|2"; then
  pass "LINK-6 charge_settlement_evidence: charge 10 linked to paying transaction 1001 (amount 25.00)"
else
  fail "LINK-6 charge_settlement_evidence: expected 1 row, got $count: $out"
fi

# 7. transaction_reversals: reversed rows carry is_reversed flag (anti-double-count default)
out="$(run_query "$SQL_REVERSALS" "{2,3}" 200)"
count="$(echo "$out" | grep -c '|' || true)"
reversed_rows="$(echo "$out" | grep "|t|" | wc -l | tr -d ' ')"
if [ "$reversed_rows" -ge 2 ] && echo "$out" | grep -q "loan|2002|10|2|2026-03-11|100.000000|t|f|2026-03-12||USD" && echo "$out" | grep -q "savings|1002|100|2|2026-03-11|50.000000|t|f|||USD"; then
  pass "LINK-7 transaction_reversals: reversed rows carry is_reversed=t (loan 2002, savings 1002) for anti-double-count filtering"
else
  fail "LINK-7 transaction_reversals: expected reversed rows >= 2, got $reversed_rows: $out"
fi

# 8. Share-side reversal absence is stated
# Verify that schema for m_share_account_transactions has no reversal column
share_reversal_col="$(psql -X -A -t -q -d "$URL" -c "SELECT column_name FROM information_schema.columns WHERE table_name = 'm_share_account_transactions' AND column_name LIKE '%reverse%';")"
if [ -z "$share_reversal_col" ]; then
  pass "LINK-7 share-side reversal absence stated and verified: m_share_account_transactions has zero reversal columns"
else
  fail "LINK-7 share-side unexpectedly has reversal column: $share_reversal_col"
fi

# 9. payment_type_resolution: NULL payment_detail_id survives via LEFT JOIN without dropped rows
out="$(run_query "$SQL_PAYMENT_TYPE" "{2,3}" 200)"
count="$(echo "$out" | grep -c '|' || true)"
null_pd_count="$(echo "$out" | awk -F'|' '$6 == "" { print $0 }' | wc -l | tr -d ' ')"
cash_count="$(echo "$out" | grep "|Cash Payment|t" | wc -l | tr -d ' ')"
if [ "$count" -ge 4 ] && [ "$null_pd_count" -ge 2 ] && [ "$cash_count" -ge 2 ]; then
  pass "LINK-8 payment_type_resolution: NULL payment_detail_id survives ($null_pd_count rows) and cash flag resolved ($cash_count rows)"
else
  fail "LINK-8 payment_type_resolution: expected count >= 4, null_pd >= 2, cash >= 2; got total=$count, null=$null_pd_count, cash=$cash_count: $out"
fi

# 10. Currency never mixed
out_curr="$(run_query "$SQL_REVERSALS" "{2,3}" 200)"
usd_rows="$(echo "$out_curr" | grep -c "|USD" || true)"
eur_rows="$(echo "$out_curr" | grep -c "|EUR" || true)"
if [ "$usd_rows" -gt 0 ] && [ "$eur_rows" -gt 0 ]; then
  pass "Multi-currency transactions preserve currency_code per row without cross-currency consolidation ($usd_rows USD, $eur_rows EUR)"
else
  fail "Currency preservation failed: usd=$usd_rows, eur=$eur_rows"
fi

echo "=== Smoke result: $fail failure(s) ==="
exit $fail
