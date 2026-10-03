-- Loan arrears/delinquency snapshot (dataset-inventory.md LOAN-9). Only
-- present while a loan is overdue (1:1 loan). XR-ASOF: overdue_since_date_derived
-- is the row's own as-of signal; this is a point-in-time snapshot, not
-- asserted as always-current — freshness depends on COB having run (D14,
-- still gap, no job-run join here). Office via the owning loan.
SELECT
    a.loan_id,
    a.principal_overdue_derived,
    a.interest_overdue_derived,
    a.fee_charges_overdue_derived,
    a.penalty_charges_overdue_derived,
    a.total_overdue_derived,
    a.overdue_since_date_derived
FROM m_loan_arrears_aging a
JOIN m_loan l ON l.id = a.loan_id
LEFT JOIN m_client c ON c.id = l.client_id
LEFT JOIN m_group g ON g.id = l.group_id
WHERE COALESCE(c.office_id, g.office_id) = ANY($1::bigint[])
ORDER BY a.overdue_since_date_derived ASC, a.loan_id
LIMIT $2
