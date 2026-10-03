-- params: {"limit": 100}
-- FIN-108 slice 3 LOAN-7: loan charges/penalties, scope via a CTE of
-- in-scope loan ids (independent of the production COALESCE join).
WITH scoped AS (
    SELECT l.id FROM m_loan l JOIN m_client c ON c.id = l.client_id WHERE c.office_id = ANY(:'office_ids'::bigint[])
    UNION
    SELECT l.id FROM m_loan l JOIN m_group g ON g.id = l.group_id WHERE l.client_id IS NULL AND g.office_id = ANY(:'office_ids'::bigint[])
)
SELECT
    lc.id AS loan_charge_id,
    lc.loan_id,
    lc.charge_id,
    ch.name AS charge_name,
    lc.is_penalty,
    lc.amount,
    lc.amount_paid_derived,
    lc.amount_waived_derived,
    lc.amount_writtenoff_derived,
    lc.amount_outstanding_derived,
    lc.is_paid_derived,
    lc.waived
FROM m_loan_charge lc
JOIN scoped s ON s.id = lc.loan_id
JOIN m_charge ch ON ch.id = lc.charge_id
ORDER BY lc.loan_id, lc.id
LIMIT 100
