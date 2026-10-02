-- params: {}
-- FIN-108 LOAN-2: loans per status and currency, scope via a CTE of in-scope loan ids.
WITH scoped AS (
    SELECT l.id FROM m_loan l JOIN m_client c ON c.id = l.client_id WHERE c.office_id = ANY(:'office_ids'::bigint[])
    UNION
    SELECT l.id FROM m_loan l JOIN m_group g ON g.id = l.group_id WHERE l.client_id IS NULL AND g.office_id = ANY(:'office_ids'::bigint[])
)
SELECT l.loan_status_id::bigint AS loan_status_id,
       CASE e.enum_value
           WHEN 'Submitted and awaiting approval' THEN 'submitted_and_pending_approval'
           ELSE lower(replace(replace(e.enum_value, ' ', '_'), '-', '_'))
       END AS loan_status,
       l.currency_code,
       count(*) AS loan_count
FROM scoped s
JOIN m_loan l ON l.id = s.id
LEFT JOIN r_enum_value e ON e.enum_name = 'loan_status_id' AND e.enum_id = l.loan_status_id
GROUP BY 1, 2, 3
ORDER BY 1, 3
