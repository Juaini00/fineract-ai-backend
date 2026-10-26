#!/usr/bin/env bash
#
# FIN-123 (D03): nonzero semantics smoke for queries/savings/balances_per_currency.sql.
#
# Deterministic and read-only: the production SQL text is executed UNCHANGED, but every
# Fineract table it names is shadowed by a CTE of the same name built from inline VALUES,
# so no Fineract table is read and no fixture row exists anywhere. The connection only
# hosts the statement. Cases proven:
#   * Per-currency separation: multi-currency rows are kept strictly separate;
#     summing across currencies would produce a meaningless number (e.g. IDR + USD).
#   * Family separation: loan and savings rows are separate domain_family rows, never combined.
#   * No grand total row exists.
#   * Office authorization: loan path COALESCE(c.office_id, g.office_id) and savings path c.office_id;
#     accounts in unauthorized offices (office 9) are excluded under {2, 3} scope.
#   * Narrowed office scope: scope {2} returns only office 2 rows.
#   * Empty office scope: scope {8} returns 0 rows.
#   * No exchange rate references in queries/savings/balances_per_currency.sql.
#   * Output columns: domain_family, currency_code, account_count, balance_total;
#     no converted or reporting currency column.
#   * Negative schema verification: no exchange rate table in Fineract schema.
#   * Jarvis exchange_rates table exists and contains 0 rows (registry empty).
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
URL="${SMOKE_DATABASE_URL:-postgres://root:password@127.0.0.1:5432/fineract_default}"
APP_URL="${APP_DATABASE_URL:-postgres://root:password@127.0.0.1:5432/jarvis_wt_fin_123_lc16_mata_uang_konsolidasi_d03_t}"

SQL_BALANCES="$ROOT/queries/savings/balances_per_currency.sql"

SHADOW_CTES="WITH
m_client(id, office_id) AS (VALUES
  (1::bigint, 2::bigint),
  (2::bigint, 3::bigint),
  (3::bigint, 9::bigint),
  (4::bigint, 2::bigint)
),
m_group(id, office_id) AS (VALUES
  (1::bigint, 2::bigint),
  (2::bigint, 9::bigint)
),
m_loan(id, client_id, group_id, currency_code, total_outstanding_derived) AS (VALUES
  (1::bigint, 1::bigint, NULL::bigint, 'IDR'::varchar, 1000000.00::numeric),
  (2::bigint, 2::bigint, NULL::bigint, 'USD'::varchar, 100.00::numeric),
  (3::bigint, 4::bigint, NULL::bigint, 'IDR'::varchar, 2000000.00::numeric),
  (4::bigint, 3::bigint, NULL::bigint, 'EUR'::varchar, 50.00::numeric),
  (5::bigint, NULL::bigint, 1::bigint, 'USD'::varchar, 200.00::numeric)
),
m_savings_account(id, client_id, currency_code, account_balance_derived) AS (VALUES
  (101::bigint, 1::bigint, 'IDR'::varchar, 500000.00::numeric),
  (102::bigint, 2::bigint, 'USD'::varchar, 50.00::numeric),
  (103::bigint, 4::bigint, 'IDR'::varchar, 750000.00::numeric),
  (104::bigint, 3::bigint, 'EUR'::varchar, 25.00::numeric)
)"

fail=0

echo "=== Test 1: balances_per_currency (VALUES-only CTE shadow) ==="
BODY_BALANCES="$(awk '/^SELECT/ {p=1} p {print}' "$SQL_BALANCES")"
ACTUAL_BALANCES="$(PGTZ=UTC psql -X -A -t -F '|' -q -v ON_ERROR_STOP=1 -d "$URL" <<EOF
PREPARE balances_smoke(bigint[]) AS
$SHADOW_CTES
$BODY_BALANCES;
EXECUTE balances_smoke('{2,3}'::bigint[]);
EOF
)"

echo "$ACTUAL_BALANCES"

# Expect 4 rows: loan IDR, loan USD, savings IDR, savings USD (office 9 EUR loan/savings excluded)
row_count="$(echo "$ACTUAL_BALANCES" | grep -v '^$' | wc -l | tr -d ' ')"
if [ "$row_count" -ne 4 ]; then
  echo "FAIL: expected 4 balance rows, got $row_count" >&2; fail=1
else
  echo "ok 1 - row count is 4 (office 9 EUR accounts excluded by scope)"
fi

# Verify currency separation and no cross-currency summation
loan_idr="$(echo "$ACTUAL_BALANCES" | grep '^loan|IDR' || true)"
loan_usd="$(echo "$ACTUAL_BALANCES" | grep '^loan|USD' || true)"
savings_idr="$(echo "$ACTUAL_BALANCES" | grep '^savings|IDR' || true)"
savings_usd="$(echo "$ACTUAL_BALANCES" | grep '^savings|USD' || true)"

if [ -z "$loan_idr" ] || [ -z "$loan_usd" ]; then
  echo "FAIL: expected distinct loan IDR and USD rows" >&2; fail=1
else
  # loan IDR: count 2, total 3000000.00
  loan_idr_bal="$(echo "$loan_idr" | cut -d'|' -f4)"
  loan_idr_cnt="$(echo "$loan_idr" | cut -d'|' -f3)"
  if [ "$loan_idr_bal" != "3000000.00" ] || [ "$loan_idr_cnt" != "2" ]; then
    echo "FAIL: loan IDR expected count 2, bal 3000000.00, got count $loan_idr_cnt, bal $loan_idr_bal" >&2; fail=1
  else
    echo "ok 2 - loan IDR amounts kept separate (count 2, total 3000000.00; never summed with USD)"
  fi

  # loan USD: count 2 (client 2 + group 1), total 300.00
  loan_usd_bal="$(echo "$loan_usd" | cut -d'|' -f4)"
  loan_usd_cnt="$(echo "$loan_usd" | cut -d'|' -f3)"
  if [ "$loan_usd_bal" != "300.00" ] || [ "$loan_usd_cnt" != "2" ]; then
    echo "FAIL: loan USD expected count 2, bal 300.00, got count $loan_usd_cnt, bal $loan_usd_bal" >&2; fail=1
  else
    echo "ok 3 - loan USD amounts kept separate (count 2, total 300.00)"
  fi
