-- params: {"limit": 200}
-- NARROWED to offices 2 and 3 (partial authorization proof): the job body carries office_ids [2,3].
-- FIN-130 LINK-5: loan payment allocation components per transaction in offices 2 and 3.
WITH loan_ref AS (
    SELECT id AS loan_id, currency_code
    FROM m_loan
)
SELECT
    lt.id AS loan_transaction_id,
    lt.loan_id,
    lt.office_id,
    lt.transaction_date,
    lt.transaction_type_enum::bigint AS transaction_type_enum,
    lt.amount,
    lt.principal_portion_derived,
    lt.interest_portion_derived,
    lt.fee_charges_portion_derived,
    lt.penalty_charges_portion_derived,
    lr.currency_code,
    lt.is_reversed
FROM m_loan_transaction lt
JOIN loan_ref lr ON lr.loan_id = lt.loan_id
WHERE lt.office_id = ANY(ARRAY[2,3]::bigint[])
ORDER BY lt.office_id, lt.transaction_date DESC, lt.id DESC
LIMIT 200
