-- Loan outstanding balances read directly from m_loan's own *_derived
-- columns — never recomputed from schedule+transactions (dataset-inventory.md
-- LOAN-8, mirrors the GL read-recorded-value rule in D12).
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
LEFT JOIN m_client c ON c.id = l.client_id
LEFT JOIN m_group g ON g.id = l.group_id
WHERE COALESCE(c.office_id, g.office_id) = ANY($1::bigint[])
ORDER BY l.id
LIMIT $2