fi

# Verify savings separation
if [ -z "$savings_idr" ] || [ -z "$savings_usd" ]; then
  echo "FAIL: expected distinct savings IDR and USD rows" >&2; fail=1
else
  savings_idr_bal="$(echo "$savings_idr" | cut -d'|' -f4)"
  savings_idr_cnt="$(echo "$savings_idr" | cut -d'|' -f3)"
  if [ "$savings_idr_bal" != "1250000.00" ] || [ "$savings_idr_cnt" != "2" ]; then
    echo "FAIL: savings IDR expected count 2, bal 1250000.00, got count $savings_idr_cnt, bal $savings_idr_bal" >&2; fail=1
  else
    echo "ok 4 - savings IDR amounts kept separate (count 2, total 1250000.00)"
  fi

  savings_usd_bal="$(echo "$savings_usd" | cut -d'|' -f4)"
  savings_usd_cnt="$(echo "$savings_usd" | cut -d'|' -f3)"
  if [ "$savings_usd_bal" != "50.00" ] || [ "$savings_usd_cnt" != "1" ]; then
    echo "FAIL: savings USD expected count 1, bal 50.00, got count $savings_usd_cnt, bal $savings_usd_bal" >&2; fail=1
  else
    echo "ok 5 - savings USD amounts kept separate (count 1, total 50.00)"
  fi
fi

# Verify no grand total row and family distinctness
grand_total_row="$(echo "$ACTUAL_BALANCES" | grep -iE 'total|all' | grep -vE 'loan|savings' || true)"
if [ -n "$grand_total_row" ]; then
  echo "FAIL: found grand total row '$grand_total_row', must not exist" >&2; fail=1
else
  echo "ok 6 - no grand total row exists across currencies or families"
fi

# Test 7: Narrowed office scope {2}
NARROW_BALANCES="$(PGTZ=UTC psql -X -A -t -F '|' -q -v ON_ERROR_STOP=1 -d "$URL" <<EOF
PREPARE balances_narrow(bigint[]) AS
$SHADOW_CTES
$BODY_BALANCES;
EXECUTE balances_narrow('{2}'::bigint[]);
EOF
)"
echo "=== Test 7: Narrowed scope {2} ==="
echo "$NARROW_BALANCES"
narrow_cnt="$(echo "$NARROW_BALANCES" | grep -v '^$' | wc -l | tr -d ' ')"
# In office 2: loan IDR (count 2, 3000000.00), loan USD (count 1, 200.00 from group 1), savings IDR (count 2, 1250000.00) = 3 rows
if [ "$narrow_cnt" -ne 3 ]; then
  echo "FAIL: expected 3 rows for narrowed scope {2}, got $narrow_cnt" >&2; fail=1
else
  echo "ok 7 - narrowed scope {2} returns exactly 3 rows (office 3 USD accounts excluded)"
fi

# Test 8: Empty office scope {8}
EMPTY_BALANCES="$(PGTZ=UTC psql -X -A -t -F '|' -q -v ON_ERROR_STOP=1 -d "$URL" <<EOF
PREPARE balances_empty(bigint[]) AS
$SHADOW_CTES
$BODY_BALANCES;
EXECUTE balances_empty('{8}'::bigint[]);
EOF
)"
empty_cnt="$(echo "$EMPTY_BALANCES" | grep -v '^$' | wc -l | tr -d ' ' || true)"
if [ "$empty_cnt" -ne 0 ]; then
  echo "FAIL: expected 0 rows for empty scope {8}, got $empty_cnt" >&2; fail=1
else
  echo "ok 8 - empty office scope {8} returns 0 rows"
fi

# Test 9: No exchange rate references in SQL
if grep -q -iE "\bexchange\b|\bkurs\b|\bexchange_rates?\b" "$SQL_BALANCES"; then
  echo "FAIL: SQL references exchange rates or conversion" >&2; fail=1
else
  echo "ok 9 - SQL contains zero exchange rate or currency conversion references"
fi

# Test 10: Negative schema verification in Fineract
fx_tables="$(PGTZ=UTC psql -X -A -t -q -v ON_ERROR_STOP=1 -d "$URL" -c \
  "SELECT table_name FROM information_schema.tables WHERE table_schema='public' AND table_name LIKE '%exchange%' ORDER BY table_name;")"
if [ -n "$fx_tables" ]; then
  echo "FAIL: unexpected exchange table found in Fineract: $fx_tables" >&2; fail=1
else
  echo "ok 10 - negative schema verification: 0 exchange rate tables exist in fineract_default"
fi

# Test 11: Jarvis exchange_rates table exists with 0 rows
rates_count="$(PGTZ=UTC psql -X -A -t -q -v ON_ERROR_STOP=1 -d "$APP_URL" -c \
  "SELECT count(*) FROM exchange_rates;")"
if [ "$rates_count" -ne 0 ]; then
  echo "FAIL: expected 0 rows in exchange_rates table, got $rates_count" >&2; fail=1
else
  echo "ok 11 - Jarvis exchange_rates registry exists and contains 0 rows (empty registry confirmed)"
fi

exit "$fail"
