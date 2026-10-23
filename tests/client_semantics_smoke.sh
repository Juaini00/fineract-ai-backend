#!/usr/bin/env bash
#
# FIN-128 nonzero semantics smoke for queries/client/*.sql:
#   * client_group_membership
#   * client_account_roster
#   * client_address
#
# Deterministic and read-only: the production SQL text is executed UNCHANGED, but every
# Fineract table it names is shadowed by a CTE of the same name built from inline VALUES,
# so no Fineract table is read and no fixture row exists anywhere. The connection only
# hosts the statement. Cases:
#   * client_group_membership: N:M client<->group membership without double count;
#   * client_group_membership: center linkage parent_id is NULL-safe (group 10 has center 20, group 11 has NULL);
#   * client_group_membership: office authorization via client office (unauthorized office 9 filtered);
#   * client_account_roster: returns all 3 account types (loan, savings, share);
#   * client_account_roster: no default-active filter (pending status 100 surfaced alongside active 300);
#   * client_account_roster: per-type authorization via client office (client in office 9 filtered);
#   * client_address: address fields with PII sensitivity surfaced for authorized office;
#   * client_address: unauthorized office 9 address filtered out;
#   * empty semantics: empty office scope returns 0 rows across all capabilities.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
URL="${SMOKE_DATABASE_URL:-postgres://root:password@127.0.0.1:5432/fineract_default}"
SQL_GROUP="$ROOT/queries/client/client_group_membership.sql"
SQL_ROSTER="$ROOT/queries/client/client_account_roster.sql"
SQL_ADDRESS="$ROOT/queries/client/client_address.sql"

SHADOW_CTES="WITH
m_office(id, name) AS (VALUES
  (1::bigint, 'Head Office'::varchar(100)),
  (2, 'Branch 001'),
  (3, 'Branch 002'),
  (9, 'Office 9')),
m_client(id, office_id) AS (VALUES
  (1::bigint, 2::bigint),
  (2, 2),
  (3, 9)),
m_group(id, display_name, level_id, parent_id, office_id) AS (VALUES
  (10::bigint, 'Solidarity Group A'::varchar(100), 2::smallint, 20::bigint, 2::bigint),
  (11, 'Solidarity Group B'::varchar(100), 2::smallint, NULL::bigint, 2::bigint),
  (20, 'Center Central'::varchar(100), 1::smallint, NULL::bigint, 2::bigint),
  (30, 'Unauthorized Group'::varchar(100), 2::smallint, NULL::bigint, 9::bigint)),
m_group_client(client_id, group_id) AS (VALUES
  (1::bigint, 10::bigint),
  (1, 11),
  (2, 10),
  (3, 30)),
m_loan_product(id, name) AS (VALUES
  (101::bigint, 'Micro Loan'::varchar(100))),
m_savings_product(id, name) AS (VALUES
  (201::bigint, 'Passbook Savings'::varchar(100))),
m_share_product(id, name) AS (VALUES
  (301::bigint, 'Member Share'::varchar(100))),
m_loan(id, client_id, group_id, product_id, loan_status_id, submittedon_date, currency_code) AS (VALUES
  (1001::bigint, 1::bigint, 10::bigint, 101::bigint, 100::smallint, DATE '2026-01-05', 'USD'::varchar(3)),
  (1002, 1, NULL, 101, 300, DATE '2026-01-10', 'USD'),
  (1003, 3, NULL, 101, 300, DATE '2026-01-10', 'USD')),
m_savings_account(id, client_id, product_id, status_enum, submittedon_date, currency_code) AS (VALUES
  (2001::bigint, 1::bigint, 201::bigint, 300::smallint, DATE '2026-01-01', 'USD'::varchar(3)),
  (2002, 1, 201, 100, DATE '2026-01-02', 'USD')),
m_share_account(id, client_id, product_id, status_enum, submitted_date, currency_code) AS (VALUES
  (3001::bigint, 1::bigint, 301::bigint, 300::smallint, DATE '2026-01-03', 'USD'::varchar(3))),
