-- params: {"limit": 100}
-- FIN-108 slice 4 LOAN-11: recorded write-off per loan, scope via a UNION of
-- in-scope loan ids. 0 rows on this deployment (no loan is written off
-- locally).
WITH scoped AS (
    SELECT l.id FROM m_loan l JOIN m_client c ON c.id = l.client_id WHERE c.office_id = ANY(:'office_ids'::bigint[])
    UNION
    SELECT l.id FROM m_loan l JOIN m_group g ON g.id = l.group_id WHERE l.client_id IS NULL AND g.office_id = ANY(:'office_ids'::bigint[])
)
SELECT
    l.id AS loan_id,
    l.writtenoffon_date,
    cv.code_value AS writeoff_reason,
    l.principal_writtenoff_derived,
    l.interest_writtenoff_derived,
    l.fee_charges_writtenoff_derived,
    l.penalty_charges_writtenoff_derived,
    l.total_writtenoff_derived,
    l.currency_code
FROM m_loan l
JOIN scoped s ON s.id = l.id
LEFT JOIN m_code_value cv ON cv.id = l.writeoff_reason_cv_id
WHERE l.writtenoffon_date IS NOT NULL
ORDER BY l.writtenoffon_date DESC, l.id
LIMIT 100
