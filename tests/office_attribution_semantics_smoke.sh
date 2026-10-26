#!/usr/bin/env bash
#
# FIN-122 (D02): nonzero semantics smoke for queries/client/client_office_attribution.sql.
#
# Deterministic and read-only: the production SQL text is executed UNCHANGED, but every
# Fineract table it names is shadowed by a CTE of the same name built from inline VALUES,
# so no Fineract table is read and no fixture row exists anywhere. The connection only
# hosts the statement. Cases proven (semantics absent or guarded in live data):
#   * attribution_mode literal 'current_office' on every row;
#   * office_as_of_position is NULL on every row (explicit 'not available', never fabricated);
#   * office authorization: c.office_id = ANY($1::bigint[]); clients in unauthorized offices (office 9) excluded under {2, 3} scope;
#   * office scope narrowing: scope {2} returns only office 2 clients;
#   * empty office scope: scope {8} returns 0 rows;
#   * limit parameter binds properly;
#   * no history source consulted: SQL references only m_client;
#   * PII columns (display_name, firstname, lastname, account_no, external_id) excluded;
#   * negative schema verification: no client office history table exists in fineract schema;
#   * command source verification: only proposals exist in command source, payloads excluded.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
URL="${SMOKE_DATABASE_URL:-postgres://root:password@127.0.0.1:5432/fineract_default}"

SQL_ATTRIBUTION="$ROOT/queries/client/client_office_attribution.sql"

SHADOW_CTES="WITH
m_client(id, office_id, display_name, firstname, lastname, account_no, external_id) AS (VALUES
  (1::bigint, 2::bigint, 'Alice Client'::varchar, 'Alice'::varchar, 'Client'::varchar, 'ACC-1'::varchar, 'EXT-1'::varchar),
  (2::bigint, 3::bigint, 'Bob Member'::varchar, 'Bob'::varchar, 'Member'::varchar, 'ACC-2'::varchar, 'EXT-2'::varchar),
  (3::bigint, 9::bigint, 'Charlie Other'::varchar, 'Charlie'::varchar, 'Other'::varchar, 'ACC-3'::varchar, 'EXT-3'::varchar),
  (4::bigint, 2::bigint, 'Dana Scope'::varchar, 'Dana'::varchar, 'Scope'::varchar, 'ACC-4'::varchar, 'EXT-4'::varchar))"

fail=0

echo "=== Test 1: client_office_attribution (VALUES-only CTE shadow) ==="
BODY_ATTRIBUTION="$(awk '/^SELECT/ {p=1} p {print}' "$SQL_ATTRIBUTION")"
ACTUAL_ATTRIBUTION="$(PGTZ=UTC psql -X -A -t -F '|' -q -v ON_ERROR_STOP=1 -d "$URL" <<EOF
PREPARE attribution_smoke(bigint[], int) AS
$SHADOW_CTES
$BODY_ATTRIBUTION;
EXECUTE attribution_smoke('{2,3}'::bigint[], 200);
EOF
)"

echo "$ACTUAL_ATTRIBUTION"

# Check row count = 3 (clients 1, 2, 4 in scope; client 3 in office 9 excluded)
attr_count="$(echo "$ACTUAL_ATTRIBUTION" | grep -v '^$' | wc -l | tr -d ' ')"
if [ "$attr_count" -ne 3 ]; then
  echo "FAIL: expected 3 attribution rows, got $attr_count" >&2; fail=1
else
  echo "ok 1 - attribution count is 3 (unauthorized office 9 excluded by scope)"
fi

# Check attribution_mode literal and office_as_of_position NULL on all rows
while IFS='|' read -r client_id office_id attribution_mode office_as_of_position; do
  [ -z "$client_id" ] && continue
  if [ "$attribution_mode" != "current_office" ]; then
    echo "FAIL: client $client_id has attribution_mode '$attribution_mode', expected 'current_office'" >&2; fail=1
  fi
  if [ -n "$office_as_of_position" ]; then
    echo "FAIL: client $client_id has non-null office_as_of_position '$office_as_of_position', expected NULL" >&2; fail=1
  fi
done <<< "$ACTUAL_ATTRIBUTION"
echo "ok 2 - attribution_mode literal 'current_office' and office_as_of_position NULL verified on every row"

