-- params: {"limit": 100}
-- FIN-108 slice 4 LOAN-11: recovery payments (transaction_type_enum = 8),
-- reversed excluded. office_id is direct on m_loan_transaction (LOAN-6
-- path); written independently (explicit boolean literal, filter order
-- swapped vs production). 0 rows on this deployment (no recovery
-- transaction recorded locally).
SELECT
    t.id AS transaction_id,
    t.loan_id,
    t.transaction_date,
    t.amount,
    t.principal_portion_derived,
    t.interest_portion_derived,
    t.fee_charges_portion_derived,
    t.penalty_charges_portion_derived,
    t.office_id,
    (SELECT l.currency_code FROM m_loan l WHERE l.id = t.loan_id) AS currency_code
FROM m_loan_transaction t
WHERE t.office_id = ANY(:'office_ids'::bigint[])
  AND t.transaction_type_enum = 8
  AND NOT t.is_reversed
ORDER BY t.transaction_date DESC, t.id DESC
LIMIT 100
