#!/usr/bin/env bash
#
# FIN-115 anti-fanout smoke for queries/teller_cashier/cashier_recorded_cash_position.sql.
#
# Deterministic and read-only: the production SQL text is executed UNCHANGED, but every
# Fineract table it names is shadowed by a CTE of the same name built from inline VALUES,
# so no Fineract table is read and no fixture row exists anywhere. The connection only
# hosts the statement. Cases:
#   * staff 10 holds two OVERLAPPING assignments (cashiers 1 and 2, from 2026-03-01):
#     a savings event on 2026-03-15 matches both -> ambiguous, added to neither amount,
#     counted for both groups, positions withheld (NULL), not a fake zero;
#   * a June event of staff 10 also matches cashier 6 whose teller is in an
#     UNAUTHORIZED office: still ambiguous (counted over all assignments), and nothing of
#     cashier 6 is returned;
#   * staff 11 holds two NON-overlapping assignments (cashier 3 to 2026-02-28, cashier 4
#     from 2026-03-01): each event matches exactly one cashier and is admitted;
#   * cashier 3 nets to exactly 0 (alloc 50 - settle 80 + savings in 30) and is returned as 0;
#   * a journal allocation with NULL currency is its own group, not merged with USD.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
URL="${SMOKE_DATABASE_URL:-postgres://root:password@127.0.0.1:5432/jarvis}"
SQL="$ROOT/queries/teller_cashier/cashier_recorded_cash_position.sql"

PREFIX="WITH
m_tellers(id, office_id) AS (VALUES (1::bigint, 2::bigint), (2, 2), (3, 9)),
m_cashiers(id, staff_id, teller_id, start_date, end_date) AS (VALUES
  (1::bigint, 10::bigint, 1::bigint, DATE '2026-01-01', DATE '2026-12-31'),
  (2, 10, 2, DATE '2026-03-01', DATE '2026-12-31'),
  (3, 11, 1, DATE '2026-01-01', DATE '2026-02-28'),
  (4, 11, 1, DATE '2026-03-01', DATE '2026-12-31'),
  (6, 10, 3, DATE '2026-06-01', DATE '2026-12-31')),
m_appuser(id, staff_id) AS (VALUES (1::bigint, 10::bigint), (2, 11)),
m_savings_account(id, currency_code) AS (VALUES (1::bigint, 'USD'::text)),
m_payment_type(id, is_cash_payment) AS (VALUES (1::bigint, true)),
m_payment_detail(id, payment_type_id) AS (VALUES (1::bigint, 1::bigint)),
m_account_transfer_transaction(from_savings_transaction_id, to_savings_transaction_id, from_loan_transaction_id, to_loan_transaction_id)
  AS (SELECT NULL::bigint, NULL::bigint, NULL::bigint, NULL::bigint WHERE false),
m_loan_transaction(id, loan_id, office_id, payment_detail_id, is_reversed, transaction_type_enum, transaction_date, amount, created_by)
  AS (SELECT NULL::bigint, NULL::bigint, NULL::bigint, NULL::bigint, NULL::boolean, NULL::int, NULL::date, NULL::numeric, NULL::bigint WHERE false),
m_loan(id, currency_code) AS (SELECT NULL::bigint, NULL::text WHERE false),
m_client_transaction(id, office_id, currency_code, payment_detail_id, is_reversed, transaction_date, transaction_type_enum, amount, created_by)
  AS (SELECT NULL::bigint, NULL::bigint, NULL::text, NULL::bigint, NULL::boolean, NULL::date, NULL::int, NULL::numeric, NULL::bigint WHERE false),
m_cashier_transactions(cashier_id, txn_type, txn_date, txn_amount, currency_code) AS (VALUES
  (1::bigint, 101, DATE '2026-01-05', 1000::numeric, 'USD'::text),
  (3, 101, DATE '2026-01-10', 50, 'USD'),
  (3, 102, DATE '2026-02-20', 80, 'USD'),
  (4, 101, DATE '2026-03-02', 10, NULL)),
m_savings_account_transaction(id, savings_account_id, office_id, payment_detail_id, transaction_type_enum, is_reversed, transaction_date, amount, created_by) AS (VALUES
  (1::bigint, 1::bigint, 2::bigint, 1::bigint, 1, false, DATE '2026-02-10', 100::numeric, 1::bigint),
  (2, 1, 2, 1, 1, false, DATE '2026-03-15', 50, 1),
  (3, 1, 2, 1, 1, false, DATE '2026-02-28', 30, 2),
  (4, 1, 2, 1, 1, false, DATE '2026-03-01', 20, 2),
  (5, 1, 2, 1, 1, false, DATE '2026-06-10', 5, 1))"

# The production statement starts with its own WITH; chain it after the shadow CTEs.
BODY="$(awk '!done && /^WITH journal AS \(/ {sub(/^WITH /, ", "); done=1} {print}' "$SQL")"
ACTUAL="$(psql -X -A -t -F '|' -q -v ON_ERROR_STOP=1 -d "$URL" <<EOF
PREPARE position_smoke(bigint[], date, date, int) AS
$PREFIX
$BODY;
EXECUTE position_smoke('{2}'::bigint[], DATE '2026-03-01', DATE '2026-12-31', 200);
EOF
)"
# Columns used below: $1 cashier, $3 currency, $16 position, $17 ambiguous events, $18 status.
echo "$ACTUAL" | cut -d'|' -f1-3,16-18
fail=0
check() { # $1 = cashier|currency prefix, $2 = expected position|ambiguous|status
  local line
  line="$(printf '%s\n' "$ACTUAL" | awk -F'|' -v k="$1" '($1"|"($3==""?"NULL":$3))==k {print $16"|"$17"|"$18}')"
  if [ "$line" != "$2" ]; then echo "FAIL $1: got '$line' expected '$2'" >&2; fail=1; else echo "ok   $1 -> $2"; fi
}
check '1|USD' '|2|withheld_ambiguous_attribution'
check '2|USD' '|2|withheld_ambiguous_attribution'
check '3|USD' '0|0|complete'
check '4|USD' '20|0|complete'
check '4|NULL' '10|0|complete'
if printf '%s\n' "$ACTUAL" | awk -F'|' '$1==6 {found=1} END {exit !found}'; then
  echo "FAIL cashier 6 (unauthorized office) leaked" >&2; fail=1
else echo "ok   cashier 6 absent"; fi
# every cashier/currency appears exactly once
if [ "$(printf '%s\n' "$ACTUAL" | awk -F'|' '{print $1"|"$3}' | sort | uniq -d | wc -l)" -ne 0 ]; then echo "FAIL duplicate group" >&2; fail=1; fi
exit $fail
