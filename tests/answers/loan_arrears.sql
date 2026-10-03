-- params: {"limit": 100}
-- FIN-108 slice 3 LOAN-9: loan arrears/delinquency snapshot, scope via a CTE
-- of in-scope loan ids (independent of the production COALESCE join).
-- overdue_since_date_derived travels on every row as the XR-ASOF signal.
WITH scoped AS (
    SELECT l.id FROM m_loan l JOIN m_client c ON c.id = l.client_id WHERE c.office_id = ANY(:'office_ids'::bigint[])
    UNION
    SELECT l.id FROM m_loan l JOIN m_group g ON g.id = l.group_id WHERE l.client_id IS NULL AND g.office_id = ANY(:'office_ids'::bigint[])
)
SELECT
    a.loan_id,
    a.principal_overdue_derived,
    a.interest_overdue_derived,
    a.fee_charges_overdue_derived,
    a.penalty_charges_overdue_derived,
    a.total_overdue_derived,
    a.overdue_since_date_derived
FROM m_loan_arrears_aging a
JOIN scoped s ON s.id = a.loan_id
ORDER BY a.overdue_since_date_derived ASC, a.loan_id
LIMIT 100
