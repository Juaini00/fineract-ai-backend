-- params: {"client_id": 14}
-- FIN-108 LOAN-5: planned repayment schedule for client 14 (78 installments,
-- mixed completed/pending so the "planned, not an event" label is exercised),
-- written independently (EXISTS scope check instead of a LEFT JOIN).
SELECT s.loan_id, s.installment::bigint AS installment, s.duedate,
       s.principal_amount, s.principal_completed_derived,
       s.interest_amount, s.interest_completed_derived,
       s.fee_charges_amount, s.fee_charges_completed_derived,
       s.penalty_charges_amount, s.penalty_charges_completed_derived,
       s.completed_derived
FROM m_loan_repayment_schedule s
WHERE s.loan_id IN (SELECT id FROM m_loan WHERE client_id = 14)
  AND EXISTS (SELECT 1 FROM m_client c WHERE c.id = 14 AND c.office_id = ANY(:'office_ids'::bigint[]))
ORDER BY s.loan_id, s.installment
