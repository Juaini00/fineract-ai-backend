-- params: {}
-- FIN-120 D11: Loan completeness analysis by classification bucket and currency.
WITH scoped_loans AS (
    SELECT
        l.id AS loan_id,
        l.client_id,
        l.currency_code,
        l.total_outstanding_derived,
        COALESCE(c.office_id, g.office_id) AS office_id,
        c.client_classification_cv_id
    FROM m_loan l
    LEFT JOIN m_client c ON c.id = l.client_id
    LEFT JOIN m_group g ON g.id = l.group_id
    WHERE COALESCE(c.office_id, g.office_id) = ANY(:'office_ids'::bigint[])
),
classifications AS (
    SELECT id, code_value FROM m_code_value
)
SELECT
    COALESCE(cl.code_value, 'Sektor belum tercatat') AS classification,
    sl.currency_code,
    COUNT(DISTINCT sl.client_id) AS client_count,
    COUNT(sl.loan_id) AS loan_count,
    COALESCE(SUM(sl.total_outstanding_derived), 0) AS outstanding_total
FROM scoped_loans sl
LEFT JOIN classifications cl ON cl.id = sl.client_classification_cv_id
GROUP BY COALESCE(cl.code_value, 'Sektor belum tercatat'), sl.currency_code
ORDER BY classification, sl.currency_code
