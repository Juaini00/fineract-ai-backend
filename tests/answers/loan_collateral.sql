-- params: {"limit": 100}
-- FIN-108 slice 4 LOAN-10: pledged collateral per loan, scope via a UNION of
-- in-scope loan ids, value computed independently (same master-rate formula,
-- different join order than production).
WITH scoped AS (
    SELECT l.id FROM m_loan l JOIN m_client c ON c.id = l.client_id WHERE c.office_id = ANY(:'office_ids'::bigint[])
    UNION
    SELECT l.id FROM m_loan l JOIN m_group g ON g.id = l.group_id WHERE l.client_id IS NULL AND g.office_id = ANY(:'office_ids'::bigint[])
)
SELECT
    lcm.id AS loan_collateral_id,
    lcm.loan_id,
    lcm.quantity,
    lcm.is_released,
    cm.id AS collateral_type_id,
    cm.name AS collateral_type_name,
    cm.quality,
    cm.base_price,
    cm.pct_to_base,
    round((lcm.quantity * cm.base_price * cm.pct_to_base) / 100.0, 2) AS current_master_collateral_value,
    (SELECT cur.code FROM m_currency cur WHERE cur.id = cm.currency) AS collateral_currency_code,
    (SELECT l.currency_code FROM m_loan l WHERE l.id = lcm.loan_id) AS loan_currency_code
FROM m_collateral_management cm
JOIN m_client_collateral_management ccm ON ccm.collateral_id = cm.id
JOIN m_loan_collateral_management lcm ON lcm.client_collateral_id = ccm.id
JOIN scoped s ON s.id = lcm.loan_id
ORDER BY lcm.loan_id, lcm.id
LIMIT 100
