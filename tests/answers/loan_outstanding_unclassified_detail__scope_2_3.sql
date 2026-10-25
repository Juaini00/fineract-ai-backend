-- params: {"limit": 100}
-- NARROWED to offices 2 and 3: partial authorization proof for loan_outstanding_unclassified_detail.
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
    WHERE COALESCE(c.office_id, g.office_id) = ANY(ARRAY[2, 3]::bigint[])
)
SELECT
    sl.loan_id,
    sl.client_id,
    sl.currency_code,
    sl.total_outstanding_derived,
    sl.office_id
FROM scoped_loans sl
WHERE sl.client_classification_cv_id IS NULL
ORDER BY sl.loan_id
LIMIT 100
