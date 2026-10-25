#!/usr/bin/env bash
#
# FIN-118 nonzero semantics smoke for queries/source_audit/*.sql.
#
# Deterministic and read-only: the production SQL text is executed UNCHANGED, but every
# Fineract table it names is shadowed by a CTE of the same name built from inline VALUES,
# so no Fineract table is read and no fixture row exists anywhere. The connection only
# hosts the statement. Cases proven (semantics absent or guarded in live data):
#   * actor attribution: maker_id joined to m_appuser.username (never officer assignment table);
#   * maker-checker attribution: checker_id joined to m_appuser.username when present, NULL when absent;
#   * office authorization: cs.office_id = ANY($1::bigint[]); commands in unauthorized office (office 9) excluded under {2, 3} scope;
#   * NULL-office handling: commands with NULL office_id (org-wide actions) strictly excluded under office scope;
#   * date window filter binds on cs.made_on_date_utc;
#   * status returned raw as smallint/integer (not mapped in SQL);
#   * named-subject guard on trace: returns 0 rows when all subject parameters are NULL;
#   * named-subject filter: returns specific resource actions when savings_account_id is provided;
#   * raw payload columns (command_as_json, result, client_ip, api_get_url, *_external_id) absent from output;
#   * boundary safeguards: payload, reason extraction, and Jarvis I8 execution audit guarded in unsupported_intents.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
URL="${SMOKE_DATABASE_URL:-postgres://root:password@127.0.0.1:5432/fineract_default}"

SQL_ACTIONS="$ROOT/queries/source_audit/command_source_actions.sql"
SQL_TRACE="$ROOT/queries/source_audit/command_source_trace.sql"

SHADOW_CTES="WITH
m_appuser(id, username) AS (VALUES
  (1::bigint, 'mifos'::varchar),
  (4, 'Admin'),
  (5, 'checker_alice')),
m_portfolio_command_source(
  id, action_name, entity_name, office_id, group_id, client_id, loan_id, savings_account_id,
  api_get_url, resource_id, subresource_id, command_as_json, maker_id, made_on_date,
  checker_id, checked_on_date, status, product_id, transaction_id, creditbureau_id,
  organisation_creditbureau_id, made_on_date_utc, checked_on_date_utc, job_name,
  idempotency_key, resource_external_id, subresource_external_id, result, result_status_code,
  loan_external_id, is_sanitized, client_ip
) AS (VALUES
  (101::bigint, 'WAIVE'::varchar, 'SAVINGSACCOUNTCHARGE'::varchar, 2::bigint, NULL::bigint, 17::bigint, NULL::bigint, 37::bigint,
   '/api/v1/savingsaccounts/37/charges/2'::varchar, 2::bigint, NULL::bigint, '{\"reason\":\"hardship\"}'::text, 4::bigint, NULL::timestamp,
   5::bigint, NULL::timestamp, 1::smallint, NULL::bigint, NULL::varchar, NULL::bigint,
   NULL::bigint, TIMESTAMPTZ '2026-03-15 10:00:00+00', TIMESTAMPTZ '2026-03-15 10:05:00+00', NULL::varchar,
   'idem-101'::varchar, 'ext-res-1'::varchar, NULL::varchar, '{\"status\":\"ok\"}'::text, 200::integer,
   NULL::varchar, true, '192.168.1.1'::varchar),
  (102, 'APPROVE', 'LOAN', 3, NULL, 20, 55, NULL,
   '/api/v1/loans/55'::varchar, 55, NULL, '{\"note\":\"approved\"}'::text, 1, NULL,
   NULL::bigint, NULL::timestamp, 1, NULL, NULL, NULL,
   NULL, TIMESTAMPTZ '2026-04-01 12:00:00+00', NULL::timestamptz, NULL,
   'idem-102', NULL, NULL, NULL, 200,
   NULL, true, '192.168.1.2'),
  (103, 'CREATE', 'CLIENT', 9, NULL, 30, NULL, NULL,
   '/api/v1/clients'::varchar, 30, NULL, '{}'::text, 4, NULL,
   NULL, NULL, 2, NULL, NULL, NULL,
   NULL, TIMESTAMPTZ '2026-03-20 09:00:00+00', NULL, NULL,
   'idem-103', NULL, NULL, NULL, 200,
   NULL, true, '192.168.1.3'),
  (104, 'CREATE', 'GLACCOUNT', NULL::bigint, NULL, NULL, NULL, NULL,
   '/api/v1/glaccounts'::varchar, 10, NULL, '{}'::text, 4, NULL,
   NULL, NULL, 1, NULL, NULL, NULL,
   NULL, TIMESTAMPTZ '2026-03-10 08:00:00+00', NULL, NULL,
   'idem-104', NULL, NULL, NULL, 200,
   NULL, true, '192.168.1.4'),
  (105, 'DISBURSE', 'LOAN', 2, NULL, 20, 55, NULL,
   '/api/v1/loans/55'::varchar, 55, NULL, '{}'::text, 4, NULL,
   NULL, NULL, 5, NULL, NULL, NULL,
   NULL, TIMESTAMPTZ '2024-01-01 00:00:00+00', NULL, NULL,
   'idem-105', NULL, NULL, NULL, 500,
   NULL, true, '192.168.1.5'))"

