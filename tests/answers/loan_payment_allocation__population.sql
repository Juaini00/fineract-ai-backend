-- params: {}
-- Full population size for loan_payment_allocation without row cap.
SELECT count(*) AS n
FROM m_loan_transaction lt
WHERE lt.office_id = ANY(:'office_ids'::bigint[])
