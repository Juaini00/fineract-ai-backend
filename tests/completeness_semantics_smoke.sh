#!/usr/bin/env bash
#
# FIN-120 nonzero semantics smoke for queries/completeness/*.sql.
#
# Deterministic and read-only: the production SQL text is executed UNCHANGED, but every
# Fineract table it names is shadowed by a CTE of the same name built from inline VALUES,
# so no Fineract table is read and no fixture row exists anywhere. The connection only
# hosts the statement. Cases proven (semantics absent or guarded in live data):
#   * sentinel bucket present when every client unclassified ('Sektor belum tercatat');
#   * bucket sums reconcile to population total (reconciliation invariant, D11/I4/I5);
#   * classified + unclassified mix counts both correctly;
#   * currency split (currency_code in grain, no cross-currency sums);
#   * office scope binds (COALESCE(c.office_id, g.office_id) = ANY($1::bigint[]));
#   * client names and account_no never selected (PII safe);
#   * detail capability returns only unclassified loans and respects limit;
#   * unsupported boundaries declared for custom datatables, surveys, as-of past date, and silent omission.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
URL="${SMOKE_DATABASE_URL:-postgres://root:password@127.0.0.1:5432/fineract_default}"

SQL_SUMMARY="$ROOT/queries/completeness/loan_outstanding_by_classification.sql"
SQL_DETAIL="$ROOT/queries/completeness/loan_outstanding_unclassified_detail.sql"

SHADOW_CTES="WITH
m_code_value(id, code_value) AS (VALUES
  (10::bigint, 'Agriculture'::varchar),
  (20::bigint, 'Retail'::varchar)
),
m_client(id, office_id, client_classification_cv_id) AS (VALUES
  (1::bigint, 1::bigint, 10::bigint),
  (2::bigint, 1::bigint, 20::bigint),
  (3::bigint, 1::bigint, NULL::bigint),
  (4::bigint, 2::bigint, NULL::bigint),
  (5::bigint, 9::bigint, NULL::bigint)
),
m_group(id, office_id) AS (VALUES
  (100::bigint, 1::bigint)
),
m_loan(id, client_id, group_id, currency_code, total_outstanding_derived) AS (VALUES
  (101::bigint, 1::bigint, NULL::bigint, 'USD'::varchar, 1000.00::decimal),
  (102::bigint, 1::bigint, NULL::bigint, 'USD'::varchar,  500.00::decimal),
  (103::bigint, 2::bigint, NULL::bigint, 'USD'::varchar, 2000.00::decimal),
  (104::bigint, 3::bigint, NULL::bigint, 'USD'::varchar,  300.00::decimal),
  (105::bigint, 3::bigint, NULL::bigint, 'EUR'::varchar,  400.00::decimal),
  (106::bigint, 4::bigint, NULL::bigint, 'USD'::varchar,  700.00::decimal),
  (107::bigint, 5::bigint, NULL::bigint, 'USD'::varchar, 9999.00::decimal)
)"

SHADOW_ALL_UNCLASSIFIED="WITH
m_code_value(id, code_value) AS (VALUES
  (10::bigint, 'Agriculture'::varchar)
),
m_client(id, office_id, client_classification_cv_id) AS (VALUES
  (3::bigint, 1::bigint, NULL::bigint),
  (4::bigint, 2::bigint, NULL::bigint)
),
m_group(id, office_id) AS (VALUES
  (100::bigint, 1::bigint)
),
m_loan(id, client_id, group_id, currency_code, total_outstanding_derived) AS (VALUES
  (104::bigint, 3::bigint, NULL::bigint, 'USD'::varchar, 300.00::decimal),
  (105::bigint, 3::bigint, NULL::bigint, 'EUR'::varchar, 400.00::decimal),
  (106::bigint, 4::bigint, NULL::bigint, 'USD'::varchar, 700.00::decimal)
)"

BODY_SUMMARY="$(cat "$SQL_SUMMARY" | sed -e 's/^--.*//' | tr '\n' ' ')"
BODY_DETAIL="$(cat "$SQL_DETAIL" | sed -e 's/^--.*//' | tr '\n' ' ')"