fail=0

echo "=== Test 1: command_source_actions ==="
BODY_ACTIONS="$(awk '/^SELECT/ {p=1} p {print}' "$SQL_ACTIONS")"
ACTUAL_ACTIONS="$(PGTZ=UTC psql -X -A -t -F '|' -q -v ON_ERROR_STOP=1 -d "$URL" <<EOF
PREPARE actions_smoke(bigint[], varchar, varchar, smallint, date, date, int) AS
$SHADOW_CTES
$BODY_ACTIONS;
EXECUTE actions_smoke('{2,3}'::bigint[], NULL, NULL, NULL, DATE '2026-01-01', DATE '2026-04-30', 200);
EOF
)"

echo "$ACTUAL_ACTIONS"

# Check row count = 2 (rows 101 and 102 in scope; row 103 in office 9 excluded; row 104 NULL office excluded; row 105 in 2024 excluded)
action_count="$(echo "$ACTUAL_ACTIONS" | grep -v '^$' | wc -l | tr -d ' ')"
if [ "$action_count" -ne 2 ]; then
  echo "FAIL: expected 2 action rows, got $action_count" >&2; fail=1
else
  echo "ok   actions count is 2 (office 9 excluded by scope, NULL office excluded by convention, 2024 excluded by window)"
fi

# Check row 101 fields: maker attribution, checker attribution, status
row101="$(echo "$ACTUAL_ACTIONS" | grep '^101|' || true)"
if [ -z "$row101" ]; then
  echo "FAIL: row 101 missing" >&2; fail=1
else
  maker_id="$(echo "$row101" | cut -d'|' -f5)"
  maker_username="$(echo "$row101" | cut -d'|' -f6)"
  checker_id="$(echo "$row101" | cut -d'|' -f8)"
  checker_username="$(echo "$row101" | cut -d'|' -f9)"
  status="$(echo "$row101" | cut -d'|' -f11)"
  savings_acc="$(echo "$row101" | cut -d'|' -f15)"
  if [ "$maker_id" != "4" ] || [ "$maker_username" != "Admin" ]; then
    echo "FAIL: maker attribution mismatch: maker_id='$maker_id' maker_username='$maker_username'" >&2; fail=1
  else
    echo "ok   maker attribution strictly from command row maker_id (4 -> Admin)"
  fi
  if [ "$checker_id" != "5" ] || [ "$checker_username" != "checker_alice" ]; then
    echo "FAIL: checker attribution mismatch: checker_id='$checker_id' checker_username='$checker_username'" >&2; fail=1
  else
    echo "ok   checker attribution strictly from command row checker_id (5 -> checker_alice)"
  fi
  if [ "$status" != "1" ]; then
    echo "FAIL: raw status expected '1', got '$status'" >&2; fail=1
  else
    echo "ok   status returned raw as smallint/integer (1)"
  fi
  if [ "$savings_acc" != "37" ]; then
    echo "FAIL: savings_account_id expected '37', got '$savings_acc'" >&2; fail=1
  else
    echo "ok   resource pointer savings_account_id preserved (37)"
  fi
fi

# Check row 102 fields: no checker (NULL checker fields)
row102="$(echo "$ACTUAL_ACTIONS" | grep '^102|' || true)"
if [ -z "$row102" ]; then
  echo "FAIL: row 102 missing" >&2; fail=1
else
  maker102="$(echo "$row102" | cut -d'|' -f6)"
  checker102="$(echo "$row102" | cut -d'|' -f8)"
  checker_user102="$(echo "$row102" | cut -d'|' -f9)"
  if [ "$maker102" != "mifos" ] || [ -n "$checker102" ] || [ -n "$checker_user102" ]; then
    echo "FAIL: row 102 maker/checker mismatch: maker='$maker102' checker='$checker102'" >&2; fail=1
  else
    echo "ok   row 102 maker is mifos and checker is cleanly NULL"
  fi
fi

# Check exclusions
if echo "$ACTUAL_ACTIONS" | grep -q '^103|'; then
  echo "FAIL: row 103 (unauthorized office 9) leaked" >&2; fail=1
else
  echo "ok   row 103 (unauthorized office 9) strictly excluded"
fi

if echo "$ACTUAL_ACTIONS" | grep -q '^104|'; then
  echo "FAIL: row 104 (NULL office) leaked under office scope" >&2; fail=1
else
  echo "ok   row 104 (NULL office) strictly excluded under office scope"
