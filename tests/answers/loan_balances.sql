-- params: {"limit": 100}
-- FIN-108 slice 3 LOAN-8: loan outstanding balances read from m_loan's own
-- *_derived columns, scope via a CTE of in-scope loan ids (independent of
-- the production COALESCE join).
WITH scoped AS (
    SELECT l.id FROM m_loan l JOIN m_client c ON c.id = l.client_id WHERE c.office_id = ANY(:'office_ids'::bigint[])
    UNION
    SELECT l.id FROM m_loan l JOIN m_group g ON g.id = l.group_id WHERE l.client_id IS NULL AND g.office_id = ANY(:'office_ids'::bigint[])
)
SELECT
    l.id AS loan_id,
    l.currency_code,
    l.principal_outstanding_derived,
    l.interest_outstanding_derived,
    l.fee_charges_outstanding_derived,
    l.penalty_charges_outstanding_derived,
    l.total_outstanding_derived,
    l.total_overpaid_derived
FROM m_loan l
JOIN scoped s ON s.id = l.id
ORDER BY l.id
LIMIT 100