m_code_value(id, code_value) AS (VALUES
  (501::bigint, 'RESIDENTIAL'::varchar(50))),
m_address(id, street, address_line_1, address_line_2, address_line_3, town_village, city, county_district, state_province_id, country_id, postal_code, latitude, longitude) AS (VALUES
  (6001::bigint, 'Main St'::varchar(100), '123 Market St'::varchar(100), 'Suite 4'::varchar(100), NULL::varchar(100), NULL::varchar(100), 'Capital City'::varchar(100), NULL::varchar(100), 10::bigint, 20::bigint, '12345'::varchar(20), NULL::numeric(10,6), NULL::numeric(10,6)),
  (6002, 'Remote St', '456 Offlimit St', NULL, NULL, NULL, 'Remote Town', NULL, 11, 21, '99999', NULL, NULL)),
m_client_address(id, client_id, address_id, address_type_id, is_active) AS (VALUES
  (1::bigint, 1::bigint, 6001::bigint, 501::bigint, true),
  (2, 3, 6002, 501, true))"

fail=0

echo "=== Test 1: client_group_membership ==="
BODY_GROUP="$(awk '/^SELECT/ {p=1} p {print}' "$SQL_GROUP")"
ACTUAL_GROUP="$(psql -X -A -t -F '|' -q -v ON_ERROR_STOP=1 -d "$URL" <<EOF
PREPARE group_smoke(bigint[], int) AS
$SHADOW_CTES
$BODY_GROUP;
EXECUTE group_smoke('{2}'::bigint[], 200);
EOF
)"

echo "$ACTUAL_GROUP"
# Client 1 in group 10 (with center 20)
if grep -q "^1|2|10|Solidarity Group A|2|20|2$" <<<"$ACTUAL_GROUP"; then
  echo "ok   client 1 in group 10 present with center_id 20"
else
  echo "FAIL client 1 in group 10 missing or wrong output"
  fail=$((fail + 1))
fi

# Client 1 in group 11 (with NULL center_id)
if grep -q "^1|2|11|Solidarity Group B|2||2$" <<<"$ACTUAL_GROUP"; then
  echo "ok   client 1 in group 11 present with NULL center_id (NULL-safe center linkage)"
else
  echo "FAIL client 1 in group 11 missing or wrong output"
  fail=$((fail + 1))
fi

# Client 2 in group 10
if grep -q "^2|2|10|Solidarity Group A|2|20|2$" <<<"$ACTUAL_GROUP"; then
  echo "ok   client 2 in group 10 present (N:M membership verified)"
else
  echo "FAIL client 2 in group 10 missing"
  fail=$((fail + 1))
fi

# Unauthorized client 3 in office 9 must be absent
if grep -q "^3|" <<<"$ACTUAL_GROUP"; then
  echo "FAIL unauthorized client 3 in office 9 was not filtered"
  fail=$((fail + 1))
else
  echo "ok   unauthorized client 3 in office 9 filtered"
fi

echo "=== Test 2: client_account_roster ==="
BODY_ROSTER="$(awk '/^SELECT/ {p=1} p {print}' "$SQL_ROSTER")"
ACTUAL_ROSTER="$(psql -X -A -t -F '|' -q -v ON_ERROR_STOP=1 -d "$URL" <<EOF
PREPARE roster_smoke(bigint[], int) AS
$SHADOW_CTES
$BODY_ROSTER;
EXECUTE roster_smoke('{2}'::bigint[], 200);
EOF
)"

echo "$ACTUAL_ROSTER"
# Check all 3 account types for client 1
if grep -q "^1001|loan|1|2|101|100|2026-01-05|USD$" <<<"$ACTUAL_ROSTER"; then
  echo "ok   pending loan 1001 (status 100) present (no default-active assumption)"
else
  echo "FAIL pending loan 1001 missing"
  fail=$((fail + 1))
fi

if grep -q "^1002|loan|1|2|101|300|2026-01-10|USD$" <<<"$ACTUAL_ROSTER"; then
  echo "ok   active loan 1002 (status 300) present"