fi

if echo "$ACTUAL_ACTIONS" | grep -q '^105|'; then
  echo "FAIL: row 105 (outside date window) leaked" >&2; fail=1
else
  echo "ok   row 105 (outside date window) strictly excluded"
fi

echo "=== Test 2: command_source_trace ==="
BODY_TRACE="$(awk '/^SELECT/ {p=1} p {print}' "$SQL_TRACE")"

# Trace test A: named-subject guard returns 0 rows when all subjects are NULL
GUARD_OUTPUT="$(PGTZ=UTC psql -X -A -t -F '|' -q -v ON_ERROR_STOP=1 -d "$URL" <<EOF
PREPARE trace_guard_smoke(bigint[], bigint, bigint, bigint, bigint, bigint, varchar, varchar, date, date, int) AS
$SHADOW_CTES
$BODY_TRACE;
EXECUTE trace_guard_smoke('{2,3}'::bigint[], NULL, NULL, NULL, NULL, NULL, NULL, NULL, DATE '2026-01-01', DATE '2026-04-30', 200);
EOF
)"

guard_count="$(echo "$GUARD_OUTPUT" | grep -v '^$' | wc -l | tr -d ' ' || true)"
if [ "$guard_count" -ne 0 ]; then
  echo "FAIL: named-subject guard expected 0 rows, got $guard_count" >&2; fail=1
else
  echo "ok   named-subject guard returns 0 rows when all subject parameters are NULL"
fi

# Trace test B: named subject provided (savings_account_id = 37)
TRACE_OUTPUT="$(PGTZ=UTC psql -X -A -t -F '|' -q -v ON_ERROR_STOP=1 -d "$URL" <<EOF
PREPARE trace_smoke(bigint[], bigint, bigint, bigint, bigint, bigint, varchar, varchar, date, date, int) AS
$SHADOW_CTES
$BODY_TRACE;
EXECUTE trace_smoke('{2,3}'::bigint[], NULL, NULL, 37::bigint, NULL, NULL, NULL, NULL, DATE '2026-01-01', DATE '2026-04-30', 200);
EOF
)"

trace_count="$(echo "$TRACE_OUTPUT" | grep -v '^$' | wc -l | tr -d ' ')"
if [ "$trace_count" -ne 1 ]; then
  echo "FAIL: trace with savings_account_id=37 expected 1 row, got $trace_count" >&2; fail=1
else
  trace_id="$(echo "$TRACE_OUTPUT" | cut -d'|' -f1)"
  trace_action="$(echo "$TRACE_OUTPUT" | cut -d'|' -f2)"
  trace_maker="$(echo "$TRACE_OUTPUT" | cut -d'|' -f6)"
  if [ "$trace_id" != "101" ] || [ "$trace_action" != "WAIVE" ] || [ "$trace_maker" != "Admin" ]; then
    echo "FAIL: trace row mismatch: '$TRACE_OUTPUT'" >&2; fail=1
  else
    echo "ok   trace with savings_account_id=37 returns only row 101 (WAIVE, Admin)"
  fi
fi

echo "=== Test 3: Boundaries & Safeguards ==="

# Check that raw payload columns are NOT in query output_fields
for forbidden in command_as_json result client_ip api_get_url resource_external_id subresource_external_id loan_external_id; do
  if grep -E "name: +$forbidden *," "$ROOT/knowledge/queries/source_audit/"*.yaml >/dev/null 2>&1; then
    echo "FAIL: forbidden column $forbidden leaked into query output_fields" >&2; fail=1
  else
    echo "ok   forbidden column $forbidden absent from query output_fields"
  fi
done

# Check that recorded reason is declared in unsupported_intents
if grep -rn "recorded reason" "$ROOT/knowledge/domains/audit.yaml" >/dev/null 2>&1; then
  echo "ok   recorded reason extraction is guarded under unsupported_intents"
else
  echo "FAIL: recorded reason missing from unsupported_intents" >&2; fail=1
fi

# Check that raw payload is declared in unsupported_intents
if grep -rn "raw command payload" "$ROOT/knowledge/domains/audit.yaml" >/dev/null 2>&1; then
  echo "ok   raw command payload is guarded under unsupported_intents"
else
  echo "FAIL: raw command payload missing from unsupported_intents" >&2; fail=1
fi

# Check that Jarvis execution audit log is declared in unsupported_intents (I8)
if grep -rn "jarvis execution audit log" "$ROOT/knowledge/domains/audit.yaml" >/dev/null 2>&1; then
  echo "ok   jarvis execution audit log is guarded under unsupported_intents (I8 boundary)"
else
  echo "FAIL: jarvis execution audit log missing from unsupported_intents" >&2; fail=1
fi

if [ "$fail" -eq 0 ]; then
  echo "=== All source_audit nonzero semantics smoke checks PASSED ==="
fi
exit $fail
