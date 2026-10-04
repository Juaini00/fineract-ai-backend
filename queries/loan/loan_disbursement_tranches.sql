-- Per-tranche planned vs actual disbursement for staged (multi-tranche) loans
-- (dataset-inventory.md LOAN-4). Separate capability from loan.loan_disbursement
-- so a staged loan's tranche rows are never fanned out against the loan-grain
-- planned/actual fields. A reversed tranche disbursement is returned with
-- is_reversed = true (planned/actual record, not a cash total). Amounts are in
-- the loan's currency. 0 rows on this deployment (every local loan is
-- single-disbursement); positive behaviour proven by a read-only fixture, see
-- knowledge/VERIFICATION.md.
SELECT
    d.id AS disbursement_detail_id,
    d.loan_id,
    d.expected_disburse_date,
    d.disbursedon_date,
    d.principal,
    d.net_disbursal_amount,
    d.is_reversed,
    l.currency_code
FROM m_loan_disbursement_detail d
JOIN m_loan l ON l.id = d.loan_id
LEFT JOIN m_client c ON c.id = l.client_id
LEFT JOIN m_group g ON g.id = l.group_id
WHERE COALESCE(c.office_id, g.office_id) = ANY($1::bigint[])
ORDER BY d.loan_id, d.expected_disburse_date, d.id
LIMIT $2
