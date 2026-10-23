-- params: {"limit": 200}
-- FIN-130 LINK-8: payment type resolution and cash flags for transactions in authorized offices.
-- Written independently of queries/linking_resources/payment_type_resolution.sql:
-- payment detail and payment type joined through CTEs across loan and savings.
WITH pt_ref AS (
    SELECT pd.id AS payment_detail_id, pt.id AS payment_type_id, pt.value AS payment_type_name, pt.is_cash_payment AS is_cash
    FROM m_payment_detail pd
    LEFT JOIN m_payment_type pt ON pt.id = pd.payment_type_id
)
SELECT
    'loan'::text AS account_type,
    lt.id AS transaction_id,
    lt.loan_id AS account_id,
    lt.office_id,
    lt.transaction_date,
    lt.payment_detail_id,
    ptr.payment_type_id::bigint AS payment_type_id,
    ptr.payment_type_name,
    ptr.is_cash
FROM m_loan_transaction lt
LEFT JOIN pt_ref ptr ON ptr.payment_detail_id = lt.payment_detail_id
WHERE lt.office_id = ANY(:'office_ids'::bigint[])

UNION ALL

SELECT
    'savings'::text AS account_type,
    sat.id AS transaction_id,
    sat.savings_account_id AS account_id,
    sat.office_id,
    sat.transaction_date,
    sat.payment_detail_id,
    ptr.payment_type_id::bigint AS payment_type_id,
    ptr.payment_type_name,
    ptr.is_cash
FROM m_savings_account_transaction sat
LEFT JOIN pt_ref ptr ON ptr.payment_detail_id = sat.payment_detail_id
WHERE sat.office_id = ANY(:'office_ids'::bigint[])

ORDER BY office_id, transaction_date DESC, account_type, transaction_id DESC
LIMIT 200
