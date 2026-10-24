#!/usr/bin/env bash
#
# FIN-112 nonzero semantics smoke for queries/accounting/*.sql.
#
# Deterministic and read-only: the production SQL text is executed UNCHANGED, but every
# Fineract table it names is shadowed by a CTE of the same name built from inline VALUES,
# so no Fineract table is read and no fixture row exists anywhere. The connection only
# hosts the statement. Cases:
#   * gl_account_balance: exclusion of reversed original & paired reversal entry from sums;
#   * gl_account_balance: count verification (entry_count, reversed_entry_count, reversal_entry_count, manual_entry_count);
#   * gl_account_balance: debit/credit mapping from raw type_enum 2/1;
#   * gl_account_balance: multi-currency rows never consolidated into single sum;
#   * gl_account_balance: office 9 filtered out under {2,3} scope;
#   * gl_journal_for_account: reversal flags and linked transaction IDs surfaced;
#   * gl_journal_for_account: office 9 excluded;
#   * gl_journal_for_transaction: lookup by transaction_id with office scoping.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
URL="${SMOKE_DATABASE_URL:-postgres://root:password@127.0.0.1:5432/fineract_default}"

SQL_BALANCE="$ROOT/queries/accounting/gl_account_balance.sql"
SQL_JOURNAL_ACCOUNT="$ROOT/queries/accounting/gl_journal_for_account.sql"
SQL_JOURNAL_TX="$ROOT/queries/accounting/gl_journal_for_transaction.sql"

SHADOW_CTES="WITH
acc_gl_account(id, gl_code, name, classification_enum) AS (VALUES
  (1::bigint, '10101'::varchar(45), 'Petty Cash'::varchar(45), 1::smallint),
  (2, '20101', 'Savings Deposits', 2)),
acc_gl_journal_entry(id, account_id, office_id, entry_date, type_enum, amount, reversed, reversal_id, currency_code, transaction_id, manual_entry, loan_transaction_id, savings_transaction_id, client_transaction_id, share_transaction_id) AS (VALUES
  (1::bigint, 1::bigint, 2::bigint, DATE '2026-03-01', 2::smallint, 100.00::numeric(19,6), false, NULL::bigint, 'USD'::varchar(3), 'TX-101'::varchar(50), true, NULL::bigint, NULL::bigint, NULL::bigint, NULL::bigint),
  (2, 1, 2, DATE '2026-03-02', 1, 40.00, false, NULL, 'USD', 'TX-102', false, 501::bigint, NULL, NULL, NULL),
  (3, 1, 2, DATE '2026-03-03', 2, 50.00, true, 4::bigint, 'USD', 'TX-103', false, NULL, 1001::bigint, NULL, NULL),
  (4, 1, 2, DATE '2026-03-03', 1, 50.00, false, NULL, 'USD', 'TX-104', false, NULL, NULL, NULL, NULL),
  (5, 1, 2, DATE '2026-03-04', 2, 200.00, false, NULL, 'EUR', 'TX-105', false, NULL, NULL, NULL, NULL),
  (6, 1, 3, DATE '2026-03-05', 2, 300.00, false, NULL, 'USD', 'TX-106', false, NULL, NULL, NULL, NULL),
  (7, 1, 9, DATE '2026-03-05', 2, 999.00, false, NULL, 'USD', 'TX-107', false, NULL, NULL, NULL, NULL),
  (8, 2, 2, DATE '2026-03-06', 1, 500.00, false, NULL, 'USD', 'TX-108', false, NULL, NULL, NULL, NULL))"

fail=0
pass() { echo "  PASS: $1"; }
fail() { echo "  FAIL: $1" >&2; fail=$((fail + 1)); }

run_balance() {
  local scope_array="$1"
  local gl_code="$2"
  local name="$3"
  local from_date="$4"
  local to_date="$5"
  local body
  body="$(cat "$SQL_BALANCE")"
  local full_sql="${SHADOW_CTES}, prod_query AS (${body}) SELECT * FROM prod_query;"
  psql -X -A -t -q -v ON_ERROR_STOP=1 -d "$URL" \
    -c "PREPARE stmt (bigint[], varchar, varchar, date, date) AS ${full_sql}; EXECUTE stmt ('${scope_array}', ${gl_code}, ${name}, ${from_date}, ${to_date}); DEALLOCATE stmt;"
}

run_journal_account() {
  local scope_array="$1"
  local gl_code="$2"
  local name="$3"
  local from_date="$4"
  local to_date="$5"
  local limit="$6"
  local body
  body="$(cat "$SQL_JOURNAL_ACCOUNT")"
  local full_sql="${SHADOW_CTES}, prod_query AS (${body}) SELECT * FROM prod_query;"
  psql -X -A -t -q -v ON_ERROR_STOP=1 -d "$URL" \
    -c "PREPARE stmt (bigint[], varchar, varchar, date, date, integer) AS ${full_sql}; EXECUTE stmt ('${scope_array}', ${gl_code}, ${name}, ${from_date}, ${to_date}, ${limit}); DEALLOCATE stmt;"
}

run_journal_tx() {
  local scope_array="$1"
  local tx_id="$2"
  local limit="$3"
  local body
  body="$(cat "$SQL_JOURNAL_TX")"
  local full_sql="${SHADOW_CTES}, prod_query AS (${body}) SELECT * FROM prod_query;"
  psql -X -A -t -q -v ON_ERROR_STOP=1 -d "$URL" \
    -c "PREPARE stmt (bigint[], varchar, integer) AS ${full_sql}; EXECUTE stmt ('${scope_array}', ${tx_id}, ${limit}); DEALLOCATE stmt;"
}