fail=0

echo "=== Test 1: Classified + Unclassified Mix & Reconciliation (Offices 1, 2) ==="
SUMMARY_OUTPUT="$(PGTZ=UTC psql -X -A -t -F '|' -q -v ON_ERROR_STOP=1 -d "$URL" <<EOF
PREPARE summary_smoke(bigint[]) AS
$SHADOW_CTES
$BODY_SUMMARY;
EXECUTE summary_smoke('{1,2}'::bigint[]);
EOF
)"

echo "$SUMMARY_OUTPUT"

# Expect 4 rows: Agriculture (USD), Retail (USD), Sektor belum tercatat (EUR), Sektor belum tercatat (USD)
row_count="$(echo "$SUMMARY_OUTPUT" | grep -v '^$' | wc -l | tr -d ' ')"
if [ "$row_count" -ne 4 ]; then
  echo "FAIL: expected 4 buckets, got $row_count" >&2; fail=1
else
  echo "ok   mix produced exactly 4 buckets"
fi

# Verify sentinel bucket presence
sentinel_rows="$(echo "$SUMMARY_OUTPUT" | grep 'Sektor belum tercatat' | wc -l | tr -d ' ')"
if [ "$sentinel_rows" -ne 2 ]; then
  echo "FAIL: expected 2 sentinel buckets (EUR + USD), got $sentinel_rows" >&2; fail=1
else
  echo "ok   sentinel bucket 'Sektor belum tercatat' present for both EUR and USD"
fi

# Verify currency split: EUR sentinel outstanding = 400.00, USD sentinel outstanding = 1000.00
eur_sentinel="$(echo "$SUMMARY_OUTPUT" | grep 'Sektor belum tercatat|EUR' | cut -d'|' -f5)"
usd_sentinel="$(echo "$SUMMARY_OUTPUT" | grep 'Sektor belum tercatat|USD' | cut -d'|' -f5)"
if [ "$eur_sentinel" != "400.00" ] || [ "$usd_sentinel" != "1000.00" ]; then
  echo "FAIL: sentinel amounts mismatch: EUR='$eur_sentinel', USD='$usd_sentinel'" >&2; fail=1
else
  echo "ok   sentinel bucket amounts strictly segregated by currency"
fi

# Reconciliation check: sum of USD buckets = 1500 (Agri) + 2000 (Retail) + 1000 (Sent) = 4500.00
total_usd_loans="$(echo "$SUMMARY_OUTPUT" | grep '|USD|' | cut -d'|' -f4 | paste -sd+ - | bc)"
total_usd_out="$(echo "$SUMMARY_OUTPUT" | grep '|USD|' | cut -d'|' -f5 | paste -sd+ - | bc)"
if [ "$total_usd_loans" -ne 5 ] || [ "$total_usd_out" != "4500.00" ]; then
  echo "FAIL: USD reconciliation failed: loans=$total_usd_loans (want 5), out=$total_usd_out (want 4500.00)" >&2; fail=1
else
  echo "ok   bucket reconciliation verified: sum(buckets) == total population (USD 4500.00 across 5 loans)"
fi

echo "=== Test 2: 100% Unclassified Population ==="
ALL_UNCLASS_OUTPUT="$(PGTZ=UTC psql -X -A -t -F '|' -q -v ON_ERROR_STOP=1 -d "$URL" <<EOF
PREPARE all_unclass_smoke(bigint[]) AS
$SHADOW_ALL_UNCLASSIFIED
$BODY_SUMMARY;
EXECUTE all_unclass_smoke('{1,2}'::bigint[]);
EOF
)"

echo "$ALL_UNCLASS_OUTPUT"

