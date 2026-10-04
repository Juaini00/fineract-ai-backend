-- Pledged collateral per loan (dataset-inventory.md LOAN-10), kept distinct
-- from guarantor (see loan.loan_guarantor) -- no fanout, since each pledge
-- row (m_loan_collateral_management) resolves to exactly one client
-- collateral item and exactly one collateral master row (FK chain).
-- current_master_collateral_value is quantity * base_price * pct_to_base / 100
-- using the collateral master's CURRENT base_price/pct_to_base: Fineract keeps
-- no historic pledged valuation, so this is a present-rate computation, not
-- the value recorded when the pledge was made. It is denominated in the
-- collateral master's own currency (m_collateral_management.currency is a FK
-- to m_currency.id), which can differ from the loan currency (locally the
-- master is USD while the loans are AED) -- both are returned, never assumed
-- equal. Office-scoped via the owning loan's client-or-group office.
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
    ROUND(lcm.quantity * cm.base_price * cm.pct_to_base / 100.0, 2) AS current_master_collateral_value,
    cur.code AS collateral_currency_code,
    l.currency_code AS loan_currency_code
FROM m_loan_collateral_management lcm
JOIN m_loan l ON l.id = lcm.loan_id
JOIN m_client_collateral_management ccm ON ccm.id = lcm.client_collateral_id
JOIN m_collateral_management cm ON cm.id = ccm.collateral_id
LEFT JOIN m_currency cur ON cur.id = cm.currency
LEFT JOIN m_client c ON c.id = l.client_id
LEFT JOIN m_group g ON g.id = l.group_id
WHERE COALESCE(c.office_id, g.office_id) = ANY($1::bigint[])
ORDER BY lcm.loan_id, lcm.id
LIMIT $2
