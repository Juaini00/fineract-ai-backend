-- params: {}
-- Full population size of loans for loan_outstanding_by_classification.
SELECT count(*) AS n
FROM m_loan l
LEFT JOIN m_client c ON c.id = l.client_id
LEFT JOIN m_group g ON g.id = l.group_id
WHERE COALESCE(c.office_id, g.office_id) = ANY(:'office_ids'::bigint[])
