-- D03: separate base totals per currency and domain family (loan vs savings).
-- Anti-fanout §33: loan and savings rows are segregated via UNION ALL and never summed together.
-- No cross-currency consolidation, conversion, or reporting currency column (XR-CUR invariant).
-- Office authorization enforced per family:
--   loan: COALESCE(c.office_id, g.office_id) = ANY($1::bigint[])
--   savings: c.office_id = ANY($1::bigint[])
SELECT
    'loan'::text AS domain_family,
    l.currency_code,
    COUNT(DISTINCT l.id)::bigint AS account_count,
    ROUND(SUM(l.total_outstanding_derived), 2)::numeric AS balance_total
FROM m_loan l
LEFT JOIN m_client c ON c.id = l.client_id
LEFT JOIN m_group g ON g.id = l.group_id
WHERE COALESCE(c.office_id, g.office_id) = ANY($1::bigint[])
GROUP BY l.currency_code

UNION ALL

SELECT
    'savings'::text AS domain_family,
    sa.currency_code,
    COUNT(DISTINCT sa.id)::bigint AS account_count,
    ROUND(SUM(sa.account_balance_derived), 2)::numeric AS balance_total
FROM m_savings_account sa
JOIN m_client c ON c.id = sa.client_id
WHERE c.office_id = ANY($1::bigint[])
GROUP BY sa.currency_code

ORDER BY domain_family, currency_code;
