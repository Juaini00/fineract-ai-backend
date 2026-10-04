-- params: {"limit": 100}
-- FIN-108 slice 4 LOAN-4: per-tranche disbursement rows, scope via a UNION
-- of in-scope loan ids. 0 rows on this deployment (no multi-tranche
-- loans locally).
WITH scoped AS (
    SELECT l.id FROM m_loan l JOIN m_client c ON c.id = l.client_id WHERE c.office_id = ANY(:'office_ids'::bigint[])
    UNION
    SELECT l.id FROM m_loan l JOIN m_group g ON g.id = l.group_id WHERE l.client_id IS NULL AND g.office_id = ANY(:'office_ids'::bigint[])
)
SELECT
    d.id AS disbursement_detail_id,
    d.loan_id,
    d.expected_disburse_date,
    d.disbursedon_date,
    d.principal,
    d.net_disbursal_amount,
    d.is_reversed,
    (SELECT l.currency_code FROM m_loan l WHERE l.id = d.loan_id) AS currency_code
FROM m_loan_disbursement_detail d
JOIN scoped s ON s.id = d.loan_id
ORDER BY d.loan_id, d.expected_disburse_date, d.id
LIMIT 100