echo "=== FIN-112 accounting semantics smoke ==="

# 1. gl_account_balance: exclusion of reversed original (id 3) and paired reversal (id 4)
# Office 2 USD: normal entries: id 1 (Debit 100), id 2 (Credit 40). Balance = 60.00.
# Without exclusion, debit would be 150 and credit 90.
out="$(run_balance "{2,3}" "'10101'" "NULL" "'2026-03-01'" "'2026-03-31'")"
row_usd_off2="$(echo "$out" | grep "^2|USD|" || true)"
if [ -n "$row_usd_off2" ] && echo "$row_usd_off2" | grep -q "2|USD|100.000000|40.000000|60.000000|4|1|1|1"; then
  pass "ACC-1 gl_account_balance: reversal pair excluded from sums (debit 100, credit 40, balance 60) and counted (total 4, reversed 1, reversal 1, manual 1)"
else
  fail "ACC-1 gl_account_balance: unexpected row for Office 2 USD: $row_usd_off2"
fi

# 2. gl_account_balance: multi-currency never consolidated into single sum
row_eur_off2="$(echo "$out" | grep "^2|EUR|" || true)"
if [ -n "$row_eur_off2" ] && echo "$row_eur_off2" | grep -q "2|EUR|200.000000|0.000000|200.000000|1|0|0|0"; then
  pass "ACC-2 gl_account_balance: multi-currency preserved per office without cross-currency consolidation (EUR debit 200, balance 200)"
else
  fail "ACC-2 gl_account_balance: unexpected row for Office 2 EUR: $row_eur_off2"
fi

# 3. gl_account_balance: office 9 entries excluded under {2,3} scope
if echo "$out" | grep -q "^9|"; then
  fail "ACC-3 gl_account_balance: Office 9 entry leaked into authorized scope {2,3}"
else
  pass "ACC-3 gl_account_balance: Office 9 strictly excluded under authorized scope {2,3}"
fi

# 4. gl_account_balance: debit/credit mapping verified (type_enum 2 -> debit, 1 -> credit)
row_usd_off3="$(echo "$out" | grep "^3|USD|" || true)"
if [ -n "$row_usd_off3" ] && echo "$row_usd_off3" | grep -q "3|USD|300.000000|0.000000|300.000000|1|0|0|0"; then
  pass "ACC-4 gl_account_balance: debit/credit correctly mapped from type_enum 2/1 for Office 3 (debit 300, credit 0)"
else
  fail "ACC-4 gl_account_balance: unexpected row for Office 3 USD: $row_usd_off3"
fi

# 5. gl_journal_for_account: reversal flags and links surfaced, office 9 excluded
out_journal="$(run_journal_account "{2,3}" "'10101'" "NULL" "NULL" "NULL" 100)"
count_journal="$(echo "$out_journal" | grep -c '|' || true)"
# We expect 6 rows (ids 1, 2, 3, 4, 5, 6; id 7 is office 9, id 8 is account 2)
if [ "$count_journal" -eq 6 ]; then
  pass "ACC-5 gl_journal_for_account: exactly 6 authorized rows returned (office 9 and other accounts excluded)"
else
  fail "ACC-5 gl_journal_for_account: expected 6 rows, got $count_journal: $out_journal"
fi

# Check reversal flags on original reversed entry (id 3) and reversal entry (id 4)
row_id3="$(echo "$out_journal" | grep "^3|" || true)"
row_id4="$(echo "$out_journal" | grep "^4|" || true)"
if echo "$row_id3" | grep -q "|t|4|f|" && echo "$row_id4" | grep -q "|f||t|"; then
  pass "ACC-6 gl_journal_for_account: reversal flags accurately surfaced (id 3: reversed=t, reversal_id=4, is_reversal=f; id 4: reversed=f, reversal_id=null, is_reversal=t)"
else
  fail "ACC-6 gl_journal_for_account: reversal flags mismatch: id3=$row_id3, id4=$row_id4"
fi

# Check transaction links surfaced (id 2 has loan_transaction_id 501; id 3 has savings_transaction_id 1001)
row_id2="$(echo "$out_journal" | grep "^2|" || true)"
if echo "$row_id2" | grep -q "|501|||" && echo "$row_id3" | grep -q "||1001||"; then
  pass "ACC-7 gl_journal_for_account: business transaction links surfaced (loan_tx 501, savings_tx 1001)"
else
  fail "ACC-7 gl_journal_for_account: transaction links mismatch: id2=$row_id2, id3=$row_id3"
fi

# 6. gl_journal_for_transaction: lookup by transaction_id with office scoping
out_tx="$(run_journal_tx "{2,3}" "'TX-103'" 10)"
if echo "$out_tx" | grep -q "^3|1|10101|Petty Cash|1|2|USD|TX-103|"; then
  pass "ACC-8 gl_journal_for_transaction: successfully resolved journal entry 3 for transaction TX-103"
else
  fail "ACC-8 gl_journal_for_transaction: lookup failed for TX-103: $out_tx"
fi

out_tx9="$(run_journal_tx "{2,3}" "'TX-107'" 10)"
if [ -z "$out_tx9" ]; then
  pass "ACC-9 gl_journal_for_transaction: transaction TX-107 in office 9 blocked under scope {2,3}"
else
  fail "ACC-9 gl_journal_for_transaction: transaction TX-107 in office 9 leaked under scope {2,3}: $out_tx9"
fi

echo "=== Smoke result: $fail failure(s) ==="
exit $fail