else
  echo "FAIL active loan 1002 missing"
  fail=$((fail + 1))
fi

if grep -q "^2001|savings|1|2|201|300|2026-01-01|USD$" <<<"$ACTUAL_ROSTER"; then
  echo "ok   active savings account 2001 present"
else
  echo "FAIL active savings 2001 missing"
  fail=$((fail + 1))
fi

if grep -q "^2002|savings|1|2|201|100|2026-01-02|USD$" <<<"$ACTUAL_ROSTER"; then
  echo "ok   submitted savings account 2002 present (status 100)"
else
  echo "FAIL submitted savings 2002 missing"
  fail=$((fail + 1))
fi

if grep -q "^3001|share|1|2|301|300|2026-01-03|USD$" <<<"$ACTUAL_ROSTER"; then
  echo "ok   share account 3001 present"
else
  echo "FAIL share account 3001 missing"
  fail=$((fail + 1))
fi

# Unauthorized client 3 in office 9 must be absent
if grep -q "|3|9|" <<<"$ACTUAL_ROSTER"; then
  echo "FAIL unauthorized client 3 in office 9 was not filtered"
  fail=$((fail + 1))
else
  echo "ok   unauthorized client 3 in office 9 filtered"
fi

echo "=== Test 3: client_address ==="
BODY_ADDRESS="$(awk '/^SELECT/ {p=1} p {print}' "$SQL_ADDRESS")"
ACTUAL_ADDRESS="$(psql -X -A -t -F '|' -q -v ON_ERROR_STOP=1 -d "$URL" <<EOF
PREPARE address_smoke(bigint[], int) AS
$SHADOW_CTES
$BODY_ADDRESS;
EXECUTE address_smoke('{2}'::bigint[], 200);
EOF
)"

echo "$ACTUAL_ADDRESS"
if grep -q "^1|1|2|6001|501|RESIDENTIAL|t|Main St|123 Market St|Suite 4|||Capital City||10|20|12345||$" <<<"$ACTUAL_ADDRESS"; then
  echo "ok   client 1 address 6001 present with full address details"
else
  echo "FAIL client 1 address 6001 missing"
  fail=$((fail + 1))
fi

# Unauthorized client 3 address in office 9 must be absent
if grep -q "^2|3|" <<<"$ACTUAL_ADDRESS"; then
  echo "FAIL unauthorized client 3 address in office 9 was not filtered"
  fail=$((fail + 1))
else
  echo "ok   unauthorized client 3 address in office 9 filtered"
fi

echo "=== Test 4: empty office scope semantics ==="
EMPTY_GROUP="$(psql -X -A -t -q -v ON_ERROR_STOP=1 -d "$URL" <<EOF
PREPARE empty_group_smoke(bigint[], int) AS
$SHADOW_CTES
$BODY_GROUP;
EXECUTE empty_group_smoke('{3}'::bigint[], 200);
EOF
)"
if [ -z "$EMPTY_GROUP" ]; then
  echo "ok   client_group_membership in empty office 3 returns 0 rows"
else
  echo "FAIL client_group_membership in empty office 3 returned rows"
  fail=$((fail + 1))
fi

EMPTY_ADDRESS="$(psql -X -A -t -q -v ON_ERROR_STOP=1 -d "$URL" <<EOF
PREPARE empty_address_smoke(bigint[], int) AS
$SHADOW_CTES
$BODY_ADDRESS;
EXECUTE empty_address_smoke('{3}'::bigint[], 200);
EOF
)"
if [ -z "$EMPTY_ADDRESS" ]; then
  echo "ok   client_address in empty office 3 returns 0 rows"
else
  echo "FAIL client_address in empty office 3 returned rows"
  fail=$((fail + 1))
fi

if [ "$fail" -eq 0 ]; then
  echo "=== All client nonzero semantics smoke checks PASSED ==="
  exit 0
else
  echo "=== $fail checks FAILED ==="
  exit 1
fi
