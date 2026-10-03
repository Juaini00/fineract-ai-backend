-- Loan charges/penalties: charged vs actually-collected/waived/written-off
-- (dataset-inventory.md LOAN-7). Office-scoped via the owning loan's
-- client-or-group office (same path as LOAN-1..3); m_loan_charge has no
-- direct office_id.
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
JOIN m_loan l ON l.id = lc.loan_id
JOIN m_charge ch ON ch.id = lc.charge_id
LEFT JOIN m_client c ON c.id = l.client_id
LEFT JOIN m_group g ON g.id = l.group_id
WHERE COALESCE(c.office_id, g.office_id) = ANY($1::bigint[])
ORDER BY lc.loan_id, lc.id
LIMIT $2
