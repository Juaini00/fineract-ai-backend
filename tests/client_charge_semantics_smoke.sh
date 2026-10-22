#!/usr/bin/env bash
#
# FIN-113 nonzero semantics smoke for queries/client/client_charge_assessments.sql
# and queries/client/client_charge_payment_allocations.sql.
#
# Deterministic and read-only: the production SQL text is executed UNCHANGED, but every
# Fineract table it names is shadowed by a CTE of the same name built from inline VALUES,
# so no Fineract table is read and no fixture row exists anywhere. The connection only
# hosts the statement. Cases:
#   * assessment query: NULL derived amounts stay NULL (never coerced to 0);
#   * assessment query: NULL due date is admitted and returned as NULL;
#   * assessment query: unauthorized client office is filtered out;
#   * allocation query: an allocation is counted once even when its transaction
#     has multiple allocations (no fan-out);
#   * allocation query: transaction reversal is surfaced explicitly, not netted;
#   * allocation query: per-currency fields stay separate (charge currency vs txn currency);
#   * allocation query: client office governs authorization; transaction office is evidence.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
URL="${SMOKE_DATABASE_URL:-postgres://root:password@127.0.0.1:5432/jarvis}"
SQL_ASSESS="$ROOT/queries/client/client_charge_assessments.sql"
SQL_ALLOC="$ROOT/queries/client/client_charge_payment_allocations.sql"

SHADOW_CTES="WITH
m_office(id, name) AS (VALUES
  (1::bigint, 'Head Office'::varchar(100)),
  (2, 'Office 2'),
  (9, 'Office 9')),
m_client(id, office_id) AS (VALUES
  (1::bigint, 2::bigint),
  (2, 9)),
m_charge(id, name, currency_code) AS (VALUES
  (1::bigint, 'Registration Fee'::varchar(100), 'USD'::varchar(3)),
  (2, 'Maintenance Fee', 'EUR')),
m_client_charge(id, client_id, charge_id, is_penalty, charge_time_enum, charge_due_date, charge_calculation_enum, amount, amount_paid_derived, amount_waived_derived, amount_writtenoff_derived, amount_outstanding_derived, is_paid_derived, waived, is_active, inactivated_on_date) AS (VALUES
  (10::bigint, 1::bigint, 1::bigint, false, 1::smallint, DATE '2026-02-15', 1::smallint, 100::numeric(19,6), NULL::numeric(19,6), NULL::numeric(19,6), NULL::numeric(19,6), 100::numeric(19,6), false, false, true, NULL::date),
  (11, 1, 1, false, 1, NULL, 1, 50, 50, 0, 0, 0, true, false, true, NULL),
  (12, 2, 2, false, 1, DATE '2026-02-15', 1, 30, NULL, NULL, NULL, 30, false, false, true, NULL)),
m_client_transaction(id, client_id, office_id, currency_code, is_reversed, transaction_date, transaction_type_enum, amount) AS (VALUES
  (100::bigint, 1::bigint, 1::bigint, 'USD'::varchar(3), false, DATE '2026-02-10', 1::smallint, 200::numeric(19,6)),
  (101, 1, 2, 'EUR', true, DATE '2026-02-20', 1, 75),
  (102, 2, 2, 'USD', false, DATE '2026-02-20', 1, 30)),
m_client_charge_paid_by(id, client_transaction_id, client_charge_id, amount) AS (VALUES
  (1::bigint, 100::bigint, 10::bigint, 60::numeric(19,6)),
  (2, 100, 11, 40),
  (3, 101, 10, 75),
  (4, 102, 12, 30))"

echo "=== Test 1: client_charge_assessments ==="
BODY_ASSESS="$(awk '/^SELECT/ {p=1} p {print}' "$SQL_ASSESS")"
ACTUAL_ASSESS="$(psql -X -A -t -F '|' -q -v ON_ERROR_STOP=1 -d "$URL" <<EOF
PREPARE assess_smoke(bigint[], date, date, int) AS
$SHADOW_CTES
$BODY_ASSESS;
EXECUTE assess_smoke('{2}'::bigint[], DATE '2026-01-01', DATE '2026-03-31', 200);
EOF
)"

echo "$ACTUAL_ASSESS"
fail=0

# Line 1 (charge 10): client_charge_id=10, client_id=1, office_id=2, charge_id=1, currency=USD, amount=100.000000, paid=NULL, waived=NULL, writtenoff=NULL, outstanding=100.000000, due=2026-02-15
line1="$(echo "$ACTUAL_ASSESS" | grep '^10|' || true)"
if [ -z "$line1" ]; then
  echo "FAIL: charge 10 missing" >&2; fail=1
