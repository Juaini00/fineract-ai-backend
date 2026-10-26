-- params: {}
-- Reconciliation identity (D03 proof): loan-family rows must sum to loan domain total
-- (116 loans / 537,817.37) and savings-family rows to savings totals (205 accounts / 502,236.14).
-- Total population is 7 rows (4 loan currencies + 3 savings currencies).
WITH loan_agg AS (
    SELECT
        COUNT(DISTINCT l.id) AS loan_count,
        ROUND(SUM(l.total_outstanding_derived), 2) AS loan_total
    FROM m_loan l
    LEFT JOIN m_client c ON c.id = l.client_id
    LEFT JOIN m_group g ON g.id = l.group_id
    WHERE COALESCE(c.office_id, g.office_id) = ANY(:'office_ids'::bigint[])
),
savings_agg AS (
    SELECT
        COUNT(DISTINCT sa.id) AS savings_count,
        ROUND(SUM(sa.account_balance_derived), 2) AS savings_total
    FROM m_savings_account sa
    JOIN m_client c ON c.id = sa.client_id
    WHERE c.office_id = ANY(:'office_ids'::bigint[])
),
rows_agg AS (
    SELECT 'loan'::text AS domain_family, l.currency_code
    FROM m_loan l
    LEFT JOIN m_client c ON c.id = l.client_id
    LEFT JOIN m_group g ON g.id = l.group_id
    WHERE COALESCE(c.office_id, g.office_id) = ANY(:'office_ids'::bigint[])
    GROUP BY l.currency_code
    UNION ALL
    SELECT 'savings'::text AS domain_family, sa.currency_code
    FROM m_savings_account sa
    JOIN m_client c ON c.id = sa.client_id
    WHERE c.office_id = ANY(:'office_ids'::bigint[])
    GROUP BY sa.currency_code
)
SELECT count(*) AS n
FROM rows_agg
CROSS JOIN loan_agg
CROSS JOIN savings_agg
WHERE loan_agg.loan_count = 116
  AND loan_agg.loan_total = 537817.37
  AND savings_agg.savings_count = 205
  AND savings_agg.savings_total = 502236.14