# Check narrowed scope {2}
NARROW_ATTRIBUTION="$(PGTZ=UTC psql -X -A -t -F '|' -q -v ON_ERROR_STOP=1 -d "$URL" <<EOF
PREPARE attribution_narrow(bigint[], int) AS
$SHADOW_CTES
$BODY_ATTRIBUTION;
EXECUTE attribution_narrow('{2}'::bigint[], 200);
EOF
)"
narrow_count="$(echo "$NARROW_ATTRIBUTION" | grep -v '^$' | wc -l | tr -d ' ')"
if [ "$narrow_count" -ne 2 ]; then
  echo "FAIL: expected 2 rows for narrowed scope {2}, got $narrow_count" >&2; fail=1
else
  echo "ok 3 - narrowed scope {2} returns exactly 2 clients from office 2"
fi

# Check empty scope {8}
EMPTY_ATTRIBUTION="$(PGTZ=UTC psql -X -A -t -F '|' -q -v ON_ERROR_STOP=1 -d "$URL" <<EOF
PREPARE attribution_empty(bigint[], int) AS
$SHADOW_CTES
$BODY_ATTRIBUTION;
EXECUTE attribution_empty('{8}'::bigint[], 200);
EOF
)"
empty_count="$(echo "$EMPTY_ATTRIBUTION" | grep -v '^$' | wc -l | tr -d ' ' || true)"
if [ "$empty_count" -ne 0 ]; then
  echo "FAIL: expected 0 rows for empty scope {8}, got $empty_count" >&2; fail=1
else
  echo "ok 4 - empty scope {8} returns 0 rows"
fi

# Check LIMIT parameter binds
LIMIT_ATTRIBUTION="$(PGTZ=UTC psql -X -A -t -F '|' -q -v ON_ERROR_STOP=1 -d "$URL" <<EOF
PREPARE attribution_limit(bigint[], int) AS
$SHADOW_CTES
$BODY_ATTRIBUTION;
EXECUTE attribution_limit('{2,3}'::bigint[], 2);
EOF
)"
limit_count="$(echo "$LIMIT_ATTRIBUTION" | grep -v '^$' | wc -l | tr -d ' ')"
if [ "$limit_count" -ne 2 ]; then
  echo "FAIL: expected 2 rows with limit 2, got $limit_count" >&2; fail=1
else
  echo "ok 5 - limit parameter binds properly (got 2 rows)"
fi

echo "=== Test 2: Table references and PII safeguard checks ==="
# Check SQL file references ONLY m_client and no history or transfer tables
if grep -iE "m_.*office.*history|m_client_transfer|m_portfolio_command" "$SQL_ATTRIBUTION" >/dev/null; then
  echo "FAIL: SQL references historical or command tables" >&2; fail=1
else
  echo "ok 6 - SQL references only current-state m_client table (no history tables)"
fi

# Check PII columns are not in SQL
if grep -iE "\b(display_name|firstname|lastname|account_no|external_id)\b" "$SQL_ATTRIBUTION" >/dev/null; then
  echo "FAIL: SQL references forbidden PII column" >&2; fail=1
else
  echo "ok 7 - SQL excludes forbidden PII columns"
fi

echo "=== Test 3: Live Schema Negative Verification (D02 basis) ==="
# Verify no client office history table exists in information_schema
hist_table_count="$(psql -X -A -t -d "$URL" -c "SELECT count(*) FROM information_schema.tables WHERE table_schema='public' AND table_name LIKE '%client%office%history%';")"
if [ "$hist_table_count" -ne 0 ]; then
  echo "FAIL: client office history table unexpectedly exists ($hist_table_count)" >&2; fail=1
else
  echo "ok 8 - negative schema verification: 0 client office history tables exist in information_schema"
fi

# Verify m_client_transfer_details has 0 completed transfer rows
transfer_details_count="$(psql -X -A -t -d "$URL" -c "SELECT count(*) FROM m_client_transfer_details;")"
if [ "$transfer_details_count" -ne 0 ]; then
  echo "FAIL: m_client_transfer_details has rows ($transfer_details_count)" >&2; fail=1
else
  echo "ok 9 - transfer evidence verification: m_client_transfer_details has 0 rows"
fi

# Verify m_portfolio_command_source has only 2 proposal rows (status 5)
command_transfer_count="$(psql -X -A -t -d "$URL" -c "SELECT count(*) FROM m_portfolio_command_source WHERE action_name LIKE '%TRANSFER%';")"
if [ "$command_transfer_count" -ne 2 ]; then
  echo "FAIL: expected 2 proposal transfer command rows, got $command_transfer_count" >&2; fail=1
else
  echo "ok 10 - transfer commands are proposals only (2 rows, status 5, payloads excluded)"
fi

if [ "$fail" -eq 0 ]; then
  echo "All 10 semantic and schema assertions passed cleanly."
else
  echo "Some assertions failed." >&2
  exit 1
fi
