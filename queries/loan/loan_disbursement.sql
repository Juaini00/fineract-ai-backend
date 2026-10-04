-- Loan disbursement, planned vs actual at loan grain (dataset-inventory.md
-- LOAN-4). approved_principal is the account's approved amount, distinct
-- from net_disbursal_amount actually paid out (fees/charges may be netted
-- off at disbursement). tranche_count is a scalar subquery, not a join, so a
-- loan with multiple m_loan_disbursement_detail rows is never fanned out
-- here -- see loan.loan_disbursement_tranches for the tranche grain.
-- Office-scoped via the owning loan's client-or-group office (LOAN-1..3 path).
SELECT
    l.id AS loan_id,
    l.expected_disbursedon_date,
    l.disbursedon_date,
    l.approved_principal,
    l.net_disbursal_amount,
    l.currency_code,
    (SELECT COUNT(*) FROM m_loan_disbursement_detail d WHERE d.loan_id = l.id) AS tranche_count
FROM m_loan l
LEFT JOIN m_client c ON c.id = l.client_id
LEFT JOIN m_group g ON g.id = l.group_id
WHERE COALESCE(c.office_id, g.office_id) = ANY($1::bigint[])
ORDER BY l.id
LIMIT $2
