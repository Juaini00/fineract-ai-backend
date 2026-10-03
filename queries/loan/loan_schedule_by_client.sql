-- Repayment schedule rows for one client's loans — a plan, never an actual
-- payment (dataset-inventory.md LOAN-5). Scope via the owning loan's
-- client-or-group office (same path as LOAN-1..3).
SELECT
    s.loan_id,
    s.installment::bigint AS installment,
    s.duedate,
    s.principal_amount,
    s.principal_completed_derived,
    s.interest_amount,
    s.interest_completed_derived,
    s.fee_charges_amount,
    s.fee_charges_completed_derived,
    s.penalty_charges_amount,
    s.penalty_charges_completed_derived,
    s.completed_derived
FROM m_loan_repayment_schedule s
JOIN m_loan l ON l.id = s.loan_id
LEFT JOIN m_client c ON c.id = l.client_id
LEFT JOIN m_group g ON g.id = l.group_id
WHERE l.client_id = $2::bigint
  AND (EXISTS (SELECT 1 FROM m_client cc WHERE cc.id = l.client_id AND cc.office_id = ANY($1::bigint[]))
       OR EXISTS (SELECT 1 FROM m_group gg WHERE gg.id = l.group_id AND l.client_id IS NULL AND gg.office_id = ANY($1::bigint[])))
ORDER BY s.loan_id, s.installment
