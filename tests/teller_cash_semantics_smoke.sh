#!/usr/bin/env bash
#
# FIN-115 semantics smoke for the UNCHANGED production SQL under queries/teller_cashier/.
#
# Synthetic proof, not source acceptance: every Fineract table the statement names is
# shadowed by a CTE of the same name built from inline VALUES, so no Fineract table is read
# or written and no fixture row exists anywhere; the connection only hosts the statement.
# It exercises branches the local Fineract data leaves EMPTY:
#   position: loan repayment-type (2, 8, 17) = in and disbursement (1) = out; the loan
#     end_date + 1 boundary; client PAY_CHARGE (1) = in; journal 103/104; an unrecognized
#     journal type (counted, never summed); and, for loan/client/savings alike, exclusion of
#     non-cash payment types, NULL payment details, reversed rows, account transfers,
#     waivers (type 10) and out-of-scope transaction offices;
#   assignments: a part-day assignment (full_day = false, empty end_time -> NULL) and two
#     overlapping assignments of one staff member are both returned, one row per assignment;
#   inter-office: nonzero transfers; a transfer with an out-of-scope counterpart is hidden.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
URL="${SMOKE_DATABASE_URL:-postgres://root:password@127.0.0.1:5432/jarvis}"
Q="$ROOT/queries/teller_cashier"
fail=0

# run <sql file> <shadow CTE list> <EXECUTE arguments>  -> pipe-separated rows
run() {
  local body
  # The production text is used unchanged: a statement starting with WITH is chained after
  # the shadow CTEs; a plain SELECT simply follows them.
  body="$(awk '!done && /^WITH / {sub(/^WITH /, ", "); done=1} {print}' "$1")"
  if grep -q '^WITH ' "$1"; then :; else body="$(cat "$1")"; fi
  psql -X -A -t -F '|' -q -v ON_ERROR_STOP=1 -d "$URL" <<EOF
PREPARE s(bigint[], date, date, int) AS
WITH $2
$body;
EXECUTE s($3);
EOF
}
expect() { # $1 label, $2 actual, $3 expected
  if [ "$2" != "$3" ]; then echo "FAIL $1" >&2; echo " got:      $2" >&2; echo " expected: $3" >&2; fail=1; else echo "ok   $1"; fi
}

# ---- position: loan / client / journal 103,104 / unrecognized ---------------------------
POS_SHADOW="m_tellers(id, office_id) AS (VALUES (1::bigint, 2::bigint)),
m_cashiers(id, staff_id, teller_id, start_date, end_date) AS (VALUES
  (1::bigint, 10::bigint, 1::bigint, DATE '2026-01-01', DATE '2026-12-31')),
m_appuser(id, staff_id) AS (VALUES (1::bigint, 10::bigint)),
m_payment_type(id, is_cash_payment) AS (VALUES (1::bigint, true), (2, false)),
m_payment_detail(id, payment_type_id) AS (VALUES (1::bigint, 1::bigint), (2, 2)),
m_account_transfer_transaction(from_savings_transaction_id, to_savings_transaction_id, from_loan_transaction_id, to_loan_transaction_id)
  AS (VALUES (NULL::bigint, NULL::bigint, 10::bigint, NULL::bigint)),
m_savings_account(id, currency_code) AS (VALUES (1::bigint, 'USD'::text)),
m_savings_account_transaction(id, savings_account_id, office_id, payment_detail_id, transaction_type_enum, is_reversed, transaction_date, amount, created_by)
  AS (VALUES (1::bigint, 1::bigint, 2::bigint, 1::bigint, 1, false, DATE '2027-01-01', 77::numeric, 1::bigint)),
m_loan(id, currency_code) AS (VALUES (1::bigint, 'USD'::text)),
m_loan_transaction(id, loan_id, office_id, payment_detail_id, is_reversed, transaction_type_enum, transaction_date, amount, created_by) AS (VALUES
  (1::bigint, 1::bigint, 2::bigint, 1::bigint, false, 2, DATE '2026-02-10', 100::numeric, 1::bigint),
  (2, 1, 2, 1, false, 1,  DATE '2026-02-11', 40, 1),
  (3, 1, 2, 2, false, 2,  DATE '2026-02-12', 500, 1),
  (4, 1, 2, 1, true,  2,  DATE '2026-02-13', 600, 1),
  (5, 1, 2, NULL, false, 2, DATE '2026-02-14', 700, 1),
  (6, 1, 9, 1, false, 2,  DATE '2026-02-15', 800, 1),
  (7, 1, 2, 1, false, 8,  DATE '2026-02-16', 10, 1),
  (8, 1, 2, 1, false, 10, DATE '2026-02-18', 5000, 1),
  (9, 1, 2, 1, false, 2,  DATE '2027-01-01', 1000, 1),
  (10, 1, 2, 1, false, 2, DATE '2026-02-17', 50, 1)),
