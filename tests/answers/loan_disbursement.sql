-- params: {"limit": 100}
-- FIN-108 slice 4 LOAN-4: planned vs actual disbursement at loan grain,
-- scope via a UNION of in-scope loan ids (independent of the production
-- COALESCE join).
WITH scoped AS (
    SELECT l.id FROM m_loan l JOIN m_client c ON c.id = l.client_id WHERE c.office_id = ANY(:'office_ids'::bigint[])
    UNION
    SELECT l.id FROM m_loan l JOIN m_group g ON g.id = l.group_id WHERE l.client_id IS NULL AND g.office_id = ANY(:'office_ids'::bigint[])
)
SELECT
    l.id AS loan_id,
    l.expected_disbursedon_date,
    l.disbursedon_date,
    l.approved_principal,
    l.net_disbursal_amount,
    l.currency_code,
    (SELECT count(*) FROM m_loan_disbursement_detail d WHERE d.loan_id = l.id) AS tranche_count
FROM m_loan l
JOIN scoped s ON s.id = l.id
ORDER BY l.id
LIMIT 100