non_sentinel="$(echo "$ALL_UNCLASS_OUTPUT" | (grep -v 'Sektor belum tercatat' || true) | (grep -v '^$' || true) | wc -l | tr -d ' ')"
unclass_count="$(echo "$ALL_UNCLASS_OUTPUT" | (grep 'Sektor belum tercatat' || true) | wc -l | tr -d ' ')"
if [ "$non_sentinel" -ne 0 ] || [ "$unclass_count" -ne 2 ]; then
  echo "FAIL: expected only sentinel buckets when all unclassified, got: '$ALL_UNCLASS_OUTPUT'" >&2; fail=1
else
  echo "ok   sentinel bucket present and dominant when 100% unclassified (no silent drop)"
fi

echo "=== Test 3: Unclassified Detail Capability ==="
DETAIL_OUTPUT="$(PGTZ=UTC psql -X -A -t -F '|' -q -v ON_ERROR_STOP=1 -d "$URL" <<EOF
PREPARE detail_smoke(bigint[], int) AS
$SHADOW_CTES
$BODY_DETAIL;
EXECUTE detail_smoke('{1,2}'::bigint[], 100);
EOF
)"

echo "$DETAIL_OUTPUT"

# Expect loans 104, 105, 106 (3 loans)
detail_count="$(echo "$DETAIL_OUTPUT" | grep -v '^$' | wc -l | tr -d ' ')"
if [ "$detail_count" -ne 3 ]; then
  echo "FAIL: detail expected 3 unclassified loans, got $detail_count" >&2; fail=1
else
  echo "ok   detail capability returned exactly the 3 unclassified loans"
fi

# Verify office scope exclusion: loan 107 in office 9 must be absent
if echo "$DETAIL_OUTPUT" | grep '^107|' >/dev/null; then
  echo "FAIL: loan 107 in unauthorized office 9 was not excluded" >&2; fail=1
else
  echo "ok   unauthorized office 9 strictly excluded from detail"
fi

# Verify limit parameter works
DETAIL_LIMITED="$(PGTZ=UTC psql -X -A -t -F '|' -q -v ON_ERROR_STOP=1 -d "$URL" <<EOF
PREPARE detail_limited_smoke(bigint[], int) AS
$SHADOW_CTES
$BODY_DETAIL;
EXECUTE detail_limited_smoke('{1,2}'::bigint[], 2);
EOF
)"
limited_count="$(echo "$DETAIL_LIMITED" | grep -v '^$' | wc -l | tr -d ' ')"
if [ "$limited_count" -ne 2 ]; then
  echo "FAIL: detail with LIMIT 2 expected 2 rows, got $limited_count" >&2; fail=1
else
  echo "ok   detail capability respects bound LIMIT parameter"
fi

echo "=== Test 4: PII & Boundary Safeguards ==="
# Check that client name and account_no are NEVER selected
for forbidden in account_no external_id client_name display_name; do
  if grep -E "name: +$forbidden *," "$ROOT/knowledge/queries/completeness/"*.yaml >/dev/null 2>&1; then
    echo "FAIL: forbidden column $forbidden leaked into query output_fields" >&2; fail=1
  else
    echo "ok   forbidden column $forbidden absent from query output_fields"
  fi
done

# Check domain yaml guards
for guard in sentinel_bucket_never_dropped office_scope_required lo8_no_recompute; do
  if grep -rn "$guard" "$ROOT/knowledge/domains/completeness.yaml" >/dev/null 2>&1; then
    echo "ok   guard $guard declared in completeness.yaml"
  else
    echo "FAIL: guard $guard missing from completeness.yaml" >&2; fail=1
  fi
done

# Check unsupported intents in completeness.yaml
for unsupported in "custom datatables" "survey" "past date" "silent omission"; do
  if grep -rn "$unsupported" "$ROOT/knowledge/domains/completeness.yaml" >/dev/null 2>&1; then
    echo "ok   unsupported intent '$unsupported' declared in completeness.yaml"
  else
    echo "FAIL: unsupported intent '$unsupported' missing from completeness.yaml" >&2; fail=1
  fi
done

if [ "$fail" -eq 0 ]; then
  echo "=== All completeness nonzero semantics smoke checks PASSED ==="
fi
exit $fail