m_client_transaction(id, office_id, currency_code, payment_detail_id, is_reversed, transaction_date, transaction_type_enum, amount, created_by) AS (VALUES
  (1::bigint, 2::bigint, 'USD'::text, 1::bigint, false, DATE '2026-02-20', 1, 25::numeric, 1::bigint),
  (2, 2, 'USD', 1, false, DATE '2026-02-21', 2, 300, 1),
  (3, 2, 'USD', 1, true,  DATE '2026-02-22', 1, 400, 1),
  (4, 2, 'USD', 2, false, DATE '2026-02-23', 1, 500, 1)),
m_cashier_transactions(cashier_id, txn_type, txn_date, txn_amount, currency_code) AS (VALUES
  (1::bigint, 101, DATE '2026-01-05', 1000::numeric, 'USD'::text),
  (1, 103, DATE '2026-02-01', 200, 'USD'),
  (1, 104, DATE '2026-02-02', 70, 'USD'),
  (1, 102, DATE '2026-03-01', 300, 'USD'),
  (1, 105, DATE '2026-02-03', 999, 'USD'))"

POS="$(run "$Q/cashier_recorded_cash_position.sql" "$POS_SHADOW" "'{2}'::bigint[], DATE '2026-02-12', DATE '2027-12-31', 200")"
# cashier|office|currency|window|as_of|before|alloc|settled|jin|jout|sin|sout|lin|lout|cin|position|ambiguous|status|unrecognized
expect "position: loan in/out, client in, journal 103/104, unrecognized type" \
  "$(echo "$POS" | cut -d'|' -f1-19)" \
  "1|2|USD|2026-02-12|2027-12-31|1190|1000|300|200|70|0|0|1110|40|25|1925|0|complete|1"
expect "position: transaction office outside scope returns no group" \
  "$(run "$Q/cashier_recorded_cash_position.sql" "$POS_SHADOW" "'{9}'::bigint[], DATE '2026-02-12', DATE '2027-12-31', 200")" ""

# ---- assignments: part-day and overlapping assignments of one staff member ---------------
ASG_SHADOW="m_tellers(id, office_id, valid_from, valid_to, state) AS (VALUES
  (1::bigint, 2::bigint, DATE '2026-01-01', DATE '2026-12-31', 300), (2, 9, DATE '2026-01-01', NULL::date, 300)),
m_cashiers(id, teller_id, staff_id, start_date, end_date, full_day, start_time, end_time) AS (VALUES
  (1::bigint, 1::bigint, 10::bigint, DATE '2026-02-01', DATE '2026-02-28', true,  '', ''),
  (2, 1, 10, DATE '2026-02-15', DATE '2026-03-31', false, '09:00', ''),
  (3, 2, 11, DATE '2026-02-01', DATE '2026-02-28', true, '', ''),
  (4, NULL, 12, DATE '2026-02-01', DATE '2026-02-28', true, '', ''))"
expect "assignments: part-day and overlapping rows, one per assignment, other office hidden" \
  "$(run "$Q/cashier_assignments.sql" "$ASG_SHADOW" "'{2}'::bigint[], DATE '2026-02-01', DATE '2026-02-28', 200" | cut -d'|' -f1,2,3,4,7,8,9)" \
  "$(printf '1|1|2|10|t||\n2|1|2|10|f|09:00|')"

# ---- inter-office transfers: nonzero rows, counterpart outside scope hidden --------------
OT_SHADOW="m_office_transaction(id, from_office_id, to_office_id, transaction_date, transaction_amount, currency_code) AS (VALUES
  (1::bigint, 2::bigint, 3::bigint, DATE '2026-03-01', 500::numeric, 'USD'::text),
  (2, 2, 9, DATE '2026-03-02', 600, 'USD'),
  (3, NULL, 2, DATE '2026-04-01', 700, NULL),
  (4, NULL, NULL, DATE '2026-04-02', 800, 'USD'),
  (5, 2, 3, DATE '2025-12-31', 900, 'USD'))"
expect "inter-office: nonzero rows, out-of-scope counterpart and no-office rows hidden" \
  "$(run "$Q/inter_office_cash_transfers.sql" "$OT_SHADOW" "'{2,3}'::bigint[], DATE '2026-01-01', DATE '2026-06-30', 200" | tr '\n' ';')" \
  "3||2|2026-04-01|700|;1|2|3|2026-03-01|500|USD;"
exit $fail