else
  # Column 7 = amount_paid_derived, column 8 = amount_waived_derived, column 9 = amount_writtenoff_derived
  c7="$(echo "$line1" | cut -d'|' -f7)"
  c8="$(echo "$line1" | cut -d'|' -f8)"
  c9="$(echo "$line1" | cut -d'|' -f9)"
  if [ -n "$c7" ] || [ -n "$c8" ] || [ -n "$c9" ]; then
    echo "FAIL: derived amounts not NULL: '$c7', '$c8', '$c9'" >&2; fail=1
  else
    echo "ok   charge 10 derived amounts stay NULL (never coerced to 0)"
  fi
fi

# Line 2 (charge 11): NULL due date admitted
line2="$(echo "$ACTUAL_ASSESS" | grep '^11|' || true)"
if [ -z "$line2" ]; then
  echo "FAIL: charge 11 (NULL due date) missing" >&2; fail=1
else
  c11="$(echo "$line2" | cut -d'|' -f11)"
  if [ -n "$c11" ]; then
    echo "FAIL: charge 11 due date not NULL: '$c11'" >&2; fail=1
  else
    echo "ok   charge 11 NULL due date admitted and returned as NULL"
  fi
fi

# Charge 12 (office 9) must be absent
if echo "$ACTUAL_ASSESS" | grep -q '^12|'; then
  echo "FAIL: charge 12 (unauthorized office 9) leaked" >&2; fail=1
else
  echo "ok   charge 12 (unauthorized office 9) absent"
fi

echo "=== Test 2: client_charge_payment_allocations ==="
BODY_ALLOC="$(awk '/^SELECT/ {p=1} p {print}' "$SQL_ALLOC")"
ACTUAL_ALLOC="$(psql -X -A -t -F '|' -q -v ON_ERROR_STOP=1 -d "$URL" <<EOF
PREPARE alloc_smoke(bigint[], date, date, int) AS
$SHADOW_CTES
$BODY_ALLOC;
EXECUTE alloc_smoke('{2}'::bigint[], DATE '2026-01-01', DATE '2026-03-31', 200);
EOF
)"

echo "$ACTUAL_ALLOC"

# Row count should be 3 (allocations 1, 2, 3; allocation 4 belongs to client in office 9)
count="$(echo "$ACTUAL_ALLOC" | grep -v '^$' | wc -l | tr -d ' ')"
if [ "$count" -ne 3 ]; then
  echo "FAIL: expected 3 allocations, got $count" >&2; fail=1
else
  echo "ok   allocation count is 3 (unauthorized client office 9 excluded)"
fi

# Allocation grain no fan-out: transaction 100 backs allocations 1 (60) and 2 (40)
alloc1="$(echo "$ACTUAL_ALLOC" | grep '^1|' || true)"
alloc2="$(echo "$ACTUAL_ALLOC" | grep '^2|' || true)"
amt1="$(echo "$alloc1" | cut -d'|' -f4 | sed 's/\.0*$//')"
amt2="$(echo "$alloc2" | cut -d'|' -f4 | sed 's/\.0*$//')"
if [ "$amt1" != "60" ] || [ "$amt2" != "40" ]; then
  echo "FAIL: allocation amounts expected 60 and 40, got '$amt1' and '$amt2'" >&2; fail=1
else
  echo "ok   allocation grain: amounts are 60 and 40 (no fan-out to txn amount 200)"
fi

# Reversal surfaced and currencies distinct: allocation 3
alloc3="$(echo "$ACTUAL_ALLOC" | grep '^3|' || true)"
rev3="$(echo "$alloc3" | cut -d'|' -f9)"
ch_cur3="$(echo "$alloc3" | cut -d'|' -f5)"
tx_cur3="$(echo "$alloc3" | cut -d'|' -f6)"
if [ "$rev3" != "t" ]; then
  echo "FAIL: allocation 3 is_reversed expected 't', got '$rev3'" >&2; fail=1
else
  echo "ok   allocation 3 reversal surfaced (is_reversed = true)"
fi
if [ "$ch_cur3" != "USD" ] || [ "$tx_cur3" != "EUR" ]; then
  echo "FAIL: currencies expected USD and EUR, got '$ch_cur3' and '$tx_cur3'" >&2; fail=1
else
  echo "ok   per-currency separated: charge currency USD, transaction currency EUR"
fi

# Allocation 4 (client in office 9, transaction in office 2) must be excluded
if echo "$ACTUAL_ALLOC" | grep -q '^4|'; then
  echo "FAIL: allocation 4 leaked (client in office 9 authorized via transaction office 2)" >&2; fail=1
else
  echo "ok   allocation 4 absent: client office governs authorization, not transaction office"
fi

if [ "$fail" -eq 0 ]; then
  echo "=== All client charge nonzero semantics smoke checks PASSED ==="
fi
exit $fail
